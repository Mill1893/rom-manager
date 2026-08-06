//! Gating coverage for durable sync-core state (issue #33).
//!
//! The rules with teeth: a reloaded plan is revalidated by identity rather than
//! trusted, an approval is single-use durably as well as in memory, and an
//! operation interrupted by a crash becomes **indeterminate** on the next start
//! rather than resuming.

mod common;

use common::{TARGET_ID, core_with, expected, fake, manifest_naming, path};
use rom_manager::{
    Approval, DeviceProfile, OperationState, Store, SyncCore, TransportCapabilities,
};

fn store_at(directory: &std::path::Path) -> Store {
    Store::open(&directory.join("library.sqlite3")).expect("the store opens and migrates")
}

/// A sealed plan for the fixture, without touching any durable state.
fn a_plan() -> rom_manager::SyncPlan {
    let mut core: SyncCore<_> = core_with(fake());
    core.refresh().unwrap();
    core.build_plan().unwrap()
}

#[test]
fn migrations_apply_and_are_idempotent() {
    let directory = tempfile::tempdir().unwrap();

    let store = store_at(directory.path());
    assert_eq!(store.schema_version().unwrap(), 1);
    drop(store);

    // Reopening must not reapply a migration that already ran.
    let reopened = store_at(directory.path());
    assert_eq!(reopened.schema_version().unwrap(), 1);
}

#[test]
fn target_identity_bindings_and_manifest_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let manifest = manifest_naming(common::ROM_BYTES);

    {
        let store = store_at(directory.path());
        store.upsert_target(TARGET_ID, 1).unwrap();
        store
            .record_binding(
                TARGET_ID,
                "wpd://odin/storage",
                &TransportCapabilities::filesystem(),
                10,
            )
            .unwrap();
        store.save_manifest(&manifest).unwrap();
    }

    let store = store_at(directory.path());
    assert_eq!(
        store.bindings_for(TARGET_ID).unwrap(),
        vec!["wpd://odin/storage".to_string()]
    );
    assert_eq!(store.load_manifest(TARGET_ID).unwrap(), Some(manifest));
}

#[test]
fn a_relocated_target_keeps_its_identity_and_manifest() {
    // A Media Target is identified by its marker, not by where it happens to be
    // plugged in. Re-binding at a new locator must not disturb what the
    // application knows it manages there.
    let directory = tempfile::tempdir().unwrap();
    let store = store_at(directory.path());
    let manifest = manifest_naming(common::ROM_BYTES);

    store.upsert_target(TARGET_ID, 1).unwrap();
    store.save_manifest(&manifest).unwrap();
    store
        .record_binding(TARGET_ID, "D:/", &TransportCapabilities::filesystem(), 1)
        .unwrap();
    store
        .record_binding(TARGET_ID, "E:/", &TransportCapabilities::filesystem(), 2)
        .unwrap();

    assert_eq!(store.bindings_for(TARGET_ID).unwrap(), vec!["D:/", "E:/"]);
    assert_eq!(store.load_manifest(TARGET_ID).unwrap(), Some(manifest));
}

#[test]
fn a_reloaded_plan_is_revalidated_by_identity() {
    let directory = tempfile::tempdir().unwrap();
    let store = store_at(directory.path());
    let plan = a_plan();

    store.upsert_target(TARGET_ID, 1).unwrap();
    store.save_plan(&plan, 100).unwrap();

    let reloaded = store.load_plan(&plan.digest).unwrap().expect("stored");
    assert_eq!(reloaded, plan);
    assert!(
        reloaded.digest_is_valid(),
        "a reloaded plan must still hash to the digest it is filed under"
    );
    assert!(store.load_plan("0".repeat(64).as_str()).unwrap().is_none());
}

#[test]
fn an_approval_is_single_use_durably() {
    let directory = tempfile::tempdir().unwrap();
    let store = store_at(directory.path());
    let plan = a_plan();

    store.upsert_target(TARGET_ID, 1).unwrap();
    store.save_plan(&plan, 100).unwrap();
    store
        .save_approval(&Approval::grant(&plan, plan.removal_count()), 101)
        .unwrap();

    let taken = store.take_approval(&plan.digest).unwrap();
    assert!(taken.is_some(), "the approval is available exactly once");
    assert!(
        store.take_approval(&plan.digest).unwrap().is_none(),
        "consuming an approval must remove it durably"
    );
}

