//! Event-driven source transfer detection for embedded presentation.

use std::path::Path;

use crate::{ipc::MpvIpc, protocol::MpvCommand, MpvError};

const OBSERVER: i64 = 1;
const PROPERTY: &str = "video-params/gamma";

/// Observes decoded video, not the renderer's target color space. PQ and HLG
/// request HDR; unavailable, unknown and SDR transfers do not.
///
/// Owns a separate read-only IPC connection: MPV events on the control client
/// are a work queue, not a broadcast. Dropping this observer closes only its
/// connection and never stops playback or destroys the embedded host.
pub struct VideoSourceObserver {
  ipc: MpvIpc,
  previous: Option<bool>,
}

impl VideoSourceObserver {
  /// Connect to an existing embedded host and subscribe to its current source.
  /// Does not launch MPV, capture duplicate player logs or modify playback.
  pub async fn connect(path: &Path) -> Result<Self, MpvError> {
    let path = path
      .to_str()
      .ok_or_else(|| MpvError::IpcConnectionFailed("Embedded IPC path is not UTF-8".into()))?;
    Self::observe(MpvIpc::connect_observer(path).await?).await
  }

  async fn observe(ipc: MpvIpc) -> Result<Self, MpvError> {
    let response = ipc
      .send_command(MpvCommand::observe_property(OBSERVER, PROPERTY))
      .await?;
    if !response.is_success() {
      return Err(MpvError::CommandFailed);
    }
    Ok(Self {
      ipc,
      previous: None,
    })
  }

  /// Yield the initial HDR classification, then only changes. MPV reports an
  /// unavailable transfer when video is unloaded or disabled, returning SDR;
  /// pause retains the source classification. A disconnect returns an error,
  /// allowing the presentation owner to clear its state and reconnect.
  pub async fn next_hdr(&mut self) -> Result<bool, MpvError> {
    let events = self.ipc.events();
    loop {
      let event = events.recv().await.map_err(|_| MpvError::IpcDisconnected)?;
      if event.event != "property-change"
        || event.id != Some(OBSERVER)
        || event.name.as_deref() != Some(PROPERTY)
      {
        continue;
      }
      let hdr = matches!(
        event.data.as_ref().and_then(|value| value.as_str()),
        Some("pq" | "hlg")
      );
      if self.previous != Some(hdr) {
        self.previous = Some(hdr);
        return Ok(hdr);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader};

  #[tokio::test]
  async fn source_changes_ignore_output_and_clear_on_unload_or_disconnect() {
    let (client, peer) = duplex(4096);
    let (reader, writer) = tokio::io::split(client);
    let ipc = MpvIpc::from_io_for_test(reader, writer).await.unwrap();
    let peer = tokio::spawn(async move {
      let (reader, mut writer) = tokio::io::split(peer);
      let mut lines = BufReader::new(reader).lines();
      let request: serde_json::Value =
        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
      writer
        .write_all(
          format!(
            "{}\n",
            serde_json::json!({
              "request_id": request["request_id"], "error": "success"
            })
          )
          .as_bytes(),
        )
        .await
        .unwrap();
      // Output PQ must not create a self-sustaining HDR feedback loop. A
      // disabled video track (missing data) and an SDR replacement clear HDR.
      for event in [
        serde_json::json!({"event":"property-change", "id":1, "name":"video-params/gamma"}),
        serde_json::json!({"event":"property-change", "id":1, "name":"video-target-params/gamma", "data":"pq"}),
        serde_json::json!({"event":"property-change", "id":1, "name":"video-params/gamma", "data":"pq"}),
        serde_json::json!({"event":"property-change", "id":1, "name":"video-params/gamma", "data":"hlg"}),
        serde_json::json!({"event":"property-change", "id":1, "name":"video-params/gamma", "data":"bt.1886"}),
        serde_json::json!({"event":"property-change", "id":1, "name":"video-params/gamma", "data":"hlg"}),
        serde_json::json!({"event":"property-change", "id":1, "name":"video-params/gamma"}),
      ] {
        writer
          .write_all(format!("{event}\n").as_bytes())
          .await
          .unwrap();
      }
    });
    let mut observer = VideoSourceObserver::observe(ipc).await.unwrap();
    for expected in [false, true, false, true, false] {
      assert_eq!(observer.next_hdr().await.unwrap(), expected);
    }
    assert!(matches!(
      observer.next_hdr().await,
      Err(MpvError::IpcDisconnected)
    ));
    peer.await.unwrap();
  }
}
