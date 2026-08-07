//! Running a Sync Plan through durable state (issue #33).
//!
//! This is the seam between the in-memory sync core and the store. It exists so
//! the ordering below lives in exactly one place rather than being re-derived
//! by every caller:
//!
//! 1. The plan is persisted **before** approval, so an approval can only ever
//!    name a plan that already exists durably.
//! 2. The operation is marked running **before** any mutation, so a crash
//!    mid-operation is always visible on the next start.
//! 3. The transport does its work with **no transaction open** — the store is
//!    touched only at the boundaries.
//! 4. The outcome and the mirrored manifest are recorded **after** execution
//!    returns, so durable state never claims more than execution proved.
//!
//! An interrupted run leaves step 2's row behind, which
//! [`Store::recover_interrupted`] turns into an indeterminate operation and a
//! stale inventory on the next start.

use crate::{
    Approval, CancellationToken, ExecutionOutcome, OperationState, Store, StoreError, SyncCore,
    SyncError, SyncPlan, Transport,
};

#[derive(Debug, thiserror::Error)]
pub enum DurableError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Sync(#[from] SyncError),
    #[error("no approval is stored for this Sync Plan")]
    NotApproved,
}

/// Persists `plan` and the approval granted for it.
///
/// The plan is written first: an approval that referenced a plan the store did
/// not hold would be authority for something nobody could reconstruct.
pub fn approve(
    store: &Store,
    plan: &SyncPlan,
    approval: &Approval,
    now: i64,
) -> Result<(), DurableError> {
    store.save_plan(plan, now)?;
    store.save_approval(approval, now)?;
    Ok(())
}

/// Executes the plan filed under `plan_digest`, using the stored approval.
///
/// The plan is **reloaded from the store and revalidated by digest** rather
/// than taken from the caller, so a plan supplied by a frontend action cannot
/// stand in for the one that was approved. The approval is consumed as it is
/// read, so a retry needs a fresh one.
pub fn execute_approved<T: Transport>(
    store: &Store,
    core: &mut SyncCore<T>,
    plan_digest: &str,
    cancellation: &CancellationToken,
    now: i64,
) -> Result<ExecutionOutcome, DurableError> {
    let plan = store
        .load_plan(plan_digest)?
        .ok_or(DurableError::NotApproved)?;
    let approval = store
        .take_approval(plan_digest)?
        .ok_or(DurableError::NotApproved)?;

    let operation = store.begin_operation(plan_digest, &plan.target_id, now)?;

    // No transaction is open across this call. It talks to a device.
    let outcome = core.execute(&plan, approval, cancellation)?;

    let (state, reason) = match &outcome {
        ExecutionOutcome::Completed { .. } => (OperationState::Completed, None),
        ExecutionOutcome::Cancelled { .. } => (OperationState::Cancelled, None),
        ExecutionOutcome::Incomplete { reason, .. } => {
            (OperationState::Incomplete, Some(reason.as_str()))
        }
        ExecutionOutcome::Indeterminate { reason, .. } => {
            (OperationState::Indeterminate, Some(reason.as_str()))
        }
    };
    store.finish_operation(operation, state, reason, Some(outcome.report()), now)?;

    match &outcome {
        // Only a completed operation proved what is on the target, so only a
        // completed operation may update the mirrored manifest.
        ExecutionOutcome::Completed { .. } => {
            if let Some(manifest) = core.local_manifest() {
                store.save_manifest(manifest)?;
            }
        }
        // Anything else leaves the target in a state the application has not
        // established. The recorded inventory is no longer evidence.
        _ => store.mark_inventory_stale(&plan.target_id)?,
    }

    Ok(outcome)
}
