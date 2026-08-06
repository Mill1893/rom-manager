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
const MIGRATIONS: &[(u32, &str)] = &[(1, include_str!("../migrations/0001_initial.sql"))];

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

    pub fn bindings_for(&self, target_id: &str) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT locator FROM transport_binding WHERE target_id = ?1 ORDER BY locator",
        )?;
        let rows = statement.query_map(params![target_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
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
