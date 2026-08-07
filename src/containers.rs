//! Header validation for CHD, CSO, and RVZ (issue #19, under #17).
//!
//! # Signature *and* extension, never either alone
//!
//! #17: "Binary formats require both the expected extension and matching
//! signature/version. A known extension with the wrong signature is invalid; a
//! recognized signature under the wrong extension is unsupported."
//!
//! Those two clauses point in opposite directions on purpose. A `.chd` holding
//! something else is a broken file, and the user should be told so. A CHD that
//! someone renamed to `.bin` is a *fine* file this release will not guess
//! about, because guessing is how a Mega Drive ROM and a PlayStation track —
//! both bare `.bin` — become the same thing.
//!
//! # What this module does and does not establish
//!
//! It validates structure: magic, version, codecs, geometry, declared bounds.
//! It does **not** decode, and #17 requires full decoding with recomputed
//! hashes before a set is `Complete`. So passing here earns a candidate the
//! right to be decoded, nothing more.
//!
//! That ordering matters for safety as much as correctness. Every rejection
//! below happens before a single byte is handed to a decompressor, so a file
//! declaring an unknown codec or a parent it does not carry is refused while it
//! is still inert.

use crate::{
    manifest::{self, CHD_VERSION},
    outcomes::{Diagnostic, Location, Outcome, ReasonCode},
};

/// A validated container header. Decoding may still fail; this only says the
/// structure is one this release accepts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerHeader {
    pub format: Format,
    /// Bytes of logical content the container claims to hold. Declared, not
    /// verified — the decode step checks it.
    pub logical_bytes: u64,
    pub codecs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    Chd,
    Cso,
    Rvz,
}

impl Format {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Chd => "chd",
            Self::Cso => "cso",
            Self::Rvz => "rvz",
        }
    }
}

fn invalid(reason: ReasonCode, format: Format) -> Diagnostic {
    Diagnostic::new(Outcome::Invalid, reason).for_format(format.extension())
}

fn unsupported(reason: ReasonCode, format: Format) -> Diagnostic {
    Diagnostic::new(Outcome::Unsupported, reason).for_format(format.extension())
}

fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn be_u64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn le_u64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// A CHD codec tag is a FourCC. Zero means "no compression" in a slot.
fn fourcc(value: u32) -> String {
    if value == 0 {
        return "none".to_owned();
    }
    value
        .to_be_bytes()
        .iter()
        .map(|byte| *byte as char)
        .collect()
}

const CHD_MAGIC: &[u8] = b"MComprHD";
const CHD_V5_HEADER_BYTES: u32 = 124;

/// Validates a CHD v5 header.
///
/// Rejects parent-referencing CHDs explicitly. A delta CHD is not corrupt —
/// it is simply incomplete without a parent this application has no way to
/// locate, and importing it would produce a Library entry that can never
/// materialize.
pub fn validate_chd(bytes: &[u8]) -> Result<ContainerHeader, Diagnostic> {
    let format = Format::Chd;
    if bytes.len() < 16 || !bytes.starts_with(CHD_MAGIC) {
        return Err(invalid(ReasonCode::SignatureMismatch, format));
    }
    let (Some(header_bytes), Some(version)) = (be_u32(bytes, 8), be_u32(bytes, 12)) else {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    };
    if version != CHD_VERSION {
        return Err(unsupported(ReasonCode::UnsupportedVersion, format)
            .measured(CHD_VERSION as u64, version as u64));
    }
    if header_bytes != CHD_V5_HEADER_BYTES {
        return Err(invalid(ReasonCode::MalformedStructure, format)
            .measured(CHD_V5_HEADER_BYTES as u64, header_bytes as u64));
    }
    if bytes.len() < CHD_V5_HEADER_BYTES as usize {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    }

    // Four codec slots. Unused trailing slots are zero; a zero *between* used
    // slots is still just "none", which the decoder handles.
    let mut codecs = Vec::new();
    for slot in 0..4 {
        let Some(raw) = be_u32(bytes, 16 + slot * 4) else {
            return Err(invalid(ReasonCode::MalformedStructure, format));
        };
        if raw == 0 {
            continue;
        }
        let tag = fourcc(raw);
        if !manifest::accepted(manifest::CHD_CODECS, &tag) {
            // AVHUFF and anything unrecognized land here together: this
            // release accepts a declared list, not whatever the library grew.
            return Err(unsupported(ReasonCode::UnsupportedMethod, format));
        }
        codecs.push(tag);
    }

    let (Some(logical_bytes), Some(hunk_bytes), Some(unit_bytes)) =
        (be_u64(bytes, 32), be_u32(bytes, 56), be_u32(bytes, 60))
    else {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    };

    // A parent CHD stores its parent's SHA-1 here; a self-contained one leaves
    // it zero.
    let parent = bytes
        .get(104..124)
        .ok_or_else(|| invalid(ReasonCode::MalformedStructure, format))?;
    if parent.iter().any(|byte| *byte != 0) {
        return Err(unsupported(ReasonCode::ParentReferenceRequired, format));
    }

    if hunk_bytes == 0 || unit_bytes == 0 || hunk_bytes % unit_bytes != 0 {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    }
    if logical_bytes > manifest::LIMITS.max_decoded_member_bytes {
        return Err(
            Diagnostic::new(Outcome::LimitExceeded, ReasonCode::LimitExceeded)
                .for_format(format.extension())
                .measured(manifest::LIMITS.max_decoded_member_bytes, logical_bytes),
        );
    }

    Ok(ContainerHeader {
        format,
        logical_bytes,
        codecs,
    })
}

