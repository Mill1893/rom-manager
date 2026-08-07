//! The workflow state machine behind the desktop commands (issue #34).
//!
//! The rules with teeth are the ones about *ordering*: what a command refuses
//! to do because an earlier step has not happened, or has happened again since.

mod common;

use common::{ROM_BYTES, TARGET_ID, expected, fake};
use rom_manager::{
    FakeTransport, MediaTargetChoice, RomPackChoice, Session, SessionError, Store, WizardStep,
};

fn packs() -> Vec<RomPackChoice> {
    vec![
        RomPackChoice {
            rom_pack_id: "pack-a".into(),
            revision: 1,
            title: "Everything".into(),
            rom_set_count: 1,
        },
        RomPackChoice {
            rom_pack_id: "pack-b".into(),
            revision: 3,
            title: "Just the platformers".into(),
            rom_set_count: 1,
        },
    ]
}

fn targets(connected: bool) -> Vec<MediaTargetChoice> {
    vec![MediaTargetChoice {
        target_id: TARGET_ID.into(),
        label: "Odin SD card".into(),
        binding_locator: Some("wpd://odin/storage".into()),
        connected,
    }]
}

fn session(connected: bool) -> Session<FakeTransport> {
    let store = Store::open_in_memory().expect("an in-memory store opens");
    let mut session = Session::new(store, Box::new(|_| Ok(fake())), packs(), targets(connected));
    session.set_desired(vec![expected()]);
    session
}

/// Carries a session as far as a reviewable plan.
fn planned() -> Session<FakeTransport> {
    let mut session = session(true);
    session.select_rom_pack("pack-a", 1).unwrap();
    session.select_media_target(TARGET_ID).unwrap();
    session.initialize_target(true).unwrap();
    session.refresh_target().unwrap();
    session.build_plan().unwrap();
    session
}

// ── Ordering ────────────────────────────────────────────────────────────────

#[test]
fn a_fresh_session_starts_at_the_first_step_with_nothing_chosen() {
    let mut session = session(true);
    let snapshot = session.load_snapshot().unwrap();

    assert_eq!(snapshot.step, WizardStep::SelectRomPack);
    assert!(snapshot.rom_pack.is_none());
    assert!(snapshot.plan.is_none());
}

#[test]
fn a_device_cannot_be_chosen_before_a_rom_pack() {
    let mut session = session(true);
    assert!(matches!(
        session.select_media_target(TARGET_ID),
        Err(SessionError::OutOfOrder(_))
    ));
}

#[test]
fn a_plan_cannot_be_built_before_a_device_is_chosen() {
    let mut session = session(true);
    session.select_rom_pack("pack-a", 1).unwrap();
    assert!(matches!(
        session.build_plan(),
        Err(SessionError::OutOfOrder(_))
    ));
}

#[test]
fn an_unknown_identifier_is_refused_rather_than_guessed_at() {
    let mut session = session(true);
    assert!(matches!(
        session.select_rom_pack("pack-a", 99),
        Err(SessionError::UnknownRomPack)
    ));
    session.select_rom_pack("pack-a", 1).unwrap();
    assert!(matches!(
        session.select_media_target("some-other-device"),
        Err(SessionError::UnknownMediaTarget)
    ));
}

#[test]
fn a_disconnected_device_cannot_be_selected() {
    let mut session = session(false);
    session.select_rom_pack("pack-a", 1).unwrap();
    assert!(matches!(
        session.select_media_target(TARGET_ID),
        Err(SessionError::NotConnected)
    ));
}

// ── Invalidation ────────────────────────────────────────────────────────────

#[test]
fn choosing_a_different_rom_pack_discards_the_plan_built_for_the_old_one() {
    // Keeping it would let a user approve a plan for content they are no
    // longer syncing.
    let mut session = planned();
    assert!(session.snapshot().plan.is_some());

    let snapshot = session.select_rom_pack("pack-b", 3).unwrap();
    assert!(snapshot.plan.is_none());
    assert_eq!(snapshot.step, WizardStep::SelectMediaTarget);
}

#[test]
fn refreshing_discards_the_plan_that_rested_on_the_old_observation() {
    let mut session = planned();
    assert!(session.snapshot().plan.is_some());

    let snapshot = session.refresh_target().unwrap();
    assert!(
        snapshot.plan.is_none(),
        "a plan describing a superseded target state must not remain executable"
    );
}

#[test]
fn no_command_refreshes_or_replans_on_its_own() {
    // Every automatic step between "the user looked at this" and "bytes were
    // written" is a step where the two can diverge.
    let mut session = session(true);
    session.select_rom_pack("pack-a", 1).unwrap();
    let after_target = session.select_media_target(TARGET_ID).unwrap();

    assert!(
        after_target.plan.is_none(),
        "choosing a device must not build a plan by itself"
    );
    assert_eq!(after_target.step, WizardStep::ReviewPlan);
}

