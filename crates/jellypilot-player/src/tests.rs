use super::*;
use iced::futures::StreamExt;
use std::pin::Pin;

async fn wait_for(
  player: &mut Player,
  notifications: &mut Pin<Box<impl Stream<Item = ()>>>,
  predicate: impl Fn(&Status) -> bool,
) {
  tokio::time::timeout(Duration::from_secs(15), async {
    loop {
      player.refresh().expect("refresh player");
      if predicate(player.status()) {
        break;
      }
      notifications
        .next()
        .await
        .expect("worker notification stream remains live");
    }
  })
  .await
  .unwrap_or_else(|_| panic!("player transition timed out: {:?}", player.status()));
}

async fn close(player: &mut Player) {
  let task = player.close();
  assert!(
    player.open("ignored.mp4").is_err(),
    "closed player rejects opens"
  );
  assert!(
    player.set_muted(true).is_err(),
    "closed player rejects controls"
  );
  let mut completion = iced_runtime::task::into_stream(task).expect("first close owns completion");
  let action = tokio::time::timeout(Duration::from_secs(15), completion.next())
    .await
    .expect("worker shutdown deadline")
    .expect("shutdown result");
  match action {
    iced_runtime::Action::Output(result) => result.expect("worker reached Null and joined"),
    _ => panic!("unexpected shutdown action"),
  }
  assert!(
    iced_runtime::task::into_stream(player.close()).is_none(),
    "repeated close cannot complete ahead of the original task"
  );
}

#[test]
fn replay_and_seek_without_refresh_preserve_latest_transport_intent() {
  let (_directory, path) = playback::tests::fixture();
  tokio::runtime::Builder::new_current_thread()
    .enable_time()
    .build()
    .unwrap()
    .block_on(async {
      let (mut player, notifications) = Player::new(AudioOutput::Discard).unwrap();
      let mut notifications = Box::pin(notifications);
      player.open(path).unwrap();
      wait_for(&mut player, &mut notifications, |s| {
        s.phase == PlaybackPhase::Ended
      })
      .await;

      // Replay reserves a seek identity even when no new snapshot has reached the UI.
      player.set_playing(true).unwrap();
      player.seek(Duration::from_millis(1200)).unwrap();
      player.set_playing(false).unwrap();
      wait_for(&mut player, &mut notifications, |s| {
        s.phase == PlaybackPhase::Paused
          && !s.seeking
          && s
            .position
            .is_some_and(|p| p.abs_diff(Duration::from_millis(1200)) < Duration::from_millis(50))
      })
      .await;
      assert!(
        !player.status().playing,
        "last pause intent wins over replay"
      );
      assert!(player.status().error.is_none());

      player.seek(Duration::from_millis(400)).unwrap();
      wait_for(&mut player, &mut notifications, |s| {
        !s.seeking
          && s
            .position
            .is_some_and(|p| p.abs_diff(Duration::from_millis(400)) < Duration::from_millis(50))
      })
      .await;
      assert_eq!(player.status().phase, PlaybackPhase::Paused);
      close(&mut player).await;
    });
}

#[test]
fn invalid_replacement_preserves_media_and_valid_replacement_recovers() {
  let (directory, path) = playback::tests::fixture();
  tokio::runtime::Builder::new_current_thread()
    .enable_time()
    .build()
    .unwrap()
    .block_on(async {
      let (mut player, notifications) = Player::new(AudioOutput::Discard).unwrap();
      let mut notifications = Box::pin(notifications);
      player.open(path.clone()).unwrap();
      wait_for(&mut player, &mut notifications, |s| s.can_seek()).await;
      player.set_playing(false).unwrap();
      wait_for(&mut player, &mut notifications, |s| {
        s.phase == PlaybackPhase::Paused
      })
      .await;
      let duration = player.status().duration;
      player.open(directory.path().join("missing.mp4")).unwrap();
      wait_for(&mut player, &mut notifications, |s| {
        !s.opening && s.error.is_some()
      })
      .await;
      assert!(matches!(
        player.status().error,
        Some(PlaybackError::Open { .. })
      ));
      assert_eq!(player.status().phase, PlaybackPhase::Paused);
      assert_eq!(player.status().duration, duration);
      assert!(player.status().can_seek());

      player.open(path.clone()).unwrap();
      wait_for(&mut player, &mut notifications, |s| {
        !s.opening && s.phase == PlaybackPhase::Playing
      })
      .await;
      assert!(player.status().error.is_none());
      assert!(player.status().playing);
      let corrupt = directory.path().join("not-video.mp4");
      std::fs::write(&corrupt, b"not a media container").unwrap();
      player.open(corrupt).unwrap();
      wait_for(&mut player, &mut notifications, |s| {
        s.phase == PlaybackPhase::Error
      })
      .await;
      assert!(player.status().error.is_some());
      player.open(path).unwrap();
      player.refresh().unwrap();
      wait_for(&mut player, &mut notifications, |s| {
        !s.opening && s.phase == PlaybackPhase::Playing
      })
      .await;
      assert!(
        player.status().error.is_none(),
        "accepted recovery does not retain an earlier pipeline error"
      );
      close(&mut player).await;
    });
}
