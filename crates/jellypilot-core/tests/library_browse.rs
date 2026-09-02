use jellypilot_core::{
    LibraryBrowseAction, LibraryBrowseCacheMode, LibraryBrowseCommand, LibraryBrowseCore,
    LibraryBrowseCoreError, LibraryBrowseFailure, LibraryBrowseLoadPriority,
    LibraryBrowseLoadToken, LibraryBrowseMode, LibraryBrowsePageOutcome, LibraryBrowseSlot,
    LibraryBrowseStatus,
};

fn configure(core: &mut LibraryBrowseCore, source_id: &str) -> LibraryBrowseLoadToken {
    let update = core
        .dispatch(LibraryBrowseAction::Configure {
            source_id: source_id.to_owned(),
            enabled: true,
        })
        .expect("configuration should be valid");
    match update.commands.as_slice() {
        [LibraryBrowseCommand::ResetViewport, LibraryBrowseCommand::LoadPage { token, .. }] => {
            *token
        }
        commands => panic!("expected reset and bootstrap load, found {commands:?}"),
    }
}

fn loaded(
    core: &mut LibraryBrowseCore,
    token: LibraryBrowseLoadToken,
    start_index: u32,
    total_record_count: u32,
    item_count: u32,
    has_more: bool,
) -> jellypilot_core::LibraryBrowseUpdate {
    core.dispatch(LibraryBrowseAction::PageSettled {
        token,
        outcome: LibraryBrowsePageOutcome::Loaded {
            start_index,
            limit: 24,
            total_record_count,
            item_count,
            has_more,
        },
    })
    .expect("loaded metadata should dispatch")
}

fn failed(
    core: &mut LibraryBrowseCore,
    token: LibraryBrowseLoadToken,
    message: &str,
    retryable: bool,
) -> jellypilot_core::LibraryBrowseUpdate {
    core.dispatch(LibraryBrowseAction::PageSettled {
        token,
        outcome: LibraryBrowsePageOutcome::Failed {
            failure: LibraryBrowseFailure {
                message: message.to_owned(),
                retryable,
            },
        },
    })
    .expect("failure metadata should dispatch")
}

fn load_tokens(update: &jellypilot_core::LibraryBrowseUpdate) -> Vec<LibraryBrowseLoadToken> {
    update
        .commands
        .iter()
        .filter_map(|command| match command {
            LibraryBrowseCommand::LoadPage { token, .. } => Some(*token),
            LibraryBrowseCommand::ResetViewport
            | LibraryBrowseCommand::CancelLoad { .. }
            | LibraryBrowseCommand::ReleasePages { .. } => None,
        })
        .collect()
}

#[test]
fn new_core_is_inactive() {
    let snapshot = LibraryBrowseCore::new().snapshot();

    assert_eq!(snapshot.status, LibraryBrowseStatus::Inactive);
}

#[test]
fn configure_resets_viewport_before_bootstrap_load() {
    let mut core = LibraryBrowseCore::new();

    let update = core
        .dispatch(LibraryBrowseAction::Configure {
            source_id: "movies:title:asc".to_owned(),
            enabled: true,
        })
        .expect("configuration should be valid");

    assert_eq!(
        update.commands,
        vec![
            LibraryBrowseCommand::ResetViewport,
            LibraryBrowseCommand::LoadPage {
                token: LibraryBrowseLoadToken {
                    generation: 1,
                    sequence: 1,
                },
                start_index: 0,
                limit: 24,
                priority: LibraryBrowseLoadPriority::Bootstrap,
                cache_mode: LibraryBrowseCacheMode::ReuseSuccess,
            },
        ]
    );
}

#[test]
fn configure_with_unchanged_values_is_idempotent() {
    let mut core = LibraryBrowseCore::new();
    configure(&mut core, "movies");

    let update = core
        .dispatch(LibraryBrowseAction::Configure {
            source_id: "movies".to_owned(),
            enabled: true,
        })
        .expect("configuration should stay valid");

    assert!(update.commands.is_empty());
}

#[test]
fn enabled_empty_source_is_rejected_without_mutating_state() {
    let mut core = LibraryBrowseCore::new();
    let before = core.snapshot();

    let error = core
        .dispatch(LibraryBrowseAction::Configure {
            source_id: "   ".to_owned(),
            enabled: true,
        })
        .expect_err("empty enabled source should fail");

    assert_eq!(
        (error, core.snapshot()),
        (LibraryBrowseCoreError::EmptySourceId, before)
    );
}

