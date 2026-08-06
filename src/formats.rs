//! The first-release format compatibility matrix (issue #19, under #14).
//!
//! # Four states, and why the fourth matters
//!
//! Required, experimental, and unsupported are the obvious three. The fourth —
//! *rejected rather than attempted implicitly* — is the one that does work.
//!
//! A standalone `.bin` is the clearest case. It could be a PlayStation track, a
//! Sega Mega Drive ROM, or a firmware blob, and nothing about the file says
//! which. Guessing would produce a Library entry that looks complete and is
//! wrong. So a bare `.bin` is only ever accepted as a track a descriptor
//! *refers to*, never as a Platform in its own right.
//!
//! The same logic governs the unsupported list. Encrypted, nested,
//! multi-volume, and key-dependent inputs are excluded explicitly, because
//! "opaque transfer must not be presented as a complete verified ROM Set" —
//! copying bytes the application cannot inspect and calling it a game is the
//! failure mode this whole matrix exists to prevent.

use serde::{Deserialize, Serialize};

/// How well a representation is supported.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    /// Release-blocking: documented, tested, supported end to end.
    Required,
    /// Shipped with an explicit warning. Non-safety limitations do not block
    /// release, but the safety suite must still pass.
    Experimental,
    /// Deliberately excluded, and rejected rather than attempted.
    Unsupported,
}

/// How a ROM Set is physically represented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Representation {
    /// A single file that is the whole ROM.
    SingleFile,
    /// A descriptor naming tracks that must resolve within one constrained
    /// import root.
    DescriptorWithTracks,
    /// A playlist naming discs.
    Playlist,
    /// A container holding exactly one recognized playable set.
    Archive,
}

/// One accepted form for one Platform.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedForm {
    pub platform: &'static str,
    pub extension: &'static str,
    pub representation: Representation,
    pub support: Support,
    /// True when byte order is part of ROM identity, so two orderings of the
    /// same game are genuinely different content rather than the same content
    /// stored differently.
    pub byte_order_is_identity: bool,
}

/// The certified first-release baseline.
pub const BASELINE: &[AcceptedForm] = &[
    single("Nintendo Entertainment System", ".nes"),
    single("Super Nintendo", ".sfc"),
    single("Super Nintendo", ".smc"),
    single("Game Boy", ".gb"),
    single("Game Boy Color", ".gbc"),
    single("Game Boy Advance", ".gba"),
    // N64 dumps exist in three byte orders and they are not interchangeable.
    ordered("Nintendo 64", ".z64"),
    ordered("Nintendo 64", ".n64"),
    ordered("Nintendo 64", ".v64"),
    single("Nintendo DS", ".nds"),
    single("Sega Genesis", ".md"),
    single("Sega Genesis", ".gen"),
    descriptor("Sony PlayStation", ".cue"),
    single("Sony PlayStation", ".chd"),
    playlist("Sony PlayStation", ".m3u"),
    single("Sony PlayStation 2", ".iso"),
    single("Sony PlayStation 2", ".chd"),
    playlist("Sony PlayStation 2", ".m3u"),
    single("Sony PSP", ".iso"),
    single("Sony PSP", ".cso"),
    descriptor("Sega Saturn", ".cue"),
    single("Sega Saturn", ".chd"),
    playlist("Sega Saturn", ".m3u"),
    descriptor("Sega Dreamcast", ".gdi"),
    single("Sega Dreamcast", ".chd"),
    playlist("Sega Dreamcast", ".m3u"),
    single("Nintendo GameCube", ".iso"),
    single("Nintendo GameCube", ".rvz"),
    single("Nintendo Wii", ".iso"),
    single("Nintendo Wii", ".rvz"),
];

const fn single(platform: &'static str, extension: &'static str) -> AcceptedForm {
    AcceptedForm {
        platform,
        extension,
        representation: Representation::SingleFile,
        support: Support::Required,
        byte_order_is_identity: false,
    }
}

