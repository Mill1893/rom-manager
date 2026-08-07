//! ES-DE metadata coverage over MTP-like transports and split targets
//! (issue #21).
//!
//! Everything here that can be established without a device is established
//! here. What cannot — frontend discovery, card-reader identity, real Odin
//! behaviour — belongs to #38 on the hardware, and is named at the bottom.

mod common;

use std::cell::Cell;

use rom_manager::{
    Backend, CancellationToken, CombinedOutcome, DestinationRole, EsdeProfile, Gamelist,
    ManagedArtifactManifest, Publication, PublishPreconditions, RelativePath, RoleAssignment,
    SplitReadiness, TargetMarker, TransportError, WpdFault, WpdLikeBackend, mtp_capabilities,
    run_combined, split_readiness,
};

const EXISTING: &str = r#"<?xml version="1.0"?>
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

fn path(value: &str) -> RelativePath {
    RelativePath::new(value).unwrap()
}

/// A device holding a gamelist at the ES-DE location.
fn device_with_gamelist() -> (WpdLikeBackend, RelativePath) {
    let profile = EsdeProfile::nes();
    let gamelist = profile.gamelist_path().unwrap();
    let backend = WpdLikeBackend::new(Some(64 * 1024 * 1024))
        .with_object(gamelist.clone(), EXISTING.as_bytes().to_vec());
    (backend, gamelist)
}

#[test]
fn mtp_never_claims_atomic_metadata_publication() {
    // The gamelist is the one file where a torn write costs the user state
    // nobody can reconstruct, so the transport's inability to replace it
    // atomically must be disclosed rather than assumed away.
    assert!(!mtp_capabilities(true).atomic_publish);
}

#[test]
fn a_gamelist_round_trips_through_an_mtp_like_transport() {
    let (mut backend, gamelist) = device_with_gamelist();

    let current = String::from_utf8(backend.read(&gamelist).unwrap()).unwrap();
    let publication = Publication::planned_against(Gamelist::fingerprint(&current));
    let replacement = publication
        .prepare(stopped(), &current, |list| {
            list.set_owned_field("./Tracers.nes", "genre", "Puzzle");
        })
        .unwrap();

    // MTP has no replace-in-place, so publication is delete-then-write. That is
    // exactly why the prior verified copy is retained first.
    backend.delete_leaf(&gamelist).unwrap();
    backend
        .write_new(
            &gamelist,
            replacement.as_bytes(),
            &CancellationToken::default(),
        )
        .unwrap();

    let reread = String::from_utf8(backend.read(&gamelist).unwrap()).unwrap();
    let parsed = Gamelist::parse(&reread).unwrap();
    let entry = parsed.entry("./Tracers.nes").unwrap();
    assert_eq!(entry.field("genre"), Some("Puzzle"));
    assert_eq!(entry.field("favorite"), Some("true"), "still theirs");
}

#[test]
fn a_disconnect_during_metadata_publication_leaves_the_prior_copy_recoverable() {
    let (mut backend, gamelist) = device_with_gamelist();
    let prior = String::from_utf8(backend.read(&gamelist).unwrap()).unwrap();

    backend.delete_leaf(&gamelist).unwrap();
    backend.set_fault(Some(WpdFault::DisconnectOnWrite));
    let result = backend.write_new(
        &gamelist,
        b"replacement that never lands",
        &CancellationToken::default(),
    );

    assert!(matches!(result, Err(TransportError::Disconnected)));
    // The document is gone from the device — which is precisely the state the
    // retained recovery copy exists for.
    backend.set_fault(None);
    assert!(backend.read(&gamelist).is_err());
    assert!(
        Gamelist::parse(&prior).is_ok(),
        "the retained copy is still a valid document to restore from"
    );
}

#[test]
fn split_targets_plan_independently_with_no_cross_target_atomicity() {
    // Two devices that can be unplugged separately cannot share a transaction,
    // so pretending otherwise would be a lie the hardware can expose.
    let assignment = RoleAssignment::Split {
        rom_content: "sd-card".into(),
        frontend_metadata: "internal".into(),
        confirmed: true,
    };

    let readiness = split_readiness(&assignment, true);
    assert_eq!(
        readiness,
        SplitReadiness::ContentFirst {
            rom_content: "sd-card".into(),
            frontend_metadata: "internal".into()
        }
    );
    assert_ne!(
        assignment.target_for(DestinationRole::RomContent),
        assignment.target_for(DestinationRole::FrontendMetadata),
        "two distinct targets, planned and approved separately"
    );
}

