//! Bounded parsers for structured media descriptors (issue #19, under #17).
//!
//! # These read hostile input
//!
//! A CUE sheet or M3U playlist arrives from wherever the user got their ROMs.
//! It is untrusted text that names *other files to open*, which makes it the
//! most dangerous kind of input this application handles — a descriptor that
//! can choose what gets read is a descriptor that can choose what gets leaked.
//!
//! So every parser here works to the ceilings in the compatibility manifest and
//! refuses rather than repairs:
//!
//! - **Bounded size and line count**, so a malformed or hostile file cannot
//!   exhaust memory.
//! - **References confined to one import root** — no `..`, no absolute paths,
//!   no separators at all. A CUE naming `../../.ssh/id_rsa` is not a track.
//! - **Malformed input is never treated as empty.** An unreadable descriptor
//!   means an incomplete ROM Set, not a complete one with no tracks. That
//!   distinction is the whole point of #17's contract: opaque content must
//!   never be presented as a complete verified set.
//!
//! # Why the model is parsed, not just the file names
//!
//! An earlier version of this module read only `FILE` lines, on the reasoning
//! that the application needs to know *which files belong to the set*, not how
//! to play them, and that every field parsed is another field that can be
//! malformed.
//!
//! That reasoning was wrong in a specific way. #17 makes the track model
//! identity-bearing — "identity is the ordered file/track/index/gap/flag model
//! plus logical track bytes" — so a parser that skips it cannot tell two
//! different cuts of the same tracks apart, and cannot generate the rewritten
//! descriptor the Device Profile needs. Refusing to parse a field does not make
//! the field safe; it makes it unvalidated, and it still reaches the emulator.

use crate::{
    Incompleteness,
    manifest::{self, LIMITS},
};

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
    #[error("unsupported: {0}")]
    Unsupported(String),
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
        if trimmed.len() > LIMITS.max_path_component_bytes {
            return Err(DescriptorError::TooManyEntries);
        }
        if trimmed.contains('\0') {
            return Err(DescriptorError::Malformed("NUL in reference".into()));
        }
        // A URL is refused as an escape rather than as a bad name: it is a
        // reference that resolves somewhere this application must never reach.
        if trimmed.contains("://") {
            return Err(DescriptorError::EscapingReference(trimmed.to_owned()));
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

    /// The lowercased extension, or an empty string when there is none.
    pub fn extension(&self) -> String {
        self.0
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .unwrap_or_default()
    }
}

fn check_bounds(text: &str) -> Result<Vec<&str>, DescriptorError> {
    if text.len() as u64 > LIMITS.max_descriptor_bytes {
        return Err(DescriptorError::TooLarge);
    }
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() > LIMITS.max_descriptor_lines {
        return Err(DescriptorError::TooManyEntries);
    }
    Ok(lines)
}

/// A position on a CD, in frames. 75 frames to the second.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Frames(pub u32);

impl Frames {
    /// Parses `mm:ss:ff`.
    ///
    /// The seconds and frames fields are range-checked rather than merely
    /// parsed: `00:99:99` is arithmetically representable and is not a time on
    /// a disc, and accepting it would put a track at an address that cannot
    /// exist.
    pub fn parse(text: &str) -> Result<Self, DescriptorError> {
        let parts: Vec<&str> = text.trim().split(':').collect();
        let [minutes, seconds, frames] = parts.as_slice() else {
            return Err(DescriptorError::Malformed(format!(
                "expected mm:ss:ff, found {text}"
            )));
        };
        let parse = |field: &str| -> Result<u32, DescriptorError> {
            field
                .parse::<u32>()
                .map_err(|_| DescriptorError::Malformed(format!("non-numeric time field {field}")))
        };
        let (minutes, seconds, frames) = (parse(minutes)?, parse(seconds)?, parse(frames)?);
        if seconds >= 60 || frames >= 75 {
            return Err(DescriptorError::Malformed(format!(
                "time out of range: {text}"
            )));
        }
        Ok(Self((minutes * 60 + seconds) * 75 + frames))
    }
}

