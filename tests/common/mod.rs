//! Fixture helpers shared by the sync-core test suites.
//!
//! Each suite pulls in only what it uses, so `dead_code` is expected here.
#![allow(dead_code)]

use rom_manager::{
    DeviceProfile, FakeTransport, ManagedArtifactManifest, ManagedEvidence, ManagementOrigin,
    RelativePath, SyncCore, TargetArtifact,
};

pub const TARGET_ID: &str = "target-fixture-001";
pub const ROM_BYTES: &[u8] = include_bytes!("../../fixtures/nes/tracers.nes");
/// Where the Generic profile places the fixture ROM Set.
pub const DESIRED: &str = "ROMs/nes/Tracers.nes";
/// Comfortably larger than the fixture, so capacity never incidentally blocks.
pub const CAPACITY: u64 = 8 * 1024 * 1024;

pub fn path(value: &str) -> RelativePath {
    RelativePath::new(value).expect("test path is a valid target path")
}

pub fn expected() -> TargetArtifact {
    TargetArtifact::new("rom-set-tracer", path(DESIRED), ROM_BYTES.to_vec())
}

pub fn fake() -> FakeTransport {
    FakeTransport::new("wpd://odin/storage", CAPACITY)
}

/// An initialized core wanting exactly the fixture ROM Set on `transport`.
pub fn core_with(transport: FakeTransport) -> SyncCore<FakeTransport> {
    let mut core = SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    );
    core.initialize_target(true)
        .expect("a fresh target initializes");
    core
}

/// A manifest claiming [`DESIRED`] holds `bytes`.
pub fn manifest_naming(bytes: &[u8]) -> ManagedArtifactManifest {
    let mut manifest = ManagedArtifactManifest::empty(TARGET_ID, &DeviceProfile::generic_nes());
    manifest.generation = 1;
    manifest.artifacts.insert(
        path(DESIRED),
        ManagedEvidence {
            rom_set_id: "rom-set-tracer".into(),
            size: bytes.len() as u64,
            sha256: rom_manager::sha256(bytes),
            origin: ManagementOrigin::Placed,
        },
    );
    manifest
}
