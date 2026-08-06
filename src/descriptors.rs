//! Bounded parsers for structured media descriptors (issue #19, under #17).
//!
//! # These read hostile input
//!
//! A CUE sheet or M3U playlist arrives from wherever the user got their ROMs.
//! It is untrusted text that names *other files to open*, which makes it the
//! most dangerous kind of input this application handles — a descriptor that
//! can choose what gets read is a descriptor that can choose what gets leaked.
//!
//! So every parser here works to explicit ceilings and refuses rather than
//! repairs:
//!
//! - **Bounded size and line count**, so a malformed or hostile file cannot
//!   exhaust memory.
//! - **References confined to one import root** — no `..`, no absolute paths,
//!   no separators at all. A CUE naming `../../.ssh/id_rsa` is not a track.
//! - **Malformed input is never treated as empty.** An unreadable descriptor
//!   means an incomplete ROM Set, not a complete one with no tracks. That
//!   distinction is the whole point of #17's contract: opaque content must
//!   never be presented as a complete verified set.

use crate::Incompleteness;

/// Largest descriptor this release will read. Generous for real sheets,
/// nowhere near enough to hurt.
const MAX_DESCRIPTOR_BYTES: usize = 256 * 1024;
/// Largest number of lines. A CUE with more tracks than this is not a CUE.
const MAX_LINES: usize = 4_096;
/// Largest number of referenced members.
const MAX_MEMBERS: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DescriptorError {
    #[error("the descriptor exceeds the size ceiling")]
    TooLarge,
    #[error("the descriptor has too many lines or members")]
    TooManyEntries,
    #[error("the descriptor could not be read: {0}")]
    Malformed(String),
    #[error("a reference escapes the import root: {0}")]
    EscapingReference(String),
    #[error("the descriptor references nothing")]
    NoMembers,
}

impl From<DescriptorError> for Incompleteness {
    fn from(error: DescriptorError) -> Self {
        match error {
            DescriptorError::EscapingReference(reference) => Self::EscapingReference(reference),
            other => Self::MalformedDescriptor(other.to_string()),
        }
    }
}

/// A file name a descriptor refers to, already checked for confinement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberReference(String);

impl MemberReference {
    /// Accepts a reference only if it names a plain file beside the descriptor.
    ///
    /// Refused rather than sanitized, on the same footing as the target-path
    /// namespace: repairing a name the caller could not prove is exactly how a
    /// confinement check becomes decorative.
    pub fn new(raw: &str) -> Result<Self, DescriptorError> {
        let trimmed = raw.trim().trim_matches('"');
        if trimmed.is_empty() {
            return Err(DescriptorError::Malformed("empty reference".into()));
        }
        if trimmed.contains("..")
            || trimmed.starts_with('/')
            || trimmed.contains('/')
            || trimmed.contains('\\')
            || trimmed.contains(':')
        {
            return Err(DescriptorError::EscapingReference(trimmed.to_owned()));
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn check_bounds(text: &str) -> Result<Vec<&str>, DescriptorError> {
    if text.len() > MAX_DESCRIPTOR_BYTES {
        return Err(DescriptorError::TooLarge);
    }
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() > MAX_LINES {
        return Err(DescriptorError::TooManyEntries);
    }
    Ok(lines)
}

/// The files a CUE sheet refers to, in order.
///
/// Only `FILE` lines are interpreted. Track modes, indices, pregaps, and
/// everything else are deliberately ignored — this release needs to know *which
/// files belong to the set*, not how to play them, and every field parsed is
/// another field that can be malformed.
pub fn parse_cue(text: &str) -> Result<Vec<MemberReference>, DescriptorError> {
    let lines = check_bounds(text)?;
    let mut members = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        let Some(rest) = trimmed
            .strip_prefix("FILE ")
            .or_else(|| trimmed.strip_prefix("file "))
        else {
            continue;
        };
        // FILE "name.bin" BINARY — the name is quoted, the type follows.
        let name = if let Some(start) = rest.find('"') {
            let after = &rest[start + 1..];
            let end = after
                .find('"')
                .ok_or_else(|| DescriptorError::Malformed("unterminated FILE name".into()))?;
            &after[..end]
        } else {
            rest.split_whitespace()
                .next()
                .ok_or_else(|| DescriptorError::Malformed("FILE with no name".into()))?
        };

        members.push(MemberReference::new(name)?);
        if members.len() > MAX_MEMBERS {
            return Err(DescriptorError::TooManyEntries);
        }
    }

    if members.is_empty() {
        // Not an empty set — a sheet naming no files is one this application
        // could not read, and treating it as complete would be the exact
        // failure #17 forbids.
        return Err(DescriptorError::NoMembers);
    }
    Ok(members)
}

/// The discs an M3U playlist refers to, in order.
///
/// Blank lines and `#` comments are skipped. Extended-M3U directives are
/// comments by construction, so nothing needs to interpret them.
pub fn parse_m3u(text: &str) -> Result<Vec<MemberReference>, DescriptorError> {
    let lines = check_bounds(text)?;
    let mut members = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        members.push(MemberReference::new(trimmed)?);
        if members.len() > MAX_MEMBERS {
            return Err(DescriptorError::TooManyEntries);
        }
    }

    if members.is_empty() {
        return Err(DescriptorError::NoMembers);
    }
    Ok(members)
}

/// The tracks a GDI refers to.
///
/// GDI lines are `index lba type sectorSize filename offset`; only the file
/// name is taken, for the same reason as CUE.
pub fn parse_gdi(text: &str) -> Result<Vec<MemberReference>, DescriptorError> {
    let lines = check_bounds(text)?;
    let mut members = Vec::new();

    for (number, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // The first line is the track count, not a track.
        if number == 0 && trimmed.parse::<u32>().is_ok() {
            continue;
        }
        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        if fields.len() < 5 {
            continue;
        }
        members.push(MemberReference::new(fields[4])?);
        if members.len() > MAX_MEMBERS {
            return Err(DescriptorError::TooManyEntries);
        }
    }

    if members.is_empty() {
        return Err(DescriptorError::NoMembers);
    }
    Ok(members)
}

/// Whether every member a descriptor names is present beside it.
pub fn membership_is_complete(
    referenced: &[MemberReference],
    present: &[String],
) -> Result<(), Incompleteness> {
    for member in referenced {
        if !present.iter().any(|name| name == member.as_str()) {
            return Err(Incompleteness::MissingMember(member.as_str().to_owned()));
        }
    }
    Ok(())
}