/// One track in a CUE sheet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CueTrack {
    pub number: u32,
    pub mode: String,
    /// The file this track lives in, as an index into [`CueSheet::files`].
    pub file: usize,
    /// `INDEX 00` — the pregap start, when written explicitly.
    pub index_zero: Option<Frames>,
    /// `INDEX 01` — where the track proper begins. Exactly one is required.
    pub index_one: Frames,
    pub pregap: Option<Frames>,
    pub postgap: Option<Frames>,
    /// Behavior-bearing flags such as `DCP`, `4CH`, `PRE`, `SCMS`.
    pub flags: Vec<String>,
}

impl CueTrack {
    /// Bytes per sector for this track's mode. Part of extent arithmetic.
    pub fn sector_bytes(&self) -> u32 {
        match self.mode.as_str() {
            "MODE1/2048" => 2048,
            "MODE2/2336" => 2336,
            _ => 2352,
        }
    }
}

/// The identity-bearing model of a CUE sheet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CueSheet {
    pub files: Vec<MemberReference>,
    pub tracks: Vec<CueTrack>,
}

/// Directives that change how the disc behaves and that this release does not
/// implement. Encountering one is an explicit rejection, never a skip.
const REJECTED_CUE_DIRECTIVES: &[&str] = &["CDTEXTFILE", "SESSION"];

/// `FILE` types other than `BINARY`. Every one implies audio this release
/// cannot verify byte-for-byte, so accepting them would mean claiming an
/// identity that was never checked.
const REJECTED_FILE_TYPES: &[&str] = &["WAVE", "AIFF", "MP3", "MOTOROLA"];

