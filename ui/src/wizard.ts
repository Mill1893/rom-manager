/**
 * The modal sync workflow's decision logic, kept out of the components so it
 * can be reasoned about (and tested) without a DOM.
 *
 * Every rule here mirrors one the Rust core also enforces. The duplication is
 * deliberate: the UI must not *offer* an action the core would reject, and the
 * core must not *trust* the UI to have checked.
 */

import type { OutcomeView, PlanView, Progress, Snapshot } from "./bindings";

/** Whether the plan may be executed, and if not, why — in the user's terms. */
export type ExecutionGate =
  | { readonly allowed: true }
  | { readonly allowed: false; readonly reason: string };

export function executionGate(
  plan: PlanView | null,
  acknowledgedRemovals: number,
): ExecutionGate {
  if (plan === null) {
    return { allowed: false, reason: "No sync plan has been built yet." };
  }
  if (!plan.inventoryFresh) {
    return {
      allowed: false,
      reason:
        "This device changed since the plan was built. Refresh and build a new plan.",
    };
  }
  if (plan.conflicts.length > 0) {
    return {
      allowed: false,
      reason: `This plan is blocked by ${plan.conflicts.length} conflict(s) that must be resolved first.`,
    };
  }
  if (!plan.executable) {
    return { allowed: false, reason: "This plan cannot be run." };
  }
  if (acknowledgedRemovals !== plan.permanentRemovalCount) {
    return {
      allowed: false,
      reason:
        plan.permanentRemovalCount === 1
          ? "Confirm that 1 file will be permanently removed."
          : `Confirm that ${plan.permanentRemovalCount} files will be permanently removed.`,
    };
  }
  return { allowed: true };
}

/**
 * The assistive announcement for a progress update.
 *
 * Phase is named rather than reduced to a percentage: "verifying" and "writing"
 * fail differently, and a screen-reader user needs the same distinction a
 * sighted one gets.
 */
export function progressAnnouncement(progress: Progress): string {
  if (progress.cancellation === "requested") {
    return "Stopping after the current step.";
  }
  if (progress.cancellation === "stopped") {
    return "Sync stopped.";
  }
  const scope = `${progress.artifactsDone} of ${progress.artifactsTotal} items`;
  switch (progress.phase) {
    case "preparing":
      return "Preparing to sync.";
    case "writing":
      return `Copying ${scope}${progress.currentRomSet === null ? "" : `, currently ${progress.currentRomSet}`}.`;
    case "verifying":
      return `Verifying ${scope}.`;
    case "removing":
      return `Removing files no longer selected, ${scope}.`;
    case "publishing":
      return "Finishing up.";
  }
}

/**
 * How a terminal outcome is announced.
 *
 * `indeterminate` deliberately does not say the sync failed — the application
 * cannot establish what reached the device, and saying either "done" or
 * "failed" would be a claim it cannot support.
 */
export function outcomeAnnouncement(outcome: OutcomeView): string {
  switch (outcome.kind) {
    case "completed":
      return `Sync complete. ${outcome.performed.length} item(s) synced.`;
    case "cancelled":
      return `Sync stopped. ${outcome.performed.length} item(s) completed, ${outcome.notAttempted.length} not started.`;
    case "incomplete":
      return `Sync did not finish. ${outcome.performed.length} item(s) completed, ${outcome.notAttempted.length} not started.`;
    case "indeterminate":
      return "Sync was interrupted and the device's contents could not be confirmed. Refresh to see its current state.";
  }
}

/** Whether the result step must push the user back to a refresh. */
export function mustRefreshBeforeContinuing(snapshot: Snapshot): boolean {
  return snapshot.outcome?.refreshRequired ?? false;
}

/**
 * True when the operation is safely recorded. Until then the UI must not tell
 * the user it is finished, even though execution has returned.
 */
export function isDurablyFinished(progress: Progress | null): boolean {
  return progress?.durablyRecorded ?? false;
}
