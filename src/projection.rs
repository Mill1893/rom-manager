//! Metadata Projections (issue #67).
//!
//! A **projection** is the export-eligible slice of effective Library metadata,
//! mapped to one frontend entry. The emphasis is on *slice*: ROM Manager owns
//! the fields it maps and nothing else, and a fact it cannot represent
//! faithfully is omitted rather than approximated.
//!
//! # Why omission beats approximation
//!
//! A partial release date rendered as `1994-01-01`, or a player range of
//! "2 or more" rendered as `2`, would read as fact in ES-DE. The user cannot
//! tell an exported guess from an exported certainty, so anything uncertain is
//! left out — an absent field is honestly absent, while a wrong one is
//! indistinguishable from a right one.
//!
//! # What is never mapped
//!
//! Ratings, alternative titles, region, languages, provider identifiers,
//! provenance, attribution, and artwork tags are all excluded. So is every
//! frontend-owned field — favorites, completion, visibility, broken state, play
//! statistics, and per-game emulator or controller settings belong to ES-DE and
//! its user.

use std::collections::BTreeMap;

/// Effective Library metadata for one Release, before export filtering.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReleaseFacts {
    pub title: String,
    pub sort_title: Option<String>,
    pub description: Option<String>,
    /// Exported only when complete: year, month, and day all known.
    pub release_date: Option<CalendarDate>,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    /// The effective primary genre. Secondary genres are not exported.
    pub primary_genre: Option<String>,
    pub players: Option<PlayerCount>,
    // Distinctions used only to disambiguate colliding titles.
    pub region: Option<String>,
    pub language: Option<String>,
    pub revision: Option<String>,
    pub representation: Option<String>,
    /// A user-visible label, required when nothing else separates two Releases.
    pub local_label: Option<String>,
}

/// A date is exported only when every component is known.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalendarDate {
    Complete {
        year: i32,
        month: u32,
        day: u32,
    },
    /// Year, or year and month, but not a full date. Never exported — rendering
    /// it would invent precision the Library does not have.
    Partial,
}

/// How many players a game supports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerCount {
    Exact(u32),
    /// A closed range, both ends known.
    Range {
        min: u32,
        max: u32,
    },
    /// Open-ended or unknown. Never exported.
    Open,
}

/// One frontend entry's worth of owned fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataProjection {
    /// The `./`-relative path this entry is keyed by.
    pub entry_path: String,
    /// Owned fields, in document order. Only these are ever written.
    pub fields: BTreeMap<&'static str, String>,
}

impl MetadataProjection {
    /// Builds a projection from effective facts.
    ///
    /// `display_title` is the disambiguated title from
    /// [`disambiguate_titles`]; the raw title is not used directly, because two
    /// Releases of the same Game would otherwise be indistinguishable in the
    /// frontend.
    pub fn build(entry_path: String, facts: &ReleaseFacts, display_title: &str) -> Self {
        let mut fields = BTreeMap::new();
        fields.insert("name", display_title.to_owned());

        if let Some(sort) = facts.sort_title.as_ref().filter(|value| !value.is_empty()) {
            fields.insert("sortname", sort.clone());
        }
        if let Some(description) = facts.description.as_ref().filter(|v| !v.is_empty()) {
            // The only permitted change is line-ending normalization; XML
            // safety is the writer's concern, not a rewrite of the text.
            fields.insert("desc", description.replace("\r\n", "\n"));
        }
        if let Some(CalendarDate::Complete { year, month, day }) = facts.release_date {
            // ES-DE's format. Emitted only for a complete date.
            fields.insert("releasedate", format!("{year:04}{month:02}{day:02}T000000"));
        }
        if let Some(joined) = join_credits(&facts.developers) {
            fields.insert("developer", joined);
        }
        if let Some(joined) = join_credits(&facts.publishers) {
            fields.insert("publisher", joined);
        }
        if let Some(genre) = facts.primary_genre.as_ref().filter(|v| !v.is_empty()) {
            fields.insert("genre", genre.clone());
        }
        match facts.players {
            Some(PlayerCount::Exact(count)) => {
                fields.insert("players", count.to_string());
            }
            Some(PlayerCount::Range { min, max }) => {
                fields.insert("players", format!("{min}-{max}"));
            }
            // Open-ended is left out: "2+" rendered as "2" would read as fact.
            Some(PlayerCount::Open) | None => {}
        }

        Self { entry_path, fields }
    }

    pub fn owned_field_names(&self) -> Vec<&'static str> {
        self.fields.keys().copied().collect()
    }
}

/// Deterministic ` / ` join for multiple credits, or `None` when there are
/// none. Order is preserved so the same facts always produce the same string.
fn join_credits(credits: &[String]) -> Option<String> {
    let present: Vec<&str> = credits
        .iter()
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .collect();
    (!present.is_empty()).then(|| present.join(" / "))
}

/// Resolves colliding display titles by adding the **minimum** distinction.
///
/// Two Releases of the same Game must be tellable apart in the frontend, but
/// over-qualifying every title makes the list unreadable. So distinctions are
/// added in order of how much a person is likely to care — region, then
/// language, then revision, then representation — and only while a collision
/// remains.
///
/// A collision that survives all of them requires a user-visible local label.
/// A hash or internal identifier would be technically unique and useless to
/// read.
pub fn disambiguate_titles(releases: &[ReleaseFacts]) -> Vec<String> {
    let mut titles: Vec<String> = releases.iter().map(|facts| facts.title.clone()).collect();

    let distinctions: [fn(&ReleaseFacts) -> Option<&String>; 4] = [
        |facts| facts.region.as_ref(),
        |facts| facts.language.as_ref(),
        |facts| facts.revision.as_ref(),
        |facts| facts.representation.as_ref(),
    ];

    for distinction in distinctions {
        if !has_collision(&titles) {
            break;
        }
        let colliding = colliding_indices(&titles);
        for index in colliding {
            if let Some(value) = distinction(&releases[index]) {
                titles[index] = format!("{} ({})", titles[index], value);
            }
        }
    }

    if has_collision(&titles) {
        for index in colliding_indices(&titles) {
            if let Some(label) = releases[index].local_label.as_ref() {
                titles[index] = format!("{} ({label})", titles[index]);
            }
        }
    }
    titles
}

fn has_collision(titles: &[String]) -> bool {
    !colliding_indices(titles).is_empty()
}

fn colliding_indices(titles: &[String]) -> Vec<usize> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for title in titles {
        *counts.entry(title.as_str()).or_default() += 1;
    }
    titles
        .iter()
        .enumerate()
        .filter(|(_, title)| counts[title.as_str()] > 1)
        .map(|(index, _)| index)
        .collect()
}

/// Whether a ROM Set should receive its own gamelist entry.
///
/// Only frontend-launchable sets do. A referenced track, a disc represented by
/// an M3U, a dependency, a directory, and a Source Container are all things the
/// frontend should never offer as a game — listing them would put entries in
/// the user's library that do nothing when selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryEligibility {
    Launchable,
    ReferencedTrack,
    DiscRepresentedByPlaylist,
    Dependency,
    Directory,
    SourceContainer,
}

impl EntryEligibility {
    pub fn gets_an_entry(self) -> bool {
        self == Self::Launchable
    }
}
