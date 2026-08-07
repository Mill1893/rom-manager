//! The identity-bearing descriptor models (issue #19, under #17).
//!
//! #17 makes the track model part of ROM identity — "identity is the ordered
//! file/track/index/gap/flag model plus logical track bytes" — so these tests
//! cover the structure a name-only parser would have thrown away.
//!
//! Everything here is plain text naming files. No game content is involved,
//! which is exactly why descriptor correctness can be proven honestly in a
//! public repository.

use rom_manager::{DescriptorError, Frames, parse_cue_sheet, parse_gdi_model, parse_m3u_for};

// ── CUE ─────────────────────────────────────────────────────────────────────

const MIXED_MODE: &str = "\
FILE \"game.bin\" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    PREGAP 00:02:00
    INDEX 00 05:20:00
    INDEX 01 05:22:00
    FLAGS DCP
";

#[test]
fn the_full_track_model_is_read_not_just_the_file_names() {
    let sheet = parse_cue_sheet(MIXED_MODE).expect("a valid mixed-mode sheet");
    assert_eq!(sheet.files.len(), 1);
    assert_eq!(sheet.tracks.len(), 2);

    let data = &sheet.tracks[0];
    assert_eq!(data.mode, "MODE1/2352");
    assert_eq!(data.index_one, Frames(0));
    assert_eq!(data.sector_bytes(), 2352);

    let audio = &sheet.tracks[1];
    assert_eq!(audio.mode, "AUDIO");
    // 5:20:00 is (5*60 + 20) * 75 frames.
    assert_eq!(audio.index_zero, Some(Frames(320 * 75)));
    assert_eq!(audio.index_one, Frames(322 * 75));
    assert_eq!(audio.pregap, Some(Frames(150)));
    assert_eq!(audio.flags, vec!["DCP"]);
}

#[test]
fn every_accepted_track_mode_parses() {
    for mode in [
        "AUDIO",
        "MODE1/2048",
        "MODE1/2352",
        "MODE2/2336",
        "MODE2/2352",
    ] {
        let text = format!("FILE \"a.bin\" BINARY\n TRACK 01 {mode}\n  INDEX 01 00:00:00\n");
        assert!(
            parse_cue_sheet(&text).is_ok(),
            "{mode} is an accepted track mode"
        );
    }
}

#[test]
fn a_cdg_or_cdi_track_mode_is_refused() {
    // #17 excludes these explicitly. They are not malformed — they are modes
    // this release will not claim to reproduce.
    for mode in ["CDG", "MODE2/2448", "CDI/2352"] {
        let text = format!("FILE \"a.bin\" BINARY\n TRACK 01 {mode}\n  INDEX 01 00:00:00\n");
        assert!(
            matches!(parse_cue_sheet(&text), Err(DescriptorError::Unsupported(_))),
            "{mode} must be refused"
        );
    }
}

#[test]
fn compressed_and_non_binary_audio_files_are_refused() {
    // Accepting these would mean claiming an identity that was never verified
    // byte-for-byte.
    for kind in ["WAVE", "MP3", "AIFF", "MOTOROLA"] {
        let text = format!("FILE \"a.wav\" {kind}\n TRACK 01 AUDIO\n  INDEX 01 00:00:00\n");
        assert!(
            matches!(parse_cue_sheet(&text), Err(DescriptorError::Unsupported(_))),
            "{kind} must be refused"
        );
    }
}

#[test]
fn a_cd_text_or_multi_session_sheet_is_refused() {
    let cdtext =
        "CDTEXTFILE \"disc.cdt\"\nFILE \"a.bin\" BINARY\n TRACK 01 AUDIO\n  INDEX 01 00:00:00\n";
    assert!(matches!(
        parse_cue_sheet(cdtext),
        Err(DescriptorError::Unsupported(_))
    ));

    let session = "FILE \"a.bin\" BINARY\n TRACK 01 AUDIO\n  INDEX 01 00:00:00\nSESSION 02\n";
    assert!(matches!(
        parse_cue_sheet(session),
        Err(DescriptorError::Unsupported(_))
    ));
}