#[test]
fn source_change_cancels_releases_resets_then_bootstraps() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    loaded(&mut core, page_zero, 0, 200, 24, true);
    let window = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![48, 72],
        })
        .expect("window should dispatch");
    let pending = load_tokens(&window);

    let update = core
        .dispatch(LibraryBrowseAction::Configure {
            source_id: "shows".to_owned(),
            enabled: true,
        })
        .expect("new source should configure");

    assert_eq!(
        update.commands,
        vec![
            LibraryBrowseCommand::CancelLoad { token: pending[0] },
            LibraryBrowseCommand::CancelLoad { token: pending[1] },
            LibraryBrowseCommand::ReleasePages {
                page_starts: vec![0],
            },
            LibraryBrowseCommand::ResetViewport,
            LibraryBrowseCommand::LoadPage {
                token: LibraryBrowseLoadToken {
                    generation: 2,
                    sequence: 1,
                },
                start_index: 0,
                limit: 24,
                priority: LibraryBrowseLoadPriority::Bootstrap,
                cache_mode: LibraryBrowseCacheMode::ReuseSuccess,
            },
        ]
    );
}

#[test]
fn disabling_same_source_cancels_and_releases_without_reset() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    loaded(&mut core, page_zero, 0, 80, 24, true);
    let next = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: (24..48).collect(),
        })
        .expect("window changed should dispatch");
    let tokens = load_tokens(&next);

    let update = core
        .dispatch(LibraryBrowseAction::Configure {
            source_id: "movies".to_owned(),
            enabled: false,
        })
        .expect("disabling should dispatch");

    assert_eq!(
        update.commands,
        vec![
            LibraryBrowseCommand::CancelLoad { token: tokens[0] },
            LibraryBrowseCommand::CancelLoad { token: tokens[1] },
            LibraryBrowseCommand::ReleasePages {
                page_starts: vec![0],
            },
        ]
    );
}
#[test]
fn virtual_window_waits_for_successful_page_zero() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    let before = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![72, 73],
        })
        .expect("window should dispatch");

    let after = loaded(&mut core, page_zero, 0, 200, 24, true);

    assert_eq!((before.commands.len(), load_tokens(&after).len()), (0, 2));
}

#[test]
fn non_empty_count_uses_virtual_mode() {
    let mut core = LibraryBrowseCore::new();
    let token = configure(&mut core, "movies");

    let update = loaded(&mut core, token, 0, 100, 24, true);

    assert!(matches!(
        update.snapshot.status,
        LibraryBrowseStatus::Ready {
            mode: LibraryBrowseMode::Virtual,
            ..
        }
    ));
}

#[test]
fn zero_record_page_is_empty() {
    let mut core = LibraryBrowseCore::new();
    let token = configure(&mut core, "movies");

    let update = loaded(&mut core, token, 0, 0, 0, false);

    assert_eq!(
        update.snapshot.status,
        LibraryBrowseStatus::Empty {
            total_record_count: 0,
        }
    );
}

#[test]
fn filtered_empty_page_with_remaining_records_stays_ready_for_continuation() {
    let mut core = LibraryBrowseCore::new();
    let token = configure(&mut core, "movies");

    let update = loaded(&mut core, token, 0, 25, 0, true);

    assert!(matches!(
        update.snapshot.status,
        LibraryBrowseStatus::Ready {
            mode: LibraryBrowseMode::Virtual,
            total_record_count: 25,
            ..
        }
    ));
    let continuation = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![24],
        })
        .expect("the UI should be able to continue after a filtered empty page");
    assert!(matches!(
        continuation.commands.as_slice(),
        [LibraryBrowseCommand::LoadPage {
            start_index: 24,
            ..
        }]
    ));
}

#[test]
fn terminal_page_settles_and_updates_ready_status() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    loaded(&mut core, page_zero, 0, 49, 24, true);
    let next_24 = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: (24..48).collect(),
        })
        .expect("page 24 and 48 should schedule");
    let tokens = load_tokens(&next_24);
    loaded(&mut core, tokens[0], 24, 49, 24, true);
    let update = loaded(&mut core, tokens[1], 48, 49, 1, false);

    assert!(matches!(
        update.snapshot.status,
        LibraryBrowseStatus::Ready {
            total_record_count: 49,
            ..
        }
    ));
}

