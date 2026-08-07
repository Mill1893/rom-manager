//! Typed import outcomes and their diagnostics (issue #19, under #17).
//!
//! # Only one outcome is eligible, and it is the narrow one
//!
//! #17: "Only `complete` is ROM Pack eligible. No failure or unsupported input
//! may become an opaque complete ROM Set."
//!
//! Every other outcome — including the *successful* `Incomplete` — is
//! ineligible. That asymmetry is the whole design. A ROM Set that copies onto a
//! device and does not run is worse than one that never copied, because the
//! failure surfaces later, on the device, away from any explanation. So
//! eligibility is granted by exactly one variant and denied by construction
//! everywhere else, rather than being a flag someone can set.
//!
//! # A fault of ours is never reported as a fault of theirs
//!
//! #17 again: "a worker fault is never reported as malformed user input."
//!
//! This is a diagnostic honesty rule, and it is easy to violate by accident. A
//! decoder worker that runs out of memory, crashes, or is killed produces the
//! same *symptom* as a truncated file: no output. Reporting that as `Invalid`
//! tells the user their ROM is corrupt, which sends them to re-dump a disc that
//! was fine. `ParserFailure` says the application failed, which is true and
//! actionable.
//!
//! [`Outcome::blames_the_input`] draws that line explicitly so the distinction
//! is testable rather than a matter of care at each call site.

use serde::{Deserialize, Serialize};

/// What became of one import candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Structure valid, membership whole, every hash verified. The only
    /// ROM-Pack-eligible outcome.
    Complete,
    /// Identified exactly one expected ROM Set, but membership is missing. It
    /// enters the Library and can be completed later by an explicit rescan.
    Incomplete,
    /// Structurally fine, but nothing determines which Platform it is for.
    NeedsPlatform,
    /// More than one candidate set, or members that cannot be classified.
    Ambiguous,
    /// A form this release deliberately excludes.
    Unsupported,
    /// Malformed against the accepted grammar or signature.
    Invalid,
    /// A declared ceiling was crossed by streamed observation.
    LimitExceeded,
    /// The bytes could not be read at all.
    IoFailure,
    /// The application's own parser or decoder worker failed. Never the user's
    /// fault, and never reported as though it were.
    ParserFailure,
    /// Cancelled before a verdict was reached.
    Cancelled,
}

impl Outcome {
    /// Whether this outcome may enter a ROM Pack. True for exactly one variant.
    pub fn is_rom_pack_eligible(&self) -> bool {
        matches!(self, Self::Complete)
    }

    /// Whether the candidate is retained in the Library at all.
    ///
    /// `Incomplete` is kept deliberately: it is a real, identified ROM Set that
    /// is missing a piece, and discarding it would lose the identification work
    /// and leave the user nothing to add the missing member *to*.
    pub fn enters_the_library(&self) -> bool {
        matches!(self, Self::Complete | Self::Incomplete)
    }

    /// Whether this outcome attributes the failure to the user's input.
    ///
    /// `ParserFailure`, `IoFailure`, and `Cancelled` are ours or the host's,
    /// and must never be phrased as a defect in the file.
    pub fn blames_the_input(&self) -> bool {
        matches!(
            self,
            Self::Invalid
                | Self::Unsupported
                | Self::Ambiguous
                | Self::LimitExceeded
                | Self::NeedsPlatform
        )
    }
}

/// Where in a source a diagnostic applies. Every field is optional because a
/// signature mismatch has no line and a bad descriptor line has no track.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub source: Option<String>,
    pub member: Option<String>,
    pub reference: Option<String>,
    pub byte_offset: Option<u64>,
    pub line: Option<usize>,
    pub track: Option<u32>,
}

impl Location {
    pub fn in_source(source: impl Into<String>) -> Self {
        Self {
            source: Some(source.into()),
            ..Self::default()
        }
    }

