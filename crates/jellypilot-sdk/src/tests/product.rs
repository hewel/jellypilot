use super::*;

#[tokio::test]
async fn watchlist_batch_undo_preserves_original_order_and_rejects_an_ended_scope() {
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let token = sdk.new_operation_token().unwrap();
    for id in ["one", "two", "three"] {
        sdk.watchlist_add(Arc::clone(&token), test_item(id))
            .await
            .unwrap();
    }
    let before = sdk.watchlist_items(Arc::clone(&token)).await.unwrap();
    let removal = sdk
        .remove_watchlist_items(Arc::clone(&token), vec!["one".into(), "three".into()])
        .await
        .unwrap();
    assert_eq!(
        sdk.watchlist_items(Arc::clone(&token)).await.unwrap().len(),
        1
    );
    assert!(removal.undo().await.unwrap());
    let after = sdk.watchlist_items(Arc::clone(&token)).await.unwrap();
    assert_eq!(
        before
            .iter()
            .map(|record| (record.item_id(), record.added_at_unix_millis()))
            .collect::<Vec<_>>(),
        after
            .iter()
            .map(|record| (record.item_id(), record.added_at_unix_millis()))
            .collect::<Vec<_>>()
    );
    assert!(!removal.undo().await.unwrap());
    let stale_removal = sdk
        .remove_watchlist_items(token, vec!["two".into()])
        .await
        .unwrap();
    sdk.adopt_test_session(test_session("grace", "https://media.example.test"));
    assert_eq!(stale_removal.undo().await, Err(SdkError::Stale));
    assert!(sdk
        .watchlist_items(sdk.new_operation_token().unwrap())
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn batch_removal_does_not_partially_apply_when_one_selected_item_is_busy() {
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let token = sdk.new_operation_token().unwrap();
    for id in ["one", "two"] {
        sdk.watchlist_add(Arc::clone(&token), test_item(id))
            .await
            .unwrap();
    }
    let _busy = sdk
        .inner
        .admit_item(&token, "two", Action::Favorite(false))
        .unwrap();
    assert!(matches!(
        sdk.remove_watchlist_items(Arc::clone(&token), vec!["one".into(), "two".into()])
            .await,
        Err(SdkError::OperationInProgress)
    ));
    assert_eq!(
        sdk.watchlist_items(Arc::clone(&token)).await.unwrap().len(),
        2
    );
    assert!(
        sdk.inner
            .admit_item(&token, "one", Action::Favorite(false))
            .is_ok(),
        "failed batch must release earlier admissions"
    );
}

#[test]
fn mobile_preferences_survive_reopening_without_changing_desktop_intro_overrides() {
    let (sdk, dir) = test_sdk();
    let mut desktop =
        jellypilot_core::config::SettingsStore::load_in_dir(dir.path().to_owned()).unwrap();
    desktop
        .set_intro_mode(jellypilot_core::config::IntroMode::Manual)
        .unwrap();
    sdk.set_intro_mode(jellypilot_core::intro_skipper::IntroSkipMode::Off)
        .unwrap();
    sdk.set_auto_play_next(false).unwrap();
    sdk.add_subtitle_language(" PT-BR ".into()).unwrap();
    assert!(matches!(
        sdk.add_subtitle_language("pt-br".into()),
        Err(SdkError::InvalidInput(_))
    ));
    let reopened = Sdk::new(
        SdkConfig {
            storage_dir: dir.path().to_owned(),
            device_name: "reopened".into(),
        },
        Arc::new(MemoryCredential::default()),
        None,
    )
    .unwrap();
    let preferences = reopened.business_preferences().unwrap();
    assert_eq!(
        preferences.intro_mode,
        jellypilot_core::intro_skipper::IntroSkipMode::Off
    );
    assert!(!preferences.auto_play_next);
    assert_eq!(preferences.subtitle_languages, ["pt-br"]);
    let desktop =
        jellypilot_core::config::SettingsStore::load_in_dir(dir.path().to_owned()).unwrap();
    assert_eq!(
        desktop.snapshot().intro_mode(),
        jellypilot_core::config::IntroMode::Manual
    );
}

#[test]
fn subtitle_list_validation_is_atomic_and_prefill_never_persists_credentials() {
    let (sdk, dir) = test_sdk();
    sdk.set_subtitle_languages(vec!["eng".into(), "spa".into()])
        .unwrap();
    assert!(matches!(
        sdk.set_subtitle_languages(vec!["fra".into(), "English (CC)".into()]),
        Err(SdkError::InvalidInput(_))
    ));
    assert_eq!(
        sdk.business_preferences().unwrap().subtitle_languages,
        ["eng", "spa"]
    );
    sdk.save_login_prefill(
        "https://media.example.test".into(),
        "Ada".into(),
        MediaServerProvider::Jellyfin,
        true,
    )
    .unwrap();
    for server in [
        "https://user:secret@media.example.test",
        "https://media.example.test?api_key=secret",
        "https://media.example.test/#secret",
    ] {
        assert!(matches!(
            sdk.save_login_prefill(
                server.into(),
                "Ada".into(),
                MediaServerProvider::Jellyfin,
                true
            ),
            Err(SdkError::InvalidInput(_))
        ));
    }
    let persisted = std::fs::read_to_string(dir.path().join("config.json")).unwrap();
    assert!(!persisted.contains("secret"));
    let cleared = sdk
        .save_login_prefill(
            "bad input".into(),
            "Ada".into(),
            MediaServerProvider::Emby,
            false,
        )
        .unwrap();
    assert!(!cleared.remember);
    assert!(cleared.server_url.is_empty() && cleared.username.is_empty());
}
