/// Marker-area locations. Behavior-bearing: they are part of the Device
/// Profile snapshot digest, so changing one requires a new profile revision.
pub(crate) const MARKER_PATH: &str = "ROMManager/target.json";
pub(crate) const MANIFEST_PATH: &str = "ROMManager/manifest.json";

mod app;
mod cache;
mod combined;
mod confined;
pub mod containers;
pub mod descriptors;
mod domain;
mod durable;
mod esde;
mod filesystems;
pub mod formats;
mod gamelist;
mod library;
pub mod manifest;
mod membership;
mod merge;
mod outcomes;
mod paths;
mod projection;
mod provider;
mod publish;
mod retire;
mod session;
mod store;
mod transport;
pub mod worker;
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
pub use containers::{ContainerHeader, Format, validate_chd, validate_cso, validate_rvz};
pub use descriptors::{
    CueSheet, CueTrack, DescriptorError, Frames, Gdi, GdiRecord, MemberReference,
    membership_is_complete, parse_cue, parse_cue_sheet, parse_gdi, parse_gdi_model, parse_m3u,
    parse_m3u_for,
};
pub use domain::{
    Action, Approval, BlockReason, DeviceProfile, ManagedArtifactManifest, ManagedEvidence,
    ManagementOrigin, PathError, PlanAction, RelativePath, SyncPlan, TargetArtifact, TargetMarker,
};
pub use durable::{DurableError, approve, execute_approved};
pub use esde::{DestinationRole, EsdeProfile, RoleAssignment};
pub use filesystems::{
    FilesystemSupport, ObservedFilesystem, fits, maximum_file_size, support_for,
};
pub use formats::{
    AcceptedForm, BASELINE, Incompleteness, Representation, Support, UNSUPPORTED, forms_for,
    may_stand_alone, needs_members, resolve_members,
};
pub use gamelist::{FRONTEND_OWNED_FIELDS, GameEntry, Gamelist, GamelistError, OWNED_FIELDS};
pub use library::{
    Container, DeletionBlocked, ImportError, Imported, IntegrityReport, Library, RemovalImpact,
    ScanReport, SetAvailability, SetState, Skipped,
};
pub use manifest::{LIMITS, Limits, MANIFEST_REVISION};
pub use membership::{Assessment, Member, MemberClass, assess, classify, resolve_descriptor};
pub use merge::{
    FieldOutcome, LedgerEntry, conflicts, merge_entry, merge_field, requires_user_decision,
};
pub use outcomes::{Diagnostic, Location, Measurement, Outcome, ReasonCode};
pub use paths::AppPaths;
pub use projection::{
    CalendarDate, EntryEligibility, MetadataProjection, PlayerCount, ReleaseFacts,
    disambiguate_titles,
};
#[cfg(feature = "provider-http")]
pub use provider::http;
pub use provider::wire;
pub use provider::{
    Allowance, BatchRefusal, CachedLookup, CredentialReference, FixtureTransport, LookupOutcome,
    Provider, ProviderFailure, ProviderRecord, ProviderTransport,
    provider_artwork_may_reach_a_media_target, redact,
};
pub use publish::{
    DocumentState, Publication, PublishError, PublishPreconditions, RecoveryChoice, RecoveryCopy,
    recover_missing_document,
};
pub use retire::{
    EligibilityAction, Ineligibility, ProjectionMove, Retirement, plan_retirement,
    withdraw_ineligible_field,
};
pub use session::{Connect, Session, SessionError};
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