/// Parses a CUE sheet into its full model.
pub fn parse_cue_sheet(text: &str) -> Result<CueSheet, DescriptorError> {
    let lines = check_bounds(text)?;
    let mut files: Vec<MemberReference> = Vec::new();
    let mut tracks: Vec<CueTrack> = Vec::new();
    let mut pending: Option<CueTrack> = None;

    for (number, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut words = trimmed.split_whitespace();
        let Some(directive) = words.next() else {
            continue;
        };
        let directive_upper = directive.to_ascii_uppercase();

        if REJECTED_CUE_DIRECTIVES.contains(&directive_upper.as_str()) {
            return Err(DescriptorError::Unsupported(format!(
                "{directive_upper} at line {}",
                number + 1
            )));
        }
        if manifest::CUE_METADATA_DIRECTIVES.contains(&directive_upper.as_str()) {
            // Permitted, bounded by the line ceiling already applied, and not
            // identity-bearing — so it is read past rather than recorded.
            continue;
        }

        match directive_upper.as_str() {
            "FILE" => {
                if let Some(track) = pending.take() {
                    tracks.push(track);
                }
                let rest = trimmed[directive.len()..].trim();
                let (name, kind) = split_file_line(rest, number + 1)?;
                let kind_upper = kind.to_ascii_uppercase();
                if REJECTED_FILE_TYPES.contains(&kind_upper.as_str()) {
                    return Err(DescriptorError::Unsupported(format!(
                        "FILE type {kind_upper} at line {}",
                        number + 1
                    )));
                }
                if kind_upper != "BINARY" {
                    return Err(DescriptorError::Unsupported(format!(
                        "FILE type {kind_upper} at line {}",
                        number + 1
                    )));
                }
                files.push(MemberReference::new(name)?);
                if files.len() > LIMITS.max_descriptor_references {
                    return Err(DescriptorError::TooManyEntries);
                }
            }
            "TRACK" => {
                if let Some(track) = pending.take() {
                    tracks.push(track);
                }
                if files.is_empty() {
                    return Err(DescriptorError::Malformed(format!(
                        "TRACK before any FILE at line {}",
                        number + 1
                    )));
                }
                let (Some(raw_number), Some(mode)) = (words.next(), words.next()) else {
                    return Err(DescriptorError::Malformed(format!(
                        "incomplete TRACK at line {}",
                        number + 1
                    )));
                };
                let track_number = raw_number.parse::<u32>().map_err(|_| {
                    DescriptorError::Malformed(format!(
                        "non-numeric track number at line {}",
                        number + 1
                    ))
                })?;
                let mode_upper = mode.to_ascii_uppercase();
                if !manifest::accepted(manifest::CUE_TRACK_MODES, &mode_upper) {
                    // CDG and CD-I modes land here, as #17 requires.
                    return Err(DescriptorError::Unsupported(format!(
                        "track mode {mode_upper} at line {}",
                        number + 1
                    )));
                }
                pending = Some(CueTrack {
                    number: track_number,
                    mode: mode_upper,
                    file: files.len() - 1,
                    index_zero: None,
                    // Replaced when INDEX 01 is seen; absence is caught below.
                    index_one: Frames(u32::MAX),
                    pregap: None,
                    postgap: None,
                    flags: Vec::new(),
                });
            }
            "INDEX" => {
                let Some(track) = pending.as_mut() else {
                    return Err(DescriptorError::Malformed(format!(
                        "INDEX outside a track at line {}",
                        number + 1
                    )));
                };
                let (Some(raw_index), Some(raw_time)) = (words.next(), words.next()) else {
                    return Err(DescriptorError::Malformed(format!(
                        "incomplete INDEX at line {}",
                        number + 1
                    )));
                };
                let time = Frames::parse(raw_time)?;
                match raw_index.trim_start_matches('0') {
                    "" => track.index_zero = Some(time),
                    "1" => {
                        if track.index_one != Frames(u32::MAX) {
                            return Err(DescriptorError::Malformed(format!(
                                "duplicate INDEX 01 at line {}",
                                number + 1
                            )));
                        }
                        track.index_one = time;
                    }
                    other => {
                        return Err(DescriptorError::Unsupported(format!(
                            "INDEX {other} at line {}",
                            number + 1
                        )));
                    }
                }
            }
            "PREGAP" | "POSTGAP" => {
                let Some(track) = pending.as_mut() else {
                    return Err(DescriptorError::Malformed(format!(
                        "{directive_upper} outside a track at line {}",
                        number + 1
                    )));
                };
                let Some(raw_time) = words.next() else {
                    return Err(DescriptorError::Malformed(format!(
                        "{directive_upper} with no time at line {}",
                        number + 1
                    )));
                };
                let time = Frames::parse(raw_time)?;
                if directive_upper == "PREGAP" {
                    track.pregap = Some(time);
                } else {
                    track.postgap = Some(time);
                }
            }
            "FLAGS" => {
                let Some(track) = pending.as_mut() else {
                    return Err(DescriptorError::Malformed(format!(
                        "FLAGS outside a track at line {}",
                        number + 1
                    )));
                };
                track.flags = words.map(|flag| flag.to_ascii_uppercase()).collect();
            }
            other => {
                return Err(DescriptorError::Unsupported(format!(
                    "directive {other} at line {}",
                    number + 1
                )));
            }
        }
    }

    if let Some(track) = pending.take() {
        tracks.push(track);
    }
    if files.is_empty() {
        // Not an empty set — a sheet naming no files is one this application
        // could not read, and treating it as complete would be the exact
        // failure #17 forbids.
        return Err(DescriptorError::NoMembers);
    }
    if tracks.is_empty() {
        return Err(DescriptorError::Malformed("no tracks".into()));
    }
    if tracks.len() > LIMITS.max_descriptor_references {
        return Err(DescriptorError::TooManyEntries);
    }

    validate_cue_tracks(&tracks)?;
    Ok(CueSheet { files, tracks })
}

fn split_file_line(rest: &str, line: usize) -> Result<(&str, &str), DescriptorError> {
    if let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let end = after.find('"').ok_or_else(|| {
            DescriptorError::Malformed(format!("unterminated FILE name at line {line}"))
        })?;
        let kind = after[end + 1..].trim();
        if kind.is_empty() {
            return Err(DescriptorError::Malformed(format!(
                "FILE with no type at line {line}"
            )));
        }
        return Ok((&after[..end], kind));
    }
    let mut words = rest.split_whitespace();
    let (Some(name), Some(kind)) = (words.next(), words.next()) else {
        return Err(DescriptorError::Malformed(format!(
            "FILE with no name or type at line {line}"
        )));
    };
    Ok((name, kind))
}

