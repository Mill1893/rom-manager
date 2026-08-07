//! Archive membership classification (issue #19, under #17).
//!
//! # One archive, one game, or nothing
//!
//! #17: "One archive may establish exactly one ROM Set… multiple sets or
//! unassigned non-sidecar members are ambiguous."
//!
//! The rule sounds restrictive until you consider what the alternative costs. An
//! archive holding two games has no honest answer to "which game is this?", and
//! every way of guessing is worse than asking: picking the largest member,
//! picking the first alphabetically, or picking the one whose name matches the
//! archive all produce a Library entry that is confidently wrong. Ambiguity is
//! reported so the user can split the archive, which takes them a minute and
//! costs them nothing.
//!
//! # Sidecars are ignored; everything else is not
//!
//! Real archives carry README files, box art, and checksum lists, and refusing
//! those would reject most of what people actually have. So a bounded set of
//! classes is ignored.
//!
//! The bound matters more than the list. A member is ignorable only when its
//! *signature* agrees with its extension, because "ignore anything named
//! `.txt`" is a rule an archive can exploit: name the second game `readme.txt`
//! and the ambiguity check never fires. So a `.png` that is not a PNG does not
//! become an ignorable image — it becomes an unclassified member, and the
//! archive is ambiguous.

use crate::{
    descriptors,
    manifest::{self, LIMITS},
    outcomes::{Diagnostic, Location, Outcome, ReasonCode},
};

/// What one archive member is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemberClass {
    /// Media this release recognizes as playable content.
    RomContent,
    /// A CUE, GDI, or M3U that defines a set.
    Descriptor,
    /// An ignorable sidecar whose signature agreed with its extension.
    Sidecar,
    /// Operating-system droppings, ignored wherever they appear.
    OsMetadata,
    /// A program. Never content, and its presence is not innocent.
    Executable,
    /// A ROM patch. Real, but not the ROM, and applying it is not this
    /// release's job.
    Patch,
    /// Another archive. #17 imports one container deep.
    NestedArchive,
    /// An extension this release does not recognize.
    Unknown,
    /// The extension and the bytes disagree.
    SignatureMismatch,
}

impl MemberClass {
    /// Whether a member of this class may be present without making the
    /// archive ambiguous.
    pub fn is_ignorable(&self) -> bool {
        matches!(self, Self::Sidecar | Self::OsMetadata)
    }
}

/// One member of an archive, as observed.
#[derive(Clone, Debug)]
pub struct Member {
    /// The normalized path inside the archive, already confined.
    pub path: String,
    pub size: u64,
    /// The first bytes, for signature validation. May be short for tiny files.
    pub magic: Vec<u8>,
}

/// What an archive turned out to hold.
#[derive(Clone, Debug)]
pub struct Assessment {
    pub outcome: Outcome,
    pub diagnostics: Vec<Diagnostic>,
    /// The members forming the one ROM Set, when there is exactly one.
    pub content: Vec<String>,
}

/// Media extensions this release recognizes as playable content.
///
/// A bare `.bin` is deliberately absent. It could be a PlayStation track, a
/// Mega Drive ROM, or a firmware blob, and nothing about the file says which —
/// so it is only ever accepted as a track a descriptor *refers to*.
const ROM_EXTENSIONS: &[&str] = &[
    "nes", "sfc", "smc", "gb", "gbc", "gba", "n64", "z64", "v64", "nds", "gg", "sms", "md", "gen",
    "32x", "iso", "chd", "cso", "rvz", "pbp", "gcm", "wbfs", "a26", "lnx", "ws", "wsc", "pce",
    "ngp", "ngc", "col", "int",
];

const DESCRIPTOR_EXTENSIONS: &[&str] = &["cue", "gdi", "m3u"];

