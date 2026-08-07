//! Header validation for CHD, CSO, and RVZ (issue #19, under #17).
//!
//! # Why these fixtures are built in code
//!
//! #17 requires "checked-in positive fixtures" for each accepted feature and "a
//! rejected fixture" for each exclusion. These are built by the constructors
//! below rather than committed as binary blobs, for two reasons.
//!
//! A real CHD or RVZ is a dump of a commercial disc, and this project cannot
//! redistribute one. More usefully, a committed blob is opaque: a reviewer
//! cannot see *why* a rejection fixture is invalid, so the test documents
//! nothing. `chd(.., parent: true)` says exactly what makes it unacceptable, at
//! the call site, and the reviewer can check that claim against the spec.
//!
//! Every constructor produces a header only. None contains game content, and
//! none is decodable — these prove the *gate*, not the decoder.

use rom_manager::{Outcome, ReasonCode, containers};

// ── CHD v5 ──────────────────────────────────────────────────────────────────

/// A structurally valid, self-contained CHD v5 header.
fn chd(codecs: [&[u8; 4]; 4], parent: bool) -> Vec<u8> {
    let mut bytes = vec![0u8; 124];
    bytes[0..8].copy_from_slice(b"MComprHD");
    bytes[8..12].copy_from_slice(&124u32.to_be_bytes());
    bytes[12..16].copy_from_slice(&5u32.to_be_bytes());
    for (slot, codec) in codecs.iter().enumerate() {
        bytes[16 + slot * 4..20 + slot * 4].copy_from_slice(*codec);
    }
    // 700 MiB of logical content — a plausible CD image.
    bytes[32..40].copy_from_slice(&(700u64 * 1024 * 1024).to_be_bytes());
    bytes[56..60].copy_from_slice(&19_584u32.to_be_bytes());
    bytes[60..64].copy_from_slice(&2_448u32.to_be_bytes());
    if parent {
        bytes[104] = 0x01;
    }
    bytes
}

#[test]
fn a_self_contained_chd_v5_with_accepted_codecs_passes() {
    let header = containers::validate_chd(&chd([b"cdlz", b"cdzl", b"cdfl", b"\0\0\0\0"], false))
        .expect("a valid CHD v5 header should pass");
    assert_eq!(header.logical_bytes, 700 * 1024 * 1024);
    assert_eq!(header.codecs, vec!["cdlz", "cdzl", "cdfl"]);
}

#[test]
fn a_chd_needing_a_parent_is_refused() {
    // Not corrupt — incomplete without a file this application cannot locate.
    // Importing it would produce a Library entry that can never materialize.
    let refusal =
        containers::validate_chd(&chd([b"cdlz", b"\0\0\0\0", b"\0\0\0\0", b"\0\0\0\0"], true))
            .expect_err("a parent-referencing CHD must be refused");
    assert_eq!(refusal.outcome, Outcome::Unsupported);
    assert_eq!(refusal.reason, ReasonCode::ParentReferenceRequired);
}

#[test]
fn an_unlisted_chd_codec_is_refused_rather_than_attempted() {
    // AVHUFF is real and decodable by the library. It is still refused,
    // because #17 makes the accepted set a declared list rather than whatever
    // the dependency happens to support.
    let refusal = containers::validate_chd(&chd(
        [b"avhu", b"\0\0\0\0", b"\0\0\0\0", b"\0\0\0\0"],
        false,
    ))
    .expect_err("AVHUFF must be refused");
    assert_eq!(refusal.outcome, Outcome::Unsupported);
    assert_eq!(refusal.reason, ReasonCode::UnsupportedMethod);
}

#[test]
fn an_older_chd_version_is_refused() {
    let mut bytes = chd([b"zlib", b"\0\0\0\0", b"\0\0\0\0", b"\0\0\0\0"], false);
    bytes[12..16].copy_from_slice(&4u32.to_be_bytes());
    let refusal = containers::validate_chd(&bytes).expect_err("CHD v4 must be refused");
    assert_eq!(refusal.reason, ReasonCode::UnsupportedVersion);
}

