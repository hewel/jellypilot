use super::{test_sdk, test_sdk_with, test_session, MemoryCredential};
use crate::{SdkError, ServerIdentity};
use jellypilot_media_server::MediaServerProvider;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

const JELLYFIN: &str = r#"{"ServerName":"Family Cinema","Version":"10.11.0","Id":"server-id","ProductName":"Jellyfin Server"}"#;
const AMBIGUOUS: &str =
    r#"{"ServerName":"Jellyfin Movie Room","Version":"4.9.3.0","Id":"server-id"}"#;

struct PublicServer {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for PublicServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(routes: Vec<(String, String)>) -> PublicServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("address"));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&requests);
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let request = read_headers(&mut socket).await;
            let path = request.split_whitespace().nth(1).expect("request path");
            let response = routes
                .iter()
                .find(|(route, _)| path == route)
                .map(|(_, response)| response.clone())
                .unwrap_or_else(|| response("404 Not Found", "{}"));
            observed.lock().expect("requests").push(request);
            socket
                .write_all(response.as_bytes())
                .await
                .expect("response");
        }
    });
    PublicServer {
        base,
        requests,
        task,
    }
}

fn response(status: &str, body: &str) -> String {
    format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

async fn read_headers(socket: &mut tokio::net::TcpStream) -> String {
    let mut request = Vec::new();
    loop {
        let mut chunk = [0_u8; 1024];
        let count = socket.read(&mut chunk).await.expect("read request");
        assert!(count > 0, "request closed before headers");
        request.extend_from_slice(&chunk[..count]);
        if request.windows(4).any(|part| part == b"\r\n\r\n") {
            return String::from_utf8(request).expect("request text");
        }
    }
}

#[tokio::test]
async fn probe_preserves_port_prefix_profile_and_credentials_without_authenticating() {
    let server = serve(vec![(
        "/media/System/Info/Public".to_owned(),
        response("200 OK", JELLYFIN),
    )])
    .await;
    let credential = Arc::new(MemoryCredential::default());
    let original_secret = b"untouched credentials".to_vec();
    *credential.secret.lock().expect("secret") = Some(original_secret.clone());
    let (sdk, _dir) = test_sdk_with(None, Arc::clone(&credential));
    sdk.adopt_test_session(test_session("existing", "https://existing.example.test"));
    let active_key = sdk.active_profile().expect("active profile").key;
    let token = sdk.new_operation_token().expect("token");

    let identity = sdk
        .probe_server(format!(" {}/media/ ", server.base))
        .await
        .expect("probe");

    assert_eq!(
        identity,
        ServerIdentity {
            server_url: format!("{}/media", server.base),
            server_name: "Family Cinema".to_owned(),
            provider: Some(MediaServerProvider::Jellyfin),
        }
    );
    assert_eq!(
        sdk.active_profile().expect("active profile").key,
        active_key
    );
    assert!(!token.is_cancelled());
    assert_eq!(
        *credential.secret.lock().expect("secret"),
        Some(original_secret)
    );
    let requests = server.requests.lock().expect("requests");
    assert_eq!(requests.len(), 1);
    assert!(!requests[0].contains("Token="));
    assert!(!requests[0].contains("existing"));
}

#[tokio::test]
async fn probe_discovers_emby_suffix_without_guessing_provider_from_name_or_version() {
    let server = serve(vec![(
        "/proxy/emby/System/Info/Public".to_owned(),
        response("200 OK", AMBIGUOUS),
    )])
    .await;
    let (sdk, _dir) = test_sdk();
    let identity = sdk
        .probe_server(format!("{}/proxy", server.base))
        .await
        .expect("probe");
    assert_eq!(
        identity,
        ServerIdentity {
            server_url: format!("{}/proxy/emby", server.base),
            server_name: "Jellyfin Movie Room".to_owned(),
            provider: None,
        }
    );
    assert_eq!(server.requests.lock().expect("requests").len(), 2);
}

#[tokio::test]
async fn probe_resolves_same_origin_redirect_to_actual_api_base() {
    let redirect = "HTTP/1.1 302 Found\r\nLocation: /nested/emby/System/Info/Public\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let server = serve(vec![
        ("/System/Info/Public".to_owned(), redirect.to_owned()),
        (
            "/nested/emby/System/Info/Public".to_owned(),
            response("200 OK", AMBIGUOUS),
        ),
    ])
    .await;
    let (sdk, _dir) = test_sdk();
    let identity = sdk.probe_server(server.base.clone()).await.expect("probe");
    assert_eq!(identity.server_url, format!("{}/nested/emby", server.base));
}

