//! Coverage for the first-release compatibility matrix (issue #19, under #14).

use rom_manager::formats::support_for;
use rom_manager::{
    BASELINE, Incompleteness, Representation, Support, UNSUPPORTED, forms_for, may_stand_alone,
    needs_members, resolve_members,
};

#[test]
fn every_required_platform_is_present() {
    for platform in [
        "Nintendo Entertainment System",
        "Super Nintendo",
        "Game Boy",
        "Game Boy Color",
        "Game Boy Advance",
        "Nintendo 64",
        "Nintendo DS",
        "Sega Genesis",
        "Sony PlayStation",
        "Sony PlayStation 2",
        "Sony PSP",
        "Sega Saturn",
        "Sega Dreamcast",
        "Nintendo GameCube",
        "Nintendo Wii",
    ] {
        assert!(
            !forms_for(platform).is_empty(),
            "{platform} is in the certified baseline"
        );
    }
}

#[test]
fn n64_byte_order_is_part_of_identity() {
    // The three orderings are not interchangeable; treating them as one would
    // make two genuinely different dumps look like the same content.
    for extension in [".z64", ".n64", ".v64"] {
        let form = forms_for("Nintendo 64")
            .into_iter()
            .find(|form| form.extension == extension)
            .expect("present");
        assert!(
            form.byte_order_is_identity,
            "{extension} byte order must be identity-bearing"
        );
    }
}

#[test]
fn a_bare_bin_is_never_a_platform_on_its_own() {
    // It could be a PlayStation track, a Mega Drive ROM, or firmware. Guessing
    // would produce a Library entry that looks complete and is wrong.
    assert!(!may_stand_alone(".bin"));
    assert!(!may_stand_alone(".img"));
    assert!(!may_stand_alone(".sub"));

    // Things that genuinely identify themselves may.
    assert!(may_stand_alone(".nes"));
    assert!(may_stand_alone(".chd"));
}

#[test]
fn descriptors_and_playlists_need_their_members() {
    assert!(needs_members(Representation::DescriptorWithTracks));
    assert!(needs_members(Representation::Playlist));
    assert!(!needs_members(Representation::SingleFile));
}

#[test]
fn unsupported_inputs_are_listed_with_a_reason() {
    // A reader asking "why won't it take my RAR?" deserves an answer, and the
    // explicit list is what makes the rejection testable.
    assert!(!UNSUPPORTED.is_empty());
    for (name, reason) in UNSUPPORTED {
        assert!(!name.is_empty());
        assert!(reason.len() > 10, "{name} needs a real reason");
    }
    assert_eq!(
        support_for("Sony PlayStation", ".rar"),
        Support::Unsupported
    );
    assert_eq!(
        support_for("Sony PlayStation", ".pbp"),
        Support::Unsupported
    );
}

#[test]
fn an_unknown_pairing_is_unsupported_rather_than_assumed() {
    assert_eq!(support_for("Nintendo Switch", ".xci"), Support::Unsupported);
    // Right extension, wrong Platform: the pairing is what is certified.
    assert_eq!(
        support_for("Nintendo Entertainment System", ".gba"),
        Support::Unsupported
    );
    assert_eq!(
        support_for("Nintendo Entertainment System", ".nes"),
        Support::Required
    );
}

#[test]
fn a_descriptor_resolves_its_members_within_the_import_root() {
    let resolved = resolve_members(
        &["track01.bin".into(), "track02.bin".into()],
        &[
            "track01.bin".into(),
            "track02.bin".into(),
            "game.cue".into(),
        ],
    )
    .unwrap();

    assert_eq!(resolved.len(), 2);
}

#[test]
fn a_missing_member_makes_the_set_incomplete() {
    let error = resolve_members(
        &["track01.bin".into(), "track02.bin".into()],
        &["track01.bin".into()],
    )
    .unwrap_err();

    assert_eq!(error, Incompleteness::MissingMember("track02.bin".into()));
}

#[test]
fn an_escaping_reference_is_refused_never_resolved() {
    // A CUE naming ../../etc/passwd is not a track. Resolving it would let a
    // downloaded file choose what the application reads.
    for hostile in ["../outside.bin", "/etc/passwd", "..\\windows\\system32"] {
        let error = resolve_members(&[hostile.to_string()], &[hostile.to_string()]).unwrap_err();
        assert!(
            matches!(error, Incompleteness::EscapingReference(_)),
            "{hostile} must be refused"
        );
    }
}

#[test]
fn the_baseline_has_no_duplicate_pairings() {
    let mut pairs: Vec<(&str, &str)> = BASELINE
        .iter()
        .map(|form| (form.platform, form.extension))
        .collect();
    let before = pairs.len();
    pairs.sort_unstable();
    pairs.dedup();

    assert_eq!(pairs.len(), before, "a pairing is listed twice");
}

#[test]
fn multi_disc_platforms_all_accept_a_playlist() {
    // Disc-based systems need a way to say "these discs are one game".
    for platform in [
        "Sony PlayStation",
        "Sony PlayStation 2",
        "Sega Saturn",
        "Sega Dreamcast",
    ] {
        assert!(
            forms_for(platform)
                .iter()
                .any(|form| form.representation == Representation::Playlist),
            "{platform} needs a playlist form"
        );
    }
}
