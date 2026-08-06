//! Gating coverage for the portable target-path namespace (issue #42) and the
//! frozen Generic profile identity (issue #46).
//!
//! These assert the *application's* rules, not the host's. Probe evidence from
//! issue #52 informed them, but nothing here depends on what any particular
//! Windows build does, so a servicing update cannot break the build.

use rom_manager::{DeviceProfile, RelativePath};

fn rejected(value: &str) {
    assert!(
        RelativePath::new(value).is_err(),
        "expected {value:?} to be rejected"
    );
}

fn accepted(value: &str) -> RelativePath {
    RelativePath::new(value)
        .unwrap_or_else(|error| panic!("expected {value:?} to be valid: {error}"))
}

#[test]
fn backslash_is_rejected_rather_than_rewritten() {
    // The prior implementation silently rewrote `\` to `/`, which repairs a
    // name the caller could not prove. A separator is now a rejection.
    rejected("ROMs\\nes\\tracers.nes");
    rejected("ROMs/nes\\tracers.nes");
}

#[test]
fn navigation_and_rooted_forms_are_rejected() {
    for value in [
        "",
        "/ROMs/nes/tracers.nes",
        "ROMs/nes/",
        "./tracers.nes",
        "ROMs/../tracers.nes",
        "ROMs//tracers.nes",
        "C:/ROMs/tracers.nes",
    ] {
        rejected(value);
    }
}

#[test]
fn trailing_dots_and_spaces_are_rejected() {
    // Win32 path parsing trims these, so `tracers.nes.` addresses a *different*
    // file than the one written down.
    rejected("ROMs/nes/tracers.nes.");
    rejected("ROMs/nes/tracers.nes ");
    rejected("ROMs/nes/ tracers.nes");
    rejected("ROMs./nes/tracers.nes");
}

#[test]
fn reserved_characters_and_streams_are_rejected() {
    for value in [
        "ROMs/nes/tra<cers.nes",
        "ROMs/nes/tra>cers.nes",
        "ROMs/nes/tra\"cers.nes",
        "ROMs/nes/tra|cers.nes",
        "ROMs/nes/tra?cers.nes",
        "ROMs/nes/tra*cers.nes",
        "ROMs/nes/tracers.nes:stream",
        "ROMs/nes/tra\u{1}cers.nes",
        "ROMs/nes/tra\u{7f}cers.nes",
    ] {
        rejected(value);
    }
}

#[test]
fn reserved_device_basenames_are_rejected_regardless_of_extension() {
    // Probes found these create ordinary files on Windows 11 build 26200 but
    // resolve to devices on other builds. Rejected on unpredictability, not on
    // any one version's behaviour.
    for name in [
        "CON", "con", "PRN", "aux", "NUL", "com1", "COM9", "lpt1", "LPT9", "conin$", "CONOUT$",
    ] {
        rejected(&format!("ROMs/nes/{name}"));
        rejected(&format!("ROMs/nes/{name}.nes"));
    }
    // A reserved name as a *directory* component is equally unsafe.
    rejected("ROMs/con/tracers.nes");
    // Names that merely start with a reserved stem stay valid.
    accepted("ROMs/nes/console.nes");
    accepted("ROMs/nes/com10.nes");
}

#[test]
fn bounds_are_enforced() {
    accepted(&format!("ROMs/nes/{}.nes", "a".repeat(250)));
    rejected(&format!("ROMs/nes/{}.nes", "a".repeat(256)));
    rejected(&format!("ROMs/nes/{}", "a/".repeat(600)));
}

#[test]
fn non_nfc_input_is_rejected_but_canonicalizes() {
    let nfd = "ROMs/nes/cafe\u{301}.nes";
    let nfc = "ROMs/nes/caf\u{e9}.nes";

    // `new` takes already-canonical input and does not repair.
    rejected(nfd);
    accepted(nfc);

    // `canonicalize` applies NFC — the one permitted transformation.
    let canonical = RelativePath::canonicalize(nfd).expect("NFD input canonicalizes");
    assert_eq!(canonical.as_str(), nfc);
}

