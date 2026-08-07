//! Which filesystems a Media Target may live on (issue #75).
//!
//! # Why FAT32 is refused
//!
//! Not because it is old. Because two of its properties make the safety
//! guarantees elsewhere in this system unenforceable:
//!
//! - **A 4 GiB file-size ceiling.** A ROM Set larger than that cannot be
//!   written, and the failure arrives partway through a transfer rather than at
//!   planning time.
//! - **No usable case-sensitivity story.** The effective-equivalence key is a
//!   conservative superset of the host's lookup rule, and FAT32's rule varies
//!   with the driver mounting it. A key that cannot be conservative cannot
//!   promise to catch collisions.
//!
//! It is *experimental* rather than forbidden, so the rejection is a plan-level
//! block the user can read — not a silent absence and not a mid-transfer error.
//!
//! An undeterminable filesystem is blocked too. "We could not tell" is not
//! evidence of safety, and assuming otherwise is exactly the inference the rest
//! of the system refuses to make.

use serde::{Deserialize, Serialize};

/// A filesystem as observed on the host.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedFilesystem {
    Ext4,
    ExFat,
    Ntfs,
    /// Experimental: refused by default.
    Fat32,
    /// A filesystem the host reported but this release has not qualified.
    Unqualified(String),
    /// The host could not tell us.
    Unknown,
}

impl ObservedFilesystem {
    /// Parses what a host reports. Matching is case-insensitive because
    /// `fsutil`, `statfs`, and `lsblk` all spell these differently.
    pub fn parse(reported: &str) -> Self {
        match reported.trim().to_lowercase().as_str() {
            "ext4" => Self::Ext4,
            "exfat" => Self::ExFat,
            "ntfs" | "ntfs3" => Self::Ntfs,
            "fat32" | "vfat" | "msdos" => Self::Fat32,
            "" => Self::Unknown,
            other => Self::Unqualified(other.to_owned()),
        }
    }
}

/// Whether a filesystem may be synced to, and why not when it may not.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FilesystemSupport {
    Supported,
    Blocked { reason: String },
}

impl FilesystemSupport {
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Supported)
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Supported => None,
            Self::Blocked { reason } => Some(reason),
        }
    }
}

/// Decides whether the first release will sync to `filesystem`.
///
/// `allow_experimental` lets a user opt in to FAT32 knowingly. It changes
/// nothing about the risks — it only records that they were told.
pub fn support_for(filesystem: &ObservedFilesystem, allow_experimental: bool) -> FilesystemSupport {
    match filesystem {
        ObservedFilesystem::Ext4 | ObservedFilesystem::ExFat | ObservedFilesystem::Ntfs => {
            FilesystemSupport::Supported
        }
        ObservedFilesystem::Fat32 if allow_experimental => FilesystemSupport::Supported,
        ObservedFilesystem::Fat32 => FilesystemSupport::Blocked {
            reason: "FAT32 is experimental: it cannot hold files above 4 GiB, and its \
                     case handling varies by driver, so collision detection cannot be \
                     guaranteed"
                .into(),
        },
        ObservedFilesystem::Unqualified(name) => FilesystemSupport::Blocked {
            reason: format!("{name} has not been qualified for this release"),
        },
        // "We could not tell" is not evidence of safety.
        ObservedFilesystem::Unknown => FilesystemSupport::Blocked {
            reason: "the target's filesystem could not be determined".into(),
        },
    }
}

/// The largest single file a filesystem can hold, where it has a limit worth
/// planning against.
pub fn maximum_file_size(filesystem: &ObservedFilesystem) -> Option<u64> {
    match filesystem {
        // 4 GiB minus one byte. A plan can catch this before transferring.
        ObservedFilesystem::Fat32 => Some(4 * 1024 * 1024 * 1024 - 1),
        _ => None,
    }
}

/// Whether an artifact of `size` fits, given the filesystem's ceiling.
pub fn fits(filesystem: &ObservedFilesystem, size: u64) -> bool {
    maximum_file_size(filesystem).is_none_or(|limit| size <= limit)
}
