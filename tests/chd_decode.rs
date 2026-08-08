//! Full CHD decoding with recomputed hashes (issue #19, under #17).
//!
//! # Why these CHDs are built here rather than committed
//!
//! `container_headers.rs` builds header fixtures in code and says why: a real
//! CHD is a dump of a commercial disc, which this project cannot redistribute,
//! and a committed blob is opaque to a reviewer anyway. The same holds with
//! more force here, because these fixtures must actually *decode*.
//!
//! A CHD is a container over arbitrary bytes. Nothing about it requires a disc,
//! so `chd_v5_uncompressed` packs content this repository already owns into a
//! real, openable, decodable CHD v5. Every field is set at the call site, so a
//! reviewer can see what makes a fixture valid — or, for the rejection cases,
//! exactly what makes it not.
//!
//! # What these establish that the header tests do not
//!
//! `container_headers.rs` proves the gate: the structure is one this release
//! accepts. It says at the top that it "does not decode". These decode, and
//! check the identity of what came out against the bytes that went in — which
//! is what #17 means by "fully decoded logical-content hashes".

use std::io::Cursor;

use rom_manager::{
    Outcome, ReasonCode,
    decode::decode_chd,
    worker::{Budget, Progress, Supervisor},
};
use sha2::{Digest, Sha256};

/// The v5 header is 124 bytes, and the uncompressed map is one big-endian u32
/// per hunk holding that hunk's offset *in units of the hunk size*.
const HEADER_BYTES: usize = 124;

/// Builds a real, openable CHD v5 holding `content`, uncompressed.
///
/// `declared_logical` exists so a test can state a size the file does not
/// actually carry; passing `None` means "tell the truth", which is what every
/// positive fixture does.
fn chd_v5_uncompressed(content: &[u8], hunk_bytes: u32, declared_logical: Option<u64>) -> Vec<u8> {
    let hunk_size = hunk_bytes as usize;
    let hunk_count = content.len().div_ceil(hunk_size);
    let logical = declared_logical.unwrap_or(content.len() as u64);

    let map_offset = HEADER_BYTES;
    let map_len = hunk_count * 4;
    // Hunk data starts at the next whole multiple of the hunk size, because a
    // map entry addresses hunks in hunk-size units and cannot express anything
    // finer.
    let data_offset = (map_offset + map_len).div_ceil(hunk_size) * hunk_size;

    let mut out = vec![0u8; data_offset + hunk_count * hunk_size];

    out[0..8].copy_from_slice(b"MComprHD");
    out[8..12].copy_from_slice(&(HEADER_BYTES as u32).to_be_bytes());
    out[12..16].copy_from_slice(&5u32.to_be_bytes());
    // compressors[4], all zero: CodecType::None, which is what selects the
    // 4-byte uncompressed map.
    out[32..40].copy_from_slice(&logical.to_be_bytes());
    out[40..48].copy_from_slice(&(map_offset as u64).to_be_bytes());
    out[48..56].copy_from_slice(&0u64.to_be_bytes()); // no metadata
    out[56..60].copy_from_slice(&hunk_bytes.to_be_bytes());
    out[60..64].copy_from_slice(&hunk_bytes.to_be_bytes()); // one unit per hunk
    // 64..84 raw SHA-1, 84..104 SHA-1, 104..124 parent SHA-1 — left zero.
    // The parent field being zero is what makes this self-contained.

    for hunk in 0..hunk_count {
        let entry = map_offset + hunk * 4;
        let unit = (data_offset + hunk * hunk_size) / hunk_size;
        out[entry..entry + 4].copy_from_slice(&(unit as u32).to_be_bytes());

        let start = hunk * hunk_size;
        let end = (start + hunk_size).min(content.len());
        let at = data_offset + start;
        out[at..at + (end - start)].copy_from_slice(&content[start..end]);
    }

    out
}

