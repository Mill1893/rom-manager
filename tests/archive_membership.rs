//! Archive membership classification (issue #19, under #17).
//!
//! Members are described rather than packed into real archives: classification
//! is a decision about names, sizes, and leading bytes, and a real ZIP would add
//! compression machinery without adding coverage. The ZIP reader itself is
//! covered by `archive_import.rs`.

use rom_manager::{Member, MemberClass, Outcome, ReasonCode, assess, classify, resolve_descriptor};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
const JPEG: &[u8] = b"\xFF\xD8\xFF\xE0\x00\x10JFIF";
const ZIP: &[u8] = b"PK\x03\x04\x14\x00\x00\x00";
const ELF: &[u8] = b"\x7FELF\x02\x01\x01\x00";
const PE: &[u8] = b"MZ\x90\x00\x03\x00\x00\x00";
const IPS: &[u8] = b"PATCH\x00\x00\x00";
const TEXT: &[u8] = b"a readme file\n";

fn member(path: &str, magic: &[u8]) -> Member {
    Member {
        path: path.to_owned(),
        size: magic.len() as u64,
        magic: magic.to_vec(),
    }
}

fn sized(path: &str, magic: &[u8], size: u64) -> Member {
    Member {
        path: path.to_owned(),
        size,
        magic: magic.to_vec(),
    }
}

// ── Classification ──────────────────────────────────────────────────────────

#[test]
fn recognized_media_is_content_and_descriptors_are_descriptors() {
    assert_eq!(classify("game.nes", b"NES\x1a"), MemberClass::RomContent);
    assert_eq!(classify("game.chd", b"MComprHD"), MemberClass::RomContent);
    assert_eq!(classify("game.cue", TEXT), MemberClass::Descriptor);
    assert_eq!(classify("game.m3u", TEXT), MemberClass::Descriptor);
}

#[test]
fn a_bare_bin_is_never_inferred_to_be_a_rom() {
    // It could be a PlayStation track, a Mega Drive ROM, or a firmware blob,
    // and nothing about the file says which.
    assert_eq!(
        classify("track01.bin", b"\x00\x01\x02\x03"),
        MemberClass::Unknown
    );
}

#[test]
fn well_formed_sidecars_are_ignorable() {
    for (path, magic) in [
        ("readme.txt", TEXT),
        ("box.png", PNG),
        ("cover.jpg", JPEG),
        ("hashes.sfv", TEXT),
        ("meta.json", TEXT),
    ] {
        let class = classify(path, magic);
        assert_eq!(class, MemberClass::Sidecar, "{path}");
        assert!(class.is_ignorable(), "{path}");
    }
}

#[test]
fn os_metadata_is_ignorable_wherever_it_appears() {
    for path in [".DS_Store", "sub/.DS_Store", "__MACOSX/x.bin", "._game.nes"] {
        assert_eq!(
            classify(path, b"\x00\x00"),
            MemberClass::OsMetadata,
            "{path}"
        );
    }
}

#[test]
fn a_sidecar_whose_signature_disagrees_is_not_ignorable() {
    // "Ignore anything named .txt" is a rule an archive can exploit: name the
    // second game readme.txt and the ambiguity check never fires.
    assert_eq!(
        classify("readme.txt", b"\x00\x01\x02"),
        MemberClass::SignatureMismatch
    );
    assert_eq!(classify("box.png", JPEG), MemberClass::SignatureMismatch);
}

#[test]
fn content_shape_beats_the_extension_for_dangerous_classes() {
    // An executable renamed to .txt is still an executable, and it is exactly
    // the case a name-only rule would wave through.
    assert_eq!(classify("readme.txt", ELF), MemberClass::Executable);
    assert_eq!(classify("cover.png", PE), MemberClass::Executable);
    assert_eq!(classify("notes.txt", ZIP), MemberClass::NestedArchive);
    assert_eq!(classify("readme.txt", IPS), MemberClass::Patch);
}

// ── Assessment ──────────────────────────────────────────────────────────────

#[test]
fn one_rom_beside_ordinary_sidecars_is_complete() {
    let assessment = assess(&[
        member("game.nes", b"NES\x1a"),
        member("readme.txt", TEXT),
        member("box.png", PNG),
        member(".DS_Store", b"\x00\x00"),
    ]);
    assert_eq!(assessment.outcome, Outcome::Complete);
    assert_eq!(assessment.content, vec!["game.nes"]);
}

#[test]
fn two_games_in_one_archive_are_ambiguous() {
    // There is no honest answer to "which game is this?", and every way of
    // guessing produces a Library entry that is confidently wrong.
    let assessment = assess(&[member("one.nes", b"NES\x1a"), member("two.nes", b"NES\x1a")]);
    assert_eq!(assessment.outcome, Outcome::Ambiguous);
    assert!(assessment.content.is_empty());
}

#[test]
fn a_descriptor_claims_the_bare_tracks_beside_it() {
    let assessment = assess(&[
        member("game.cue", TEXT),
        member("track01.bin", b"\x00\x01"),
        member("track02.bin", b"\x00\x01"),
    ]);
    assert_eq!(assessment.outcome, Outcome::Complete);
    assert_eq!(assessment.content.len(), 3);
}

#[test]
fn bare_tracks_with_nothing_describing_them_are_ambiguous() {
    let assessment = assess(&[member("track01.bin", b"\x00\x01")]);
    assert_eq!(assessment.outcome, Outcome::Ambiguous);
}

#[test]
fn a_descriptor_beside_a_standalone_rom_is_two_sets() {
    let assessment = assess(&[
        member("game.cue", TEXT),
        member("track01.bin", b"\x00"),
        member("other.nes", b"NES\x1a"),
    ]);
    assert_eq!(assessment.outcome, Outcome::Ambiguous);
}

