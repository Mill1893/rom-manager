/**
 * The plan review step: the last screen before anything is written.
 *
 * Its job is to make the consequences legible before the user commits. Every
 * figure the plan carries is shown, and the permanent-removal acknowledgement
 * is a deliberate act rather than a checkbox that defaults on.
 */

import { useId, useState } from "react";
import type { BlockReason, PlanView } from "./bindings";
import { executionGate } from "./wizard";

export interface PlanReviewProps {
  readonly plan: PlanView;
  readonly onExecute: (acknowledgedRemovals: number) => void;
  readonly onRefresh: () => void;
}

function describeConflict(reason: BlockReason): string {
  if ("outsideManagedRoot" in reason) {
    return `${reason.outsideManagedRoot.path} is outside the managed folder.`;
  }
  if ("effectiveCaseCollision" in reason) {
    const { path, existing } = reason.effectiveCaseCollision;
    return existing === null
      ? `${path} collides with another selected item.`
      : `${path} collides with ${existing}, which is already on the device.`;
  }
  if ("invalidTargetPath" in reason) {
    return `${reason.invalidTargetPath.path} is not a name this app can safely use.`;
  }
  if ("pathOccupiedByDirectory" in reason) {
    return `${reason.pathOccupiedByDirectory.path} is a folder. It will not be removed.`;
  }
  if ("profileRevisionChanged" in reason) {
    const { recorded, active } = reason.profileRevisionChanged;
    return `This device was organised under layout revision ${recorded}; revision ${active} is in use now.`;
  }
  if ("pathConflict" in reason) {
    return `${reason.pathConflict.path} already holds content this app did not put there. It will be kept.`;
  }
  if ("managedContentChanged" in reason) {
    return `${reason.managedContentChanged.path} was changed outside this app. It will not be overwritten.`;
  }
  if ("insufficientCapacity" in reason) {
    const { required, available } = reason.insufficientCapacity;
    return `Not enough free space: ${required} bytes needed, ${available} available.`;
  }
  if ("unsupportedCapability" in reason) {
    return `This connection cannot provide ${reason.unsupportedCapability.capability}.`;
  }
  if ("manifestDisagreement" in reason) {
    return "This app's record of the device disagrees with the device itself.";
  }
  if ("markerConflict" in reason) {
    return "This does not appear to be the device this app expected.";
  }
  return "This device's contents changed. Refresh to see them.";
}

export function PlanReview({ plan, onExecute, onRefresh }: PlanReviewProps): React.JSX.Element {
  const [acknowledged, setAcknowledged] = useState(false);
  const headingId = useId();
  const removalsId = useId();

  const acknowledgedCount = acknowledged ? plan.permanentRemovalCount : 0;
  const gate = executionGate(plan, acknowledgedCount);
  const needsAcknowledgement = plan.permanentRemovalCount > 0;

  return (
    <section aria-labelledby={headingId}>
      <h2 id={headingId}>Review sync plan</h2>

      <dl>
        <dt>Device</dt>
        <dd>{plan.targetId}</dd>
        <dt>Connected at</dt>
        <dd>{plan.bindingLocator}</dd>
        <dt>Layout</dt>
        <dd>
          {plan.profileId} revision {plan.profileRevision}
        </dd>
        <dt>Collection revision</dt>
        <dd>{plan.romPackRevision}</dd>
        <dt>Device contents</dt>
        <dd>{plan.inventoryFresh ? "Up to date" : "Changed since this plan was built"}</dd>
        <dt>Space needed</dt>
        <dd>
          {plan.peakCapacityRequired} bytes, including a {plan.safetyMargin} byte margin
        </dd>
      </dl>

      {plan.transportLimitations.length > 0 && (
        <section aria-label="Connection limitations">
          <h3>This connection&rsquo;s limitations</h3>
          <ul>
            {plan.transportLimitations.map((limitation) => (
              <li key={limitation}>{limitation}</li>
            ))}
          </ul>
        </section>
      )}

      <section aria-label="Planned changes">
        <h3>Changes</h3>
        <ul>
          {plan.actions.map((action) => (
            <li key={`${action.action}:${action.path}`}>
              <span>{action.action}</span> {action.path}
            </li>
          ))}
        </ul>
      </section>

      {plan.conflicts.length > 0 && (
        // role="alert" so a screen reader hears about blockers without having
        // to go looking for them.
        <section aria-label="Conflicts" role="alert">
          <h3>{plan.conflicts.length} conflict(s) to resolve</h3>
          <ul>
            {plan.conflicts.map((reason, index) => (
              <li key={index}>{describeConflict(reason)}</li>
            ))}
          </ul>
        </section>
      )}

      {(plan.preservedUnknowns.length > 0 ||
        plan.preservedDuplicates.length > 0 ||
        plan.preservedUnrepresentable.length > 0) && (
        <section aria-label="Content that will be kept">
          <h3>Kept untouched</h3>
          <ul>
            {[
              ...plan.preservedUnknowns,
              ...plan.preservedDuplicates,
              ...plan.preservedUnrepresentable,
            ].map((entry) => (
              <li key={entry}>{entry}</li>
            ))}
          </ul>
        </section>
      )}

      {needsAcknowledgement && (
        <p>
          <input
            type="checkbox"
            id={removalsId}
            checked={acknowledged}
            onChange={(event) => setAcknowledged(event.currentTarget.checked)}
          />
          <label htmlFor={removalsId}>
            Permanently remove {plan.permanentRemovalCount} file(s) from this device
          </label>
        </p>
      )}

      {/* The reason is tied to the button, so pressing a disabled control and
          wondering why is not a thing that can happen. */}
      {!gate.allowed && (
        <p id="execution-blocked" role="status">
          {gate.reason}
        </p>
      )}

      <button
        type="button"
        disabled={!gate.allowed}
        aria-describedby={gate.allowed ? undefined : "execution-blocked"}
        onClick={() => onExecute(acknowledgedCount)}
      >
        Start sync
      </button>
      <button type="button" onClick={onRefresh}>
        Refresh device
      </button>
    </section>
  );
}
