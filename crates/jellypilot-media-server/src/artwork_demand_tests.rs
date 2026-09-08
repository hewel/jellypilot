use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

struct Origin {
  listener: TcpListener,
  url: String,
}

impl Origin {
  async fn new() -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind origin");
    let url = format!("http://{}", listener.local_addr().expect("origin address"));
    Self { listener, url }
  }

  async fn request(&self) -> (TcpStream, String) {
    tokio::time::timeout(Duration::from_secs(5), async {
      let (mut socket, _) = self.listener.accept().await.expect("accept image request");
      let mut head = Vec::new();
      let mut buffer = [0; 1024];
      while !head.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        let count = socket.read(&mut buffer).await.expect("read request");
        assert_ne!(count, 0, "request ends after headers");
        head.extend_from_slice(&buffer[..count]);
      }
      (
        socket,
        String::from_utf8(head).expect("HTTP request is text"),
      )
    })
    .await
    .expect("image request starts")
  }

  fn image(&self, item: &str) -> String {
    crate::image_id_for_url(
      crate::MediaServerProvider::Jellyfin,
      &self.url,
      format!("{}/Items/{item}/Images/Primary", self.url),
      crate::ImageRefKind::Artwork,
    )
    .expect("valid image reference")
  }

  fn client(&self) -> Arc<JellyfinClient> {
    let client = Arc::new(JellyfinClient::new());
    super::tests::adopt_session(&client, &self.url, "user");
    client
  }
}

fn adapter(max_active_loads: usize) -> Arc<ArtworkAdapter> {
  let adapter = Arc::new(ArtworkAdapter::with_limits(ArtworkLimits {
    max_active_loads,
    ..ArtworkLimits::default()
  }));
  adapter.set_disk_cache_enabled(false);
  adapter
}

fn pending(
  adapter: &Arc<ArtworkAdapter>,
  client: &Arc<JellyfinClient>,
  image: String,
  lane: LoadLane,
) -> (
  ArtworkDemandControl,
  oneshot::Receiver<ArtworkDemandSettlement>,
) {
  match adapter.demand(
    Arc::clone(client),
    image,
    ArtworkSizeClass::Card,
    DerivedArtwork::default(),
    adapter.ticket(),
    lane,
  ) {
    ArtworkDemand::Pending { control, receiver } => (control, receiver),
    ArtworkDemand::Ready(_) => panic!("expected cold pending demand"),
  }
}

async fn respond(mut socket: TcpStream) {
  let png = super::tests::encode_test_png(8, 12);
  let head = format!(
    "HTTP/1.1 200 OK\r\ncontent-type: image/png\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
    png.len(),
  );
  socket
    .write_all(head.as_bytes())
    .await
    .expect("write headers");
  socket.write_all(&png).await.expect("write image");
}

async fn settled(receiver: oneshot::Receiver<ArtworkDemandSettlement>) -> ArtworkDemandSettlement {
  tokio::time::timeout(Duration::from_secs(5), receiver)
    .await
    .expect("demand settles")
    .expect("worker sends settlement")
}

#[tokio::test]
async fn initiating_legacy_future_can_leave_while_native_follower_finishes_shared_work() {
  let origin = Origin::new().await;
  let client = origin.client();
  let adapter = adapter(1);
  let legacy = {
    let adapter = Arc::clone(&adapter);
    let client = Arc::clone(&client);
    let image = origin.image("shared");
    tokio::spawn(async move {
      adapter
        .load(&client, &image, ArtworkSizeClass::Card, LoadLane::Offscreen)
        .await
    })
  };
  let (socket, _) = origin.request().await;
  let (_follower, receiver) = pending(&adapter, &client, origin.image("shared"), LoadLane::Visible);
  legacy.abort();
  assert!(legacy
    .await
    .expect_err("initiator cancelled")
    .is_cancelled());
  respond(socket).await;
  let (result, observation) = settled(receiver).await;
  let raster = result.expect("follower survives initiator exit");
  assert_eq!((raster.width(), raster.height()), (8, 12));
  assert_eq!(
    observation.settlement,
    ArtworkLoadSettlement::Loaded(ArtworkSource::Network)
  );
}