fn supervisor() -> Supervisor {
    Supervisor::new(Budget::default(), Progress::new())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn content() -> Vec<u8> {
    // Deterministic and not a disc: the point is that decoding reproduces it
    // byte for byte, and any content proves that equally well.
    (0..5000u32).map(|i| (i % 251) as u8).collect()
}

#[test]
fn a_decoded_chd_reports_the_hash_of_what_came_out() {
    let content = content();
    let image = chd_v5_uncompressed(&content, 1024, None);

    let decoded = decode_chd(Cursor::new(&image), &image, &mut supervisor())
        .expect("a well-formed uncompressed CHD decodes");

    assert_eq!(decoded.logical_bytes, content.len() as u64);
    assert_eq!(
        decoded.sha256,
        sha256(&content),
        "identity must come from the decoded bytes, not from the header"
    );
}

#[test]
fn the_hunk_size_does_not_change_the_identity() {
    // A packer's choice of hunk size is an encoding detail. #17: source
    // compression layout is not proof of equality, and it must not be proof of
    // *inequality* either.
    let content = content();
    let expected = sha256(&content);

    for hunk_bytes in [512u32, 1024, 4096] {
        let image = chd_v5_uncompressed(&content, hunk_bytes, None);
        let decoded = decode_chd(Cursor::new(&image), &image, &mut supervisor())
            .expect("every hunk size decodes");
        assert_eq!(
            decoded.sha256, expected,
            "hunk size {hunk_bytes} changed the identity"
        );
    }
}

#[test]
fn trailing_padding_in_the_last_hunk_is_not_content() {
    // 5000 bytes across 1024-byte hunks leaves 120 bytes of padding in the
    // final hunk. Hashing it would make identity depend on the padding, and two
    // CHDs of the same content at different hunk sizes would disagree.
    let content = content();
    assert_ne!(
        content.len() % 1024,
        0,
        "the fixture must actually be padded"
    );

    let image = chd_v5_uncompressed(&content, 1024, None);
    let decoded = decode_chd(Cursor::new(&image), &image, &mut supervisor()).unwrap();

    assert_eq!(decoded.logical_bytes, 5000);
    assert_eq!(decoded.sha256, sha256(&content));
}

#[test]
fn a_declared_size_larger_than_the_content_is_invalid() {
    // The header claims more content than the map can supply. Trusting the
    // declaration would admit a short file as a complete one.
    let content = content();
    let image = chd_v5_uncompressed(&content, 1024, Some(content.len() as u64 + 4096));

    let error = decode_chd(Cursor::new(&image), &image, &mut supervisor())
        .expect_err("a container that cannot supply what it declares is invalid");

    let diagnostic = error.diagnostic();
    assert_eq!(diagnostic.outcome, Outcome::Invalid);
    // Reported as a malformed file rather than a read fault: nothing is wrong
    // with the disk, and telling the user otherwise sends them to the wrong place.
    assert_eq!(diagnostic.reason, ReasonCode::MalformedStructure);
}

#[test]
fn a_truncated_file_is_invalid_rather_than_a_worker_fault() {
    // #17's attribution rule: this is the file being wrong, not the decoder
    // failing. It must not read as ParserFailure.
    let content = content();
    let mut image = chd_v5_uncompressed(&content, 1024, None);
    image.truncate(image.len() / 2);

    let error = decode_chd(Cursor::new(&image), &image, &mut supervisor())
        .expect_err("a truncated CHD cannot decode");

    assert_eq!(error.diagnostic().outcome, Outcome::Invalid);
}

#[test]
fn a_parent_referencing_chd_is_refused_before_any_decoding() {
    // The header gate already rejects these. Asserted here too because the
    // ordering is the safety property: a delta CHD must never reach a
    // decompressor while looking for a parent this application cannot locate.
    let content = content();
    let mut image = chd_v5_uncompressed(&content, 1024, None);
    image[104] = 0x01; // a non-zero parent SHA-1

    let error = decode_chd(Cursor::new(&image), &image, &mut supervisor())
        .expect_err("a parent-referencing CHD is refused");

    assert_eq!(
        error.diagnostic().reason,
        ReasonCode::ParentReferenceRequired
    );
}