#[test]
fn a_chd_whose_magic_is_wrong_is_invalid_not_unsupported() {
    // The extension promised CHD and the bytes are not CHD. That is a broken
    // file, and the user should be told so.
    let mut bytes = chd([b"zlib", b"\0\0\0\0", b"\0\0\0\0", b"\0\0\0\0"], false);
    bytes[0] = b'X';
    let refusal = containers::validate_chd(&bytes).expect_err("bad magic must be refused");
    assert_eq!(refusal.outcome, Outcome::Invalid);
    assert_eq!(refusal.reason, ReasonCode::SignatureMismatch);
}

// ── CSO v1 ──────────────────────────────────────────────────────────────────

/// A structurally valid CSO v1 with a monotonic in-bounds index.
fn cso(version: u8, align: u8, block_bytes: u32, blocks: u64) -> Vec<u8> {
    let uncompressed = blocks * u64::from(block_bytes);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"CISO");
    bytes.extend_from_slice(&24u32.to_le_bytes());
    bytes.extend_from_slice(&uncompressed.to_le_bytes());
    bytes.extend_from_slice(&block_bytes.to_le_bytes());
    bytes.push(version);
    bytes.push(align);
    bytes.extend_from_slice(&0u16.to_le_bytes());

    let index_end = 24 + (blocks as usize + 1) * 4;
    // Every block is one byte long and stored in order, which is enough for the
    // geometry check and keeps the fixture tiny.
    for entry in 0..=blocks {
        let position = (index_end as u32) + entry as u32;
        bytes.extend_from_slice(&(position >> align).to_le_bytes());
    }
    bytes.resize(index_end + blocks as usize + 8, 0);
    bytes
}

#[test]
fn a_valid_cso_v1_passes() {
    let header = containers::validate_cso(&cso(1, 0, 2048, 4)).expect("valid CSO v1");
    assert_eq!(header.logical_bytes, 4 * 2048);
}

#[test]
fn cso_v2_and_its_relatives_are_refused() {
    // ZISO, DAX, JSO, and CSO v2 all arrive here as "not version 1".
    let refusal = containers::validate_cso(&cso(2, 0, 2048, 4)).expect_err("CSO v2 is excluded");
    assert_eq!(refusal.outcome, Outcome::Unsupported);
    assert_eq!(refusal.reason, ReasonCode::UnsupportedVersion);
}

#[test]
fn a_cso_block_size_other_than_2048_is_refused() {
    let refusal = containers::validate_cso(&cso(1, 0, 4096, 4)).expect_err("only 2048-byte blocks");
    assert_eq!(refusal.reason, ReasonCode::UnsupportedVersion);
}

#[test]
fn a_cso_alignment_shift_above_seven_is_refused() {
    let mut bytes = cso(1, 0, 2048, 4);
    bytes[21] = 8;
    let refusal = containers::validate_cso(&bytes).expect_err("alignment 0-7 only");
    assert_eq!(refusal.outcome, Outcome::Invalid);
}

#[test]
fn a_cso_index_pointing_past_the_file_is_refused() {
    // The whole point of the geometry check: a decoder that trusts a crafted
    // index reads wherever the index points.
    let mut bytes = cso(1, 0, 2048, 4);
    bytes[24..28].copy_from_slice(&(u32::MAX / 2).to_le_bytes());
    let refusal = containers::validate_cso(&bytes).expect_err("an out-of-bounds index is refused");
    assert_eq!(refusal.outcome, Outcome::Invalid);
    let measurement = refusal.measurement.expect("bounds are reported");
    assert!(
        measurement.observed > measurement.limit,
        "the diagnostic reports how far past the end it pointed"
    );
}

#[test]
fn a_cso_index_running_backwards_is_refused() {
    let mut bytes = cso(1, 0, 2048, 4);
    // Entry 1 lands before entry 0, which no legitimate writer produces.
    bytes[28..32].copy_from_slice(&0u32.to_le_bytes());
    let refusal = containers::validate_cso(&bytes).expect_err("a non-monotonic index is refused");
    assert_eq!(refusal.outcome, Outcome::Invalid);
}

// ── RVZ v1 ──────────────────────────────────────────────────────────────────

