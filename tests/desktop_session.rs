//! The workflow state machine behind the desktop commands (issue #34).
//!
//! The rules with teeth are the ones about *ordering*: what a command refuses
//! to do because an earlier step has not happened, or has happened again since.

mod common;

use common::{ROM_BYTES, expected, fake};
use rom_manager::{FakeTransport, Session, SessionError, Store, WizardStep};

const LOCATOR: &str = "wpd://odin/storage";

/// A store already holding two ROM Packs, as an import would have left it.
fn seeded_store() -> Store {
    let store = Store::open_in_memory().expect("an in-memory store opens");
    store
        .upsert_rom_set(
            ("game-tracer", "nes", "Tracers"),
            ("release-tracer", "World"),
            ("rom-set-tracer", "digest-a", "Tracers.nes", 24_592),
        )
        .unwrap();
    store
        .record_pack_selection("pack-a", 1, &[("rom-set-tracer", "digest-a")])
        .unwrap();
    store.set_pack_title("pack-a", 1, "Everything").unwrap();
    store
        .record_pack_selection("pack-b", 3, &[("rom-set-tracer", "digest-a")])
        .unwrap();
    store
        .set_pack_title("pack-b", 3, "Just the platformers")
        .unwrap();
    store
}

/// A session whose catalogues are whatever durable state holds — nothing is
/// handed in, so the tests exercise the same path the application does.
fn bare_session(reachable: bool) -> Session<FakeTransport> {
    let mut session = Session::new(
        seeded_store(),
        Box::new(move |_| {
            if reachable {
                Ok(fake())
            } else {
                Err("the device is not connected".into())
            }
        }),
    );
    session.set_desired(vec![expected()]);
    session
}

/// A session with one nominated Media Target, and that target's identity.
fn session_with_target(reachable: bool) -> (Session<FakeTransport>, String) {
    let mut session = bare_session(true);
    let choice = session
        .nominate_media_target(LOCATOR, "Odin SD card")
        .expect("a directory can be nominated");
    let target_id = choice.target_id.clone();
    if !reachable {
        // Nomination needs to reach the device once; connectedness afterwards
        // is a separate question, so the transport is soured only now.
        session.set_connect(Box::new(|_| Err("the device is not connected".into())));
        let _ = session.reload_catalogues();
    }
    (session, target_id)
}

fn session(connected: bool) -> Session<FakeTransport> {
    session_with_target(connected).0
}