#[tokio::test]
async fn last_demand_cancels_network_and_releases_capacity_for_replacement() {
  let origin = Origin::new().await;
  let client = origin.client();
  let adapter = adapter(1);
  let (control, receiver) = pending(&adapter, &client, origin.image("cancel"), LoadLane::Visible);
  let (mut socket, _) = origin.request().await;
  drop(control);
  assert!(
    receiver.await.is_err(),
    "removed consumer cannot receive a late result"
  );
  let closed = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut [0; 1]))
    .await
    .expect("cancelled request closes")
    .expect("socket reads EOF");
  assert_eq!(closed, 0);
  let (_replacement, receiver) =
    pending(&adapter, &client, origin.image("cancel"), LoadLane::Visible);
  let (socket, _) = origin.request().await;
  respond(socket).await;
  assert_eq!(
    settled(receiver)
      .await
      .0
      .expect("replacement succeeds")
      .width(),
    8
  );
}

#[tokio::test]
async fn late_attempt_cannot_settle_or_remove_same_generation_replacement() {
  let origin = Origin::new().await;
  let client = origin.client();
  let adapter = adapter(1);
  let image = origin.image("replace");
  let (old, old_receiver) = pending(&adapter, &client, image.clone(), LoadLane::Visible);
  let context = LoadContext {
    key: old.key.clone(),
    generation: adapter.ticket().generation(),
    attempt: old.attempt,
  };
  drop(old);
  drop(old_receiver);
  let (_replacement, receiver) = pending(&adapter, &client, image, LoadLane::Visible);
  // Force a noninterruptible old attempt's completion after replacement.
  adapter.finish_pending(
    &context,
    (
      Ok(super::tests::raster(99, 99)),
      ArtworkLoadObservation::raster_hit(99 * 99 * 4),
    ),
  );
  let (socket, _) = origin.request().await;
  respond(socket).await;
  let raster = settled(receiver)
    .await
    .0
    .expect("replacement still owns key");
  assert_eq!((raster.width(), raster.height()), (8, 12));
}

#[tokio::test]
async fn live_priority_promotes_and_demotes_shared_queue_without_exceeding_budget() {
  let origin = Origin::new().await;
  let client = origin.client();
  let adapter = adapter(1);
  let (_active, active_result) =
    pending(&adapter, &client, origin.image("active"), LoadLane::Visible);
  let (active_socket, _) = origin.request().await;
  let (_first, first_result) = pending(
    &adapter,
    &client,
    origin.image("first"),
    LoadLane::Offscreen,
  );
  let (second, second_result) = pending(
    &adapter,
    &client,
    origin.image("second"),
    LoadLane::Offscreen,
  );
  let (visible, visible_result) =
    pending(&adapter, &client, origin.image("first"), LoadLane::Visible);
  // Once its only visible demand leaves, first is offscreen again. The live
  // priority update on second must win without restarting either fetch.
  drop(visible);
  drop(visible_result);
  second.set_lane(LoadLane::Visible);
  assert_eq!(adapter.lock_state().scheduler.active_loads(), 1);
  respond(active_socket).await;
  settled(active_result)
    .await
    .0
    .expect("active image succeeds");
  let (second_socket, head) = origin.request().await;
  assert!(
    head.contains("/Items/second/Images/Primary?"),
    "live promoted work runs first: {head}"
  );
  assert_eq!(adapter.lock_state().scheduler.active_loads(), 1);
  respond(second_socket).await;
  settled(second_result)
    .await
    .0
    .expect("promoted image succeeds");
  let (first_socket, head) = origin.request().await;
  assert!(head.contains("/Items/first/Images/Primary?"));
  respond(first_socket).await;
  settled(first_result)
    .await
    .0
    .expect("demoted follower still succeeds");
}

#[tokio::test]
async fn excess_live_demand_waits_without_overload_and_last_drop_reclaims_admission() {
  let origin = Origin::new().await;
  let client = origin.client();
  let adapter = adapter(1);
  let (_active, active_result) =
    pending(&adapter, &client, origin.image("active"), LoadLane::Visible);
  let (socket, _) = origin.request().await;
  let (queued, queued_result) = pending(
    &adapter,
    &client,
    origin.image("queued"),
    LoadLane::Offscreen,
  );
  let (joined, joined_result) =
    pending(&adapter, &client, origin.image("queued"), LoadLane::Visible);
  let (_next, next_result) = pending(&adapter, &client, origin.image("next"), LoadLane::Visible);
  assert_eq!(adapter.lock_state().scheduler.active_loads(), 1);
  drop(queued);
  drop(queued_result);
  drop(joined);
  drop(joined_result);
  respond(socket).await;
  settled(active_result).await.0.expect("active completes");
  let (socket, head) = origin.request().await;
  assert!(
    head.contains("/Items/next/Images/Primary?"),
    "revoked queued image never fetches"
  );
  respond(socket).await;
  settled(next_result)
    .await
    .0
    .expect("freed queue admits next");
}