#[test]
fn a_device_that_has_not_been_set_up_says_so_rather_than_reporting_a_conflict() {
    // "This is not the device I expected" is not something a user with a brand
    // new card can act on. "This card has not been set up yet" is.
    let mut session = session(true);
    session.select_rom_pack("pack-a", 1).unwrap();
    session.select_media_target(TARGET_ID).unwrap();

    assert!(matches!(
        session.refresh_target(),
        Err(SessionError::NotInitialized)
    ));
}

#[test]
fn claiming_a_device_must_be_confirmed() {
    // Writing a marker is how this application takes responsibility for a
    // device's contents. A user who plugged in the wrong card gets a question.
    let mut session = session(true);
    session.select_rom_pack("pack-a", 1).unwrap();
    session.select_media_target(TARGET_ID).unwrap();

    assert!(matches!(
        session.initialize_target(false),
        Err(SessionError::NotConfirmed)
    ));
    assert!(session.initialize_target(true).is_ok());
}

// ── Approval ────────────────────────────────────────────────────────────────

#[test]
fn a_plan_executes_when_it_is_approved_by_its_own_digest() {
    let mut session = planned();
    let digest = session.snapshot().plan.expect("a plan").plan_digest;

    let snapshot = session.approve_and_execute(&digest, 0).unwrap();
    assert_eq!(snapshot.step, WizardStep::Result);
    assert!(snapshot.outcome.is_some());
}

#[test]
fn approving_a_digest_the_session_does_not_hold_is_refused() {
    // The UI approving something other than what it holds means the two have
    // diverged, and refusing is the only safe answer.
    let mut session = planned();
    assert!(matches!(
        session.approve_and_execute("a-digest-from-somewhere-else", 0),
        Err(SessionError::PlanStale)
    ));
}

#[test]
fn the_same_approval_cannot_be_spent_twice() {
    let mut session = planned();
    let digest = session.snapshot().plan.expect("a plan").plan_digest;
    session.approve_and_execute(&digest, 0).unwrap();

    // The plan is gone, so a replay cannot even name it.
    assert!(session.snapshot().plan.is_none());
    assert!(session.approve_and_execute(&digest, 0).is_err());
}

#[test]
fn an_acknowledgement_that_does_not_match_the_plan_is_refused() {
    // A caller that acknowledged three removals cannot authorize a plan that
    // performs four.
    let mut session = planned();
    let digest = session.snapshot().plan.expect("a plan").plan_digest;

    assert!(matches!(
        session.approve_and_execute(&digest, 7),
        Err(SessionError::AcknowledgementMismatch)
    ));
}

#[test]
fn executing_marks_the_observation_stale_whatever_the_outcome() {
    // After a write the old observation is no longer trustworthy, so any
    // further claim about this target must rest on a fresh one.
    let mut session = planned();
    let digest = session.snapshot().plan.expect("a plan").plan_digest;
    session.approve_and_execute(&digest, 0).unwrap();

    assert_eq!(
        session.store().fresh_inventory_digest(TARGET_ID).unwrap(),
        None,
        "the inventory must be stale after execution"
    );
}

// ── The result step ─────────────────────────────────────────────────────────

#[test]
fn a_result_must_be_dismissed_before_anything_else_starts() {
    let mut session = planned();
    let digest = session.snapshot().plan.expect("a plan").plan_digest;
    session.approve_and_execute(&digest, 0).unwrap();

    assert_eq!(session.snapshot().step, WizardStep::Result);
    let snapshot = session.dismiss_result().unwrap();
    assert_eq!(snapshot.step, WizardStep::SelectRomPack);
    assert!(snapshot.outcome.is_none());
}

#[test]
fn there_is_nothing_to_dismiss_before_a_result_exists() {
    let mut session = planned();
    assert!(matches!(
        session.dismiss_result(),
        Err(SessionError::OutOfOrder(_))
    ));
}

#[test]
fn the_manifest_is_saved_so_the_next_session_knows_what_it_placed() {
    let mut session = planned();
    let digest = session.snapshot().plan.expect("a plan").plan_digest;
    session.approve_and_execute(&digest, 0).unwrap();

    let manifest = session
        .store()
        .load_manifest(TARGET_ID)
        .unwrap()
        .expect("the manifest was persisted");
    assert!(
        !manifest.artifacts.is_empty(),
        "what was placed must be recorded, or the next run cannot tell its own \
         content from someone else's"
    );
}

#[test]
fn what_the_plan_wanted_is_what_reached_the_device() {
    let mut session = planned();
    let plan = session.snapshot().plan.expect("a plan");
    assert_eq!(plan.actions.len(), 1);

    session.approve_and_execute(&plan.plan_digest, 0).unwrap();

    let manifest = session.store().load_manifest(TARGET_ID).unwrap().unwrap();
    let evidence = manifest.artifacts.values().next().expect("one artifact");
    assert_eq!(evidence.size, ROM_BYTES.len() as u64);
}