/// Carries a session as far as a reviewable plan.
fn planned() -> (Session<FakeTransport>, String) {
    let (mut session, target_id) = session_with_target(true);
    session.select_rom_pack("pack-a", 1).unwrap();
    session.select_media_target(&target_id).unwrap();
    session.initialize_target(true).unwrap();
    session.refresh_target().unwrap();
    session.build_plan().unwrap();
    (session, target_id)
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
    let (mut session, target_id) = session_with_target(true);
    assert!(matches!(
        session.select_media_target(&target_id),
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
    let (mut session, target_id) = session_with_target(false);
    session.select_rom_pack("pack-a", 1).unwrap();
    assert!(matches!(
        session.select_media_target(&target_id),
        Err(SessionError::NotConnected)
    ));
}

// ── Invalidation ────────────────────────────────────────────────────────────

#[test]
fn choosing_a_different_rom_pack_discards_the_plan_built_for_the_old_one() {
    // Keeping it would let a user approve a plan for content they are no
    // longer syncing.
    let (mut session, _target_id) = planned();
    assert!(session.snapshot().plan.is_some());

    let snapshot = session.select_rom_pack("pack-b", 3).unwrap();
    assert!(snapshot.plan.is_none());
    assert_eq!(snapshot.step, WizardStep::SelectMediaTarget);
}

#[test]
fn refreshing_discards_the_plan_that_rested_on_the_old_observation() {
    let (mut session, _target_id) = planned();
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
    let (mut session, target_id) = session_with_target(true);
    session.select_rom_pack("pack-a", 1).unwrap();
    let after_target = session.select_media_target(&target_id).unwrap();

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
    let (mut session, target_id) = session_with_target(true);
    session.select_rom_pack("pack-a", 1).unwrap();
    session.select_media_target(&target_id).unwrap();

    assert!(matches!(
        session.refresh_target(),
        Err(SessionError::NotInitialized)
    ));
}

#[test]
fn claiming_a_device_must_be_confirmed() {
    // Writing a marker is how this application takes responsibility for a
    // device's contents. A user who plugged in the wrong card gets a question.
    let (mut session, target_id) = session_with_target(true);
    session.select_rom_pack("pack-a", 1).unwrap();
    session.select_media_target(&target_id).unwrap();

    assert!(matches!(
        session.initialize_target(false),
        Err(SessionError::NotConfirmed)
    ));
    assert!(session.initialize_target(true).is_ok());
}

// ── Approval ────────────────────────────────────────────────────────────────

#[test]
fn a_plan_executes_when_it_is_approved_by_its_own_digest() {
    let (mut session, _target_id) = planned();
    let digest = session.snapshot().plan.expect("a plan").plan_digest;

    let snapshot = session.approve_and_execute(&digest, 0).unwrap();
    assert_eq!(snapshot.step, WizardStep::Result);
    assert!(snapshot.outcome.is_some());
}

#[test]
fn approving_a_digest_the_session_does_not_hold_is_refused() {
    // The UI approving something other than what it holds means the two have
    // diverged, and refusing is the only safe answer.
    let (mut session, _target_id) = planned();
    assert!(matches!(
        session.approve_and_execute("a-digest-from-somewhere-else", 0),
        Err(SessionError::PlanStale)
    ));
}

#[test]
fn the_same_approval_cannot_be_spent_twice() {
    let (mut session, _target_id) = planned();
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
    let (mut session, _target_id) = planned();
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
    let (mut session, target_id) = planned();
    let digest = session.snapshot().plan.expect("a plan").plan_digest;
    session.approve_and_execute(&digest, 0).unwrap();

    assert_eq!(
        session.store().fresh_inventory_digest(&target_id).unwrap(),
        None,
        "the inventory must be stale after execution"
    );
}

// ── The result step ─────────────────────────────────────────────────────────

#[test]
fn a_result_must_be_dismissed_before_anything_else_starts() {
    let (mut session, _target_id) = planned();
    let digest = session.snapshot().plan.expect("a plan").plan_digest;
    session.approve_and_execute(&digest, 0).unwrap();

    assert_eq!(session.snapshot().step, WizardStep::Result);
    let snapshot = session.dismiss_result().unwrap();
    assert_eq!(snapshot.step, WizardStep::SelectRomPack);
    assert!(snapshot.outcome.is_none());
}

#[test]
fn there_is_nothing_to_dismiss_before_a_result_exists() {
    let (mut session, _target_id) = planned();
    assert!(matches!(
        session.dismiss_result(),
        Err(SessionError::OutOfOrder(_))
    ));
}

#[test]
fn the_manifest_is_saved_so_the_next_session_knows_what_it_placed() {
    let (mut session, target_id) = planned();
    let digest = session.snapshot().plan.expect("a plan").plan_digest;
    session.approve_and_execute(&digest, 0).unwrap();

    let manifest = session
        .store()
        .load_manifest(&target_id)
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
    let (mut session, target_id) = planned();
    let plan = session.snapshot().plan.expect("a plan");
    assert_eq!(plan.actions.len(), 1);

    session.approve_and_execute(&plan.plan_digest, 0).unwrap();

    let manifest = session.store().load_manifest(&target_id).unwrap().unwrap();
    let evidence = manifest.artifacts.values().next().expect("one artifact");
    assert_eq!(evidence.size, ROM_BYTES.len() as u64);
}

// ── Nomination ──────────────────────────────────────────────────────────────

#[test]
fn a_session_with_nothing_nominated_offers_nothing() {
    // The alternative — inventing a plausible device — would be guessing at the
    // one thing this application must never guess at.
    let mut session = Session::new(Store::open_in_memory().unwrap(), Box::new(|_| Ok(fake())));
    assert!(session.available_targets().is_empty());
    assert!(session.available_packs().is_empty());
    assert!(matches!(
        session.select_media_target("anything"),
        Err(SessionError::UnknownRomPack) | Err(SessionError::OutOfOrder(_))
    ));
}

#[test]
fn packs_come_from_durable_state_rather_than_from_the_caller() {
    let session = bare_session(true);
    let titles: Vec<&str> = session
        .available_packs()
        .iter()
        .map(|pack| pack.title.as_str())
        .collect();
    assert_eq!(titles, vec!["Everything", "Just the platformers"]);
    assert_eq!(session.available_packs()[0].rom_set_count, 1);
}

#[test]
fn nominating_a_directory_makes_it_choosable() {
    let mut session = bare_session(true);
    assert!(session.available_targets().is_empty());

    let choice = session
        .nominate_media_target(LOCATOR, "Odin SD card")
        .unwrap();

    assert_eq!(session.available_targets().len(), 1);
    assert_eq!(choice.label, "Odin SD card");
    assert_eq!(choice.binding_locator.as_deref(), Some(LOCATOR));
    assert!(choice.connected);
}

#[test]
fn a_directory_that_already_carries_a_marker_keeps_that_identity() {
    // Identity lives in the marker. A card that comes back on a different
    // drive letter is the same target, and minting a new identity for it would
    // orphan its manifest and make the application forget what it placed.
    use rom_manager::{TargetMarker, Transport};

    let mut session = Session::new(
        Store::open_in_memory().unwrap(),
        Box::new(|_| {
            let mut transport = fake();
            transport
                .write_marker(&TargetMarker::new("target-from-an-earlier-run"))
                .unwrap();
            Ok(transport)
        }),
    );

    let choice = session
        .nominate_media_target(LOCATOR, "The same card")
        .unwrap();
    assert_eq!(choice.target_id, "target-from-an-earlier-run");
}

#[test]
fn nominating_the_same_place_twice_does_not_create_two_targets() {
    let mut session = bare_session(true);
    let first = session.nominate_media_target(LOCATOR, "Card").unwrap();
    let second = session
        .nominate_media_target(LOCATOR, "Card renamed")
        .unwrap();

    assert_eq!(
        first.target_id, second.target_id,
        "an un-markered directory nominated twice is still one place"
    );
    assert_eq!(session.available_targets().len(), 1);
    assert_eq!(
        session.available_targets()[0].label,
        "Card renamed",
        "the label is editable; the identity is not"
    );
}

#[test]
fn a_nominated_target_survives_a_restart() {
    // The whole point of nominating: the user should not have to find their
    // card again every time the application opens.
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("state.sqlite3");
    let target_id = {
        let mut session = Session::new(Store::open(&database).unwrap(), Box::new(|_| Ok(fake())));
        session
            .nominate_media_target(LOCATOR, "Odin SD card")
            .unwrap()
            .target_id
    };

    let reopened = Session::new(Store::open(&database).unwrap(), Box::new(|_| Ok(fake())));
    let targets = reopened.available_targets();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].target_id, target_id);
    assert_eq!(targets[0].label, "Odin SD card");
}