/// Checks track numbering, index presence, ordering, and non-overlap.
fn validate_cue_tracks(tracks: &[CueTrack]) -> Result<(), DescriptorError> {
    for (position, track) in tracks.iter().enumerate() {
        if track.index_one == Frames(u32::MAX) {
            return Err(DescriptorError::Malformed(format!(
                "track {} has no INDEX 01",
                track.number
            )));
        }
        // Track numbers are contiguous from 1. A gap means a track is missing
        // from the sheet, which is a different failure from a missing file and
        // must not be silently renumbered.
        let expected = position as u32 + 1;
        if track.number != expected {
            return Err(DescriptorError::Malformed(format!(
                "track numbering is not contiguous: expected {expected}, found {}",
                track.number
            )));
        }
        if let Some(zero) = track.index_zero
            && zero > track.index_one
        {
            return Err(DescriptorError::Malformed(format!(
                "track {} has INDEX 00 after INDEX 01",
                track.number
            )));
        }
    }

    // Within one file, successive tracks must start strictly later. Across a
    // file boundary the clock restarts, so the comparison resets with it —
    // comparing across files would reject legitimate track-per-file sheets.
    for pair in tracks.windows(2) {
        let [previous, next] = pair else { continue };
        if previous.file != next.file {
            continue;
        }
        let previous_start = previous.index_zero.unwrap_or(previous.index_one);
        let next_start = next.index_zero.unwrap_or(next.index_one);
        if next_start <= previous_start {
            return Err(DescriptorError::Malformed(format!(
                "track {} starts at or before track {}",
                next.number, previous.number
            )));
        }
    }
    Ok(())
}

/// One record in a GDI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GdiRecord {
    pub number: u32,
    pub lba: u32,
    /// `0` for audio, `4` for data.
    pub control: u32,
    pub sector_bytes: u32,
    pub file: MemberReference,
    pub offset: u64,
}

/// The identity-bearing model of a GDI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gdi {
    pub records: Vec<GdiRecord>,
}

/// Parses a GDI into its full model.
///
/// The declared count is checked against the records actually present. A GDI
/// that promises five tracks and lists three is malformed, not a three-track
/// disc — trusting the records over the header would quietly turn a truncated
/// file into a valid-looking set.
pub fn parse_gdi_model(text: &str) -> Result<Gdi, DescriptorError> {
    let lines = check_bounds(text)?;
    let mut declared: Option<u32> = None;
    let mut records: Vec<GdiRecord> = Vec::new();

    for (number, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if declared.is_none() {
            let count = trimmed.parse::<u32>().map_err(|_| {
                DescriptorError::Malformed(format!("expected a track count on line {}", number + 1))
            })?;
            if !(1..=99).contains(&count) {
                return Err(DescriptorError::Malformed(format!(
                    "track count {count} outside 1-99"
                )));
            }
            declared = Some(count);
            continue;
        }

        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        if fields.len() < 6 {
            return Err(DescriptorError::Malformed(format!(
                "incomplete GDI record at line {}",
                number + 1
            )));
        }
        let numeric = |index: usize| -> Result<u64, DescriptorError> {
            fields[index].parse::<u64>().map_err(|_| {
                DescriptorError::Malformed(format!(
                    "non-numeric field {} at line {}",
                    index + 1,
                    number + 1
                ))
            })
        };
        let track_number = numeric(0)? as u32;
        let lba = numeric(1)? as u32;
        let control = numeric(2)? as u32;
        let sector_bytes = numeric(3)? as u32;
        // The offset is the last field; the name is everything between the
        // sector size and it, so names containing spaces survive.
        let offset = fields[fields.len() - 1].parse::<u64>().map_err(|_| {
            DescriptorError::Malformed(format!("non-numeric offset at line {}", number + 1))
        })?;
        let name = fields[4..fields.len() - 1].join(" ");

        if control != 0 && control != 4 {
            return Err(DescriptorError::Malformed(format!(
                "control {control} is neither audio (0) nor data (4) at line {}",
                number + 1
            )));
        }
        let allowed: &[u32] = if control == 0 { &[2352] } else { &[2048, 2352] };
        if !allowed.contains(&sector_bytes) {
            return Err(DescriptorError::Malformed(format!(
                "sector size {sector_bytes} invalid for control {control} at line {}",
                number + 1
            )));
        }

        records.push(GdiRecord {
            number: track_number,
            lba,
            control,
            sector_bytes,
            file: MemberReference::new(&name)?,
            offset,
        });
        if records.len() > LIMITS.max_descriptor_references {
            return Err(DescriptorError::TooManyEntries);
        }
    }

    let Some(declared) = declared else {
        return Err(DescriptorError::NoMembers);
    };
    if records.len() as u32 != declared {
        return Err(DescriptorError::Malformed(format!(
            "declared {declared} tracks but found {}",
            records.len()
        )));
    }

    for (position, record) in records.iter().enumerate() {
        let expected = position as u32 + 1;
        if record.number != expected {
            return Err(DescriptorError::Malformed(format!(
                "track numbering is not contiguous: expected {expected}, found {}",
                record.number
            )));
        }
    }
    for pair in records.windows(2) {
        let [previous, next] = pair else { continue };
        if next.lba <= previous.lba {
            return Err(DescriptorError::Malformed(format!(
                "track {} starts at or before track {}",
                next.number, previous.number
            )));
        }
    }
    // Two records sharing a file would make their extents ambiguous, and #17
    // requires unique files.
    for (position, record) in records.iter().enumerate() {
        if records[..position]
            .iter()
            .any(|earlier| earlier.file == record.file)
        {
            return Err(DescriptorError::Malformed(format!(
                "file {} is referenced by more than one track",
                record.file.as_str()
            )));
        }
    }

    Ok(Gdi { records })
}