#[test]
fn two_descriptors_are_ambiguous() {
    let assessment = assess(&[member("a.cue", TEXT), member("b.cue", TEXT)]);
    assert_eq!(assessment.outcome, Outcome::Ambiguous);
}

#[test]
fn a_nested_archive_is_unsupported_not_merely_ambiguous() {
    // #17 imports one container deep, and says so rather than guessing.
    let assessment = assess(&[member("game.nes", b"NES\x1a"), member("extra.zip", ZIP)]);
    assert_eq!(assessment.outcome, Outcome::Unsupported);
    assert_eq!(
        assessment.diagnostics[0].reason,
        ReasonCode::NestedContainer
    );
}

#[test]
fn an_executable_or_patch_makes_the_archive_ambiguous() {
    for magic in [ELF, PE, IPS] {
        let assessment = assess(&[member("game.nes", b"NES\x1a"), member("thing.dat", magic)]);
        assert_eq!(assessment.outcome, Outcome::Ambiguous);
    }
}

#[test]
fn two_members_differing_only_by_case_are_refused() {
    // They would resolve to one file on a case-insensitive host, so the set
    // they describe is not well defined.
    let assessment = assess(&[
        member("Game.nes", b"NES\x1a"),
        member("game.nes", b"NES\x1a"),
    ]);
    assert_eq!(assessment.outcome, Outcome::Invalid);
    assert_eq!(
        assessment.diagnostics[0].reason,
        ReasonCode::DuplicateNormalizedPath
    );
}

#[test]
fn an_archive_holding_nothing_recognizable_is_invalid() {
    let assessment = assess(&[member("readme.txt", TEXT)]);
    assert_eq!(assessment.outcome, Outcome::Invalid);
    assert_eq!(assessment.diagnostics[0].reason, ReasonCode::NoMembers);
}

#[test]
fn an_oversized_sidecar_is_refused() {
    let assessment = assess(&[
        member("game.nes", b"NES\x1a"),
        sized("huge.png", PNG, 65 * 1024 * 1024),
    ]);
    assert_eq!(assessment.outcome, Outcome::LimitExceeded);
}

#[test]
fn sidecars_are_bounded_in_aggregate_too() {
    // Nine 60 MiB images are each individually fine and together are not.
    let mut members = vec![member("game.nes", b"NES\x1a")];
    for index in 0..9 {
        members.push(sized(&format!("art{index}.png"), PNG, 60 * 1024 * 1024));
    }
    assert_eq!(assess(&members).outcome, Outcome::LimitExceeded);
}

#[test]
fn too_many_members_is_refused_before_anything_is_classified() {
    let members: Vec<Member> = (0..10_001)
        .map(|index| member(&format!("f{index}.txt"), TEXT))
        .collect();
    let assessment = assess(&members);
    assert_eq!(assessment.outcome, Outcome::LimitExceeded);
    let measurement = assessment.diagnostics[0]
        .measurement
        .expect("both sides of the ceiling are reported");
    assert_eq!(measurement.limit, 10_000);
    assert_eq!(measurement.observed, 10_001);
}

// ── Descriptor resolution ───────────────────────────────────────────────────

const CUE: &str = "\
FILE \"track01.bin\" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
FILE \"track02.bin\" BINARY
  TRACK 02 AUDIO
    INDEX 01 00:00:00
";

#[test]
fn a_descriptor_whose_references_are_all_present_is_complete() {
    let present = vec![
        "game.cue".to_owned(),
        "track01.bin".to_owned(),
        "track02.bin".to_owned(),
    ];
    let assessment = resolve_descriptor(CUE, "game.cue", &present);
    assert_eq!(assessment.outcome, Outcome::Complete);
    assert_eq!(assessment.content.len(), 3);
}

#[test]
fn a_missing_track_makes_the_set_incomplete_not_invalid() {
    // The identification succeeded. An explicit later scan can supply the
    // missing member, and discarding the set would lose that work.
    let present = vec!["game.cue".to_owned(), "track01.bin".to_owned()];
    let assessment = resolve_descriptor(CUE, "game.cue", &present);
    assert_eq!(assessment.outcome, Outcome::Incomplete);
    assert_eq!(assessment.diagnostics[0].reason, ReasonCode::MissingMember);
    assert_eq!(
        assessment.diagnostics[0].location.reference.as_deref(),
        Some("track02.bin"),
        "the diagnostic names which member is missing"
    );
}

#[test]
fn references_resolve_beside_the_descriptor_inside_a_folder() {
    // An archive that nests its files under a folder still matches on the base
    // name, because references are relative to the descriptor.
    let present = vec![
        "Game/game.cue".to_owned(),
        "Game/track01.bin".to_owned(),
        "Game/track02.bin".to_owned(),
    ];
    let assessment = resolve_descriptor(CUE, "Game/game.cue", &present);
    assert_eq!(assessment.outcome, Outcome::Complete);
}

#[test]
fn a_descriptor_reaching_outside_the_archive_is_invalid() {
    let escaping = "FILE \"../../etc/passwd\" BINARY\n TRACK 01 AUDIO\n  INDEX 01 00:00:00\n";
    let assessment = resolve_descriptor(escaping, "game.cue", &[]);
    assert_eq!(assessment.outcome, Outcome::Invalid);
    assert_eq!(
        assessment.diagnostics[0].reason,
        ReasonCode::EscapingReference
    );
}

#[test]
fn a_malformed_descriptor_is_invalid_never_an_empty_complete_set() {
    let assessment = resolve_descriptor("FILE \"a.bin\n", "game.cue", &[]);
    assert_eq!(assessment.outcome, Outcome::Invalid);
    assert!(!assessment.outcome.is_rom_pack_eligible());
}