#[tokio::test]
async fn probe_rejects_cross_origin_redirect_without_contacting_target() {
    let target = serve(vec![(
        "/System/Info/Public".to_owned(),
        response("200 OK", JELLYFIN),
    )])
    .await;
    let redirect = format!("HTTP/1.1 302 Found\r\nLocation: {}/System/Info/Public\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", target.base);
    let server = serve(vec![("/System/Info/Public".to_owned(), redirect)]).await;
    let (sdk, _dir) = test_sdk();
    assert!(matches!(
        sdk.probe_server(server.base.clone()).await,
        Err(SdkError::Request(_))
    ));
    assert!(target.requests.lock().expect("requests").is_empty());
}

#[tokio::test]
async fn probe_rejects_invalid_inputs_before_connecting() {
    let (sdk, _dir) = test_sdk();
    for input in [
        "",
        "server:8096",
        "ftp://example.test",
        "http://user:secret@example.test",
        "http://example.test?secret=x",
        "http://example.test/%2e%2e/private",
    ] {
        assert!(
            matches!(
                sdk.probe_server(input.to_owned()).await,
                Err(SdkError::InvalidInput(_))
            ),
            "{input}"
        );
    }
}

#[tokio::test]
async fn probe_rejects_non_server_payloads_http_errors_and_oversized_responses() {
    let oversized = "x".repeat(64 * 1024 + 1);
    let (sdk, _dir) = test_sdk();
    for (status, body) in [
        ("200 OK", "{}"),
        ("200 OK", "<html>proxy login</html>"),
        ("200 OK", r#"{"ServerName":"","Version":"1","Id":"x"}"#),
        ("503 Service Unavailable", "secret response"),
        ("200 OK", oversized.as_str()),
    ] {
        let server = serve(vec![(
            "/emby/System/Info/Public".to_owned(),
            response(status, body),
        )])
        .await;
        let error = sdk
            .probe_server(format!("{}/emby", server.base))
            .await
            .expect_err("reject");
        assert!(matches!(error, SdkError::Request(_)));
        assert!(!error.to_string().contains(&server.base));
        assert!(!error.to_string().contains("secret response"));
    }
}

#[tokio::test]
async fn restricted_public_info_preserves_profile_and_allows_explicit_emby_authentication() {
    for status in ["401 Unauthorized", "403 Forbidden"] {
        let server = serve(vec![
            ("/media/System/Info/Public".to_owned(), response(status, "private response")),
            ("/media/emby/System/Info/Public".to_owned(), response(status, "private response")),
            (
                "/media/emby/Users/AuthenticateByName".to_owned(),
                response("200 OK", r#"{"User":{"Id":"new-user","Name":"New User"},"AccessToken":"new-secret-token","ServerId":"server-id"}"#),
            ),
            ("/media/emby/System/Info".to_owned(), response("200 OK", AMBIGUOUS)),
        ]).await;
        let credential = Arc::new(MemoryCredential::default());
        let original_secret = b"original credentials".to_vec();
        *credential.secret.lock().expect("secret") = Some(original_secret.clone());
        let (sdk, _dir) = test_sdk_with(None, Arc::clone(&credential));
        sdk.adopt_test_session(test_session("existing", "https://existing.example.test"));
        let active_key = sdk.active_profile().expect("active profile").key;
        let token = sdk.new_operation_token().expect("token");
        let url = format!("{}/media", server.base);

        assert_eq!(
            sdk.probe_server(url.clone()).await,
            Err(SdkError::ServerInfoRestricted)
        );
        assert_eq!(
            sdk.active_profile().expect("active profile").key,
            active_key
        );
        assert_eq!(
            *credential.secret.lock().expect("secret"),
            Some(original_secret.clone())
        );
        assert!(!token.is_cancelled());
        {
            let requests = server.requests.lock().expect("requests");
            assert_eq!(requests.len(), 2);
            assert!(requests.iter().all(|request| !request.contains("Token=")));
        }

        let candidate = sdk
            .password_login(
                MediaServerProvider::Emby,
                url,
                "New User".to_owned(),
                String::new(),
            )
            .await
            .expect("manual authentication candidate");
        assert_eq!(candidate.provider(), MediaServerProvider::Emby);
        assert_eq!(
            sdk.active_profile().expect("active profile").key,
            active_key
        );
        assert_eq!(
            *credential.secret.lock().expect("secret"),
            Some(original_secret)
        );
        assert!(!token.is_cancelled());
    }
}

#[tokio::test]
async fn restricted_probe_ignores_missing_bases_but_does_not_mask_other_failures() {
    let (sdk, _dir) = test_sdk();
    for (alternate_status, body, restricted) in [
        ("404 Not Found", "{}", true),
        ("503 Service Unavailable", "private response", false),
        ("200 OK", "invalid json", false),
    ] {
        let server = serve(vec![
            (
                "/System/Info/Public".to_owned(),
                response("403 Forbidden", "private response"),
            ),
            (
                "/emby/System/Info/Public".to_owned(),
                response(alternate_status, body),
            ),
        ])
        .await;
        let error = sdk
            .probe_server(server.base.clone())
            .await
            .expect_err("probe failed");
        if restricted {
            assert_eq!(error, SdkError::ServerInfoRestricted);
        } else {
            assert!(matches!(error, SdkError::Request(_)), "{error:?}");
        }
        assert!(!error.to_string().contains(&server.base));
        assert!(!error.to_string().contains("private response"));
    }
}

#[tokio::test]
async fn dropping_probe_waiter_cancels_socket_without_ending_active_profile() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("address"));
    let (entered_tx, entered_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        read_headers(&mut socket).await;
        entered_tx.send(()).expect("entered");
        let mut byte = [0_u8; 1];
        socket.read(&mut byte).await.expect("socket close")
    });
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("existing", "https://existing.example.test"));
    let token = sdk.new_operation_token().expect("token");
    let mut probe = Box::pin(sdk.probe_server(base));
    tokio::select! {
        result = &mut probe => panic!("probe settled before response: {result:?}"),
        result = entered_rx => result.expect("request entered"),
    }
    drop(probe);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .expect("request cancelled")
            .expect("server"),
        0
    );
    assert!(!token.is_cancelled());
    assert!(sdk.active_profile().is_some());
}