#[test]
fn short_provider_page_is_valid_when_bounds_and_has_more_match() {
    let mut core = LibraryBrowseCore::new();
    let token = configure(&mut core, "movies");

    let update = loaded(&mut core, token, 0, 80, 20, true);

    assert!(matches!(
        update.snapshot.status,
        LibraryBrowseStatus::Ready { .. }
    ));
}

#[test]
fn inconsistent_loaded_metadata_becomes_nonretryable_initial_failure() {
    let mut core = LibraryBrowseCore::new();
    let token = configure(&mut core, "movies");

    let update = loaded(&mut core, token, 0, 80, 20, false);

    assert!(matches!(
        update.snapshot.status,
        LibraryBrowseStatus::InitialFailure {
            failure: LibraryBrowseFailure {
                retryable: false,
                ..
            },
            retry_busy: false,
        }
    ));
}

#[test]
fn malformed_loaded_metadata_releases_the_external_page_payload() {
    let mut core = LibraryBrowseCore::new();
    let token = configure(&mut core, "movies");

    let update = loaded(&mut core, token, 0, 80, 20, false);

    assert_eq!(
        update.commands,
        vec![LibraryBrowseCommand::ReleasePages {
            page_starts: vec![0],
        }]
    );
}

#[test]
fn wrong_page_metadata_becomes_nonretryable_initial_failure() {
    let mut core = LibraryBrowseCore::new();
    let token = configure(&mut core, "movies");

    let update = loaded(&mut core, token, 24, 80, 24, true);

    assert!(matches!(
        update.snapshot.status,
        LibraryBrowseStatus::InitialFailure {
            failure: LibraryBrowseFailure {
                retryable: false,
                ..
            },
            ..
        }
    ));
}

#[test]
fn stale_settlement_from_old_generation_is_ignored() {
    let mut core = LibraryBrowseCore::new();
    let stale = configure(&mut core, "movies");
    let current = core
        .dispatch(LibraryBrowseAction::Configure {
            source_id: "shows".to_owned(),
            enabled: true,
        })
        .expect("new source should configure");
    let current_token = load_tokens(&current)[0];

    let update = loaded(&mut core, stale, 0, 20, 20, false);

    assert_eq!(
        (update.snapshot.pending_count, load_tokens(&update)),
        (1, Vec::new())
    );
    assert_ne!(stale, current_token);
}

#[test]
fn window_change_loads_the_requested_page() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    loaded(&mut core, page_zero, 0, 80, 24, true);

    let update = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: (24..48).collect(),
        })
        .expect("window change should dispatch");

    assert!(matches!(
        update.commands.as_slice(),
        [
            LibraryBrowseCommand::LoadPage {
                start_index: 24,
                priority: LibraryBrowseLoadPriority::Visible,
                ..
            },
            LibraryBrowseCommand::LoadPage {
                start_index: 48,
                priority: LibraryBrowseLoadPriority::Prefetch,
                ..
            },
        ]
    ));
}

#[test]
fn virtual_slots_map_window_indexes_to_stored_pages() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    loaded(&mut core, page_zero, 0, 30, 24, true);
    let next = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![24, 25],
        })
        .expect("window change should dispatch");
    let update = loaded(&mut core, load_tokens(&next)[0], 24, 30, 6, false);

    assert_eq!(
        (&update.snapshot.slots[0], &update.snapshot.slots[1]),
        (
            &LibraryBrowseSlot {
                display_index: 24,
                page_start: 24,
                index_within_page: 0,
            },
            &LibraryBrowseSlot {
                display_index: 25,
                page_start: 24,
                index_within_page: 1,
            },
        )
    );
}

#[test]
fn initial_retry_uses_reload_and_reports_busy() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    failed(&mut core, page_zero, "offline", true);

    let update = core
        .dispatch(LibraryBrowseAction::Retry)
        .expect("retry should dispatch");

    assert!(matches!(
        (update.commands.as_slice(), update.snapshot.status),
        (
            [LibraryBrowseCommand::LoadPage {
                start_index: 0,
                priority: LibraryBrowseLoadPriority::Retry,
                cache_mode: LibraryBrowseCacheMode::Reload,
                ..
            }],
            LibraryBrowseStatus::InitialFailure {
                retry_busy: true,
                ..
            }
        )
    ));
}

#[test]
fn nonretryable_initial_failure_ignores_retry() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    failed(&mut core, page_zero, "bad response", false);

    let update = core
        .dispatch(LibraryBrowseAction::Retry)
        .expect("retry event itself should be valid");

    assert!(update.commands.is_empty());
}