#[test]
fn equivalence_key_folds_case_and_normalization() {
    let planned = accepted("ROMs/nes/caf\u{e9}.nes");
    let upper = accepted("ROMs/NES/CAF\u{c9}.NES");
    let nfd = RelativePath::canonicalize("ROMs/nes/cafe\u{301}.nes").expect("canonicalizes");

    assert_eq!(planned.equivalence_key(), upper.equivalence_key());
    assert_eq!(planned.equivalence_key(), nfd.equivalence_key());
}

#[test]
fn equivalence_key_over_folds_rather_than_under_folds() {
    // Probes showed NTFS folds only simple 1:1 BMP mappings — it keeps each of
    // these pairs distinct. The key folds them together, so the app blocks where
    // the host would have allowed two files. That is the safe direction: a
    // spurious, disclosed block rather than a silent overwrite.
    //
    // Final sigma is deliberately absent: `to_lowercase` keeps it distinct from
    // sigma, and probe P1 showed NTFS does too, so neither side folds it.
    for (a, b) in [
        ("\u{212a}", "k"),          // Kelvin sign vs k
        ("\u{212b}", "\u{e5}"),     // Angstrom sign vs a-ring
        ("\u{130}", "i\u{307}"),    // dotted capital I
        ("\u{10400}", "\u{10428}"), // Deseret, non-BMP
    ] {
        let left = RelativePath::canonicalize(format!("ROMs/nes/{a}.nes")).expect("valid");
        let right = RelativePath::canonicalize(format!("ROMs/nes/{b}.nes")).expect("valid");
        assert_eq!(
            left.equivalence_key(),
            right.equivalence_key(),
            "expected {a:?} and {b:?} to fold together in the application key"
        );
    }

    // Distinct scripts must never fold: Cyrillic a is not Latin a.
    let cyrillic = accepted("ROMs/nes/\u{430}.nes");
    let latin = accepted("ROMs/nes/a.nes");
    assert_ne!(cyrillic.equivalence_key(), latin.equivalence_key());
}

#[test]
fn generic_profile_has_the_frozen_snapshot_identity() {
    let profile = DeviceProfile::generic_nes();

    assert_eq!(profile.id, "generic-folder");
    assert_eq!(profile.revision, 1);

    // `(id, revision)` identifies exactly one snapshot of behavior-bearing
    // fields. If this digest changes, the behaviour changed and the revision
    // must be bumped — a drifted digest is a build failure, not a warning.
    assert_eq!(
        profile.snapshot_digest(),
        "b59165c8a999456f1aed5a1f386f97b8cb24a92b1c834215d005dc21ddc376b9",
        "Generic profile behaviour changed without a revision bump"
    );
}

#[test]
fn profile_target_path_normalizes_and_confines() {
    let profile = DeviceProfile::generic_nes();

    let path = profile.target_path("tracers.nes").expect("accepted name");
    assert_eq!(path.as_str(), "ROMs/nes/tracers.nes");

    // NFD source names are normalized rather than refused.
    let nfd = profile.target_path("cafe\u{301}.nes").expect("normalizes");
    assert_eq!(nfd.as_str(), "ROMs/nes/caf\u{e9}.nes");

    // Extension matching is case-insensitive.
    assert!(profile.target_path("TRACERS.NES").is_ok());

    // Unaccepted extensions, separators, and reserved names never yield a path.
    assert!(profile.target_path("tracers.sfc").is_err());
    assert!(profile.target_path("sub/tracers.nes").is_err());
    assert!(profile.target_path("sub\\tracers.nes").is_err());
    assert!(profile.target_path("con.nes").is_err());
    assert!(profile.target_path("tracers.nes.").is_err());
}
