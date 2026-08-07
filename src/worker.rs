//! The supervised decoder worker protocol (issue #19, under #17).
//!
//! # Why decoding is supervised rather than called
//!
//! #17: decoding "runs behind one supervised, versioned worker protocol with
//! bounded read handles, private staging access, no network access,
//! cancellation, and enforceable process limits."
//!
//! The decoders this release depends on parse adversarial binary formats. A
//! malformed CHD map or a crafted RVZ chunk table can drive a correct-looking
//! decoder into unbounded allocation or an unbounded loop, and neither is a bug
//! this application can fix in the dependency. So decoding is *supervised*: it
//! runs against declared ceilings, and exceeding one ends the job rather than
//! the host.
//!
//! # The failure attribution rule lives here
//!
//! This is where #17's "a worker fault is never reported as malformed user
//! input" is actually enforceable, because this is the only place that can tell
//! the two apart.
//!
//! A decoder that *reports* bad input produces [`WorkerFault::Rejected`], which
//! becomes [`Outcome::Invalid`] — the file really is malformed. A decoder that
//! dies, hangs, or eats its memory ceiling produces [`WorkerFault::Crashed`],
//! [`WorkerFault::TimedOut`], or [`WorkerFault::MemoryExhausted`], and every one
//! of those becomes [`Outcome::ParserFailure`]. They present identically from
//! the outside — no output — which is exactly why the distinction has to be
//! made by the supervisor rather than inferred downstream.
//!
//! # Progress, not just elapsed time
//!
//! Two deadlines apply, because one is not enough. A total deadline bounds the
//! whole job. A no-progress deadline catches the more common shape: a decoder
//! that is still running, still consuming CPU, and has not produced a byte in a
//! minute. Only the total deadline would let that burn for half an hour first.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use crate::{
    manifest::LIMITS,
    outcomes::{Diagnostic, Outcome, ReasonCode},
};

/// The protocol version. Bumped when the request or reply shape changes, so a
/// mismatched worker is refused rather than misread.
pub const PROTOCOL_VERSION: u32 = 1;

/// Why a supervised decode ended badly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkerFault {
    /// The decoder read the input and said it was malformed. The only fault
    /// that is the file's fault.
    Rejected(String),
    /// The decoder died.
    Crashed(String),
    /// The total deadline elapsed.
    TimedOut,
    /// No output for longer than the no-progress deadline.
    Stalled,
    /// The memory ceiling was reached.
    MemoryExhausted,
    /// Output exceeded what the manifest permits.
    OutputTooLarge { limit: u64, observed: u64 },
    /// Decoded far more than was read in.
    RatioExceeded { compressed: u64, decoded: u64 },
    /// The operation was cancelled.
    Cancelled,
    /// The worker speaks a different protocol version.
    VersionMismatch { expected: u32, found: u32 },
}

impl WorkerFault {
    /// The outcome this fault maps to.
    ///
    /// Everything except [`Self::Rejected`] and the limit faults is *our*
    /// failure. A crashed decoder and a truncated file look identical from
    /// outside, and calling ours theirs sends the user to re-dump a disc that
    /// was fine.
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::Rejected(_) => Outcome::Invalid,
            Self::OutputTooLarge { .. } | Self::RatioExceeded { .. } => Outcome::LimitExceeded,
            Self::Cancelled => Outcome::Cancelled,
            Self::Crashed(_)
            | Self::TimedOut
            | Self::Stalled
            | Self::MemoryExhausted
            | Self::VersionMismatch { .. } => Outcome::ParserFailure,
        }
    }

    pub fn reason(&self) -> ReasonCode {
        match self {
            Self::Rejected(_) => ReasonCode::MalformedStructure,
            Self::OutputTooLarge { .. } => ReasonCode::LimitExceeded,
            Self::RatioExceeded { .. } => ReasonCode::RatioExceeded,
            Self::Cancelled => ReasonCode::OperationCancelled,
            _ => ReasonCode::WorkerFailed,
        }
    }

    /// A reportable diagnostic carrying the measurement where there is one.
    pub fn diagnostic(&self) -> Diagnostic {
        let diagnostic = Diagnostic::new(self.outcome(), self.reason());
        match self {
            Self::OutputTooLarge { limit, observed } => diagnostic.measured(*limit, *observed),
            Self::RatioExceeded {
                compressed,
                decoded,
            } => diagnostic.measured(*compressed, *decoded),
            _ => diagnostic,
        }
    }
}

/// The ceilings one supervised decode runs under.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub max_output_bytes: u64,
    pub max_memory_bytes: u64,
    pub total_deadline: Duration,
    pub no_progress_deadline: Duration,
}

