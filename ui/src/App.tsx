/**
 * The modal sync wizard.
 *
 * # The core is the authority, always
 *
 * Every command returns a whole [`Snapshot`], and this component replaces what
 * it holds rather than patching it. That is why there is no reducer here and no
 * derived copy of the plan: two sources of truth about what is on a device is
 * exactly how a UI ends up confidently showing a state the device left ten
 * seconds ago.
 *
 * # Nothing here decides anything the core would not
 *
 * The gates in `wizard.ts` mirror rules the Rust core also enforces. The
 * duplication is deliberate and one-directional: the UI must not *offer* an
 * action the core would reject, and the core must not *trust* the UI to have
 * checked. If the two ever disagree, the core wins and the user sees its
 * refusal — which is why command failures are surfaced rather than swallowed.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type {
  MediaTargetChoice,
  OutcomeKind,
  RomPackChoice,
  ScanSummary,
  Snapshot,
} from "./bindings";
import type { StateName } from "./tokens";
import { PlanReview } from "./PlanReview";
import { StatusBadge } from "./StatusBadge";
import { commands, subscribe } from "./invoke";
import { mustRefreshBeforeContinuing, outcomeAnnouncement, progressAnnouncement } from "./wizard";

type Busy = string | null;

export function App(): React.JSX.Element {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState<Busy>(null);
  const mounted = useRef(true);

  /**
   * Runs one command, replacing state with what it returns.
   *
   * A rejected command is shown, never discarded. The core refusing is the
   * safety mechanism working, and hiding it would leave the user pressing a
   * button that appears to do nothing.
   */
  const run = useCallback(async (label: string, command: () => Promise<Snapshot>) => {
    setBusy(label);
    setFailure(null);
    try {
      const next = await command();
      if (mounted.current) {
        setSnapshot(next);
      }
    } catch (error) {
      if (mounted.current) {
        setFailure(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (mounted.current) {
        setBusy(null);
      }
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    void run("Loading", commands.loadSnapshot);

    let dispose: (() => void) | undefined;
    void subscribe((pushed) => {
      if (mounted.current) {
        setSnapshot(pushed);
      }
    }).then((disposer) => {
      dispose = disposer;
    });

    return () => {
      mounted.current = false;
      dispose?.();
    };
  }, [run]);

  if (failure !== null && snapshot === null) {
    return (
      <main className="shell">
        <h1>ROM Manager</h1>
        <p role="alert" className="failure">
          {failure}
        </p>
      </main>
    );
  }

  if (snapshot === null) {
    return (
      <main className="shell">
        <h1>ROM Manager</h1>
        <p role="status">Loading…</p>
      </main>
    );
  }

  return (
    <main className="shell">
      <h1>ROM Manager</h1>

      {failure !== null && (
        <p role="alert" className="failure">
          {failure}
        </p>
      )}

      {mustRefreshBeforeContinuing(snapshot) && (
        <p role="alert" className="failure">
          This device changed since the plan was built. Refresh before continuing.
        </p>
      )}

      {snapshot.lastScan !== null && <ScanResult summary={snapshot.lastScan} />}

      <Step snapshot={snapshot} busy={busy} run={run} />
    </main>
  );
}

/**
 * What the last scan took in, and what it refused.
 *
 * The refusals are listed individually rather than counted. "3 files skipped"
 * tells a user that something is missing without telling them what, which is
 * the worst of both: enough to worry about, not enough to act on.
 */
function ScanResult({ summary }: { readonly summary: ScanSummary }): React.JSX.Element {
  return (
    <section aria-labelledby="scan-heading">
      <h2 id="scan-heading">Last scan</h2>
      <p role="status">
        {summary.romSetsAdded} game{summary.romSetsAdded === 1 ? "" : "s"} added from{" "}
        {summary.foldersScanned} folder{summary.foldersScanned === 1 ? "" : "s"}.
      </p>

      {summary.declined.length > 0 && (
        <>
          <h3>Not added ({summary.declined.length})</h3>
          <ul>
            {summary.declined.map((file) => (
              <li key={`${file.path}:${file.code}`}>
                <code>{file.path}</code>
                <p>{file.remediation}</p>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

interface StepProps {
  readonly snapshot: Snapshot;
  readonly busy: Busy;
  readonly run: (label: string, command: () => Promise<Snapshot>) => Promise<void>;
}

function Step({ snapshot, busy, run }: StepProps): React.JSX.Element {
  switch (snapshot.step.step) {
    case "selectRomPack":
      return (
        <SelectRomPack
          chosen={snapshot.romPack}
          available={snapshot.availablePacks}
          busy={busy}
          run={run}
        />
      );
    case "selectMediaTarget":
      return (
        <SelectMediaTarget
          chosen={snapshot.mediaTarget}
          available={snapshot.availableTargets}
          busy={busy}
          run={run}
        />
      );
    case "reviewPlan":
      return snapshot.plan === null ? (
        <section>
          <h2>Build a plan</h2>
          <p>Nothing has been planned for this device yet.</p>
          <button type="button" disabled={busy !== null} onClick={() => void run("Planning", commands.buildPlan)}>
            Build a sync plan
          </button>
          <button
            type="button"
            disabled={busy !== null}
            onClick={() => void run("Setting up", () => commands.initializeTarget(true))}
          >
            Set up this device
          </button>
        </section>
      ) : (
        <PlanReview
          plan={snapshot.plan}
          onExecute={(acknowledgedRemovals) => {
            const digest = snapshot.plan?.planDigest;
            if (digest === undefined) {
              return;
            }
            void run("Syncing", () => commands.approveAndExecute(digest, acknowledgedRemovals));
          }}
          onRefresh={() => void run("Refreshing", commands.refreshTarget)}
        />
      );
    case "executing":
      return <Executing snapshot={snapshot} busy={busy} run={run} />;
    case "result":
      return <Result snapshot={snapshot} busy={busy} run={run} />;
  }
}

function SelectRomPack({
  chosen,
  available,
  busy,
  run,
}: {
  readonly chosen: RomPackChoice | null;
  readonly available: readonly RomPackChoice[];
  readonly busy: Busy;
  readonly run: StepProps["run"];
}): React.JSX.Element {
  const empty = available.length === 0;
  return (
    <section aria-labelledby="rom-pack-heading">
      <h2 id="rom-pack-heading">Choose what to sync</h2>

      {/*
        An empty Library and an unmade choice are different situations and used
        to render identically, because this only ever saw the chosen pack. The
        result was "No ROM Packs yet" shown to someone holding 261 games, with
        the one control that could have selected them disabled.
      */}
      {empty ? (
        <p>
          No ROM Packs yet. Add a folder to look for ROMs in — nothing is read
          until you ask for a scan.
        </p>
      ) : (
        <ul aria-label="ROM Packs">
          {available.map((pack) => {
            const selected =
              chosen?.romPackId === pack.romPackId && chosen.revision === pack.revision;
            return (
              <li key={`${pack.romPackId}@${pack.revision}`}>
                <button
                  type="button"
                  aria-pressed={selected}
                  disabled={busy !== null}
                  onClick={() =>
                    void run("Selecting", () =>
                      commands.selectRomPack(pack.romPackId, pack.revision),
                    )
                  }
                >
                  <strong>{pack.title}</strong> — {pack.romSetCount} ROM Set
                  {pack.romSetCount === 1 ? "" : "s"}, revision {pack.revision}
                </button>
              </li>
            );
          })}
        </ul>
      )}

      <button
        type="button"
        disabled={busy !== null}
        onClick={() => void run("Choosing", commands.pickImportFolder)}
      >
        Add a ROM folder…
      </button>
      <button
        type="button"
        disabled={busy !== null}
        onClick={() => void run("Scanning", commands.scanImportFolders)}
      >
        Scan for ROMs
      </button>
    </section>
  );
}

function SelectMediaTarget({
  chosen,
  available,
  busy,
  run,
}: {
  readonly chosen: MediaTargetChoice | null;
  readonly available: readonly MediaTargetChoice[];
  readonly busy: Busy;
  readonly run: StepProps["run"];
}): React.JSX.Element {
  // A disconnected target is shown rather than hidden: the user picked this
  // device, and "it is not plugged in" is more useful than it vanishing.
  const unreachable = chosen !== null && !chosen.connected;
  const empty = available.length === 0;
  return (
    <section aria-labelledby="target-heading">
      <h2 id="target-heading">Choose a device</h2>

      {empty ? (
        <p>No devices yet. Choose the card or drive you sync to.</p>
      ) : (
        <ul aria-label="Devices">
          {available.map((target) => (
            <li key={target.targetId}>
              <button
                type="button"
                aria-pressed={chosen?.targetId === target.targetId}
                // A disconnected device cannot be selected, but it is still
                // listed and still says why — a row that vanishes when the card
                // is unplugged looks like the application forgot it.
                disabled={busy !== null || !target.connected}
                onClick={() =>
                  void run("Selecting", () => commands.selectMediaTarget(target.targetId))
                }
              >
                <strong>{target.label}</strong>
                {!target.connected && " — not connected"}
              </button>
            </li>
          ))}
        </ul>
      )}

      {unreachable && (
        <p role="alert" className="failure">
          Reconnect this device before continuing.
        </p>
      )}

      <button
        type="button"
        disabled={busy !== null}
        onClick={() => void run("Choosing", commands.pickMediaTarget)}
      >
        Add a device…
      </button>
    </section>
  );
}

function Executing({ snapshot, busy, run }: StepProps): React.JSX.Element {
  const progress = snapshot.progress;
  return (
    <section aria-labelledby="executing-heading">
      <h2 id="executing-heading">Syncing</h2>
      <p role="status" aria-live="polite">
        {progress === null ? "Starting…" : progressAnnouncement(progress)}
      </p>
      {progress !== null && (
        <progress
          value={progress.bytesDone}
          max={progress.bytesTotal === 0 ? 1 : progress.bytesTotal}
          aria-label="Sync progress"
        />
      )}
      <button
        type="button"
        disabled={busy !== null || progress?.cancellation !== "running"}
        onClick={() => void run("Cancelling", commands.requestCancellation)}
      >
        Stop
      </button>
    </section>
  );
}

/**
 * Which declared state an outcome is.
 *
 * `indeterminate` is deliberately not folded in with the other non-successes.
 * "We could not establish what reached the device" is a different thing to tell
 * someone than "this failed", and the tokens name it separately for that
 * reason.
 */
function badgeFor(kind: OutcomeKind): StateName {
  switch (kind) {
    case "completed":
      return "success";
    case "indeterminate":
      return "indeterminate";
    case "cancelled":
      return "stale";
    case "incomplete":
      return "blocked";
  }
}

function Result({ snapshot, busy, run }: StepProps): React.JSX.Element {
  const outcome = snapshot.outcome;
  return (
    <section aria-labelledby="result-heading">
      <h2 id="result-heading">Finished</h2>
      {outcome !== null && <StatusBadge state={badgeFor(outcome.kind)} />}
      <p role="status" aria-live="polite">
        {outcome === null ? "No result was recorded." : outcomeAnnouncement(outcome)}
      </p>

      {snapshot.recoveryDisclosure.length > 0 && (
        <div role="alert">
          <h3>Before you continue</h3>
          <ul>
            {snapshot.recoveryDisclosure.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </div>
      )}

      {outcome !== null && outcome.residue.length > 0 && (
        <>
          <h3>Left in place</h3>
          <ul>
            {outcome.residue.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
        </>
      )}

      <button type="button" disabled={busy !== null} onClick={() => void run("Closing", commands.dismissResult)}>
        Done
      </button>
    </section>
  );
}
