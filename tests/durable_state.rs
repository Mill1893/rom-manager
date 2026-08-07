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
    assert_eq!(store.schema_version().unwrap(), rom_manager::SCHEMA_VERSION);
    drop(store);

    // Reopening must not reapply a migration that already ran.
    let reopened = store_at(directory.path());
    assert_eq!(
        reopened.schema_version().unwrap(),
        rom_manager::SCHEMA_VERSION
    );
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

#[test]
fn an_old_store_migrates_forward_preserving_its_rows() {
    // A store written at schema 1 must reach the current version without losing
    // what it already held — this is what makes a migration safe to ship.
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("library.sqlite3");
    let manifest = manifest_naming(common::ROM_BYTES);

    {
        let legacy = rusqlite_open_at_v1(&file);
        legacy
            .execute(
                "INSERT INTO media_target (target_id, marker_schema) VALUES (?1, 1)",
                [TARGET_ID],
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO managed_manifest (target_id, generation, body) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    TARGET_ID,
                    manifest.generation,
                    serde_json::to_string(&manifest).unwrap()
                ],
            )
            .unwrap();
        legacy.pragma_update(None, "user_version", 1).unwrap();
    }

    let store = store_at(directory.path());
    assert_eq!(store.schema_version().unwrap(), rom_manager::SCHEMA_VERSION);
    assert_eq!(
        store.load_manifest(TARGET_ID).unwrap(),
        Some(manifest),
        "migrating forward must not disturb rows written by the older schema"
    );

    // And the tables the new migration added are usable.
    store
        .upsert_rom_set(
            ("game-tracers", "NES", "Tracers"),
            ("release-tracers-usa", "USA"),
            ("rom-set-tracer", "digest-abc", "Tracers.nes", 24),
        )
        .unwrap();
    store
        .record_pack_selection("pack-fixture", 1, &[("rom-set-tracer", "digest-abc")])
        .unwrap();
    assert_eq!(
        store.pack_selection("pack-fixture", 1).unwrap(),
        vec![("rom-set-tracer".to_string(), "digest-abc".to_string())]
    );
}

/// Creates a store containing only migration 0001, as an older build would.
fn rusqlite_open_at_v1(file: &std::path::Path) -> rusqlite::Connection {
    let connection = rusqlite::Connection::open(file).unwrap();
    connection
        .execute_batch(include_str!("../migrations/0001_initial.sql"))
        .unwrap();
    connection
}

#[test]
fn a_store_from_a_newer_build_is_refused() {
    // Guessing at a schema this build does not understand could mean writing
    // rows a newer build would read back as something else.
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("library.sqlite3");
    {
        let connection = rusqlite_open_at_v1(&file);
        connection
            .pragma_update(None, "user_version", rom_manager::SCHEMA_VERSION + 1)
            .unwrap();
    }
    assert!(
        Store::open(&file).is_err(),
        "durable state from a newer build must be refused, not migrated backwards"
    );
}

#[test]
fn an_exact_pack_selection_survives_restart() {
    let directory = tempfile::tempdir().unwrap();
    {
        let store = store_at(directory.path());
        store
            .upsert_rom_set(
                ("game-tracers", "NES", "Tracers"),
                ("release-tracers-usa", "USA"),
                ("rom-set-tracer", "digest-abc", "Tracers.nes", 24),
            )
            .unwrap();
        store
            .record_pack_selection("pack-fixture", 1, &[("rom-set-tracer", "digest-abc")])
            .unwrap();
    }

    let store = store_at(directory.path());
    assert_eq!(
        store.pack_selection("pack-fixture", 1).unwrap(),
        vec![("rom-set-tracer".to_string(), "digest-abc".to_string())],
        "an exact selection must mean the same thing after a restart"
    );
}

#[test]
fn a_durable_run_records_its_outcome_and_mirrors_the_manifest() {
    let directory = tempfile::tempdir().unwrap();
    let store = store_at(directory.path());
    let mut core = core_with(fake());
    core.refresh().unwrap();
    let plan = core.build_plan().unwrap();

    store.upsert_target(TARGET_ID, 1).unwrap();
    store
        .record_inventory(TARGET_ID, &plan.inventory_digest, 10)
        .unwrap();
    rom_manager::approve(
        &store,
        &plan,
        &Approval::grant(&plan, plan.removal_count()),
        11,
    )
    .unwrap();

    let outcome =
        rom_manager::execute_approved(&store, &mut core, &plan.digest, &Default::default(), 12)
            .unwrap();

    assert!(matches!(
        outcome,
        rom_manager::ExecutionOutcome::Completed { .. }
    ));
    assert!(
        store.load_manifest(TARGET_ID).unwrap().is_some(),
        "a completed run mirrors what it proved is on the target"
    );
    // The approval was consumed, so the same plan cannot run twice.
    assert!(
        rom_manager::execute_approved(&store, &mut core, &plan.digest, &Default::default(), 13)
            .is_err(),
        "an approval is single use across the durable boundary too"
    );
}