#[test]
fn an_interrupted_operation_becomes_indeterminate_on_restart() {
    // The process died mid-operation, so what reached the target is unknown.
    // The next start must say so rather than resuming.
    let directory = tempfile::tempdir().unwrap();
    let plan = a_plan();
    let operation;

    {
        let store = store_at(directory.path());
        store.upsert_target(TARGET_ID, 1).unwrap();
        store.save_plan(&plan, 100).unwrap();
        store.record_inventory(TARGET_ID, "abc", 100).unwrap();
        operation = store.begin_operation(&plan.digest, TARGET_ID, 101).unwrap();
        assert_eq!(
            store.operation_state(operation).unwrap(),
            Some(OperationState::Running)
        );
        // No finish_operation: the process is gone.
    }

    let store = store_at(directory.path());
    let recovered = store.recover_interrupted(200).unwrap();

    assert_eq!(recovered, vec![operation]);
    assert_eq!(
        store.operation_state(operation).unwrap(),
        Some(OperationState::Indeterminate),
        "an interrupted operation is never reported as merely failed"
    );
    assert!(
        store.fresh_inventory_digest(TARGET_ID).unwrap().is_none(),
        "the affected inventory must be stale, forcing a refresh before re-planning"
    );
}

#[test]
fn a_cleanly_finished_operation_is_untouched_by_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let store = store_at(directory.path());
    let plan = a_plan();

    store.upsert_target(TARGET_ID, 1).unwrap();
    store.save_plan(&plan, 100).unwrap();
    store.record_inventory(TARGET_ID, "abc", 100).unwrap();
    let operation = store.begin_operation(&plan.digest, TARGET_ID, 101).unwrap();
    store
        .finish_operation(operation, OperationState::Completed, None, None, 102)
        .unwrap();

    assert!(store.recover_interrupted(200).unwrap().is_empty());
    assert_eq!(
        store.operation_state(operation).unwrap(),
        Some(OperationState::Completed)
    );
    assert_eq!(
        store.fresh_inventory_digest(TARGET_ID).unwrap().as_deref(),
        Some("abc"),
        "a clean finish leaves the inventory usable"
    );
}

#[test]
fn a_stale_inventory_is_absent_evidence_not_weak_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let store = store_at(directory.path());

    store.upsert_target(TARGET_ID, 1).unwrap();
    store.record_inventory(TARGET_ID, "abc", 100).unwrap();
    assert_eq!(
        store.fresh_inventory_digest(TARGET_ID).unwrap().as_deref(),
        Some("abc")
    );

    store.mark_inventory_stale(TARGET_ID).unwrap();
    assert!(
        store.fresh_inventory_digest(TARGET_ID).unwrap().is_none(),
        "stale evidence must not be offered as if it were current"
    );
}

#[test]
fn a_plan_cannot_be_stored_against_an_unknown_target() {
    // Referential integrity is enforced by the schema, so durable state cannot
    // reach a shape the domain rules would reject.
    let directory = tempfile::tempdir().unwrap();
    let store = store_at(directory.path());
    let plan = a_plan();

    assert!(
        store.save_plan(&plan, 100).is_err(),
        "a plan must belong to a known Media Target"
    );
}

#[test]
fn the_fixture_profile_round_trips_with_its_snapshot_digest() {
    let profile = DeviceProfile::generic_nes();
    let restored: DeviceProfile =
        serde_json::from_str(&serde_json::to_string(&profile).unwrap()).unwrap();

    assert_eq!(restored, profile);
    assert_eq!(restored.snapshot_digest(), profile.snapshot_digest());
    assert_eq!(
        restored.target_path("Tracers.nes").unwrap(),
        path(common::DESIRED),
        "a restored profile must place content exactly where the original did"
    );
    let _ = expected();
}
