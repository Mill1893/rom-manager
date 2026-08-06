/// Marker-area locations. Behavior-bearing: they are part of the Device
/// Profile snapshot digest, so changing one requires a new profile revision.
pub(crate) const MARKER_PATH: &str = "ROMManager/target.json";
pub(crate) const MANIFEST_PATH: &str = "ROMManager/manifest.json";

mod app;
mod cache;
mod combined;
mod confined;
mod domain;
mod durable;
mod esde;
mod gamelist;
mod library;
mod merge;
mod projection;
mod publish;
mod retire;
mod store;
mod transport;
mod workflow;
mod wpd;

pub use app::{
    AppEvent, CancellationState, MediaTargetChoice, OutcomeKind, OutcomeView, Phase, PlanView,
    Progress, RomPackChoice, Snapshot, WizardStep,
};
pub use cache::{CacheError, Lease, MaterializationCache};
pub use combined::{
    COMBINED_ORDER, CombinedOutcome, MetadataAction, MetadataPreview, MetadataPreviewRow,
    SplitReadiness, SyncStage, run_combined, split_readiness,
};
pub use confined::ConfinedRoot;
pub use domain::{
    Action, Approval, BlockReason, DeviceProfile, ManagedArtifactManifest, ManagedEvidence,
    ManagementOrigin, PathError, PlanAction, RelativePath, SyncPlan, TargetArtifact, TargetMarker,
};
pub use durable::{DurableError, approve, execute_approved};
pub use esde::{DestinationRole, EsdeProfile, RoleAssignment};
pub use gamelist::{FRONTEND_OWNED_FIELDS, GameEntry, Gamelist, GamelistError, OWNED_FIELDS};
pub use library::{
    Container, DeletionBlocked, ImportError, Imported, IntegrityReport, Library, RemovalImpact,
    ScanReport, SetAvailability, SetState, Skipped,
};
pub use merge::{
    FieldOutcome, LedgerEntry, conflicts, merge_entry, merge_field, requires_user_decision,
};
pub use projection::{
    CalendarDate, EntryEligibility, MetadataProjection, PlayerCount, ReleaseFacts,
    disambiguate_titles,
};
pub use publish::{
    DocumentState, Publication, PublishError, PublishPreconditions, RecoveryChoice, RecoveryCopy,
    recover_missing_document,
};
pub use retire::{
    EligibilityAction, Ineligibility, ProjectionMove, Retirement, plan_retirement,
    withdraw_ineligible_field,
};
pub use store::{OperationState, SCHEMA_VERSION, Store, StoreError};
pub use transport::{
    CancellationToken, EntryKind, FakeFault, FakeTransport, FilesystemTransport, Inventory,
    InventoryArtifact, Transport, TransportCapabilities, TransportError,
};
pub use workflow::{ExecutionOutcome, OperationReport, Residue, SyncCore, SyncError};
#[cfg(windows)]
pub use wpd::Apartment;
pub use wpd::{Backend, Reply, Request, Worker, WpdFault, WpdLikeBackend, mtp_capabilities};

pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(bytes))
}
