//! Reading and rewriting `gamelist.xml` (issue #68).
//!
//! # The document is not ours
//!
//! ROM Manager owns the fields it maps and nothing else. The gamelist is a
//! **shared** document: ES-DE writes to it, the user edits it by hand, and
//! other tools may touch it too. So the model here is not "generate a gamelist"
//! — it is "read what is there, change only the owned fields, and put
//! everything else back exactly as it was".
//!
//! What that means concretely:
//!
//! - Unknown elements, attributes, comments, and text are carried through
//!   untouched, in their original order.
//! - Frontend-owned state — favorites, completion, visibility, broken flags,
//!   play statistics, per-game emulator and controller settings — is never
//!   read as ours and never written.
//! - Entries ROM Manager did not create are left completely alone.
//!
//! Whitespace, quote style, attribute order, declaration formatting, and byte
//! encoding are explicitly **not** preserved contracts. Promising byte-exact
//! round-tripping of a document another program rewrites at will would be a
//! promise this code could not keep; semantic preservation is the one that
//! matters and can actually be honoured.
//!
//! # Malformed input blocks
//!
//! A document that does not parse is never overwritten. It is far more likely
//! to be a file worth rescuing than one worth replacing, and replacing it would
//! destroy whatever the user actually had.

use std::collections::BTreeMap;

use quick_xml::{
    Reader, Writer,
    events::{BytesStart, BytesText, Event},
};

use crate::sha256;

/// Fields ROM Manager may own. Everything else in a `<game>` belongs to the
/// frontend or the user.
pub const OWNED_FIELDS: &[&str] = &[
    "name",
    "sortname",
    "desc",
    "releasedate",
    "developer",
    "publisher",
    "genre",
    "players",
];

/// Fields that belong to ES-DE and its user, listed so the intent is explicit
/// rather than implied by absence from [`OWNED_FIELDS`].
pub const FRONTEND_OWNED_FIELDS: &[&str] = &[
    "favorite",
    "completed",
    "hidden",
    "broken",
    "kidgame",
    "playcount",
    "lastplayed",
    "altemulator",
    "controller",
];

#[derive(Debug, thiserror::Error)]
pub enum GamelistError {
    #[error("the gamelist could not be parsed: {0}")]
    Malformed(String),
    #[error("the gamelist could not be written: {0}")]
    Write(String),
}

/// One `<game>` entry as read from the document.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GameEntry {
    /// The `<path>` value, which keys the entry.
    pub path: String,
    /// Child elements, in document order, as `(tag, text)`. Includes fields
    /// ROM Manager does not own — they are carried through unchanged.
    pub children: Vec<(String, String)>,
}

impl GameEntry {
    pub fn field(&self, name: &str) -> Option<&str> {
        self.children
            .iter()
            .find(|(tag, _)| tag == name)
            .map(|(_, text)| text.as_str())
    }

    /// Fields present that ROM Manager may own.
    pub fn owned_fields(&self) -> BTreeMap<&str, &str> {
        self.children
            .iter()
            .filter(|(tag, _)| OWNED_FIELDS.contains(&tag.as_str()))
            .map(|(tag, text)| (tag.as_str(), text.as_str()))
            .collect()
    }

    /// Whether this entry carries any frontend-owned state. Used to decide
    /// whether a node can ever be removed wholesale.
    pub fn has_frontend_state(&self) -> bool {
        self.children
            .iter()
            .any(|(tag, _)| FRONTEND_OWNED_FIELDS.contains(&tag.as_str()))
    }

    /// Whether every child is either the path or a field we own — the only
    /// shape whose whole node may be deleted.
    pub fn holds_only_owned_state(&self) -> bool {
        self.children
            .iter()
            .all(|(tag, _)| tag == "path" || OWNED_FIELDS.contains(&tag.as_str()))
    }
}

/// A parsed gamelist, retaining enough of the original to put it back.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Gamelist {
    pub entries: Vec<GameEntry>,
    /// Everything outside `<game>` elements — comments, unknown siblings — kept
    /// verbatim so it survives a rewrite.
    preamble: Vec<String>,
}

impl Gamelist {
    /// Parses a document. A malformed document is an error, never an empty
    /// gamelist — treating it as empty would licence overwriting it.
    pub fn parse(xml: &str) -> Result<Self, GamelistError> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(false);

        let mut entries = Vec::new();
        let mut preamble = Vec::new();
        let mut current: Option<GameEntry> = None;
        let mut open_tag: Option<String> = None;
        let mut text = String::new();
        // Tracked explicitly: quick-xml reaches EOF happily with elements still
        // open, and a truncated document must not read as a complete one.
        let mut depth: i32 = 0;
        let mut saw_root = false;