fn extension(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Whether `magic` matches what `extension` promises.
///
/// Text-shaped sidecars have no signature, so they are checked for the one
/// thing that would prove they are *not* text: an embedded NUL. That is a weak
/// check by design — it exists to stop a renamed binary, not to validate
/// grammar.
fn signature_agrees(extension: &str, magic: &[u8]) -> bool {
    let starts = |prefix: &[u8]| magic.starts_with(prefix);
    match extension {
        "png" => starts(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => starts(b"\xFF\xD8\xFF"),
        "gif" => starts(b"GIF87a") || starts(b"GIF89a"),
        "bmp" => starts(b"BM"),
        "webp" => starts(b"RIFF") && magic.len() >= 12 && &magic[8..12] == b"WEBP",
        "pdf" => starts(b"%PDF-"),
        "rtf" => starts(b"{\\rtf"),
        // Text classes: any NUL means this is not the text it claims to be.
        "txt" | "nfo" | "md" | "sfv" | "md5" | "sha1" | "sha256" | "json" | "xml" | "yaml"
        | "yml" => !magic.contains(&0),
        _ => true,
    }
}

fn is_executable(magic: &[u8]) -> bool {
    magic.starts_with(b"MZ")
        || magic.starts_with(b"\x7FELF")
        || magic.starts_with(b"\xCF\xFA\xED\xFE")
}

fn is_archive(magic: &[u8]) -> bool {
    magic.starts_with(b"PK\x03\x04")
        || magic.starts_with(b"7z\xBC\xAF\x27\x1C")
        || magic.starts_with(b"Rar!")
        || magic.starts_with(b"\x1F\x8B")
        || magic.starts_with(b"\xFD7zXZ")
}

fn is_patch(magic: &[u8]) -> bool {
    magic.starts_with(b"PATCH")
        || magic.starts_with(b"BPS1")
        || magic.starts_with(b"UPS1")
        || magic.starts_with(b"\xD6\xC3\xC4")
}

/// Classifies one member by extension and signature together.
pub fn classify(path: &str, magic: &[u8]) -> MemberClass {
    if manifest::is_os_metadata(path) {
        return MemberClass::OsMetadata;
    }
    let extension = extension(path);

    // Content shape is checked before extension for the classes whose presence
    // is a problem regardless of what they are called. An executable renamed to
    // `.txt` is still an executable, and it is exactly the case a
    // name-only rule would wave through.
    if is_executable(magic) {
        return MemberClass::Executable;
    }
    if is_patch(magic) {
        return MemberClass::Patch;
    }
    if is_archive(magic) {
        return MemberClass::NestedArchive;
    }

    if DESCRIPTOR_EXTENSIONS.contains(&extension.as_str()) {
        return MemberClass::Descriptor;
    }
    if ROM_EXTENSIONS.contains(&extension.as_str()) {
        return MemberClass::RomContent;
    }
    // A `.bin` is content only when a descriptor claims it, which the caller
    // resolves. On its own it is unclassified rather than inferred.
    if manifest::is_sidecar_extension(&extension) {
        return if signature_agrees(&extension, magic) {
            MemberClass::Sidecar
        } else {
            MemberClass::SignatureMismatch
        };
    }
    MemberClass::Unknown
}

/// Decides what an archive's members amount to.
///
/// Returns the first blocking problem rather than every one: the user fixes the
/// archive and re-imports, and a list of twelve consequences of one stray file
/// is harder to act on than the one that matters.
pub fn assess(members: &[Member]) -> Assessment {
    let mut diagnostics = Vec::new();

    let refuse = |reason: ReasonCode, outcome: Outcome, path: &str| Assessment {
        outcome,
        diagnostics: vec![
            Diagnostic::new(outcome, reason).at(Location::default().within(path.to_owned())),
        ],
        content: Vec::new(),
    };

    if members.len() > LIMITS.max_archive_members {
        return Assessment {
            outcome: Outcome::LimitExceeded,
            diagnostics: vec![
                Diagnostic::new(Outcome::LimitExceeded, ReasonCode::LimitExceeded)
                    .measured(LIMITS.max_archive_members as u64, members.len() as u64),
            ],
            content: Vec::new(),
        };
    }

    // Two members whose names differ only by case would resolve to one file on
    // a case-insensitive host, so the set they describe is not well defined.
    let mut folded: Vec<String> = Vec::new();
    for member in members {
        let key = member.path.to_lowercase();
        if folded.contains(&key) {
            return refuse(
                ReasonCode::DuplicateNormalizedPath,
                Outcome::Invalid,
                &member.path,
            );
        }
        if member.path.len() > LIMITS.max_normalized_path_bytes {
            return refuse(
                ReasonCode::LimitExceeded,
                Outcome::LimitExceeded,
                &member.path,
            );
        }
        folded.push(key);
    }

    let mut roms = Vec::new();
    let mut descriptors_found = Vec::new();
    let mut sidecar_total: u64 = 0;

    for member in members {
        match classify(&member.path, &member.magic) {
            MemberClass::OsMetadata => {}
            MemberClass::Sidecar => {
                if member.size > LIMITS.max_sidecar_bytes {
                    return refuse(
                        ReasonCode::LimitExceeded,
                        Outcome::LimitExceeded,
                        &member.path,
                    );
                }
                sidecar_total += member.size;
                if sidecar_total > LIMITS.max_total_sidecar_bytes {
                    return refuse(
                        ReasonCode::LimitExceeded,
                        Outcome::LimitExceeded,
                        &member.path,
                    );
                }
            }
            MemberClass::RomContent => roms.push(member.path.clone()),
            MemberClass::Descriptor => descriptors_found.push(member.path.clone()),
            MemberClass::NestedArchive => {
                return refuse(
                    ReasonCode::NestedContainer,
                    Outcome::Unsupported,
                    &member.path,
                );
            }
            MemberClass::Executable | MemberClass::Patch => {
                return refuse(
                    ReasonCode::UnclassifiedMember,
                    Outcome::Ambiguous,
                    &member.path,
                );
            }
            MemberClass::SignatureMismatch => {
                return refuse(
                    ReasonCode::SignatureMismatch,
                    Outcome::Ambiguous,
                    &member.path,
                );
            }
            MemberClass::Unknown => {
                // A `.bin` is the common case here and is resolved below when a
                // descriptor claims it. Anything still unclaimed is ambiguous.
                if extension(&member.path) != "bin" {
                    return refuse(
                        ReasonCode::UnclassifiedMember,
                        Outcome::Ambiguous,
                        &member.path,
                    );
                }
            }
        }
    }

    let present: Vec<String> = members.iter().map(|member| member.path.clone()).collect();
    let unclaimed_bin: Vec<&String> = present
        .iter()
        .filter(|path| extension(path) == "bin")
        .collect();

    if descriptors_found.len() > 1 {
        return Assessment {
            outcome: Outcome::Ambiguous,
            diagnostics: vec![Diagnostic::new(
                Outcome::Ambiguous,
                ReasonCode::AmbiguousMembership,
            )],
            content: Vec::new(),
        };
    }

    if let Some(descriptor) = descriptors_found.first() {
        if !roms.is_empty() {
            // A descriptor and a standalone ROM are two sets, not one.
            return Assessment {
                outcome: Outcome::Ambiguous,
                diagnostics: vec![Diagnostic::new(
                    Outcome::Ambiguous,
                    ReasonCode::AmbiguousMembership,
                )],
                content: Vec::new(),
            };
        }
        let mut content = vec![descriptor.clone()];
        content.extend(unclaimed_bin.into_iter().cloned());
        return Assessment {
            outcome: Outcome::Complete,
            diagnostics,
            content,
        };
    }

    if !unclaimed_bin.is_empty() {
        // Bare tracks with nothing describing them. Standalone `.bin` is never
        // inferred, so this is not a set.
        return refuse(
            ReasonCode::UnclassifiedMember,
            Outcome::Ambiguous,
            unclaimed_bin[0],
        );
    }

    match roms.len() {
        0 => {
            diagnostics.push(Diagnostic::new(Outcome::Invalid, ReasonCode::NoMembers));
            Assessment {
                outcome: Outcome::Invalid,
                diagnostics,
                content: Vec::new(),
            }
        }
        1 => Assessment {
            outcome: Outcome::Complete,
            diagnostics,
            content: roms,
        },
        _ => Assessment {
            outcome: Outcome::Ambiguous,
            diagnostics: vec![Diagnostic::new(
                Outcome::Ambiguous,
                ReasonCode::AmbiguousMembership,
            )],
            content: Vec::new(),
        },
    }
}

/// Resolves a descriptor's references against what the archive actually holds.
///
/// A descriptor naming a file that is not present makes the set *incomplete*,
/// not invalid: the identification succeeded, and an explicit later scan can
/// supply the missing member.
pub fn resolve_descriptor(
    descriptor_text: &str,
    descriptor_path: &str,
    present: &[String],
) -> Assessment {
    let extension = extension(descriptor_path);
    let parsed = match extension.as_str() {
        "cue" => descriptors::parse_cue(descriptor_text),
        "gdi" => descriptors::parse_gdi(descriptor_text),
        "m3u" => descriptors::parse_m3u(descriptor_text),
        _ => {
            return Assessment {
                outcome: Outcome::Unsupported,
                diagnostics: vec![Diagnostic::new(
                    Outcome::Unsupported,
                    ReasonCode::UnknownExtension,
                )],
                content: Vec::new(),
            };
        }
    };

    let references = match parsed {
        Ok(references) => references,
        Err(descriptors::DescriptorError::EscapingReference(reference)) => {
            return Assessment {
                outcome: Outcome::Invalid,
                diagnostics: vec![
                    Diagnostic::new(Outcome::Invalid, ReasonCode::EscapingReference)
                        .at(Location::default().naming(reference)),
                ],
                content: Vec::new(),
            };
        }
        Err(_) => {
            return Assessment {
                outcome: Outcome::Invalid,
                diagnostics: vec![Diagnostic::new(
                    Outcome::Invalid,
                    ReasonCode::MalformedStructure,
                )],
                content: Vec::new(),
            };
        }
    };

    let mut missing = Vec::new();
    let mut content = vec![descriptor_path.to_owned()];
    for reference in &references {
        // References resolve beside the descriptor, so an archive that nests
        // its files under a folder still matches on the base name.
        let found = present
            .iter()
            .find(|path| path.rsplit('/').next().unwrap_or(path) == reference.as_str());
        match found {
            Some(path) => content.push(path.clone()),
            None => missing.push(reference.as_str().to_owned()),
        }
    }

    if missing.is_empty() {
        return Assessment {
            outcome: Outcome::Complete,
            diagnostics: Vec::new(),
            content,
        };
    }

    Assessment {
        outcome: Outcome::Incomplete,
        diagnostics: missing
            .into_iter()
            .map(|reference| {
                Diagnostic::new(Outcome::Incomplete, ReasonCode::MissingMember)
                    .at(Location::default().naming(reference))
            })
            .collect(),
        content,
    }
}