#[test]
fn ready_cache_hit_is_synchronous_and_authorized_without_runtime() {
  let client = Arc::new(JellyfinClient::new());
  let adapter = adapter(1);
  let server = "https://first.example.com";
  let image = super::tests::image_id(server);
  super::tests::adopt_session(&client, server, "first");
  adapter.seed_raster_for_test(&image, ArtworkSizeClass::Card, super::tests::raster(2, 3));
  assert!(matches!(
    adapter.demand(
      Arc::clone(&client),
      image.clone(),
      ArtworkSizeClass::Card,
      DerivedArtwork::default(),
      adapter.ticket(),
      LoadLane::Visible,
    ),
    ArtworkDemand::Ready((
      Ok(_),
      ArtworkLoadObservation {
        settlement: ArtworkLoadSettlement::Loaded(ArtworkSource::Raster),
        ..
      }
    ))
  ));
  super::tests::adopt_session(&client, "https://second.example.com", "second");
  assert!(matches!(
    adapter.demand(
      client,
      image,
      ArtworkSizeClass::Card,
      DerivedArtwork::default(),
      adapter.ticket(),
      LoadLane::Visible,
    ),
    ArtworkDemand::Ready((Err(ArtworkError::RequestRejected), _))
  ));
}

#[tokio::test]
async fn session_reset_cancels_waiting_and_active_demands_and_rejects_old_ticket() {
  let origin = Origin::new().await;
  let client = origin.client();
  let adapter = adapter(1);
  let old_ticket = adapter.ticket();
  let (active, active_result) =
    pending(&adapter, &client, origin.image("active"), LoadLane::Visible);
  let (mut socket, _) = origin.request().await;
  let (waiting, waiting_result) = pending(
    &adapter,
    &client,
    origin.image("waiting"),
    LoadLane::Visible,
  );
  adapter.reset_session();
  assert!(matches!(
    settled(active_result).await.0,
    Err(ArtworkError::Cancelled)
  ));
  assert!(matches!(
    settled(waiting_result).await.0,
    Err(ArtworkError::Cancelled)
  ));
  assert!(matches!(
    adapter.demand(
      Arc::clone(&client),
      origin.image("active"),
      ArtworkSizeClass::Card,
      DerivedArtwork::default(),
      old_ticket,
      LoadLane::Visible,
    ),
    ArtworkDemand::Ready((Err(ArtworkError::Cancelled), _))
  ));
  let closed = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut [0; 1]))
    .await
    .expect("reset closes old request")
    .expect("read EOF");
  assert_eq!(closed, 0);
  let (_replacement, result) =
    pending(&adapter, &client, origin.image("active"), LoadLane::Visible);
  drop(active);
  drop(waiting);
  let (socket, _) = origin.request().await;
  respond(socket).await;
  settled(result)
    .await
    .0
    .expect("old controls cannot revoke new session demand");
}

#[tokio::test]
async fn clearing_caches_preserves_live_work_and_its_result_reenters_cache() {
  let origin = Origin::new().await;
  let client = origin.client();
  let adapter = adapter(1);
  let image = origin.image("live");
  let (_control, result) = pending(&adapter, &client, image.clone(), LoadLane::Visible);
  let (socket, _) = origin.request().await;
  adapter.clear_caches();
  respond(socket).await;
  settled(result)
    .await
    .0
    .expect("cache clear does not cancel demand");
  assert!(matches!(
    adapter.demand(
      client,
      image,
      ArtworkSizeClass::Card,
      DerivedArtwork::default(),
      adapter.ticket(),
      LoadLane::Visible,
    ),
    ArtworkDemand::Ready((Ok(_), _))
  ));
}
