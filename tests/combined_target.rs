//! Coverage for combined-target ordering (issue #72).
//!
//! The order is the safety property: it is what stops a metadata failure from
//! costing the user files.

use std::cell::Cell;

use rom_manager::{
    COMBINED_ORDER, CombinedOutcome, MetadataAction, MetadataPreview, MetadataPreviewRow,
    RoleAssignment, SplitReadiness, SyncStage, run_combined, split_readiness,
};

#[test]
fn the_order_is_content_then_metadata_then_removals() {
    assert_eq!(
        COMBINED_ORDER,
        [
            SyncStage::AddAndVerifyContent,
            SyncStage::PublishAndRereadMetadata,
            SyncStage::RemoveManagedContent
        ]
    );
}

#[test]
fn a_clean_run_performs_removals_last() {
    let removed = Cell::new(0usize);

    let outcome = run_combined(3, || Ok(()), || Ok(()), |count| removed.set(count));

    assert_eq!(outcome, CombinedOutcome::Completed);
    assert_eq!(removed.get(), 3);
}

#[test]
fn a_metadata_failure_skips_every_removal() {
    // Losing files *and* failing to update metadata is the worst available
    // outcome, and it is entirely avoidable.
    let removed = Cell::new(0usize);

    let outcome = run_combined(
        3,
        || Ok(()),
        || Err("device disconnected during publish".into()),
        |count| removed.set(count),
    );

    assert_eq!(removed.get(), 0, "no removal may run after metadata failed");
    assert!(matches!(
        outcome,
        CombinedOutcome::ContentSyncedMetadataPending {
            removals_skipped: 3,
            ..
        }
    ));
}

#[test]
fn a_metadata_failure_never_rolls_back_verified_content() {
    let outcome = run_combined(
        1,
        || Ok(()),
        || Err("publish failed".into()),
        |_| panic!("removals must not run"),
    );

    assert!(outcome.content_retained());
    assert_eq!(outcome.summary(), "ROM content synced; metadata pending.");
}

#[test]
fn a_content_failure_never_attempts_metadata() {
    // A gamelist describing content that did not land would be wrong the moment
    // it was written.
    let metadata_ran = Cell::new(false);

    let outcome = run_combined(
        2,
        || Err("out of space".into()),
        || {
            metadata_ran.set(true);
            Ok(())
        },
        |_| panic!("removals must not run"),
    );

    assert!(!metadata_ran.get());
    assert!(matches!(outcome, CombinedOutcome::ContentFailed { .. }));
    assert!(!outcome.content_retained());
}

#[test]
fn a_combined_target_runs_as_one_ordered_plan() {
    let assignment = RoleAssignment::Combined {
        target_id: "target-1".into(),
    };

    assert_eq!(split_readiness(&assignment, true), SplitReadiness::Combined);
}

#[test]
fn split_targets_require_the_content_target_to_converge_first() {
    let assignment = RoleAssignment::Split {
        rom_content: "sd-card".into(),
        frontend_metadata: "internal".into(),
        confirmed: true,
    };

    assert_eq!(
        split_readiness(&assignment, false),
        SplitReadiness::Blocked("the ROM-content target is not current"),
        "metadata naming files that have not converged would name files that are not there"
    );

    assert_eq!(
        split_readiness(&assignment, true),
        SplitReadiness::ContentFirst {
            rom_content: "sd-card".into(),
            frontend_metadata: "internal".into()
        }
    );
}

#[test]
fn an_unconfirmed_pairing_blocks_planning_entirely() {
    let assignment = RoleAssignment::Split {
        rom_content: "sd-card".into(),
        frontend_metadata: "internal".into(),
        confirmed: false,
    };

    assert_eq!(
        split_readiness(&assignment, true),
        SplitReadiness::Blocked("the Destination Role pairing is not confirmed")
    );
}

#[test]
fn the_preview_reports_actions_by_system_path_and_field() {
    let preview = MetadataPreview {
        rows: vec![
            MetadataPreviewRow {
                system_key: "nes".into(),
                entry_path: "./Tracers.nes".into(),
                field: Some("name".into()),
                action: MetadataAction::Update,
            },
            MetadataPreviewRow {
                system_key: "nes".into(),
                entry_path: "./Other.nes".into(),
                field: Some("genre".into()),
                action: MetadataAction::Conflict,
            },
            MetadataPreviewRow {
                system_key: "nes".into(),
                entry_path: "./Third.nes".into(),
                field: None,
                action: MetadataAction::PreservedSharedState,
            },
        ],
        atomic_publication: false,
    };

    assert_eq!(preview.count(MetadataAction::Update), 1);
    assert_eq!(preview.count(MetadataAction::Conflict), 1);
    assert_eq!(preview.count(MetadataAction::PreservedSharedState), 1);
    assert!(
        preview.requires_decision(),
        "a conflict must be decided before publication"
    );
    assert!(
        !preview.atomic_publication,
        "the transport's inability to replace atomically is disclosed, not hidden"
    );
}

#[test]
fn a_preview_with_no_conflicts_or_adoptions_needs_no_decision() {
    let preview = MetadataPreview {
        rows: vec![MetadataPreviewRow {
            system_key: "nes".into(),
            entry_path: "./Tracers.nes".into(),
            field: Some("name".into()),
            action: MetadataAction::Add,
        }],
        atomic_publication: true,
    };

    assert!(!preview.requires_decision());
}
