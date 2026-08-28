use jellypilot_media_server::{JellyfinClient, PlaybackEngineKind};
use serde_json::Value;

use crate::JellyfinWebSocketEvent;

/// Consumer-visible lifecycle state for the remote-control command stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteControlState {
    Unavailable,
    Connecting,
    Available,
    Lost,
}

/// Applies a WebSocket event to the consumer-visible remote-control lifecycle.
#[must_use]
pub const fn remote_state_after_event(
    state: RemoteControlState,
    event: &JellyfinWebSocketEvent,
) -> RemoteControlState {
    match event {
        JellyfinWebSocketEvent::Connected | JellyfinWebSocketEvent::Reconnected => {
            RemoteControlState::Available
        }
        JellyfinWebSocketEvent::ConnectionLost => RemoteControlState::Lost,
        JellyfinWebSocketEvent::Command(_) => state,
    }
}

/// Parses Jellyfin's numeric or string volume payload and clamps it to MPV's range.
#[must_use]
pub fn remote_volume_value(value: Option<&Value>) -> Option<f64> {
    let volume = match value? {
        Value::Number(number) => number.as_f64()?,
        Value::String(value) => value.trim().parse().ok()?,
        _ => return None,
    };
    volume.is_finite().then(|| volume.clamp(0.0, 100.0))
}

/// Registers remote-control capabilities before informational session validation.
///
/// A fresh socket session may not be visible to validation yet, so validation
/// failure is returned as `Ok(false)`. Capability registration remains required.
///
/// # Errors
///
/// Returns `Err(())` when capability registration is rejected.
pub async fn finalize_remote_target(client: &JellyfinClient) -> Result<bool, ()> {
    client
        .playback()
        .report_capabilities_for_checked(PlaybackEngineKind::ExternalMpv)
        .await
        .map_err(|_| ())?;
    Ok(client.playback().validate_session().await.is_ok())
}

#[cfg(test)]
mod tests {
    use std::future::Future;

    use jellypilot_media_server::{Credentials, MediaServerProvider};

    use super::*;

    fn run_async<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("test runtime should build")
            .block_on(future)
    }

    fn serve_http_responses(
        responses: Vec<(&'static str, &'static str)>,
    ) -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("fake server should bind");
        let address = listener
            .local_addr()
            .expect("fake server should have an address");
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().expect("fake server should accept");
                let mut buffer = [0_u8; 8192];
                let read = stream
                    .read(&mut buffer)
                    .expect("fake server should read the request");
                sender
                    .send(String::from_utf8_lossy(&buffer[..read]).into_owned())
                    .expect("request log should send");
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("fake server should write the response");
            }
        });
        (format!("http://{address}"), receiver)
    }

    async fn authenticated_client(server_url: String) -> JellyfinClient {
        let client = JellyfinClient::new();
        client
            .login()
            .authenticate(&Credentials {
                provider: MediaServerProvider::Jellyfin,
                server_url,
                username: "Ada".to_owned(),
                password: "correct horse battery staple".to_owned(),
            })
            .await
            .expect("authentication against the fake server should succeed");
        client
    }

    #[test]
    fn finalize_remote_target_reports_capabilities_before_informational_validation() {
        let (server_url, requests) = serve_http_responses(vec![
            (
                "200 OK",
                r#"{"User":{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"},"AccessToken":"token-1","ServerId":"server-1"}"#,
            ),
            (
                "200 OK",
                r#"{"ServerName":"Fake","Version":"10.10.0","Id":"server-1"}"#,
            ),
            ("200 OK", ""),
            ("500 Internal Server Error", r#"{"Message":"boom"}"#),
        ]);
        let client = run_async(async {
            let client = authenticated_client(server_url).await;
            let validated = finalize_remote_target(&client)
                .await
                .expect("a validation failure must not fail remote-target setup");
            assert!(!validated, "validation failed softly");
            client
        });
        drop(client);

        let _authentication = requests.recv().expect("authentication request captured");
        let _information = requests.recv().expect("information request captured");
        let capabilities = requests.recv().expect("capabilities request captured");
        let validation = requests.recv().expect("validation request captured");
        assert!(capabilities.starts_with("POST /Sessions/Capabilities"));
        assert!(validation.starts_with("GET /Sessions"));
    }

    #[test]
    fn finalize_remote_target_fails_when_capability_report_is_rejected() {
        let (server_url, _requests) = serve_http_responses(vec![
            (
                "200 OK",
                r#"{"User":{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"},"AccessToken":"token-1","ServerId":"server-1"}"#,
            ),
            (
                "200 OK",
                r#"{"ServerName":"Fake","Version":"10.10.0","Id":"server-1"}"#,
            ),
            ("500 Internal Server Error", r#"{"Message":"boom"}"#),
        ]);
        run_async(async {
            let client = authenticated_client(server_url).await;
            assert!(finalize_remote_target(&client).await.is_err());
        });
    }

    #[test]
    fn remote_volume_accepts_wire_string_and_number_forms() {
        assert_eq!(
            remote_volume_value(Some(&serde_json::json!("50"))),
            Some(50.0)
        );
        assert_eq!(
            remote_volume_value(Some(&serde_json::json!(125))),
            Some(100.0)
        );
        assert_eq!(
            remote_volume_value(Some(&serde_json::json!("invalid"))),
            None
        );
    }

    #[test]
    fn remote_lifecycle_transitions_remain_honest() {
        assert_eq!(
            remote_state_after_event(
                RemoteControlState::Connecting,
                &JellyfinWebSocketEvent::Connected,
            ),
            RemoteControlState::Available
        );
        assert_eq!(
            remote_state_after_event(
                RemoteControlState::Available,
                &JellyfinWebSocketEvent::ConnectionLost,
            ),
            RemoteControlState::Lost
        );
    }
}