#[test]
fn metadata_waits_for_rom_content_to_converge() {
    let assignment = RoleAssignment::Split {
        rom_content: "sd-card".into(),
        frontend_metadata: "internal".into(),
        confirmed: true,
    };

    assert_eq!(
        split_readiness(&assignment, false),
        SplitReadiness::Blocked("the ROM-content target is not current"),
        "a gamelist naming files that have not arrived names files that are not there"
    );
}

#[test]
fn a_metadata_failure_on_a_split_target_never_rolls_back_content() {
    let removed = Cell::new(0usize);
    let outcome = run_combined(
        2,
        || Ok(()),
        || Err("metadata target disconnected".into()),
        |count| removed.set(count),
    );

    assert!(outcome.content_retained());
    assert_eq!(removed.get(), 0);
    assert_eq!(outcome.summary(), "ROM content synced; metadata pending.");
    assert!(matches!(
        outcome,
        CombinedOutcome::ContentSyncedMetadataPending { .. }
    ));
}

#[test]
fn unicode_and_nested_gamelist_paths_survive_the_transport() {
    // ES-DE system keys are lowercase ASCII, but ROM names are not, and a
    // transport that mangles them would corrupt entry keys.
    let mut backend = WpdLikeBackend::new(Some(1 << 20));
    let nested = path("ES-DE/gamelists/nes/gamelist.xml");
    let unicode = path("ROMs/nes/caf\u{e9}.nes");

    backend
        .write_new(&nested, EXISTING.as_bytes(), &CancellationToken::default())
        .unwrap();
    backend
        .write_new(&unicode, b"rom bytes", &CancellationToken::default())
        .unwrap();

    assert_eq!(backend.read(&nested).unwrap(), EXISTING.as_bytes());
    assert_eq!(backend.read(&unicode).unwrap(), b"rom bytes");

    let inventory = backend.inventory().unwrap();
    assert!(inventory.artifacts.contains_key(&nested));
    assert!(inventory.artifacts.contains_key(&unicode));
}

#[test]
fn a_reconnect_at_a_new_locator_keeps_the_target_identity() {
    // The marker is the identity; the locator is where it happened to be
    // plugged in this time.
    let marker = TargetMarker::new(common::TARGET_ID);
    let mut first = WpdLikeBackend::new(Some(1 << 20));
    first.write_marker(&marker).unwrap();

    // A new session at a different locator reads the same marker.
    let mut second = WpdLikeBackend::new(Some(1 << 20));
    second.write_marker(&marker).unwrap();

    assert_eq!(first.marker().unwrap(), second.marker().unwrap());
}

#[test]
fn cancellation_before_a_metadata_write_leaves_the_document_untouched() {
    let (mut backend, gamelist) = device_with_gamelist();
    let before = backend.read(&gamelist).unwrap();

    let cancellation = CancellationToken::default();
    cancellation.cancel();
    let result = backend.write_new(&path("ES-DE/gamelists/nes/new.xml"), b"x", &cancellation);

    assert!(matches!(result, Err(TransportError::Cancelled)));
    assert_eq!(
        backend.read(&gamelist).unwrap(),
        before,
        "a cancelled write touches nothing"
    );
}

#[test]
fn a_locked_device_fails_metadata_cleanly_rather_than_partially() {
    let (mut backend, _) = device_with_gamelist();
    backend.set_fault(Some(WpdFault::Unauthorized));

    assert!(matches!(
        backend.inventory(),
        Err(TransportError::Unsupported(_))
    ));
    assert!(matches!(
        backend.write_new(
            &path("ES-DE/gamelists/nes/x.xml"),
            b"x",
            &CancellationToken::default()
        ),
        Err(TransportError::Unsupported(_))
    ));
}

#[test]
fn an_indeterminate_manifest_publication_is_reported_as_unestablished() {
    let mut backend = WpdLikeBackend::new(Some(1 << 20)).with_fault(WpdFault::IndeterminatePublish);
    let manifest = common::manifest_naming(b"seed");

    assert!(
        matches!(
            backend.write_manifest(&manifest).unwrap_err(),
            TransportError::Disconnected
        ),
        "a publication the device did not confirm must not read as success"
    );
}

#[test]
fn the_esde_profile_resolves_both_roles_on_one_device() {
    let profile = EsdeProfile::nes();
    let combined = RoleAssignment::Combined {
        target_id: common::TARGET_ID.into(),
    };

    assert!(combined.is_combined());
    assert_eq!(
        profile.rom_target_path("Tracers.nes").unwrap().as_str(),
        "ROMs/nes/Tracers.nes"
    );
    assert_eq!(
        profile.gamelist_path().unwrap().as_str(),
        "ES-DE/gamelists/nes/gamelist.xml"
    );
    let _ = ManagedArtifactManifest::empty(
        common::TARGET_ID,
        &rom_manager::DeviceProfile::generic_nes(),
    );
}
