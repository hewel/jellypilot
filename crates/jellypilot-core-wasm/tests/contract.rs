use jellypilot_core_wasm::{
    LibraryBrowseCacheMode, LibraryBrowseCommand, LibraryBrowseCore, LibraryBrowseEvent,
    LibraryBrowseFailure, LibraryBrowseLoadPriority, LibraryBrowseLoadToken, LibraryBrowseMode,
    LibraryBrowsePageOutcome, LibraryBrowseStatus,
};
use serde_json::json;

#[test]
fn wrapper_constructor_exposes_inactive_snapshot() {
    let core = LibraryBrowseCore::new();

    assert_eq!(core.snapshot().status, LibraryBrowseStatus::Inactive);
}

#[test]
fn wrapper_dispatch_returns_direct_typed_commands() {
    let mut core = LibraryBrowseCore::new();

    let update = core
        .dispatch(LibraryBrowseEvent::Configure {
            source_id: "movies:title:asc".to_owned(),
            enabled: true,
        })
        .expect("configuration should dispatch");

    assert!(matches!(
        update.commands.as_slice(),
        [
            LibraryBrowseCommand::ResetViewport,
            LibraryBrowseCommand::LoadPage {
                start_index: 0,
                limit: 24,
                priority: LibraryBrowseLoadPriority::Bootstrap,
                cache_mode: LibraryBrowseCacheMode::ReuseSuccess,
                ..
            },
        ]
    ));
}

#[test]
fn event_contract_uses_tagged_camel_case_fields() {
    let event = LibraryBrowseEvent::PageSettled {
        token: LibraryBrowseLoadToken {
            generation: 4,
            sequence: 9,
        },
        outcome: LibraryBrowsePageOutcome::Loaded {
            start_index: 24,
            limit: 24,
            total_record_count: 80,
            item_count: 20,
            has_more: true,
        },
    };

    let value = serde_json::to_value(event).expect("event should serialize");

    assert_eq!(
        value,
        json!({
            "tag": "pageSettled",
            "token": { "generation": 4, "sequence": 9 },
            "outcome": {
                "tag": "loaded",
                "startIndex": 24,
                "limit": 24,
                "totalRecordCount": 80,
                "itemCount": 20,
                "hasMore": true
            }
        })
    );
}

#[test]
fn command_contract_uses_string_priority_and_cache_mode() {
    let command = LibraryBrowseCommand::LoadPage {
        token: LibraryBrowseLoadToken {
            generation: 2,
            sequence: 3,
        },
        start_index: 48,
        limit: 24,
        priority: LibraryBrowseLoadPriority::Visible,
        cache_mode: LibraryBrowseCacheMode::ReuseSuccess,
    };

    let value = serde_json::to_value(command).expect("command should serialize");

    assert_eq!(
        value,
        json!({
            "tag": "loadPage",
            "token": { "generation": 2, "sequence": 3 },
            "startIndex": 48,
            "limit": 24,
            "priority": "visible",
            "cacheMode": "reuseSuccess"
        })
    );
}

#[test]
fn status_contract_uses_tagged_payload_and_nullable_failure() {
    let status = LibraryBrowseStatus::Ready {
        mode: LibraryBrowseMode::Virtual,
        total_record_count: 200,
        is_fetching_more: false,
        can_load_next: false,
        load_more_failure: None,
        retry_busy: false,
    };

    let value = serde_json::to_value(status).expect("status should serialize");

    assert_eq!(
        value,
        json!({
            "tag": "ready",
            "mode": "virtual",
            "totalRecordCount": 200,
            "isFetchingMore": false,
            "canLoadNext": false,
            "loadMoreFailure": null,
            "retryBusy": false
        })
    );
}

#[test]
fn failure_outcome_deserializes_from_direct_typescript_shape() {
    let value = json!({
        "tag": "failed",
        "failure": { "message": "offline", "retryable": true }
    });

    let outcome: LibraryBrowsePageOutcome =
        serde_json::from_value(value).expect("failure should deserialize");

    assert_eq!(
        outcome,
        LibraryBrowsePageOutcome::Failed {
            failure: LibraryBrowseFailure {
                message: "offline".to_owned(),
                retryable: true,
            },
        }
    );
}

#[test]
fn malformed_event_conversion_does_not_prevent_a_later_typed_dispatch() {
    let malformed = serde_json::from_value::<LibraryBrowseEvent>(json!({
        "tag": "configure",
        "enabled": true
    }));
    assert!(malformed.is_err());

    let mut core = LibraryBrowseCore::new();
    let update = core
        .dispatch(LibraryBrowseEvent::Configure {
            source_id: "movies:title:asc".to_owned(),
            enabled: true,
        })
        .expect("a valid event should still dispatch after conversion rejects invalid input");

    assert_eq!(update.snapshot.status, LibraryBrowseStatus::Loading);
}
