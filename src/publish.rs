//! Publishing metadata safely (issue #71).
//!
//! # Rewriting a file someone else owns
//!
//! Replacing a shared document is the most dangerous thing in this system. ES-DE
//! may be running, the user may have edited by hand, and the file is the only
//! copy of state ROM Manager does not own. So publication is a sequence of
//! refusals rather than a write:
//!
//! 1. **Confirm ES-DE is stopped.** A running frontend rewrites gamelists on
//!    exit, which would silently undo the publication — or worse, interleave.
//! 2. **Parse and validate** the current document. A file that does not parse
//!    is never replaced.
//! 3. **Recheck the fingerprint** immediately before mutating. Planning and
//!    publishing are different moments, and anything that changed in between
//!    invalidates the plan.
//! 4. **Stage and read back** the complete replacement before it goes live.
//! 5. **Retain the prior verified file** in the marker area.
//! 6. Only then, replace.
//!
//! # Recovery is offered, never assumed
//!
//! One prior verified version is kept per gamelist, replaced only after a later
//! publication *and* its reread both succeed. A missing document offers
//! restoration from it first, because regeneration silently loses whatever
//! frontend state the lost file held.
//!
//! A recovery copy is never current state. It is never copied into the Library
//! or onto another target — it describes one document on one target at one
//! moment, and moving it elsewhere would make it a lie.

use crate::{Gamelist, GamelistError};

#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("confirm that ES-DE is not running before metadata is written")]
    FrontendNotConfirmedStopped,
    #[error("the document changed after the plan was built")]
    FingerprintChanged { planned: String, observed: String },
    #[error(transparent)]
    Gamelist(#[from] GamelistError),
    #[error("the staged document did not read back as written")]
    ReadBackMismatch,
    #[error("regenerating would lose frontend-owned state; confirm to proceed")]
    RegenerationNotConfirmed,
}

/// What the caller has established before publication may proceed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PublishPreconditions {
    /// The user confirmed ES-DE is stopped.
    pub frontend_stopped: bool,
}

/// Publishing one gamelist.
pub struct Publication {
    /// The fingerprint observed when the plan was built.
    planned_fingerprint: String,
}

impl Publication {
    pub fn planned_against(fingerprint: impl Into<String>) -> Self {
        Self {
            planned_fingerprint: fingerprint.into(),
        }
    }

    /// Runs the checks and returns the bytes to write.
    ///
    /// `current` is the document as it exists *right now*, read immediately
    /// before this call — not the copy the plan was built from.
    pub fn prepare(
        &self,
        preconditions: PublishPreconditions,
        current: &str,
        apply: impl FnOnce(&mut Gamelist),
    ) -> Result<String, PublishError> {
        // A running frontend rewrites gamelists on exit; publishing underneath
        // it would be silently undone.
        if !preconditions.frontend_stopped {
            return Err(PublishError::FrontendNotConfirmedStopped);
        }

        // Never replace a document that does not parse.
        let mut gamelist = Gamelist::parse(current)?;

        // Planning and publishing are different moments.
        let observed = Gamelist::fingerprint(current);
        if observed != self.planned_fingerprint {
            return Err(PublishError::FingerprintChanged {
                planned: self.planned_fingerprint.clone(),
                observed,
            });
        }

        apply(&mut gamelist);
        let replacement = gamelist.to_xml()?;

        // Read-back: the staged document must parse to what was intended, or it
        // is not fit to become the live one.
        let restaged = Gamelist::parse(&replacement)?;
        if restaged != gamelist {
            return Err(PublishError::ReadBackMismatch);
        }

        Ok(replacement)
    }
}

/// The state a gamelist can be found in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentState {
    Present(String),
    /// Gone, with a recovery copy available.
    MissingWithRecovery(String),
    Missing,
}

/// What to do about a missing document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryChoice {
    /// Offered first: it holds whatever frontend state the lost file had.
    Restore(String),
    /// Only with confirmation, and only creating currently desired
    /// projections.
    Regenerate,
    /// A missing document with nothing to write is not recreated — an empty
    /// gamelist is not an improvement on no gamelist.
    LeaveAbsent,
}

/// Decides how to handle a document that is not there.
pub fn recover_missing_document(
    state: &DocumentState,
    desired_projection_count: usize,
    regeneration_confirmed: bool,
) -> Result<RecoveryChoice, PublishError> {
    match state {
        DocumentState::Present(_) => Ok(RecoveryChoice::LeaveAbsent),
        // Restoration comes first: regeneration cannot bring back frontend
        // state, and the recovery copy can.
        DocumentState::MissingWithRecovery(copy) => Ok(RecoveryChoice::Restore(copy.clone())),
        DocumentState::Missing if desired_projection_count == 0 => Ok(RecoveryChoice::LeaveAbsent),
        DocumentState::Missing if regeneration_confirmed => Ok(RecoveryChoice::Regenerate),
        DocumentState::Missing => Err(PublishError::RegenerationNotConfirmed),
    }
}

/// The single retained prior version of one gamelist.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecoveryCopy {
    contents: Option<String>,
}

impl RecoveryCopy {
    pub fn get(&self) -> Option<&str> {
        self.contents.as_deref()
    }

    /// Replaces the retained copy — only after a later publication **and** its
    /// reread have both succeeded.
    ///
    /// Rotating any earlier would leave a window with no good copy at all,
    /// which is exactly the moment a recovery copy exists for.
    pub fn rotate_after_verified_publication(
        &mut self,
        previous_contents: String,
        publication_verified: bool,
    ) -> bool {
        if !publication_verified {
            return false;
        }
        self.contents = Some(previous_contents);
        true
    }

    /// Explicitly discards it. Never automatic.
    pub fn discard(&mut self) {
        self.contents = None;
    }
}