/// Extensions a playlist may name, per Platform.
///
/// Anything else — an archive, another playlist, a bare track — is refused.
fn playlist_extensions(platform: &str) -> Option<&'static [&'static str]> {
    match platform {
        "playstation" | "saturn" => Some(&["cue", "chd"]),
        "playstation-2" => Some(&["iso", "chd"]),
        "dreamcast" => Some(&["gdi", "chd"]),
        _ => None,
    }
}

/// Parses an M3U for a known Platform, enforcing that Platform's child forms.
///
/// A playlist is the one descriptor whose children are themselves whole ROM
/// Sets, so the accepted extensions differ per Platform and a nested `.m3u` is
/// rejected outright rather than followed.
pub fn parse_m3u_for(text: &str, platform: &str) -> Result<Vec<MemberReference>, DescriptorError> {
    let Some(allowed) = playlist_extensions(platform) else {
        return Err(DescriptorError::Unsupported(format!(
            "playlists are not accepted for {platform}"
        )));
    };
    // A BOM is permitted and carries no meaning, so it is stripped before the
    // first line is read rather than becoming part of the first reference.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let members = parse_m3u(text)?;

    if members.len() < 2 {
        return Err(DescriptorError::Malformed(
            "a playlist needs at least two discs".into(),
        ));
    }
    for (position, member) in members.iter().enumerate() {
        let extension = member.extension();
        if extension == "m3u" {
            return Err(DescriptorError::Unsupported("nested playlist".into()));
        }
        if !allowed.contains(&extension.as_str()) {
            return Err(DescriptorError::Unsupported(format!(
                "{platform} playlists do not accept .{extension}"
            )));
        }
        if members[..position].contains(member) {
            return Err(DescriptorError::Malformed(format!(
                "disc {} appears more than once",
                member.as_str()
            )));
        }
    }
    Ok(members)
}

/// The files a CUE sheet refers to, in order.
pub fn parse_cue(text: &str) -> Result<Vec<MemberReference>, DescriptorError> {
    Ok(parse_cue_sheet(text)?.files)
}

/// The tracks a GDI refers to, in order.
pub fn parse_gdi(text: &str) -> Result<Vec<MemberReference>, DescriptorError> {
    Ok(parse_gdi_model(text)?
        .records
        .into_iter()
        .map(|record| record.file)
        .collect())
}

/// The discs an M3U playlist refers to, in order.
///
/// Blank lines and `#` comments are skipped. Extended-M3U directives are
/// comments by construction, so nothing needs to interpret them.
pub fn parse_m3u(text: &str) -> Result<Vec<MemberReference>, DescriptorError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines = check_bounds(text)?;
    let mut members = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        members.push(MemberReference::new(trimmed)?);
        if members.len() > LIMITS.max_descriptor_references {
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