const CSO_MAGIC: &[u8] = b"CISO";
const CSO_HEADER_BYTES: u32 = 24;
const CSO_BLOCK_BYTES: u32 = 2048;

/// Validates a CSO v1 header and its index geometry.
///
/// #17 assigns CSO "a narrow validated v1 reader" rather than a dependency,
/// so the geometry checks here are the real gate: index entries must be
/// monotonic and land inside the file, because a decoder that trusts a
/// crafted index reads wherever the index points.
pub fn validate_cso(bytes: &[u8]) -> Result<ContainerHeader, Diagnostic> {
    let format = Format::Cso;
    if bytes.len() < CSO_HEADER_BYTES as usize || !bytes.starts_with(CSO_MAGIC) {
        return Err(invalid(ReasonCode::SignatureMismatch, format));
    }
    let (Some(header_bytes), Some(uncompressed), Some(block_bytes)) =
        (le_u32(bytes, 4), le_u64(bytes, 8), le_u32(bytes, 16))
    else {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    };
    let (Some(&version), Some(&align)) = (bytes.get(20), bytes.get(21)) else {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    };

    if version != 1 {
        // ZISO, DAX, JSO, and CSO v2 all arrive as "not version 1" and are all
        // excluded by #17.
        return Err(unsupported(ReasonCode::UnsupportedVersion, format).measured(1, version as u64));
    }
    if header_bytes != CSO_HEADER_BYTES {
        return Err(invalid(ReasonCode::MalformedStructure, format)
            .measured(CSO_HEADER_BYTES as u64, header_bytes as u64));
    }
    if block_bytes != CSO_BLOCK_BYTES {
        return Err(unsupported(ReasonCode::UnsupportedVersion, format)
            .measured(CSO_BLOCK_BYTES as u64, block_bytes as u64));
    }
    if align > 7 {
        return Err(invalid(ReasonCode::MalformedStructure, format).measured(7, align as u64));
    }
    if uncompressed == 0 || uncompressed % u64::from(block_bytes) != 0 {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    }
    if uncompressed > manifest::LIMITS.max_decoded_member_bytes {
        return Err(
            Diagnostic::new(Outcome::LimitExceeded, ReasonCode::LimitExceeded)
                .for_format(format.extension())
                .measured(manifest::LIMITS.max_decoded_member_bytes, uncompressed),
        );
    }

    // One index entry per block, plus a terminator giving the last block's end.
    let blocks = (uncompressed / u64::from(block_bytes)) as usize;
    let index_entries = blocks + 1;
    let index_end = CSO_HEADER_BYTES as usize + index_entries * 4;
    if bytes.len() < index_end {
        return Err(invalid(ReasonCode::MalformedStructure, format)
            .measured(index_end as u64, bytes.len() as u64));
    }

    let mut previous = 0u64;
    for entry in 0..index_entries {
        let at = CSO_HEADER_BYTES as usize + entry * 4;
        let Some(raw) = le_u32(bytes, at) else {
            return Err(invalid(ReasonCode::MalformedStructure, format));
        };
        // The high bit marks a stored-uncompressed block; the rest is a
        // shifted file position.
        let position = u64::from(raw & 0x7FFF_FFFF) << align;
        if position < previous {
            return Err(invalid(ReasonCode::MalformedStructure, format)
                .at(Location::default().at_byte(at as u64)));
        }
        if position > bytes.len() as u64 {
            return Err(invalid(ReasonCode::MalformedStructure, format)
                .at(Location::default().at_byte(at as u64))
                .measured(bytes.len() as u64, position));
        }
        previous = position;
    }

    Ok(ContainerHeader {
        format,
        logical_bytes: uncompressed,
        codecs: vec!["zlib".to_owned()],
    })
}