impl Default for Budget {
    /// The manifest's ceilings.
    fn default() -> Self {
        Self {
            max_output_bytes: LIMITS.max_decoded_member_bytes,
            max_memory_bytes: LIMITS.max_worker_memory_bytes,
            total_deadline: Duration::from_secs(LIMITS.candidate_deadline_seconds),
            no_progress_deadline: Duration::from_secs(LIMITS.no_progress_deadline_seconds),
        }
    }
}

/// Shared handle a decode reports through and is cancelled by.
///
/// Cheap to clone, so the supervisor and the decoding work can hold the same
/// one across a thread boundary.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    compressed_read: Arc<AtomicU64>,
    decoded_written: Arc<AtomicU64>,
    peak_memory: Arc<AtomicU64>,
    cancelled: Arc<AtomicBool>,
}

impl Progress {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `compressed` bytes went in and `decoded` bytes came out.
    pub fn advance(&self, compressed: u64, decoded: u64) {
        self.compressed_read
            .fetch_add(compressed, Ordering::Relaxed);
        self.decoded_written.fetch_add(decoded, Ordering::Relaxed);
    }

    pub fn record_memory(&self, bytes: u64) {
        self.peak_memory.fetch_max(bytes, Ordering::Relaxed);
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn compressed_read(&self) -> u64 {
        self.compressed_read.load(Ordering::Relaxed)
    }

    pub fn decoded_written(&self) -> u64 {
        self.decoded_written.load(Ordering::Relaxed)
    }

    pub fn peak_memory(&self) -> u64 {
        self.peak_memory.load(Ordering::Relaxed)
    }
}

/// Checks one decode against its budget.
///
/// Held separately from [`Progress`] so the decoding side can only *report*,
/// never decide. A decoder that could waive its own ceiling would not be
/// supervised at all.
pub struct Supervisor {
    budget: Budget,
    progress: Progress,
    started: Instant,
    last_output: Instant,
    last_seen_output: u64,
}

impl Supervisor {
    pub fn new(budget: Budget, progress: Progress) -> Self {
        let now = Instant::now();
        Self {
            budget,
            progress,
            started: now,
            last_output: now,
            last_seen_output: 0,
        }
    }

    /// Whether the decode may continue. Called between units of work.
    ///
    /// Cancellation is checked first: a user who has asked to stop should not
    /// be told their file is too large on the way out.
    pub fn check(&mut self) -> Result<(), WorkerFault> {
        self.check_at(Instant::now())
    }

    /// [`Self::check`] against a caller-supplied instant, so deadline behaviour
    /// is testable without waiting in real time.
    pub fn check_at(&mut self, now: Instant) -> Result<(), WorkerFault> {
        if self.progress.is_cancelled() {
            return Err(WorkerFault::Cancelled);
        }

        let decoded = self.progress.decoded_written();
        if decoded > self.last_seen_output {
            self.last_seen_output = decoded;
            self.last_output = now;
        }

        if decoded > self.budget.max_output_bytes {
            return Err(WorkerFault::OutputTooLarge {
                limit: self.budget.max_output_bytes,
                observed: decoded,
            });
        }

        let peak = self.progress.peak_memory();
        if peak > self.budget.max_memory_bytes {
            return Err(WorkerFault::MemoryExhausted);
        }

        let compressed = self.progress.compressed_read();
        if LIMITS.ratio_exceeded(compressed, decoded) {
            return Err(WorkerFault::RatioExceeded {
                compressed,
                decoded,
            });
        }

        if now.duration_since(self.started) > self.budget.total_deadline {
            return Err(WorkerFault::TimedOut);
        }
        // The more common shape: still running, still burning CPU, and has not
        // produced a byte in a minute.
        if now.duration_since(self.last_output) > self.budget.no_progress_deadline {
            return Err(WorkerFault::Stalled);
        }
        Ok(())
    }

    pub fn progress(&self) -> &Progress {
        &self.progress
    }
}

/// Turns the result of a supervised decode into an outcome.
///
/// A decoder is expected to return `Err(reason)` for input it read and
/// rejected. Anything the supervisor caught is passed through with its own
/// attribution intact.
pub fn attribute<T>(
    result: Result<T, WorkerFault>,
    protocol_version: u32,
) -> Result<T, Diagnostic> {
    if protocol_version != PROTOCOL_VERSION {
        return Err(WorkerFault::VersionMismatch {
            expected: PROTOCOL_VERSION,
            found: protocol_version,
        }
        .diagnostic());
    }
    result.map_err(|fault| fault.diagnostic())
}
