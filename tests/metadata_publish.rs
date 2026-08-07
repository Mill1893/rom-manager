//! Coverage for safe publication and recovery (issue #71).
//!
//! Publication is a sequence of refusals, not a write.

use rom_manager::{
    DocumentState, Gamelist, Publication, PublishError, PublishPreconditions, RecoveryChoice,
    RecoveryCopy, recover_missing_document,
};

const DOCUMENT: &str = r#"<?xml version="1.0"?>
<gameList>
  <game>
    <path>./Tracers.nes</path>
    <name>Tracers</name>
    <favorite>true</favorite>
  </game>
</gameList>
"#;

fn stopped() -> PublishPreconditions {
    PublishPreconditions {
        frontend_stopped: true,
    }
}

#[test]
fn publication_requires_confirmation_that_es_de_is_stopped() {
    // A running frontend rewrites gamelists on exit, silently undoing this.
    let publication = Publication::planned_against(Gamelist::fingerprint(DOCUMENT));

    let result = publication.prepare(PublishPreconditions::default(), DOCUMENT, |_| {});

    assert!(matches!(
        result,
        Err(PublishError::FrontendNotConfirmedStopped)
    ));
}

#[test]
fn a_document_that_changed_since_planning_is_refused() {
    let publication = Publication::planned_against(Gamelist::fingerprint(DOCUMENT));
    let edited = DOCUMENT.replace("Tracers", "Tracers (edited by hand)");

    let result = publication.prepare(stopped(), &edited, |_| {});

    assert!(matches!(
        result,
        Err(PublishError::FingerprintChanged { .. })
    ));
}

#[test]
fn a_malformed_document_is_never_replaced() {
    let broken = "<gameList><game><path>./A.nes</path>";
    let publication = Publication::planned_against(Gamelist::fingerprint(broken));

    let result = publication.prepare(stopped(), broken, |_| {});

    assert!(matches!(result, Err(PublishError::Gamelist(_))));
}

#[test]
fn a_clean_publication_produces_a_verified_replacement() {
    let publication = Publication::planned_against(Gamelist::fingerprint(DOCUMENT));

    let replacement = publication
        .prepare(stopped(), DOCUMENT, |gamelist| {
            gamelist.set_owned_field("./Tracers.nes", "genre", "Puzzle");
        })
        .unwrap();

    let reparsed = Gamelist::parse(&replacement).unwrap();
    let entry = reparsed.entry("./Tracers.nes").unwrap();
    assert_eq!(entry.field("genre"), Some("Puzzle"));
    // And the state we do not own came through.
    assert_eq!(entry.field("favorite"), Some("true"));
}

#[test]
fn restoration_is_offered_before_regeneration() {
    // The recovery copy holds frontend state; regeneration cannot.
    let state = DocumentState::MissingWithRecovery(DOCUMENT.into());

    assert_eq!(
        recover_missing_document(&state, 5, true).unwrap(),
        RecoveryChoice::Restore(DOCUMENT.into())
    );
}

#[test]
fn regeneration_requires_confirmation() {
    let state = DocumentState::Missing;

    assert!(matches!(
        recover_missing_document(&state, 5, false),
        Err(PublishError::RegenerationNotConfirmed)
    ));
    assert_eq!(
        recover_missing_document(&state, 5, true).unwrap(),
        RecoveryChoice::Regenerate
    );
}

#[test]
fn a_missing_document_with_nothing_to_write_is_not_recreated() {
    // An empty gamelist is not an improvement on no gamelist.
    let state = DocumentState::Missing;

    assert_eq!(
        recover_missing_document(&state, 0, true).unwrap(),
        RecoveryChoice::LeaveAbsent
    );
}

#[test]
fn the_recovery_copy_rotates_only_after_a_verified_publication() {
    // Rotating earlier would leave a window with no good copy at all — exactly
    // the moment a recovery copy exists for.
    let mut copy = RecoveryCopy::default();
    assert_eq!(copy.get(), None);

    assert!(
        !copy.rotate_after_verified_publication(DOCUMENT.into(), false),
        "an unverified publication must not rotate the copy"
    );
    assert_eq!(copy.get(), None);

    assert!(copy.rotate_after_verified_publication(DOCUMENT.into(), true));
    assert_eq!(copy.get(), Some(DOCUMENT));
}

#[test]
fn discarding_a_recovery_copy_is_explicit() {
    let mut copy = RecoveryCopy::default();
    copy.rotate_after_verified_publication(DOCUMENT.into(), true);
    assert!(copy.get().is_some());

    copy.discard();
    assert_eq!(copy.get(), None);
}

#[test]
fn a_present_document_needs_no_recovery() {
    let state = DocumentState::Present(DOCUMENT.into());

    assert_eq!(
        recover_missing_document(&state, 3, false).unwrap(),
        RecoveryChoice::LeaveAbsent
    );
}