#[tokio::test]
async fn password_login_distinguishes_credentials_from_transport_and_server_failures() {
    for (provider, public_info_available) in [
        (MediaServerProvider::Jellyfin, true),
        (MediaServerProvider::Emby, true),
        (MediaServerProvider::Emby, false),
    ] {
        for status in [
            "401 Unauthorized",
            "403 Forbidden",
            "503 Service Unavailable",
        ] {
            let server = serve(vec![
                (
                    "/emby/System/Info/Public".to_owned(),
                    response(
                        if public_info_available {
                            "200 OK"
                        } else {
                            "403 Forbidden"
                        },
                        AMBIGUOUS,
                    ),
                ),
                (
                    "/emby/Users/AuthenticateByName".to_owned(),
                    response(status, "secret rejection"),
                ),
            ])
            .await;
            let (sdk, _dir) = test_sdk();
            let result = sdk
                .password_login(
                    provider,
                    format!("{}/emby", server.base),
                    "user".to_owned(),
                    String::new(),
                )
                .await;
            let error = result.expect_err("login rejected");
            if status.starts_with('4') {
                assert!(
                    matches!(error, SdkError::Authentication(_)),
                    "{provider:?}: {error:?}"
                );
            } else {
                assert!(
                    matches!(error, SdkError::Request(_)),
                    "{provider:?}: {error:?}"
                );
            }
            assert!(!error.to_string().contains("secret rejection"));
            assert!(!error.to_string().contains(&server.base));
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("address"));
    drop(listener);
    let (sdk, _dir) = test_sdk();
    assert!(matches!(
        sdk.password_login(
            MediaServerProvider::Jellyfin,
            base,
            "user".to_owned(),
            String::new()
        )
        .await,
        Err(SdkError::Request(_))
    ));
}
