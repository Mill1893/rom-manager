//! Durable sync-core state (issue #33).
//!
//! # Two rules this module exists to hold
//!
//! **No external I/O inside a transaction.** Every method here does database
//! work only. Transport calls, filesystem access, and hashing happen in the
//! caller, between short serialized writes. A transaction that waited on a
//! removable device would hold the write lock for as long as the device felt
//! like taking.
//!
//! **Nothing is trusted because a caller supplied it.** A Sync Plan is stored
//! under its own digest and revalidated on load, so a plan that arrives from a
//! frontend action cannot substitute for the one that was approved.
//!
//! Recovery is never automatic. [`Store::recover_interrupted`] marks operations
//! that were running when the process died as **indeterminate** and their
//! target's inventory as **stale**; the user must refresh, re-plan, and
//! re-approve. Resuming would assert knowledge of a target state the
//! application never observed.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::{Approval, ManagedArtifactManifest, SyncPlan};

/// Checked-in schema migrations, applied in order. Never edited once shipped —
/// a correction is the next migration.
const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("../migrations/0001_initial.sql")),
    (2, include_str!("../migrations/0002_library.sql")),
    (3, include_str!("../migrations/0003_library_storage.sql")),
    (4, include_str!("../migrations/0004_source_containers.sql")),
    (5, include_str!("../migrations/0005_import_folders.sql")),
    (6, include_str!("../migrations/0006_nominations.sql")),
];

/// The schema version this build expects. A store opened at a lower version is
/// migrated up; one opened at a *higher* version was written by a newer build
/// and is refused rather than guessed at.
pub const SCHEMA_VERSION: u32 = 6;

/// A Media Target as durable state knows it.
///
/// `last_locator` is where it was most recently seen, deliberately separate
/// from `target_id`. Identity lives in the device's marker and never changes; a
/// card that comes back on a different drive letter is the same target, and the
/// locator is only where the application tries to reach it first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetRow {
    pub target_id: String,
    /// `None` for a target adopted from a marker nobody has named yet.
    pub label: Option<String>,
    pub last_locator: Option<String>,
}

/// A ROM Pack revision as durable state knows it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackRow {
    pub rom_pack_id: String,
    pub revision: u32,
    pub title: Option<String>,
    pub rom_set_count: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error("durable state is corrupt: {0}")]
    Corrupt(String),
    #[error("the stored Sync Plan does not match its digest")]
    PlanDigestMismatch,
}

/// How a finished operation ended, as recorded durably.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationState {
    Running,
    Completed,
    Cancelled,
    Incomplete,
    Indeterminate,
}

impl OperationState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Incomplete => "incomplete",
            Self::Indeterminate => "indeterminate",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        Ok(match value {
            "running" => Self::Running,
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            "incomplete" => Self::Incomplete,
            "indeterminate" => Self::Indeterminate,
            other => {
                return Err(StoreError::Corrupt(format!(
                    "unknown operation state {other}"
                )));
            }
        })
    }
}

pub struct Store {
    connection: Connection,
}

