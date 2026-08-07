//! The compatibility manifest (issue #19, under #17).
//!
//! # Why the accepted subset is written down rather than inherited
//!
//! #17 is explicit that "accepted behavior is an explicit, versioned
//! compatibility subset. A parser dependency gaining support does not silently
//! expand it."
//!
//! That sentence is the reason this file exists. If the accepted set were
//! whatever the parsing libraries happened to handle, then upgrading a
//! dependency would silently widen what the application imports — and the new
//! surface would arrive untested, unfixtured, and unreviewed. A ZIP crate that
//! starts supporting a new compression method would change what this
//! application accepts without anyone deciding that it should.
//!
//! So the subset is declared here, checked in, and enforced against *observed*
//! input. The libraries are asked to parse; they are never asked what is
//! allowed.
//!
//! # Limits describe what was streamed, not what was claimed
//!
//! Every ceiling below is checked against actual streamed usage rather than
//! declared metadata. A ZIP header can claim any uncompressed size it likes;
//! the number that matters is how many bytes actually came out. Trusting the
//! declaration is how a decompression bomb gets waved through — the header says
//! 4 KiB, the stream delivers 40 GiB, and the check that "passed" measured the
//! wrong thing.

use serde::{Deserialize, Serialize};

/// The manifest revision. Any change to an accepted version, method,
/// directive, or limit below requires bumping this and re-running the complete
/// fixture suite.
pub const MANIFEST_REVISION: u32 = 1;

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const GIB: u64 = 1024 * MIB;

/// Resource ceilings for import. Enforced against streamed observation.
///
/// These are deliberately generous for real content and nowhere near enough to
/// exhaust a host: a 128 GiB source covers the largest legitimate disc images,
/// while the compression-ratio ceiling stops a few kilobytes from expanding
/// into a full disk.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    pub max_physical_source_bytes: u64,
    pub max_archive_members: usize,
    pub max_normalized_path_bytes: usize,
    pub max_path_component_bytes: usize,
    pub max_decoded_member_bytes: u64,
    pub max_decoded_archive_bytes: u64,
    /// Applied only after this much compressed input has been read, so that
    /// small highly-compressible members are not rejected for arithmetic that
    /// is meaningless at low volume.
    pub compression_ratio_grace_bytes: u64,
    pub max_compression_ratio: u64,
    pub max_descriptor_bytes: u64,
    pub max_descriptor_lines: usize,
    pub max_descriptor_references: usize,
    pub max_graph_nodes: usize,
    pub max_worker_memory_bytes: u64,
    pub candidate_deadline_seconds: u64,
    pub no_progress_deadline_seconds: u64,
    pub max_temporary_bytes: u64,
    pub temporary_free_space_reserve_bytes: u64,
    pub max_sidecar_bytes: u64,
    pub max_total_sidecar_bytes: u64,
}

/// The first-release limits, exactly as settled in #17.
pub const LIMITS: Limits = Limits {
    max_physical_source_bytes: 128 * GIB,
    max_archive_members: 10_000,
    max_normalized_path_bytes: 1024,
    max_path_component_bytes: 255,
    max_decoded_member_bytes: 32 * GIB,
    max_decoded_archive_bytes: 128 * GIB,
    compression_ratio_grace_bytes: MIB,
    max_compression_ratio: 10_000,
    max_descriptor_bytes: MIB,
    max_descriptor_lines: 10_000,
    max_descriptor_references: 1024,
    max_graph_nodes: 1024,
    max_worker_memory_bytes: GIB,
    candidate_deadline_seconds: 30 * 60,
    no_progress_deadline_seconds: 60,
    max_temporary_bytes: 128 * GIB,
    temporary_free_space_reserve_bytes: 10 * GIB,
    max_sidecar_bytes: 64 * MIB,
    max_total_sidecar_bytes: 512 * MIB,
};

