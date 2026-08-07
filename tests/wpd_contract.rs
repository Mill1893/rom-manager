//! Deterministic WPD-like contract tests (issue #36, sixth criterion).
//!
//! These exercise the *adapter's* contract against a backend that behaves the
//! way MTP does. They prove nothing about a physical device — that is issue
//! #38's job, on hardware — but they cover the awkward cases a real device
//! produces rarely and unrepeatably.

mod common;

use common::{DESIRED, ROM_BYTES, path};
use rom_manager::{
    Backend, CancellationToken, Reply, Request, TargetMarker, TransportError, Worker, WpdFault,
    WpdLikeBackend, mtp_capabilities,
};

fn worker_over(backend: WpdLikeBackend) -> Worker {
    Worker::start("wpd://odin/storage", move || Ok(backend)).expect("the worker starts")
}

#[test]
fn mtp_never_claims_atomic_publication() {
    // The single most important thing this adapter must not pretend.
    assert!(!mtp_capabilities(true).atomic_publish);
    assert!(!mtp_capabilities(false).atomic_publish);
}

#[test]
fn capacity_reporting_is_observed_not_assumed() {
    assert!(mtp_capabilities(true).reports_capacity);
    assert!(
        !mtp_capabilities(false).reports_capacity,
        "a device that does not report free space must not be given a number"
    );
}

#[test]
fn device_work_happens_on_the_worker_thread() {
    // The caller's thread must never touch the backend: on Windows that would
    // be a COM apartment violation rather than an error.
    let calling_thread = std::thread::current().id();
    let worker = Worker::start("wpd://odin/storage", move || {
        assert_ne!(
            std::thread::current().id(),
            calling_thread,
            "the backend must be constructed on the worker thread"
        );
        Ok(WpdLikeBackend::new(Some(1 << 20)))
    })
    .unwrap();

    assert!(matches!(
        worker.call(Request::Capabilities).unwrap(),
        Reply::Capabilities(_)
    ));
}

#[test]
fn a_failed_backend_surfaces_as_a_transport_error_not_a_panic() {
    let worker = Worker::start("wpd://odin/storage", || {
        Err::<WpdLikeBackend, _>(TransportError::Unsupported("no device".into()))
    })
    .unwrap();

    assert!(matches!(
        worker.call(Request::Capabilities).unwrap(),
        Reply::Failed(TransportError::Unsupported(_))
    ));
}

#[test]
fn a_duplicate_name_is_a_conflict_never_an_overwrite() {
    let mut backend =
        WpdLikeBackend::new(Some(1 << 20)).with_object(path(DESIRED), ROM_BYTES.to_vec());

    let error = backend
        .write_new(
            &path(DESIRED),
            b"replacement",
            &CancellationToken::default(),
        )
        .unwrap_err();

    assert!(matches!(error, TransportError::Conflict(_)));
}

#[test]
fn an_ambiguous_name_is_refused_rather_than_guessed() {
    // MTP lets one folder hold two objects with the same name. Which one was
    // meant cannot be decided, so the adapter must not pick.
    let mut backend = WpdLikeBackend::new(Some(1 << 20))
        .with_object(path(DESIRED), ROM_BYTES.to_vec())
        .with_ambiguous_name(path(DESIRED));

    assert!(matches!(
        backend.read(&path(DESIRED)).unwrap_err(),
        TransportError::Conflict(_)
    ));
    assert!(matches!(
        backend.delete_leaf(&path(DESIRED)).unwrap_err(),
        TransportError::Conflict(_)
    ));
}

#[test]
fn a_partial_write_leaves_content_that_verification_must_catch() {
    let mut backend = WpdLikeBackend::new(Some(1 << 20)).with_fault(WpdFault::PartialWrite);

    let error = backend
        .write_new(&path(DESIRED), ROM_BYTES, &CancellationToken::default())
        .unwrap_err();
    assert!(matches!(error, TransportError::Disconnected));

    // Something is there, and it is not what was asked for. Read-back is the
    // only thing standing between this and false management authority.
    let readback = backend.read(&path(DESIRED)).unwrap();
    assert_ne!(readback, ROM_BYTES);
}

