//! The ES-DE on Android metadata profile (issue #66).
//!
//! # Two roles, not one target
//!
//! ES-DE keeps ROMs and its `gamelist.xml` documents in different places, and
//! on a handheld they are frequently on *different volumes* — ROMs on an SD
//! card, application data on internal storage. So the profile declares two
//! **Destination Roles** rather than assuming one root:
//!
//! - `rom-content` — where Target Artifacts are placed.
//! - `frontend-metadata` — the ES-DE application-data root, under which
//!   documents live at `gamelists/<system-key>/gamelist.xml`.
//!
//! One Media Target may fulfil both. Two may fulfil one each, in which case
//! they are **explicitly paired by the user** and the pairing is persisted by
//! stable target identity. It is never inferred from a locator or a label,
//! because both change without the device changing — and pairing the wrong two
//! volumes would write one device's metadata describing another's ROMs.
//!
//! # Paths are relative, always
//!
//! Every `<path>` a projection emits starts with `./` and is relative to the
//! configured system ROM directory. Host paths, Android absolute paths, mount
//! locations, MTP identifiers, and anything reaching across targets are
//! prohibited: a document that names where a file was on *this* computer is
//! wrong the moment it is read on the device.
//!
//! An unprovable layout blocks export rather than guessing at one.

use serde::{Deserialize, Serialize};

use crate::{PathError, RelativePath, sha256};

/// What a Media Target is being used for.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DestinationRole {
    RomContent,
    FrontendMetadata,
}

/// How the two roles are satisfied.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RoleAssignment {
    /// One target fulfils both roles.
    Combined { target_id: String },
    /// Two targets, explicitly paired by the user.
    Split {
        rom_content: String,
        frontend_metadata: String,
        /// Pairing is only usable once the user has confirmed it. An
        /// unconfirmed pairing blocks export.
        confirmed: bool,
    },
}

impl RoleAssignment {
    /// The target fulfilling `role`, or `None` when the assignment cannot be
    /// relied on.
    pub fn target_for(&self, role: DestinationRole) -> Option<&str> {
        match self {
            Self::Combined { target_id } => Some(target_id),
            // An unconfirmed pairing is not a pairing. Guessing here could
            // write one device's metadata describing another's ROMs.
            Self::Split {
                confirmed: false, ..
            } => None,
            Self::Split {
                rom_content,
                frontend_metadata,
                ..
            } => Some(match role {
                DestinationRole::RomContent => rom_content,
                DestinationRole::FrontendMetadata => frontend_metadata,
            }),
        }
    }

    /// Whether both roles resolve. An unprovable layout blocks export.
    pub fn is_usable(&self) -> bool {
        self.target_for(DestinationRole::RomContent).is_some()
            && self.target_for(DestinationRole::FrontendMetadata).is_some()
    }

    /// True when one target holds both roles, which changes the ordering
    /// obligations during sync.
    pub fn is_combined(&self) -> bool {
        matches!(self, Self::Combined { .. })
    }
}

/// The version-pinned ES-DE on Android profile.
///
/// Pinned deliberately: ES-DE's layout and gamelist semantics are a moving
/// target, and a profile that silently tracked whatever the user installed
/// would make every exported document unreproducible.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EsdeProfile {
    pub id: String,
    pub revision: u32,
    /// The ES-DE release this profile was validated against.
    pub esde_version: String,
    /// Where ROMs go, within the `rom-content` target.
    pub rom_root: RelativePath,
    /// The ES-DE application-data root, within the `frontend-metadata` target.
    pub metadata_root: RelativePath,
    pub extensions: Vec<String>,
    /// ES-DE's key for this system, used both in the gamelist path and as the
    /// ROM subdirectory name.
    pub system_key: String,
}

#[derive(Serialize)]
struct EsdeSnapshot<'a> {
    esde_version: &'a str,
    extensions: &'a [String],
    gamelist_template: &'a str,
    metadata_root: &'a str,
    rom_root: &'a str,
    system_key: &'a str,
}

impl EsdeProfile {
    /// The NES profile the tracer uses.
    pub fn nes() -> Self {
        Self {
            id: "esde-android".into(),
            revision: 1,
            esde_version: "3.1.1".into(),
            rom_root: RelativePath::new("ROMs").expect("built-in path is valid"),
            metadata_root: RelativePath::new("ES-DE").expect("built-in path is valid"),
            extensions: vec![".nes".into()],
            system_key: "nes".into(),
        }
    }

    /// Digest over the behavior-bearing fields, frozen like the Generic
    /// profile's so behaviour cannot drift without a revision bump.
    pub fn snapshot_digest(&self) -> String {
        let snapshot = EsdeSnapshot {
            esde_version: &self.esde_version,
            extensions: &self.extensions,
            gamelist_template: "gamelists/<system-key>/gamelist.xml",
            metadata_root: self.metadata_root.as_str(),
            rom_root: self.rom_root.as_str(),
            system_key: &self.system_key,
        };
        sha256(&serde_json::to_vec(&snapshot).expect("profile snapshot is serializable"))
    }

    /// Where a ROM goes within the `rom-content` target.
    pub fn rom_target_path(&self, file_name: &str) -> Result<RelativePath, PathError> {
        if file_name.contains(['/', '\\']) {
            return Err(PathError::of(file_name));
        }
        let accepted = self.extensions.iter().any(|extension| {
            file_name
                .to_lowercase()
                .ends_with(&extension.to_lowercase())
        });
        if !accepted {
            return Err(PathError::of(file_name));
        }
        RelativePath::canonicalize(format!(
            "{}/{}/{}",
            self.rom_root, self.system_key, file_name
        ))
    }

    /// Where this system's gamelist lives within the `frontend-metadata`
    /// target.
    pub fn gamelist_path(&self) -> Result<RelativePath, PathError> {
        RelativePath::canonicalize(format!(
            "{}/gamelists/{}/gamelist.xml",
            self.metadata_root, self.system_key
        ))
    }

    /// The `<path>` value for a ROM, as it must appear in the document.
    ///
    /// Always `./`-relative to the configured system ROM directory, so the
    /// document means the same thing wherever the device mounts its storage.
    pub fn gamelist_entry_path(&self, file_name: &str) -> Result<String, PathError> {
        // Validate through the same namespace rules the target path uses, so a
        // name that could never be placed can never be described either.
        self.rom_target_path(file_name)?;
        Ok(format!("./{file_name}"))
    }
}
