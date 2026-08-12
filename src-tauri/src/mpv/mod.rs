//! Compatibility exports for the reusable MPV crate.

pub(crate) use jellypilot_mpv::has_mpv_option;
#[cfg(test)]
pub(crate) struct MpvIpc;
#[cfg(test)]
impl MpvIpc {
  pub(crate) async fn from_io_for_test<R, W>(
    reader: R,
    writer: W,
  ) -> Result<MpvClient, jellypilot_mpv::MpvError>
  where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
  {
    MpvClient::from_io_for_test(reader, writer).await
  }
}
pub use jellypilot_mpv::{
  collect_player_state, find_mpv, write_input_conf, MpvClient, MpvEvent, PlayerState,
  PropertyValue, TransportSnapshot,
};