impl Store {
    /// Opens `path`, applying any migrations it has not yet seen.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self, StoreError> {
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), StoreError> {
        let applied: u32 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        // A store written by a newer build is refused rather than guessed at.
        // Migrating forward is defined; interpreting a schema this build has
        // never seen is not, and writing into it could produce rows the newer
        // build reads back as something else.
        if applied > SCHEMA_VERSION {
            return Err(StoreError::Corrupt(format!(
                "durable state is at schema version {applied}, newer than this build's \
                 {SCHEMA_VERSION}"
            )));
        }
        for (version, sql) in MIGRATIONS {
            if *version > applied {
                self.connection.execute_batch(sql)?;
                self.connection
                    .pragma_update(None, "user_version", *version)?;
            }
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<u32, StoreError> {
        Ok(self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    // ---- Media Target identity -------------------------------------------

    pub fn upsert_target(&self, target_id: &str, marker_schema: u32) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO media_target (target_id, marker_schema) VALUES (?1, ?2)
             ON CONFLICT(target_id) DO UPDATE SET marker_schema = excluded.marker_schema",
            params![target_id, marker_schema],
        )?;
        Ok(())
    }

    /// Records where a target was last seen. The locator is evidence about this
    /// connection, never part of the target's identity, so re-binding at a new
    /// locator leaves the target — and its manifest — untouched.
    pub fn record_binding(
        &self,
        target_id: &str,
        locator: &str,
        capabilities: &crate::TransportCapabilities,
        observed_at: i64,
    ) -> Result<(), StoreError> {
        let capabilities = serde_json::to_string(capabilities)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        self.connection.execute(
            "INSERT INTO transport_binding (target_id, locator, capabilities, observed_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(target_id, locator) DO UPDATE SET
                 capabilities = excluded.capabilities,
                 observed_at  = excluded.observed_at",
            params![target_id, locator, capabilities, observed_at],
        )?;
        Ok(())
    }

    /// Names a Media Target for display. Identity is unaffected.
    pub fn set_target_label(&self, target_id: &str, label: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE media_target SET label = ?2 WHERE target_id = ?1",
            params![target_id, label],
        )?;
        Ok(())
    }

    /// Every known Media Target.
    pub fn media_targets(&self) -> Result<Vec<TargetRow>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT t.target_id, t.label,
                    (SELECT b.locator FROM transport_binding b
                      WHERE b.target_id = t.target_id
                      ORDER BY b.observed_at DESC LIMIT 1)
               FROM media_target t
              ORDER BY COALESCE(t.label, t.target_id)",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(TargetRow {
                    target_id: row.get(0)?,
                    label: row.get(1)?,
                    last_locator: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn set_pack_title(
        &self,
        rom_pack_id: &str,
        revision: u32,
        title: &str,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE rom_pack SET title = ?3 WHERE rom_pack_id = ?1 AND revision = ?2",
            params![rom_pack_id, revision, title],
        )?;
        Ok(())
    }

    /// Every known ROM Pack revision.
    pub fn rom_packs(&self) -> Result<Vec<PackRow>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT p.rom_pack_id, p.revision, p.title,
                    (SELECT COUNT(*) FROM rom_pack_selection s
                      WHERE s.rom_pack_id = p.rom_pack_id AND s.revision = p.revision)
               FROM rom_pack p
              ORDER BY COALESCE(p.title, p.rom_pack_id), p.revision",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(PackRow {
                    rom_pack_id: row.get(0)?,
                    revision: row.get::<_, i64>(1)? as u32,
                    title: row.get(2)?,
                    rom_set_count: row.get::<_, i64>(3)? as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn bindings_for(&self, target_id: &str) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT locator FROM transport_binding WHERE target_id = ?1 ORDER BY locator",
        )?;
        let rows = statement.query_map(params![target_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // ---- Library entities -------------------------------------------------

    /// Records one Platform-scoped Game with a Release and a ROM Set, which is
    /// the shape the fixture takes.
    pub fn upsert_rom_set(
        &self,
        game: (&str, &str, &str),
        release: (&str, &str),
        rom_set: (&str, &str, &str, u64),
    ) -> Result<(), StoreError> {
        let (game_id, platform, title) = game;
        let (release_id, label) = release;
        let (rom_set_id, content_digest, file_name, size) = rom_set;
        self.connection.execute(
            "INSERT INTO game (game_id, platform, title) VALUES (?1, ?2, ?3)
             ON CONFLICT(game_id) DO UPDATE SET platform = excluded.platform,
                                                title = excluded.title",
            params![game_id, platform, title],
        )?;
        self.connection.execute(
            "INSERT INTO release (release_id, game_id, label) VALUES (?1, ?2, ?3)
             ON CONFLICT(release_id) DO UPDATE SET label = excluded.label",
            params![release_id, game_id, label],
        )?;
        self.connection.execute(
            "INSERT INTO rom_set (rom_set_id, release_id, content_digest, file_name, size)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(rom_set_id) DO UPDATE SET content_digest = excluded.content_digest,
                                                   file_name = excluded.file_name,
                                                   size = excluded.size",
            params![rom_set_id, release_id, content_digest, file_name, size],
        )?;
        Ok(())
    }

    /// Records a ROM Pack revision and exactly which ROM Sets it selects.
    ///
    /// The content digest is stored alongside the id, so a reload can tell that
    /// a selection still means what it meant — an exact selection never
    /// silently becomes a different one.
    pub fn record_pack_selection(
        &self,
        rom_pack_id: &str,
        revision: u32,
        selection: &[(&str, &str)],
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO rom_pack (rom_pack_id, revision, body) VALUES (?1, ?2, '{}')
             ON CONFLICT(rom_pack_id, revision) DO NOTHING",
            params![rom_pack_id, revision],
        )?;
        for (rom_set_id, content_digest) in selection {
            self.connection.execute(
                "INSERT INTO rom_pack_selection (rom_pack_id, revision, rom_set_id, content_digest)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(rom_pack_id, revision, rom_set_id) DO NOTHING",
                params![rom_pack_id, revision, rom_set_id, content_digest],
            )?;
        }
        Ok(())
    }

    /// The exact selection for a ROM Pack revision, as `(rom_set_id, digest)`.
    pub fn pack_selection(
        &self,
        rom_pack_id: &str,
        revision: u32,
    ) -> Result<Vec<(String, String)>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT rom_set_id, content_digest FROM rom_pack_selection
              WHERE rom_pack_id = ?1 AND revision = ?2 ORDER BY rom_set_id",
        )?;
        let rows = statement.query_map(params![rom_pack_id, revision], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // ---- App-owned Library storage ---------------------------------------

    /// Records an owned source object. Immutable: re-recording identical bytes
    /// leaves the original row, and its import time, alone.
    pub fn record_source_object(
        &self,
        content_digest: &str,
        size: u64,
        stored_path: &str,
        imported_at: i64,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO source_object (content_digest, size, stored_path, imported_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(content_digest) DO NOTHING",
            params![content_digest, size, stored_path, imported_at],
        )?;
        Ok(())
    }

    /// Records where bytes were seen. Provenance only — losing this costs the
    /// trail back to the original file, never the content.
    pub fn record_origin_observation(
        &self,
        content_digest: &str,
        external_path: &str,
        file_name: &str,
        observed_at: i64,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO origin_observation
                 (content_digest, external_path, file_name, observed_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(content_digest, external_path)
                 DO UPDATE SET observed_at = excluded.observed_at,
                               unavailable_at = NULL",
            params![content_digest, external_path, file_name, observed_at],
        )?;
        Ok(())
    }

    /// `(size, stored_path, health)` for an owned object.
    pub fn source_object(
        &self,
        content_digest: &str,
    ) -> Result<Option<(u64, String, String)>, StoreError> {
        Ok(self
            .connection
            .query_row(
                "SELECT size, stored_path, health FROM source_object WHERE content_digest = ?1",
                params![content_digest],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?)
    }

    /// Every path these bytes have been seen at, oldest first.
    pub fn origin_observations(&self, content_digest: &str) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT external_path FROM origin_observation
              WHERE content_digest = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map(params![content_digest], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// How many Library identities still need this object.
    ///
    /// A shared object stays while any retained identity needs it. This is what
    /// makes deduplication safe: collapsing two imports into one object must
    /// never mean that removing one identity takes the other's content with it.
    pub fn object_reference_count(&self, content_digest: &str) -> Result<usize, StoreError> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*) FROM rom_set WHERE content_digest = ?1",
            params![content_digest],
            |row| row.get::<_, i64>(0),
        )? as usize)
    }

    /// Whether removing `content_digest` would strand a retained identity.
    pub fn object_is_still_needed(&self, content_digest: &str) -> Result<bool, StoreError> {
        Ok(self.object_reference_count(content_digest)? > 0)
    }

    /// Records an archive as a Source Container and what it was read to hold.
    pub fn record_container(
        &self,
        container_digest: &str,
        format: &str,
        members: &[(String, String, u64)],
        read_at: i64,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO source_container (content_digest, format, member_count, read_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(content_digest) DO UPDATE SET member_count = excluded.member_count,
                                                       read_at = excluded.read_at",
            params![container_digest, format, members.len(), read_at],
        )?;
        for (member_path, content_digest, size) in members {
            self.connection.execute(
                "INSERT INTO container_member
                     (container_digest, member_path, content_digest, size)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(container_digest, member_path) DO NOTHING",
                params![container_digest, member_path, content_digest, size],
            )?;
        }
        Ok(())
    }

    /// Members of a container, as `(member_path, content_digest)`.
    pub fn container_members(
        &self,
        container_digest: &str,
    ) -> Result<Vec<(String, String)>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT member_path, content_digest FROM container_member
              WHERE container_digest = ?1 ORDER BY member_path",
        )?;
        let rows = statement.query_map(params![container_digest], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every container holding a ROM with this content identity.
    pub fn containers_holding(&self, content_digest: &str) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT DISTINCT container_digest FROM container_member
              WHERE content_digest = ?1 ORDER BY container_digest",
        )?;
        let rows = statement.query_map(params![content_digest], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Marks an object's bytes as not what was stored.
    ///
    /// This is corruption, never an update: the recorded content digest is what
    /// the object *is*, so bytes that disagree are the thing that is wrong.
    pub fn quarantine_object(&self, content_digest: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE source_object SET health = 'quarantined' WHERE content_digest = ?1",
            params![content_digest],
        )?;
        Ok(())
    }

    /// Restores a quarantined object after its bytes were replaced from another
    /// exact occurrence or a strongly matching reimport.
    pub fn restore_object(&self, content_digest: &str, verified_at: i64) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE source_object SET health = 'healthy', verified_at = ?2
              WHERE content_digest = ?1",
            params![content_digest, verified_at],
        )?;
        Ok(())
    }

    pub fn record_verification(
        &self,
        content_digest: &str,
        verified_at: i64,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE source_object SET verified_at = ?2 WHERE content_digest = ?1",
            params![content_digest, verified_at],
        )?;
        Ok(())
    }

    pub fn object_is_healthy(&self, content_digest: &str) -> Result<bool, StoreError> {
        Ok(self
            .source_object(content_digest)?
            .is_some_and(|(_, _, health)| health == "healthy"))
    }

    /// Every owned object, oldest import first.
    pub fn owned_objects(&self) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT content_digest FROM source_object ORDER BY imported_at, content_digest",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Remembers a folder to look in. Remembering is not scanning.
    pub fn remember_import_folder(
        &self,
        path: &str,
        default_platform: Option<&str>,
    ) -> Result<i64, StoreError> {
        self.connection.execute(
            "INSERT INTO import_folder (path, default_platform) VALUES (?1, ?2)
             ON CONFLICT(path) DO UPDATE SET default_platform = excluded.default_platform",
            params![path, default_platform],
        )?;
        Ok(self.connection.query_row(
            "SELECT folder_id FROM import_folder WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )?)
    }

    /// Points a remembered folder at a new location.
    ///
    /// Affects future discovery only. Library content came from the bytes, not
    /// from the folder, so relinking never touches what was already imported.
    pub fn relink_import_folder(&self, folder_id: i64, new_path: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE import_folder SET path = ?2 WHERE folder_id = ?1",
            params![folder_id, new_path],
        )?;
        Ok(())
    }

    /// Forgets a folder. Provenance only — Library content is untouched.
    pub fn forget_import_folder(&self, folder_id: i64) -> Result<(), StoreError> {
        self.connection.execute(
            "DELETE FROM import_folder WHERE folder_id = ?1",
            params![folder_id],
        )?;
        Ok(())
    }

    pub fn import_folders(&self) -> Result<Vec<(i64, String)>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT folder_id, path FROM import_folder ORDER BY folder_id")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn mark_folder_scanned(&self, folder_id: i64, at: i64) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE import_folder SET last_scanned_at = ?2 WHERE folder_id = ?1",
            params![folder_id, at],
        )?;
        Ok(())
    }

    /// Marks an observation as no longer findable where it was seen.
    pub fn mark_observation_unavailable(
        &self,
        content_digest: &str,
        external_path: &str,
        at: i64,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE origin_observation SET unavailable_at = ?3
              WHERE content_digest = ?1 AND external_path = ?2",
            params![content_digest, external_path, at],
        )?;
        Ok(())
    }

    /// Observations still findable where they were last seen.
    pub fn available_observations(&self, content_digest: &str) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT external_path FROM origin_observation
              WHERE content_digest = ?1 AND unavailable_at IS NULL ORDER BY id",
        )?;
        let rows = statement.query_map(params![content_digest], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The content last seen at `external_path`, if anything was.
    pub fn observation_at(&self, external_path: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .connection
            .query_row(
                "SELECT content_digest FROM origin_observation
                  WHERE external_path = ?1 AND unavailable_at IS NULL LIMIT 1",
                params![external_path],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// ROM Sets whose content is these bytes.
    pub fn rom_sets_using(&self, content_digest: &str) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT rom_set_id FROM rom_set WHERE content_digest = ?1 ORDER BY rom_set_id",
        )?;
        let rows = statement.query_map(params![content_digest], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// ROM Packs that select content with this identity.
    pub fn rom_packs_selecting_content(
        &self,
        content_digest: &str,
    ) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT DISTINCT rom_pack_id FROM rom_pack_selection
              WHERE content_digest = ?1 ORDER BY rom_pack_id",
        )?;
        let rows = statement.query_map(params![content_digest], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Drops an owned object's record. Library identities that referenced it
    /// are deliberately left alone — they become unavailable, not deleted.
    pub fn forget_source_object(&self, content_digest: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "DELETE FROM origin_observation WHERE content_digest = ?1",
            params![content_digest],
        )?;
        self.connection.execute(
            "DELETE FROM source_object WHERE content_digest = ?1",
            params![content_digest],
        )?;
        Ok(())
    }

    /// Deletes a ROM Set identity, refusing while a ROM Pack still selects it.
    ///
    /// No action here silently cascades: a selection is a promise the user made
    /// and only the user can withdraw it.
    pub fn delete_rom_set(&self, rom_set_id: &str) -> Result<(), StoreError> {
        let selected: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM rom_pack_selection WHERE rom_set_id = ?1",
            params![rom_set_id],
            |row| row.get(0),
        )?;
        if selected > 0 {
            return Err(StoreError::Corrupt(format!(
                "{rom_set_id} is selected by {selected} ROM Pack revision(s)"
            )));
        }
        self.connection.execute(
            "DELETE FROM rom_set WHERE rom_set_id = ?1",
            params![rom_set_id],
        )?;
        Ok(())
    }

    pub fn rom_set_exists(&self, rom_set_id: &str) -> Result<bool, StoreError> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*) FROM rom_set WHERE rom_set_id = ?1",
            params![rom_set_id],
            |row| row.get::<_, i64>(0),
        )? > 0)
    }

    pub fn owned_object_count(&self) -> Result<usize, StoreError> {
        Ok(self
            .connection
            .query_row("SELECT COUNT(*) FROM source_object", [], |row| {
                row.get::<_, i64>(0)
            })? as usize)
    }

    // ---- Sync Plans and approvals ----------------------------------------

    /// Stores a plan under its own digest. Immutable: re-storing the same plan
    /// is a no-op, and a different plan can never occupy the same digest.
    pub fn save_plan(&self, plan: &SyncPlan, created_at: i64) -> Result<(), StoreError> {
        let body =
            serde_json::to_string(plan).map_err(|error| StoreError::Corrupt(error.to_string()))?;
        self.connection.execute(
            "INSERT INTO sync_plan (digest, target_id, body, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(digest) DO NOTHING",
            params![plan.digest, plan.target_id, body, created_at],
        )?;
        Ok(())
    }

    /// Loads a plan **by identity**, re-checking that the stored bytes still
    /// hash to the digest they are filed under. Corruption or tampering is a
    /// refusal, not a warning.
    pub fn load_plan(&self, digest: &str) -> Result<Option<SyncPlan>, StoreError> {
        let body: Option<String> = self
            .connection
            .query_row(
                "SELECT body FROM sync_plan WHERE digest = ?1",
                params![digest],
                |row| row.get(0),
            )
            .optional()?;
        let Some(body) = body else {
            return Ok(None);
        };
        let plan: SyncPlan =
            serde_json::from_str(&body).map_err(|error| StoreError::Corrupt(error.to_string()))?;
        if plan.digest != digest || !plan.digest_is_valid() {
            return Err(StoreError::PlanDigestMismatch);
        }
        Ok(Some(plan))
    }

    pub fn save_approval(&self, approval: &Approval, granted_at: i64) -> Result<(), StoreError> {
        let body = serde_json::to_string(&ApprovalRow::from(approval))
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        self.connection.execute(
            "INSERT INTO plan_approval (plan_digest, body, granted_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(plan_digest) DO UPDATE SET body = excluded.body,
                                                    granted_at = excluded.granted_at",
            params![approval.plan_digest, body, granted_at],
        )?;
        Ok(())
    }

    /// Consumes the approval for `plan_digest`. Single use is enforced durably
    /// as well as in the type system: the row is gone after this returns.
    pub fn take_approval(&self, plan_digest: &str) -> Result<Option<Approval>, StoreError> {
        let body: Option<String> = self
            .connection
            .query_row(
                "DELETE FROM plan_approval WHERE plan_digest = ?1 RETURNING body",
                params![plan_digest],
                |row| row.get(0),
            )
            .optional()?;
        body.map(|body| {
            serde_json::from_str::<ApprovalRow>(&body)
                .map(ApprovalRow::into_approval)
                .map_err(|error| StoreError::Corrupt(error.to_string()))
        })
        .transpose()
    }

    // ---- Operations -------------------------------------------------------

    /// Marks an operation as running *before* any mutation begins, so a crash
    /// mid-operation is always visible on the next start.
    pub fn begin_operation(
        &self,
        plan_digest: &str,
        target_id: &str,
        started_at: i64,
    ) -> Result<i64, StoreError> {
        self.connection.execute(
            "INSERT INTO operation (plan_digest, target_id, state, started_at)
             VALUES (?1, ?2, 'running', ?3)",
            params![plan_digest, target_id, started_at],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn finish_operation(
        &self,
        id: i64,
        state: OperationState,
        reason: Option<&str>,
        report: Option<&crate::OperationReport>,
        finished_at: i64,
    ) -> Result<(), StoreError> {
        let report = report
            .map(|report| serde_json::to_string(&ReportRow::from(report)))
            .transpose()
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        self.connection.execute(
            "UPDATE operation SET state = ?2, reason = ?3, report = ?4, finished_at = ?5
             WHERE id = ?1",
            params![id, state.as_str(), reason, report, finished_at],
        )?;
        Ok(())
    }

    pub fn operation_state(&self, id: i64) -> Result<Option<OperationState>, StoreError> {
        let state: Option<String> = self
            .connection
            .query_row(
                "SELECT state FROM operation WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        state.map(|state| OperationState::parse(&state)).transpose()
    }

    /// Startup recovery. Any operation still marked running was interrupted by
    /// a crash or power loss, so what reached the target is unknown: it becomes
    /// **indeterminate**, and its target's inventory is marked stale.
    ///
    /// Returns the affected operation ids. Nothing is resumed — the user must
    /// refresh, re-plan, and re-approve.
    pub fn recover_interrupted(&self, recovered_at: i64) -> Result<Vec<i64>, StoreError> {
        let ids: Vec<i64> = {
            let mut statement = self
                .connection
                .prepare("SELECT id FROM operation WHERE state = 'running' ORDER BY id")?;
            let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
            rows.collect::<Result<_, _>>()?
        };
        if ids.is_empty() {
            return Ok(ids);
        }
        self.connection.execute(
            "UPDATE operation
                SET state = 'indeterminate',
                    reason = 'the application stopped while this operation was running',
                    finished_at = ?1
              WHERE state = 'running'",
            params![recovered_at],
        )?;
        self.connection.execute(
            "UPDATE inventory_snapshot SET stale = 1
              WHERE target_id IN (SELECT target_id FROM operation WHERE id IN (
                  SELECT id FROM operation WHERE state = 'indeterminate'))",
            [],
        )?;
        Ok(ids)
    }

    // ---- Mirrored manifest and inventory freshness ------------------------

    pub fn save_manifest(&self, manifest: &ManagedArtifactManifest) -> Result<(), StoreError> {
        let body = serde_json::to_string(manifest)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        self.connection.execute(
            "INSERT INTO managed_manifest (target_id, generation, body) VALUES (?1, ?2, ?3)
             ON CONFLICT(target_id) DO UPDATE SET generation = excluded.generation,
                                                  body = excluded.body",
            params![manifest.target_id, manifest.generation, body],
        )?;
        Ok(())
    }

    pub fn load_manifest(
        &self,
        target_id: &str,
    ) -> Result<Option<ManagedArtifactManifest>, StoreError> {
        let body: Option<String> = self
            .connection
            .query_row(
                "SELECT body FROM managed_manifest WHERE target_id = ?1",
                params![target_id],
                |row| row.get(0),
            )
            .optional()?;
        body.map(|body| {
            serde_json::from_str(&body).map_err(|error| StoreError::Corrupt(error.to_string()))
        })
        .transpose()
    }

    pub fn record_inventory(
        &self,
        target_id: &str,
        digest: &str,
        observed_at: i64,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO inventory_snapshot (target_id, digest, stale, observed_at)
             VALUES (?1, ?2, 0, ?3)
             ON CONFLICT(target_id) DO UPDATE SET digest = excluded.digest,
                                                  stale = 0,
                                                  observed_at = excluded.observed_at",
            params![target_id, digest, observed_at],
        )?;
        Ok(())
    }

    /// The recorded inventory digest, or `None` when there is none **or when it
    /// is stale**. A stale snapshot is not weaker evidence, it is absent
    /// evidence: planning must observe the target again.
    pub fn fresh_inventory_digest(&self, target_id: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .connection
            .query_row(
                "SELECT digest FROM inventory_snapshot WHERE target_id = ?1 AND stale = 0",
                params![target_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn mark_inventory_stale(&self, target_id: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE inventory_snapshot SET stale = 1 WHERE target_id = ?1",
            params![target_id],
        )?;
        Ok(())
    }
}

/// The durable shape of an [`Approval`].
///
/// `Approval` itself is deliberately neither `Clone` nor serializable — #44
/// specifies an in-memory, single-use contract, and durable storage belongs to
/// this slice. Keeping the wire shape here preserves both: the in-memory type
/// cannot be duplicated by a round-trip through serde, and the schema can
/// evolve without touching it.
#[derive(serde::Serialize, serde::Deserialize)]
struct ApprovalRow {
    plan_digest: String,
    removals_acked: usize,
    target_id: String,
    profile_id: String,
    profile_revision: u32,
    binding_locator: String,
    inventory_digest: String,
}

impl From<&Approval> for ApprovalRow {
    fn from(approval: &Approval) -> Self {
        Self {
            plan_digest: approval.plan_digest.clone(),
            removals_acked: approval.removals_acked,
            target_id: approval.target_id.clone(),
            profile_id: approval.profile_id.clone(),
            profile_revision: approval.profile_revision,
            binding_locator: approval.binding_locator.clone(),
            inventory_digest: approval.inventory_digest.clone(),
        }
    }
}

impl ApprovalRow {
    fn into_approval(self) -> Approval {
        Approval {
            plan_digest: self.plan_digest,
            removals_acked: self.removals_acked,
            target_id: self.target_id,
            profile_id: self.profile_id,
            profile_revision: self.profile_revision,
            binding_locator: self.binding_locator,
            inventory_digest: self.inventory_digest,
        }
    }
}

/// Operation reports are stored by path and action so a later schema can read
/// them without depending on today's in-memory types.
#[derive(serde::Serialize, serde::Deserialize)]
struct ReportRow {
    performed: Vec<String>,
    not_attempted: Vec<String>,
    uncertain: Vec<String>,
    residue: Vec<String>,
}

impl From<&crate::OperationReport> for ReportRow {
    fn from(report: &crate::OperationReport) -> Self {
        let paths = |actions: &[crate::PlanAction]| {
            actions
                .iter()
                .map(|action| action.path.as_str().to_owned())
                .collect()
        };
        Self {
            performed: paths(&report.performed),
            not_attempted: paths(&report.not_attempted),
            uncertain: paths(&report.uncertain),
            residue: report
                .residue
                .iter()
                .map(|residue| residue.path.as_str().to_owned())
                .collect(),
        }
    }
}