const fn ordered(platform: &'static str, extension: &'static str) -> AcceptedForm {
    AcceptedForm {
        byte_order_is_identity: true,
        ..single(platform, extension)
    }
}

const fn descriptor(platform: &'static str, extension: &'static str) -> AcceptedForm {
    AcceptedForm {
        representation: Representation::DescriptorWithTracks,
        ..single(platform, extension)
    }
}

const fn playlist(platform: &'static str, extension: &'static str) -> AcceptedForm {
    AcceptedForm {
        representation: Representation::Playlist,
        ..single(platform, extension)
    }
}

/// Inputs excluded on purpose, with the reason.
///
/// Recorded rather than merely absent: a reader asking "why won't it take my
/// RAR?" deserves an answer, and an explicit list is also what makes the
/// rejection testable.
pub const UNSUPPORTED: &[(&str, &str)] = &[
    (".rar", "not a bounded format this release inspects"),
    (".7z", "import-only and experimental; never a transfer form"),
    (
        ".ccd",
        "CCD/IMG/SUB is outside the constrained descriptor set",
    ),
    (
        ".ecm",
        "ECM requires decoding before identity can be established",
    ),
    (".pbp", "PBP packs multiple discs opaquely"),
    ("encrypted", "content the application cannot inspect"),
    (
        "nested-archive",
        "an archive within an archive is unbounded",
    ),
    (
        "multi-volume",
        "membership spans files the import root cannot bound",
    ),
    (
        "delta-chd",
        "parent-dependent CHD needs content this release does not track",
    ),
];

/// Whether a bare file with this extension may be identified as a Platform on
/// its own.
///
/// `.bin` is the case this exists for: it could be a PlayStation track, a Mega
/// Drive ROM, or firmware, and nothing in the file says which. Guessing would
/// produce a Library entry that looks complete and is wrong.
pub fn may_stand_alone(extension: &str) -> bool {
    !matches!(extension.to_lowercase().as_str(), ".bin" | ".img" | ".sub")
}

/// Looks up how a Platform-and-extension pair is supported.
pub fn support_for(platform: &str, extension: &str) -> Support {
    let extension = extension.to_lowercase();
    if UNSUPPORTED
        .iter()
        .any(|(name, _)| *name == extension.as_str())
    {
        return Support::Unsupported;
    }
    BASELINE
        .iter()
        .find(|form| form.platform == platform && form.extension == extension)
        .map(|form| form.support)
        .unwrap_or(Support::Unsupported)
}

/// Every accepted form for a Platform.
pub fn forms_for(platform: &str) -> Vec<&'static AcceptedForm> {
    BASELINE
        .iter()
        .filter(|form| form.platform == platform)
        .collect()
}

/// Whether a representation needs other files to be complete.
pub fn needs_members(representation: Representation) -> bool {
    matches!(
        representation,
        Representation::DescriptorWithTracks | Representation::Playlist
    )
}

/// Why a ROM Set is not complete.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Incompleteness {
    /// A descriptor names something that is not there.
    MissingMember(String),
    /// A reference points outside the constrained import root — refused rather
    /// than resolved, on the same footing as the target-path rules.
    EscapingReference(String),
    /// The descriptor itself could not be read.
    MalformedDescriptor(String),
}

/// Resolves a descriptor's members within one constrained import root.
///
/// A reference that escapes the root is refused, never followed: a CUE naming
/// `../../etc/passwd` is not a track, and resolving it would let a downloaded
/// file choose what the application reads.
pub fn resolve_members(
    referenced: &[String],
    present: &[String],
) -> Result<Vec<String>, Incompleteness> {
    let mut resolved = Vec::new();
    for reference in referenced {
        if reference.contains("..") || reference.starts_with('/') || reference.contains('\\') {
            return Err(Incompleteness::EscapingReference(reference.clone()));
        }
        if !present.contains(reference) {
            return Err(Incompleteness::MissingMember(reference.clone()));
        }
        resolved.push(reference.clone());
    }
    Ok(resolved)
}
