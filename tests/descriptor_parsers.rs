//! Coverage for bounded descriptor parsing (issue #19, under #17).
//!
//! Fixtures are structurally valid and contain no game content — a CUE is text
//! naming files, so proving the parser needs the structure, never the bytes.

use rom_manager::{DescriptorError, membership_is_complete, parse_cue, parse_gdi, parse_m3u};

const SINGLE: &str = include_str!("../fixtures/descriptors/single-track.cue");
const MULTI: &str = include_str!("../fixtures/descriptors/multi-track.cue");
const UNQUOTED: &str = include_str!("../fixtures/descriptors/unquoted.cue");
const ESCAPING: &str = include_str!("../fixtures/descriptors/escaping.cue");
const ABSOLUTE: &str = include_str!("../fixtures/descriptors/absolute.cue");
const NO_FILES: &str = include_str!("../fixtures/descriptors/no-files.cue");
const UNTERMINATED: &str = include_str!("../fixtures/descriptors/unterminated.cue");
const TWO_DISC: &str = include_str!("../fixtures/descriptors/two-disc.m3u");
const ESCAPING_M3U: &str = include_str!("../fixtures/descriptors/escaping.m3u");
const GDI: &str = include_str!("../fixtures/descriptors/dreamcast.gdi");

fn names(members: Vec<rom_manager::MemberReference>) -> Vec<String> {
    members
        .into_iter()
        .map(|member| member.as_str().to_owned())
        .collect()
}

#[test]
fn a_single_track_sheet_names_its_file() {
    assert_eq!(names(parse_cue(SINGLE).unwrap()), vec!["track01.bin"]);
}

#[test]
fn tracks_are_returned_in_order() {
    assert_eq!(
        names(parse_cue(MULTI).unwrap()),
        vec!["track01.bin", "track02.bin", "track03.bin"]
    );
}

#[test]
fn an_unquoted_file_name_is_read() {
    // Real sheets contain them.
    assert_eq!(names(parse_cue(UNQUOTED).unwrap()), vec!["track01.bin"]);
}

#[test]
fn a_reference_climbing_out_of_the_root_is_refused() {
    // A CUE naming ../../secrets.bin is not a track. Resolving it would let a
    // downloaded file choose what the application reads.
    assert!(matches!(
        parse_cue(ESCAPING),
        Err(DescriptorError::EscapingReference(_))
    ));
    assert!(matches!(
        parse_cue(ABSOLUTE),
        Err(DescriptorError::EscapingReference(_))
    ));
    assert!(matches!(
        parse_m3u(ESCAPING_M3U),
        Err(DescriptorError::EscapingReference(_))
    ));
}

#[test]
fn a_sheet_naming_nothing_is_incomplete_never_complete_and_empty() {
    // The distinction #17 exists for: opaque content must never be presented
    // as a complete verified set.
    assert!(matches!(
        parse_cue(NO_FILES),
        Err(DescriptorError::NoMembers)
    ));
}

#[test]
fn a_malformed_file_line_is_reported() {
    assert!(matches!(
        parse_cue(UNTERMINATED),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn a_playlist_skips_comments_and_blank_lines() {
    assert_eq!(
        names(parse_m3u(TWO_DISC).unwrap()),
        vec!["disc1.chd", "disc2.chd"]
    );
}

#[test]
fn a_gdi_skips_its_leading_track_count() {
    assert_eq!(
        names(parse_gdi(GDI).unwrap()),
        vec!["track01.bin", "track02.raw", "track03.bin"]
    );
}

#[test]
fn oversized_input_is_refused_before_it_is_parsed() {
    // A malformed or hostile file must not be able to exhaust memory.
    let huge = format!("FILE \"a.bin\" BINARY\n{}", "REM padding\n".repeat(500_000));
    assert!(matches!(parse_cue(&huge), Err(DescriptorError::TooLarge)));

    let many_lines = "REM x\n".repeat(5_000);
    assert!(matches!(
        parse_cue(&many_lines),
        Err(DescriptorError::TooManyEntries)
    ));
}

#[test]
fn too_many_members_is_refused() {
    let many = "FILE \"t.bin\" BINARY\n".repeat(600);
    assert!(matches!(
        parse_cue(&many),
        Err(DescriptorError::TooManyEntries)
    ));
}

#[test]
fn membership_completeness_is_checked_against_what_is_present() {
    let members = parse_cue(MULTI).unwrap();

    assert!(
        membership_is_complete(
            &members,
            &[
                "track01.bin".into(),
                "track02.bin".into(),
                "track03.bin".into()
            ]
        )
        .is_ok()
    );

    let missing = membership_is_complete(&members, &["track01.bin".into(), "track02.bin".into()]);
    assert!(missing.is_err(), "a missing track makes the set incomplete");
}

#[test]
fn nothing_in_the_fixtures_is_game_content() {
    // A guard against someone later checking in a real dump beside these.
    for fixture in [SINGLE, MULTI, UNQUOTED, TWO_DISC, GDI] {
        assert!(
            fixture.is_ascii(),
            "descriptors are plain text; binary here would mean a dump crept in"
        );
    }
}