#[test]
fn virtual_window_prioritizes_visible_pages_before_one_lookahead() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    loaded(&mut core, page_zero, 0, 300, 24, true);

    let update = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![48, 49, 72],
        })
        .expect("window should dispatch");

    assert!(matches!(
        update.commands.as_slice(),
        [
            LibraryBrowseCommand::LoadPage {
                start_index: 48,
                priority: LibraryBrowseLoadPriority::Visible,
                ..
            },
            LibraryBrowseCommand::LoadPage {
                start_index: 72,
                priority: LibraryBrowseLoadPriority::Visible,
                ..
            },
        ]
    ));
}

#[test]
fn virtual_scheduler_fills_prefetch_after_a_visible_page_settles() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    loaded(&mut core, page_zero, 0, 300, 24, true);
    let window = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![48, 72],
        })
        .expect("window should dispatch");

    let update = loaded(&mut core, load_tokens(&window)[0], 48, 300, 24, true);

    assert!(matches!(
        update.commands.as_slice(),
        [LibraryBrowseCommand::LoadPage {
            start_index: 96,
            priority: LibraryBrowseLoadPriority::Prefetch,
            ..
        }]
    ));
}

#[test]
fn virtual_slots_preserve_unique_display_encounter_order() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    loaded(&mut core, page_zero, 0, 300, 24, true);

    let update = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![49, 48, 49, 999],
        })
        .expect("window should dispatch");

    assert_eq!(
        update.snapshot.slots,
        vec![
            LibraryBrowseSlot {
                display_index: 49,
                page_start: 48,
                index_within_page: 1,
            },
            LibraryBrowseSlot {
                display_index: 48,
                page_start: 48,
                index_within_page: 0,
            },
        ]
    );
}

#[test]
fn window_churn_keeps_real_inflight_loads_within_concurrency_limit() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    loaded(&mut core, page_zero, 0, 300, 24, true);
    let first_window = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![48, 72],
        })
        .expect("first window should dispatch");

    let second_window = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![192],
        })
        .expect("second window should dispatch");

    assert_eq!(
        (second_window.commands, second_window.snapshot.pending_count),
        (Vec::new(), 2)
    );

    let settled = loaded(&mut core, load_tokens(&first_window)[0], 48, 300, 24, true);
    assert!(matches!(
        settled.commands.as_slice(),
        [
            LibraryBrowseCommand::ReleasePages { page_starts },
            LibraryBrowseCommand::LoadPage {
                start_index: 192,
                ..
            },
        ] if page_starts == &[48]
    ));
}

#[test]
fn virtual_window_releases_stored_pages_but_always_retains_page_zero() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    loaded(&mut core, page_zero, 0, 300, 24, true);
    let first_window = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![48],
        })
        .expect("first window should dispatch");
    loaded(&mut core, load_tokens(&first_window)[0], 48, 300, 24, true);

    let update = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![144],
        })
        .expect("later window should dispatch");

    assert!(matches!(
        update.commands.first(),
        Some(LibraryBrowseCommand::ReleasePages { page_starts }) if page_starts == &[48]
    ));
}

#[test]
fn retained_virtual_failure_retries_with_reload_priority() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    loaded(&mut core, page_zero, 0, 300, 24, true);
    let window = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![48],
        })
        .expect("window should dispatch");
    failed(&mut core, load_tokens(&window)[0], "offline", true);

    let update = core
        .dispatch(LibraryBrowseAction::Retry)
        .expect("retry should dispatch");

    assert!(update.commands.iter().any(|command| matches!(
        command,
        LibraryBrowseCommand::LoadPage {
            start_index: 48,
            priority: LibraryBrowseLoadPriority::Retry,
            cache_mode: LibraryBrowseCacheMode::Reload,
            ..
        }
    )));
}

#[test]
fn failed_virtual_continuation_is_exposed_in_ready_status() {
    let mut core = LibraryBrowseCore::new();
    let page_zero = configure(&mut core, "movies");
    loaded(&mut core, page_zero, 0, 80, 24, true);
    let next = core
        .dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: vec![24],
        })
        .expect("window change should dispatch");

    let update = failed(&mut core, load_tokens(&next)[0], "offline", true);

    assert!(matches!(
        update.snapshot.status,
        LibraryBrowseStatus::Ready {
            load_more_failure: Some(LibraryBrowseFailure { ref message, .. }),
            ..
        } if message == "offline"
    ));
}