/// A structurally valid RVZ header plus the plain part of its disc structure.
fn rvz(disc_type: u32, compression: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; 0x48 + 0x10];
    bytes[0..4].copy_from_slice(b"RVZ\x01");
    bytes[4..8].copy_from_slice(&0x0103_0000u32.to_be_bytes());
    bytes[8..12].copy_from_slice(&0x0103_0000u32.to_be_bytes());
    bytes[12..16].copy_from_slice(&0xDCu32.to_be_bytes());
    // A single-layer Wii disc.
    bytes[0x24..0x2C].copy_from_slice(&4_699_979_776u64.to_be_bytes());
    bytes[0x48..0x4C].copy_from_slice(&disc_type.to_be_bytes());
    bytes[0x4C..0x50].copy_from_slice(&compression.to_be_bytes());
    bytes
}

#[test]
fn a_valid_wii_rvz_with_zstd_passes() {
    let header = containers::validate_rvz(&rvz(2, 5)).expect("valid RVZ");
    assert_eq!(header.codecs, vec!["zstd"]);
    assert_eq!(header.logical_bytes, 4_699_979_776);
}

#[test]
fn every_accepted_rvz_method_passes() {
    for (code, name) in [
        (0, "none"),
        (1, "purge"),
        (2, "bzip2"),
        (3, "lzma"),
        (4, "lzma2"),
        (5, "zstd"),
    ] {
        let header = containers::validate_rvz(&rvz(1, code))
            .unwrap_or_else(|_| panic!("{name} is an accepted RVZ method"));
        assert_eq!(header.codecs, vec![name]);
    }
}

#[test]
fn an_unknown_rvz_method_is_refused() {
    let refusal = containers::validate_rvz(&rvz(1, 9)).expect_err("unknown method");
    assert_eq!(refusal.outcome, Outcome::Unsupported);
    assert_eq!(refusal.reason, ReasonCode::UnsupportedMethod);
}

#[test]
fn an_rvz_declaring_no_disc_is_invalid() {
    let refusal = containers::validate_rvz(&rvz(0, 5)).expect_err("disc type 0 is malformed");
    assert_eq!(refusal.outcome, Outcome::Invalid);
}

#[test]
fn an_rvz_needing_a_newer_reader_is_refused() {
    let mut bytes = rvz(2, 5);
    // A compatibility floor above what this release implements.
    bytes[8..12].copy_from_slice(&0x0200_0000u32.to_be_bytes());
    let refusal = containers::validate_rvz(&bytes).expect_err("future RVZ is refused");
    assert_eq!(refusal.reason, ReasonCode::UnsupportedVersion);
}

// ── Extension and signature must agree ──────────────────────────────────────

#[test]
fn a_recognized_format_under_the_wrong_extension_is_unsupported_not_invalid() {
    // The file is fine. Telling someone their good CHD is corrupt because they
    // renamed it would send them to re-dump a disc for no reason.
    let bytes = chd([b"cdlz", b"\0\0\0\0", b"\0\0\0\0", b"\0\0\0\0"], false);
    let refusal = containers::validate("bin", &bytes).expect_err("a .bin is never inferred");
    assert_eq!(refusal.outcome, Outcome::Unsupported);
    assert_eq!(refusal.reason, ReasonCode::SignatureMismatch);
}

#[test]
fn an_unknown_extension_holding_unknown_bytes_is_unsupported() {
    let refusal = containers::validate("bin", b"not a container at all").expect_err("unknown");
    assert_eq!(refusal.reason, ReasonCode::UnknownExtension);
}

#[test]
fn every_refusal_carries_a_remediation_the_user_can_act_on() {
    let refusals = [
        containers::validate_chd(&chd(
            [b"avhu", b"\0\0\0\0", b"\0\0\0\0", b"\0\0\0\0"],
            false,
        ))
        .unwrap_err(),
        containers::validate_cso(&cso(2, 0, 2048, 4)).unwrap_err(),
        containers::validate_rvz(&rvz(1, 9)).unwrap_err(),
    ];
    for refusal in refusals {
        assert!(
            refusal.remediation().len() > 20,
            "a reason code with no remedy leaves the user stuck: {:?}",
            refusal.reason
        );
        assert!(
            refusal.outcome.blames_the_input(),
            "an unsupported input is the input's shape, not our failure"
        );
    }
}