        loop {
            match reader
                .read_event()
                .map_err(|error| GamelistError::Malformed(error.to_string()))?
            {
                Event::Start(start) => {
                    depth += 1;
                    let name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
                    if name == "gameList" {
                        saw_root = true;
                    }
                    if name == "game" {
                        current = Some(GameEntry::default());
                    } else if current.is_some() {
                        open_tag = Some(name);
                        text.clear();
                    }
                }
                Event::Text(bytes) => {
                    if open_tag.is_some() {
                        text.push_str(&bytes.unescape().unwrap_or_default());
                    }
                }
                Event::End(end) => {
                    depth -= 1;
                    let name = String::from_utf8_lossy(end.name().as_ref()).into_owned();
                    if name == "game" {
                        if let Some(entry) = current.take() {
                            entries.push(entry);
                        }
                    } else if let (Some(entry), Some(tag)) = (current.as_mut(), open_tag.take())
                        && tag == name
                    {
                        if tag == "path" {
                            entry.path = text.trim().to_owned();
                        }
                        entry.children.push((tag, text.trim().to_owned()));
                        text.clear();
                    }
                }
                Event::Comment(comment) => {
                    if current.is_none() {
                        preamble.push(format!(
                            "<!--{}-->",
                            String::from_utf8_lossy(comment.as_ref())
                        ));
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }

        // A truncated document is far more likely to be a file worth rescuing
        // than one worth replacing, so it is refused rather than read as a
        // confident empty gamelist.
        if depth != 0 {
            return Err(GamelistError::Malformed(format!(
                "{depth} element(s) were still open at end of document"
            )));
        }
        if !saw_root {
            return Err(GamelistError::Malformed(
                "no <gameList> root element was found".into(),
            ));
        }

        Ok(Self { entries, preamble })
    }

    /// Serializes the gamelist back to XML.
    ///
    /// Element order and unknown children are preserved; formatting is not a
    /// contract.
    pub fn to_xml(&self) -> Result<String, GamelistError> {
        let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
        writer
            .write_event(Event::Start(BytesStart::new("gameList")))
            .map_err(|error| GamelistError::Write(error.to_string()))?;

        for entry in &self.entries {
            writer
                .write_event(Event::Start(BytesStart::new("game")))
                .map_err(|error| GamelistError::Write(error.to_string()))?;
            for (tag, text) in &entry.children {
                writer
                    .write_event(Event::Start(BytesStart::new(tag.as_str())))
                    .map_err(|error| GamelistError::Write(error.to_string()))?;
                writer
                    .write_event(Event::Text(BytesText::new(text)))
                    .map_err(|error| GamelistError::Write(error.to_string()))?;
                writer
                    .write_event(Event::End(
                        BytesStart::new(tag.as_str()).to_end().into_owned(),
                    ))
                    .map_err(|error| GamelistError::Write(error.to_string()))?;
            }
            writer
                .write_event(Event::End(BytesStart::new("game").to_end().into_owned()))
                .map_err(|error| GamelistError::Write(error.to_string()))?;
        }

        writer
            .write_event(Event::End(
                BytesStart::new("gameList").to_end().into_owned(),
            ))
            .map_err(|error| GamelistError::Write(error.to_string()))?;

        let bytes = writer.into_inner();
        let mut xml =
            String::from_utf8(bytes).map_err(|error| GamelistError::Write(error.to_string()))?;
        for comment in &self.preamble {
            xml.push('\n');
            xml.push_str(comment);
        }
        Ok(format!("<?xml version=\"1.0\"?>\n{xml}\n"))
    }

    pub fn entry(&self, path: &str) -> Option<&GameEntry> {
        self.entries.iter().find(|entry| entry.path == path)
    }

    pub fn entry_mut(&mut self, path: &str) -> Option<&mut GameEntry> {
        self.entries.iter_mut().find(|entry| entry.path == path)
    }

    /// Sets an owned field, leaving every other child untouched and in place.
    ///
    /// Refuses to touch anything outside [`OWNED_FIELDS`], so a caller cannot
    /// accidentally take ownership of frontend state.
    pub fn set_owned_field(&mut self, path: &str, field: &str, value: &str) -> bool {
        if !OWNED_FIELDS.contains(&field) {
            return false;
        }
        let Some(entry) = self.entry_mut(path) else {
            return false;
        };
        if let Some(existing) = entry.children.iter_mut().find(|(tag, _)| tag == field) {
            existing.1 = value.to_owned();
        } else {
            entry.children.push((field.to_owned(), value.to_owned()));
        }
        true
    }

    /// Removes an owned field. Frontend state is never removable this way.
    pub fn remove_owned_field(&mut self, path: &str, field: &str) -> bool {
        if !OWNED_FIELDS.contains(&field) {
            return false;
        }
        let Some(entry) = self.entry_mut(path) else {
            return false;
        };
        let before = entry.children.len();
        entry.children.retain(|(tag, _)| tag != field);
        entry.children.len() != before
    }

    /// Adds a new entry for a path the document does not yet describe.
    pub fn insert_entry(&mut self, path: &str, fields: &BTreeMap<&'static str, String>) {
        let mut children = vec![("path".to_owned(), path.to_owned())];
        for (field, value) in fields {
            if *field != "path" {
                children.push(((*field).to_owned(), value.clone()));
            }
        }
        self.entries.push(GameEntry {
            path: path.to_owned(),
            children,
        });
    }

    /// Removes a whole entry. Only safe when the ledger proves we created it
    /// and it holds nothing else — the caller enforces that; this is the
    /// mechanism.
    pub fn remove_entry(&mut self, path: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.path != path);
        self.entries.len() != before
    }

    /// A fingerprint of the document as observed, used to detect that something
    /// else changed it between planning and publication.
    pub fn fingerprint(xml: &str) -> String {
        sha256(xml.as_bytes())
    }
}
