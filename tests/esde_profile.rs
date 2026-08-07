//! Coverage for the ES-DE profile and its Destination Roles (issue #66).

use rom_manager::{DestinationRole, EsdeProfile, RoleAssignment};

#[test]
fn the_profile_places_roms_and_gamelists_in_their_own_roles() {
    let profile = EsdeProfile::nes();

    assert_eq!(
        profile.rom_target_path("Tracers.nes").unwrap().as_str(),
        "ROMs/nes/Tracers.nes"
    );
    assert_eq!(
        profile.gamelist_path().unwrap().as_str(),
        "ES-DE/gamelists/nes/gamelist.xml"
    );
}

#[test]
fn entry_paths_are_relative_to_the_system_rom_directory() {
    // A document that names where a file was on *this* computer is wrong the
    // moment it is read on the device.
    let profile = EsdeProfile::nes();

    assert_eq!(
        profile.gamelist_entry_path("Tracers.nes").unwrap(),
        "./Tracers.nes"
    );
}

#[test]
fn a_name_that_cannot_be_placed_cannot_be_described() {
    let profile = EsdeProfile::nes();

    for rejected in [
        "sub/Tracers.nes",
        "sub\\Tracers.nes",
        "con.nes",
        "Tracers.nes.",
        "Tracers.sfc",
    ] {
        assert!(
            profile.rom_target_path(rejected).is_err(),
            "{rejected} must not yield a target path"
        );
        assert!(
            profile.gamelist_entry_path(rejected).is_err(),
            "{rejected} must not yield an entry path either"
        );
    }
}

#[test]
fn a_combined_target_fulfils_both_roles() {
    let assignment = RoleAssignment::Combined {
        target_id: "target-1".into(),
    };

    assert!(assignment.is_usable());
    assert!(assignment.is_combined());
    assert_eq!(
        assignment.target_for(DestinationRole::RomContent),
        Some("target-1")
    );
    assert_eq!(
        assignment.target_for(DestinationRole::FrontendMetadata),
        Some("target-1")
    );
}

#[test]
fn a_confirmed_pairing_resolves_each_role_to_its_own_target() {
    let assignment = RoleAssignment::Split {
        rom_content: "sd-card".into(),
        frontend_metadata: "internal".into(),
        confirmed: true,
    };

    assert!(assignment.is_usable());
    assert!(!assignment.is_combined());
    assert_eq!(
        assignment.target_for(DestinationRole::RomContent),
        Some("sd-card")
    );
    assert_eq!(
        assignment.target_for(DestinationRole::FrontendMetadata),
        Some("internal")
    );
}

#[test]
fn an_unconfirmed_pairing_blocks_export() {
    // Guessing here could write one device's metadata describing another's
    // ROMs, so an unconfirmed pairing is not a pairing.
    let assignment = RoleAssignment::Split {
        rom_content: "sd-card".into(),
        frontend_metadata: "internal".into(),
        confirmed: false,
    };

    assert!(!assignment.is_usable());
    assert_eq!(assignment.target_for(DestinationRole::RomContent), None);
    assert_eq!(
        assignment.target_for(DestinationRole::FrontendMetadata),
        None
    );
}

#[test]
fn the_profile_identity_is_frozen_and_version_pinned() {
    let profile = EsdeProfile::nes();

    assert_eq!(profile.id, "esde-android");
    assert_eq!(profile.revision, 1);
    assert_eq!(
        profile.esde_version, "3.1.1",
        "the profile is pinned to a validated ES-DE release, not to whatever is installed"
    );

    let frozen = profile.snapshot_digest();
    assert_eq!(frozen.len(), 64);
    assert_eq!(
        EsdeProfile::nes().snapshot_digest(),
        frozen,
        "the snapshot is reproducible"
    );

    // Any behavior-bearing change must move the digest.
    let mut moved = EsdeProfile::nes();
    moved.system_key = "famicom".into();
    assert_ne!(moved.snapshot_digest(), frozen);
}