    pub fn at_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }

    pub fn at_track(mut self, track: u32) -> Self {
        self.track = Some(track);
        self
    }

    pub fn at_byte(mut self, offset: u64) -> Self {
        self.byte_offset = Some(offset);
        self
    }

    pub fn naming(mut self, reference: impl Into<String>) -> Self {
        self.reference = Some(reference.into());
        self
    }

    pub fn within(mut self, member: impl Into<String>) -> Self {
        self.member = Some(member.into());
        self
    }
}

/// A ceiling and what was actually observed against it.
///
/// Both numbers are reported because "too large" without them is unactionable:
/// the user cannot tell whether they are marginally over or wildly over, and
/// therefore cannot tell whether the file is merely big or actually hostile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    pub limit: u64,
    pub observed: u64,
}

/// One diagnostic explaining an outcome.
///
/// The reason code is stable and machine-readable; the remediation is the
/// sentence a person acts on. Both are required — a code with no remedy leaves
/// the user stuck, and a remedy with no code cannot be tested or aggregated.
///
/// The remediation is a method rather than a field on purpose. It is fully
/// determined by the reason code, so storing it would be duplicated state that
/// can drift: nothing would stop a diagnostic carrying
/// `EncryptionPresent` alongside advice about track alignment.
///
/// [`Location`] is boxed because most diagnostics carry no location at all, and
/// an unboxed one makes every `Result` in the parsing path pay for the rare
/// case that does.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub outcome: Outcome,
    pub reason: ReasonCode,
    pub format: Option<String>,
    pub location: Box<Location>,
    pub measurement: Option<Measurement>,
}

impl Diagnostic {
    pub fn new(outcome: Outcome, reason: ReasonCode) -> Self {
        Self {
            outcome,
            reason,
            format: None,
            location: Box::default(),
            measurement: None,
        }
    }

    /// What the user can do about this diagnostic.
    pub fn remediation(&self) -> &'static str {
        self.reason.remediation()
    }

    pub fn for_format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }

    pub fn at(mut self, location: Location) -> Self {
        self.location = Box::new(location);
        self
    }

    pub fn measured(mut self, limit: u64, observed: u64) -> Self {
        self.measurement = Some(Measurement { limit, observed });
        self
    }
}

/// A stable, machine-readable reason. These strings are part of the
/// application's contract: they appear in reports and may be matched on, so
/// renaming one is a breaking change.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    SignatureMismatch,
    UnknownExtension,
    UnsupportedVersion,
    UnsupportedMethod,
    UnsupportedDirective,
    EncryptionPresent,
    ExternalKeyRequired,
    ParentReferenceRequired,
    NestedContainer,
    SplitVolume,
    TrailingPayload,
    DuplicateNormalizedPath,
    EscapingReference,
    MissingMember,
    NoMembers,
    AmbiguousMembership,
    UnclassifiedMember,
    ChecksumMismatch,
    MalformedStructure,
    TrackOverlap,
    TrackAlignment,
    NonMonotonicLba,
    PlatformUndetermined,
    LimitExceeded,
    RatioExceeded,
    ReadFailed,
    WorkerFailed,
    OperationCancelled,
}