const RVZ_MAGIC: &[u8] = b"RVZ\x01";
const RVZ_HEADER_BYTES: usize = 0x48;
const RVZ_DISC_MINIMUM: usize = 0x10;

/// Validates an RVZ v1 file header and its disc structure.
///
/// The disc structure is stored plainly — it is covered by the header's own
/// hash — so the disc type and compression method are readable without
/// decoding anything.
pub fn validate_rvz(bytes: &[u8]) -> Result<ContainerHeader, Diagnostic> {
    let format = Format::Rvz;
    if bytes.len() < RVZ_HEADER_BYTES || !bytes.starts_with(RVZ_MAGIC) {
        return Err(invalid(ReasonCode::SignatureMismatch, format));
    }
    let (Some(version), Some(compatible), Some(disc_bytes)) =
        (be_u32(bytes, 4), be_u32(bytes, 8), be_u32(bytes, 12))
    else {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    };

    // `version_compatible` is the oldest reader that can still open the file.
    // A file whose *compatibility floor* is above what we implement is
    // unsupported even when its version number looks familiar.
    let implemented = 0x0103_0000;
    if compatible > implemented {
        return Err(unsupported(ReasonCode::UnsupportedVersion, format)
            .measured(implemented as u64, compatible as u64));
    }
    if version < compatible {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    }

    let Some(iso_size) = be_u64(bytes, 0x24) else {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    };
    if iso_size == 0 {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    }
    if iso_size > manifest::LIMITS.max_decoded_member_bytes {
        return Err(
            Diagnostic::new(Outcome::LimitExceeded, ReasonCode::LimitExceeded)
                .for_format(format.extension())
                .measured(manifest::LIMITS.max_decoded_member_bytes, iso_size),
        );
    }

    if (disc_bytes as usize) < RVZ_DISC_MINIMUM || bytes.len() < RVZ_HEADER_BYTES + RVZ_DISC_MINIMUM
    {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    }
    let (Some(disc_type), Some(compression)) = (
        be_u32(bytes, RVZ_HEADER_BYTES),
        be_u32(bytes, RVZ_HEADER_BYTES + 4),
    ) else {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    };

    // 1 is GameCube, 2 is Wii. 0 means the file declares no disc at all.
    if disc_type == 0 {
        return Err(invalid(ReasonCode::MalformedStructure, format));
    }
    if disc_type > 2 {
        return Err(unsupported(ReasonCode::UnsupportedVersion, format));
    }

    let method = match compression {
        0 => "none",
        1 => "purge",
        2 => "bzip2",
        3 => "lzma",
        4 => "lzma2",
        5 => "zstd",
        _ => return Err(unsupported(ReasonCode::UnsupportedMethod, format)),
    };
    if !manifest::accepted(manifest::RVZ_METHODS, method) {
        return Err(unsupported(ReasonCode::UnsupportedMethod, format));
    }

    Ok(ContainerHeader {
        format,
        logical_bytes: iso_size,
        codecs: vec![method.to_owned()],
    })
}

/// Dispatches on extension, then requires the signature to agree.
///
/// A recognized signature under an unexpected extension is reported
/// `Unsupported` rather than `Invalid`: the file is fine, and telling someone
/// their good CHD is corrupt because they renamed it would send them to
/// re-dump a disc for no reason.
pub fn validate(extension: &str, bytes: &[u8]) -> Result<ContainerHeader, Diagnostic> {
    match extension.to_ascii_lowercase().as_str() {
        "chd" => validate_chd(bytes),
        "cso" => validate_cso(bytes),
        "rvz" => validate_rvz(bytes),
        other => {
            let recognized = bytes.starts_with(CHD_MAGIC)
                || bytes.starts_with(CSO_MAGIC)
                || bytes.starts_with(RVZ_MAGIC);
            let reason = if recognized {
                ReasonCode::SignatureMismatch
            } else {
                ReasonCode::UnknownExtension
            };
            Err(Diagnostic::new(Outcome::Unsupported, reason).for_format(other.to_owned()))
        }
    }
}