#[test]
fn an_unreachable_target_is_listed_and_marked_rather_than_hidden() {
    // The user nominated it. Its absence is information, and a device that
    // silently vanished from the list would look like data loss.
    let (session, target_id) = session_with_target(false);
    let listed = session.available_targets();

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].target_id, target_id);
    assert!(!listed[0].connected);
}

#[test]
fn an_import_folder_is_remembered_and_listed() {
    let mut session = bare_session(true);
    assert!(session.import_folders().unwrap().is_empty());

    session.nominate_import_folder("/home/andy/roms").unwrap();
    session.nominate_import_folder("/media/usb/roms").unwrap();

    let folders: Vec<String> = session
        .import_folders()
        .unwrap()
        .into_iter()
        .map(|(_, path)| path)
        .collect();
    assert_eq!(folders.len(), 2);
    assert!(folders.contains(&"/home/andy/roms".to_owned()));
}

#[test]
fn remembering_the_same_folder_twice_records_it_once() {
    let mut session = bare_session(true);
    let first = session.nominate_import_folder("/home/andy/roms").unwrap();
    let second = session.nominate_import_folder("/home/andy/roms").unwrap();

    assert_eq!(first, second);
    assert_eq!(session.import_folders().unwrap().len(), 1);
}

#[test]
fn nominating_the_same_place_across_a_second_boundary_is_still_one_target() {
    // The regression this guards. Target identity was seeded with the wall
    // clock in seconds, so nominating one card twice produced one target or
    // two depending on whether the calls happened to land in the same second.
    // `nominating_the_same_place_twice_does_not_create_two_targets` passed
    // almost always and failed under a loaded parallel run, which is the worst
    // way for an identity bug to present: it looks like a flaky test.
    //
    // Sleeping past a second boundary makes the old behaviour fail every time.
    let mut session = bare_session(true);
    let first = session.nominate_media_target(LOCATOR, "Card").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let second = session
        .nominate_media_target(LOCATOR, "Card again")
        .unwrap();

    assert_eq!(
        first.target_id, second.target_id,
        "one card is one Media Target however long the user took to click twice"
    );
    assert_eq!(session.available_targets().len(), 1);
}

#[test]
fn the_snapshot_carries_the_packs_the_library_holds_before_one_is_chosen() {
    // The interface can only offer what the snapshot contains. It carried the
    // *chosen* pack and nothing else, so a Library holding hundreds of games
    // was rendered as "No ROM Packs yet" — and the only control that could
    // have selected one was disabled until one already was.
    let session = bare_session(true);
    let snapshot = session.snapshot();

    assert!(snapshot.rom_pack.is_none(), "nothing is chosen yet");
    assert_eq!(
        snapshot.available_packs.len(),
        2,
        "the seeded catalogue must still be offered"
    );
}

#[test]
fn a_remembered_device_is_offered_even_before_it_is_chosen() {
    let mut session = bare_session(true);
    session
        .nominate_media_target(LOCATOR, "Odin SD card")
        .unwrap();

    let snapshot = session.snapshot();

    assert!(snapshot.media_target.is_none());
    assert_eq!(snapshot.available_targets.len(), 1);
    assert_eq!(snapshot.available_targets[0].label, "Odin SD card");
}