impl ReasonCode {
    /// The stable wire spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SignatureMismatch => "signature_mismatch",
            Self::UnknownExtension => "unknown_extension",
            Self::UnsupportedVersion => "unsupported_version",
            Self::UnsupportedMethod => "unsupported_method",
            Self::UnsupportedDirective => "unsupported_directive",
            Self::EncryptionPresent => "encryption_present",
            Self::ExternalKeyRequired => "external_key_required",
            Self::ParentReferenceRequired => "parent_reference_required",
            Self::NestedContainer => "nested_container",
            Self::SplitVolume => "split_volume",
            Self::TrailingPayload => "trailing_payload",
            Self::DuplicateNormalizedPath => "duplicate_normalized_path",
            Self::EscapingReference => "escaping_reference",
            Self::MissingMember => "missing_member",
            Self::NoMembers => "no_members",
            Self::AmbiguousMembership => "ambiguous_membership",
            Self::UnclassifiedMember => "unclassified_member",
            Self::ChecksumMismatch => "checksum_mismatch",
            Self::MalformedStructure => "malformed_structure",
            Self::TrackOverlap => "track_overlap",
            Self::TrackAlignment => "track_alignment",
            Self::NonMonotonicLba => "non_monotonic_lba",
            Self::PlatformUndetermined => "platform_undetermined",
            Self::LimitExceeded => "limit_exceeded",
            Self::RatioExceeded => "ratio_exceeded",
            Self::ReadFailed => "read_failed",
            Self::WorkerFailed => "worker_failed",
            Self::OperationCancelled => "operation_cancelled",
        }
    }

    /// What the user can actually do about it.
    ///
    /// Written as an instruction rather than a restatement of the problem.
    /// "Unsupported version" tells someone nothing; "re-create it as CHD v5"
    /// tells them where to go next.
    pub fn remediation(&self) -> &'static str {
        match self {
            Self::SignatureMismatch => {
                "The file's contents do not match its extension. Confirm it is the format its name claims, and rename it only if you are certain."
            }
            Self::UnknownExtension => {
                "This release does not import this extension. Convert the content to an accepted form for its Platform."
            }
            Self::UnsupportedVersion => {
                "Re-create the file using a version this release accepts, listed in the compatibility manifest."
            }
            Self::UnsupportedMethod => {
                "Re-compress using an accepted method. The compatibility manifest lists them per format."
            }
            Self::UnsupportedDirective => {
                "Remove the unrecognized directive, or re-generate the descriptor with a standard tool."
            }
            Self::EncryptionPresent => {
                "Decrypt the archive before importing. This release does not accept passwords or encrypted members."
            }
            Self::ExternalKeyRequired => {
                "This file needs a key held outside it. Re-create it in a self-contained form."
            }
            Self::ParentReferenceRequired => {
                "This CHD is a delta against a parent. Re-create it as a self-contained CHD v5."
            }
            Self::NestedContainer => {
                "Extract the inner container first. This release imports one container deep."
            }
            Self::SplitVolume => "Rejoin the split volumes into a single archive before importing.",
            Self::TrailingPayload => {
                "The file has data after its declared end. Re-create it with a standard tool."
            }
            Self::DuplicateNormalizedPath => {
                "Two members resolve to the same name. Rename one so the set is unambiguous."
            }
            Self::EscapingReference => {
                "A reference points outside its folder. Move every referenced file beside the descriptor and use plain file names."
            }
            Self::MissingMember => {
                "A referenced file is missing. Place it beside the descriptor and scan again to complete the set."
            }
            Self::NoMembers => {
                "The descriptor references no usable files. Re-generate it from the content it should describe."
            }
            Self::AmbiguousMembership => {
                "More than one game appears here. Import one set per archive or folder."
            }
            Self::UnclassifiedMember => {
                "A member could not be classified. Remove unrelated files, or import the game on its own."
            }
            Self::ChecksumMismatch => {
                "Stored bytes do not match their recorded checksum. The file is damaged; obtain an intact copy."
            }
            Self::MalformedStructure => {
                "The structure does not parse against the accepted grammar. Re-create it with a standard tool."
            }
            Self::TrackOverlap => {
                "Two tracks claim the same bytes. Re-generate the descriptor from the media."
            }
            Self::TrackAlignment => {
                "A track is not aligned to its sector size. Re-generate the descriptor from the media."
            }
            Self::NonMonotonicLba => {
                "Track start addresses are out of order. Re-generate the descriptor from the media."
            }
            Self::PlatformUndetermined => {
                "Choose the Platform for this content, or place it in a Platform folder."
            }
            Self::LimitExceeded => {
                "The content exceeds a safety ceiling. Import it in smaller pieces if it is legitimate."
            }
            Self::RatioExceeded => {
                "The content expands far beyond its compressed size, which this release refuses. Verify its origin."
            }
            Self::ReadFailed => {
                "The file could not be read. Check the drive, cable, or permissions and try again."
            }
            Self::WorkerFailed => {
                "The application failed to process this file. This is a defect in ROM Manager, not in your file. Please report it."
            }
            Self::OperationCancelled => "Nothing was changed. Start the import again when ready.",
        }
    }
}