impl Limits {
    /// How much temporary space import may use, given observed free space.
    ///
    /// The reserve is subtracted first and saturates at zero: a host with less
    /// than the reserve free offers no temporary budget at all, rather than
    /// wrapping into an enormous one. Getting this backwards would turn a
    /// nearly-full disk into an unlimited allowance.
    pub fn temporary_budget(&self, free_bytes: u64) -> u64 {
        free_bytes
            .saturating_sub(self.temporary_free_space_reserve_bytes)
            .min(self.max_temporary_bytes)
    }

    /// Whether streamed output has exceeded the ratio ceiling.
    ///
    /// Both figures are what actually came out and went in, never what a header
    /// promised.
    pub fn ratio_exceeded(&self, compressed_read: u64, decoded_written: u64) -> bool {
        if compressed_read < self.compression_ratio_grace_bytes {
            return false;
        }
        decoded_written / compressed_read.max(1) > self.max_compression_ratio
    }
}

/// Compression methods accepted inside a ZIP.
pub const ZIP_METHODS: &[&str] = &["store", "deflate"];

/// Compression methods accepted inside a 7z, which is import-only.
pub const SEVEN_Z_METHODS: &[&str] = &["copy", "lzma", "lzma2"];

/// The single 7z format version this release reads.
pub const SEVEN_Z_FORMAT_VERSION: (u8, u8) = (0, 4);

/// CD track modes a CUE sheet may declare.
pub const CUE_TRACK_MODES: &[&str] = &[
    "AUDIO",
    "MODE1/2048",
    "MODE1/2352",
    "MODE2/2336",
    "MODE2/2352",
];

/// CUE directives that carry no behavior and are permitted but ignored for
/// identity. Bounded so a sheet cannot smuggle bulk through a comment.
pub const CUE_METADATA_DIRECTIVES: &[&str] =
    &["REM", "TITLE", "PERFORMER", "SONGWRITER", "CATALOG", "ISRC"];

/// CHD codecs accepted from the pinned implementation.
pub const CHD_CODECS: &[&str] = &[
    "none", "zlib", "zstd", "lzma", "huffman", "flac", "cdzl", "cdlz", "cdfl",
];

/// The single CHD version this release reads. Older versions are rejected
/// rather than upgraded in place.
pub const CHD_VERSION: u32 = 5;

/// RVZ compression methods accepted from the pinned implementation.
pub const RVZ_METHODS: &[&str] = &["none", "purge", "bzip2", "lzma", "lzma2", "zstd"];

/// Sidecar member classes ignored inside an archive without making membership
/// ambiguous. Every one is validated by signature, not by extension alone.
pub const SIDECAR_DOCUMENTATION: &[&str] = &["txt", "nfo", "md", "rtf", "pdf"];
pub const SIDECAR_IMAGES: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];
pub const SIDECAR_CHECKSUMS: &[&str] = &["sfv", "md5", "sha1", "sha256"];
pub const SIDECAR_METADATA: &[&str] = &["json", "xml", "yaml", "yml"];

/// Operating-system droppings that are ignored wherever they appear.
pub const SIDECAR_OS_METADATA: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];

/// Whether a member name is OS metadata, including the directory and
/// AppleDouble forms that are prefixes rather than exact names.
pub fn is_os_metadata(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    SIDECAR_OS_METADATA.contains(&base)
        || name.starts_with("__MACOSX/")
        || name.contains("/__MACOSX/")
        || base.starts_with("._")
}

/// Whether an extension is an ignorable sidecar class.
pub fn is_sidecar_extension(extension: &str) -> bool {
    let lowered = extension.to_ascii_lowercase();
    let extension = lowered.as_str();
    SIDECAR_DOCUMENTATION.contains(&extension)
        || SIDECAR_IMAGES.contains(&extension)
        || SIDECAR_CHECKSUMS.contains(&extension)
        || SIDECAR_METADATA.contains(&extension)
}

/// Whether a declared value is inside an accepted set.
///
/// Case-insensitive because format metadata spells methods inconsistently, but
/// still an allowlist: an unrecognized value is rejected, never passed through
/// to the parser to decide.
pub fn accepted(set: &[&str], declared: &str) -> bool {
    set.iter().any(|item| item.eq_ignore_ascii_case(declared))
}