#[test]
fn a_track_without_index_01_is_malformed() {
    // #17 requires exactly one INDEX 01. Without it, nothing says where the
    // track starts, so its extent cannot be computed at all.
    let text = "FILE \"a.bin\" BINARY\n TRACK 01 AUDIO\n  INDEX 00 00:00:00\n";
    assert!(matches!(
        parse_cue_sheet(text),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn a_duplicate_index_01_is_malformed() {
    let text = "FILE \"a.bin\" BINARY\n TRACK 01 AUDIO\n  INDEX 01 00:00:00\n  INDEX 01 00:02:00\n";
    assert!(matches!(
        parse_cue_sheet(text),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn an_unsupported_index_number_is_refused() {
    let text = "FILE \"a.bin\" BINARY\n TRACK 01 AUDIO\n  INDEX 01 00:00:00\n  INDEX 02 00:02:00\n";
    assert!(matches!(
        parse_cue_sheet(text),
        Err(DescriptorError::Unsupported(_))
    ));
}

#[test]
fn an_out_of_range_time_is_refused() {
    // 00:99:99 is arithmetically representable and is not a time on a disc.
    // Accepting it would place a track at an address that cannot exist.
    let text = "FILE \"a.bin\" BINARY\n TRACK 01 AUDIO\n  INDEX 01 00:99:99\n";
    assert!(matches!(
        parse_cue_sheet(text),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn non_contiguous_track_numbering_is_malformed() {
    // A gap means a track is missing from the sheet, which is a different
    // failure from a missing file and must not be silently renumbered.
    let text = "\
FILE \"a.bin\" BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 03 AUDIO
    INDEX 01 00:05:00
";
    assert!(matches!(
        parse_cue_sheet(text),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn overlapping_tracks_in_one_file_are_refused() {
    let text = "\
FILE \"a.bin\" BINARY
  TRACK 01 AUDIO
    INDEX 01 00:10:00
  TRACK 02 AUDIO
    INDEX 01 00:05:00
";
    assert!(matches!(
        parse_cue_sheet(text),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn tracks_in_separate_files_each_restart_the_clock() {
    // A track-per-file sheet is the common Redump layout. Comparing addresses
    // across a file boundary would reject every one of them.
    let text = "\
FILE \"t1.bin\" BINARY
  TRACK 01 AUDIO
    INDEX 01 00:10:00
FILE \"t2.bin\" BINARY
  TRACK 02 AUDIO
    INDEX 01 00:00:00
";
    let sheet = parse_cue_sheet(text).expect("a track-per-file sheet is ordinary");
    assert_eq!(sheet.files.len(), 2);
    assert_eq!(sheet.tracks[1].file, 1);
}

#[test]
fn bounded_metadata_is_permitted_without_becoming_identity() {
    let with = "\
REM GENRE Adventure
TITLE \"A Game\"
PERFORMER \"Someone\"
CATALOG 1234567890123
FILE \"a.bin\" BINARY
  TRACK 01 AUDIO
    ISRC ABCDE1234567
    INDEX 01 00:00:00
";
    let without = "FILE \"a.bin\" BINARY\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n";

    let a = parse_cue_sheet(with).expect("metadata is permitted");
    let b = parse_cue_sheet(without).expect("and so is its absence");
    assert_eq!(
        a, b,
        "metadata is read past, never recorded — two sheets differing only in \
         commentary describe the same disc"
    );
}

#[test]
fn an_unknown_behaviour_bearing_directive_is_refused() {
    let text = "FILE \"a.bin\" BINARY\n TRACK 01 AUDIO\n  INDEX 01 00:00:00\n  WOBBLE 3\n";
    assert!(matches!(
        parse_cue_sheet(text),
        Err(DescriptorError::Unsupported(_))
    ));
}

// ── GDI ─────────────────────────────────────────────────────────────────────

const GDI: &str = "\
3
1 0 4 2352 track01.bin 0
2 756 0 2352 track02.raw 0
3 45000 4 2352 track03.bin 0
";

#[test]
fn a_gdi_reads_its_full_record_model() {
    let gdi = parse_gdi_model(GDI).expect("a valid GDI");
    assert_eq!(gdi.records.len(), 3);
    assert_eq!(gdi.records[1].lba, 756);
    assert_eq!(gdi.records[1].control, 0);
    assert_eq!(gdi.records[2].file.as_str(), "track03.bin");
}

#[test]
fn a_gdi_declaring_more_tracks_than_it_lists_is_malformed() {
    // Trusting the records over the header would quietly turn a truncated file
    // into a valid-looking set.
    let text = "5\n1 0 4 2352 a.bin 0\n2 756 0 2352 b.raw 0\n";
    assert!(matches!(
        parse_gdi_model(text),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn a_gdi_control_other_than_audio_or_data_is_malformed() {
    let text = "1\n1 0 7 2352 a.bin 0\n";
    assert!(matches!(
        parse_gdi_model(text),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn a_gdi_audio_track_must_use_2352_byte_sectors() {
    let text = "1\n1 0 0 2048 a.raw 0\n";
    assert!(matches!(
        parse_gdi_model(text),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn a_gdi_data_track_accepts_both_sector_sizes() {
    for size in [2048, 2352] {
        let text = format!("1\n1 0 4 {size} a.bin 0\n");
        assert!(parse_gdi_model(&text).is_ok(), "{size} is valid for data");
    }
}

#[test]
fn gdi_addresses_must_increase() {
    let text = "2\n1 45000 4 2352 a.bin 0\n2 756 0 2352 b.raw 0\n";
    assert!(matches!(
        parse_gdi_model(text),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn two_gdi_tracks_may_not_share_one_file() {
    // Sharing would make both extents ambiguous.
    let text = "2\n1 0 4 2352 a.bin 0\n2 756 0 2352 a.bin 100\n";
    assert!(matches!(
        parse_gdi_model(text),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn a_gdi_track_count_outside_1_to_99_is_malformed() {
    assert!(parse_gdi_model("0\n").is_err());
    assert!(parse_gdi_model("100\n").is_err());
}

// ── M3U ─────────────────────────────────────────────────────────────────────

#[test]
fn a_playlist_accepts_its_platforms_disc_forms() {
    let cue = "disc1.cue\ndisc2.cue\n";
    assert!(parse_m3u_for(cue, "playstation").is_ok());

    let iso = "disc1.iso\ndisc2.iso\n";
    assert!(parse_m3u_for(iso, "playstation-2").is_ok());

    let gdi = "disc1.gdi\ndisc2.gdi\n";
    assert!(parse_m3u_for(gdi, "dreamcast").is_ok());
}

#[test]
fn a_playlist_refuses_forms_its_platform_does_not_use() {
    // A GDI is a Dreamcast descriptor. In a PlayStation playlist it is a sign
    // the user mixed up two games, not a disc to load.
    let text = "disc1.gdi\ndisc2.gdi\n";
    assert!(matches!(
        parse_m3u_for(text, "playstation"),
        Err(DescriptorError::Unsupported(_))
    ));
}

#[test]
fn a_nested_playlist_is_refused() {
    let text = "disc1.cue\ninner.m3u\n";
    assert!(matches!(
        parse_m3u_for(text, "playstation"),
        Err(DescriptorError::Unsupported(_))
    ));
}

#[test]
fn an_archive_reference_in_a_playlist_is_refused() {
    let text = "disc1.zip\ndisc2.zip\n";
    assert!(matches!(
        parse_m3u_for(text, "playstation"),
        Err(DescriptorError::Unsupported(_))
    ));
}

#[test]
fn a_single_disc_playlist_is_malformed() {
    // A playlist exists to order multiple discs. One entry means the user
    // meant to load the disc directly.
    let text = "disc1.cue\n";
    assert!(matches!(
        parse_m3u_for(text, "playstation"),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn a_repeated_disc_is_malformed() {
    let text = "disc1.cue\ndisc1.cue\n";
    assert!(matches!(
        parse_m3u_for(text, "playstation"),
        Err(DescriptorError::Malformed(_))
    ));
}

#[test]
fn a_url_in_a_playlist_is_refused_as_an_escape() {
    // A reference that resolves over the network is the sharpest form of
    // "the descriptor chooses what gets read".
    let text = "https://example.invalid/disc1.cue\ndisc2.cue\n";
    assert!(matches!(
        parse_m3u_for(text, "playstation"),
        Err(DescriptorError::EscapingReference(_))
    ));
}

#[test]
fn a_byte_order_mark_is_permitted_and_carries_no_meaning() {
    let text = "\u{feff}disc1.cue\ndisc2.cue\n";
    let members = parse_m3u_for(text, "playstation").expect("a BOM is permitted");
    assert_eq!(
        members[0].as_str(),
        "disc1.cue",
        "the mark is stripped, not folded into the first name"
    );
}

#[test]
fn playlists_are_not_accepted_for_platforms_that_do_not_use_them() {
    let text = "disc1.cue\ndisc2.cue\n";
    assert!(matches!(
        parse_m3u_for(text, "nintendo-entertainment-system"),
        Err(DescriptorError::Unsupported(_))
    ));
}