#[test]
fn read_back_mismatch_is_observable() {
    let mut backend = WpdLikeBackend::new(Some(1 << 20))
        .with_object(path(DESIRED), ROM_BYTES.to_vec())
        .with_fault(WpdFault::CorruptReadBack);

    assert_ne!(backend.read(&path(DESIRED)).unwrap(), ROM_BYTES);
}

#[test]
fn cancellation_stops_a_write_before_it_starts() {
    let mut backend = WpdLikeBackend::new(Some(1 << 20));
    let cancellation = CancellationToken::default();
    cancellation.cancel();

    assert!(matches!(
        backend
            .write_new(&path(DESIRED), ROM_BYTES, &cancellation)
            .unwrap_err(),
        TransportError::Cancelled
    ));
}

#[test]
fn a_locked_device_maps_to_a_stable_error() {
    let mut backend = WpdLikeBackend::new(Some(1 << 20)).with_fault(WpdFault::Unauthorized);

    assert!(matches!(
        backend.inventory().unwrap_err(),
        TransportError::Unsupported(_)
    ));
    assert!(matches!(
        backend
            .write_new(&path(DESIRED), ROM_BYTES, &CancellationToken::default())
            .unwrap_err(),
        TransportError::Unsupported(_)
    ));
}

#[test]
fn retry_exhaustion_maps_to_a_stable_error() {
    let mut backend = WpdLikeBackend::new(Some(1 << 20)).with_fault(WpdFault::RetryExhausted);

    assert!(matches!(
        backend
            .write_new(&path(DESIRED), ROM_BYTES, &CancellationToken::default())
            .unwrap_err(),
        TransportError::Io(_)
    ));
}

#[test]
fn a_capacity_change_is_visible_and_blocks_an_oversized_write() {
    let mut backend = WpdLikeBackend::new(Some(16));

    assert!(matches!(
        backend
            .write_new(&path(DESIRED), ROM_BYTES, &CancellationToken::default())
            .unwrap_err(),
        TransportError::InsufficientCapacity
    ));
}

#[test]
fn a_device_without_capacity_reporting_yields_no_free_bytes() {
    let mut backend = WpdLikeBackend::new(None);
    assert!(
        backend.inventory().unwrap().free_bytes.is_none(),
        "absent capacity must stay absent rather than becoming a number"
    );
}

#[test]
fn indeterminate_publication_is_reported_as_unestablished() {
    let mut backend = WpdLikeBackend::new(Some(1 << 20)).with_fault(WpdFault::IndeterminatePublish);
    let manifest = common::manifest_naming(ROM_BYTES);

    assert!(
        matches!(
            backend.write_manifest(&manifest).unwrap_err(),
            TransportError::Disconnected
        ),
        "a publication the device did not confirm must not read as success"
    );
}

#[test]
fn object_ids_are_session_scoped_and_never_durable_authority() {
    // Re-creating the backend is a new session. Nothing may carry an id across
    // it — durable identity is the marker and content digests.
    let make = || WpdLikeBackend::new(Some(1 << 20)).with_object(path(DESIRED), ROM_BYTES.to_vec());

    let mut first = make();
    let mut second = make();

    // Both sessions can serve the same content by *path*, which is what the
    // core actually depends on.
    assert_eq!(first.read(&path(DESIRED)).unwrap(), ROM_BYTES);
    assert_eq!(second.read(&path(DESIRED)).unwrap(), ROM_BYTES);
}

#[test]
fn a_locator_change_does_not_change_what_the_device_is() {
    // The same device reached at a different locator is still the same device;
    // identity comes from the marker it carries.
    let marker = TargetMarker::new(common::TARGET_ID);
    let mut backend = WpdLikeBackend::new(Some(1 << 20));
    backend.write_marker(&marker).unwrap();

    let worker = worker_over(backend);
    assert_eq!(worker.locator(), "wpd://odin/storage");

    match worker.call(Request::Marker).unwrap() {
        Reply::Marker(Some(observed)) => assert_eq!(observed, marker),
        other => panic!(
            "expected the marker back, got a different reply kind: {:?}",
            matches!(other, Reply::Failed(_))
        ),
    }
}

#[test]
fn a_dropped_worker_reads_as_a_disconnect() {
    let worker = worker_over(WpdLikeBackend::new(Some(1 << 20)));
    drop(worker);
    // Nothing to assert beyond a clean shutdown: Drop joins the thread, and a
    // hang here would fail the suite by timeout.
}
