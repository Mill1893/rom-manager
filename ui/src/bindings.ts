/**
 * The Rust core's types, mirrored for the WebView.
 *
 * These correspond 1:1 to `src/app.rs`. Nothing else crosses the boundary: the
 * commands below take identifiers, never paths, SQL, shell strings, or URLs, so
 * a frontend cannot express a request that reaches past the core.
 */

export type WizardStep =
  | { step: "selectRomPack" }
  | { step: "selectMediaTarget" }
  | { step: "reviewPlan" }
  | { step: "executing" }
  | { step: "result" };

export type Action = "add" | "retain" | "adopt" | "remove";

export interface PlanAction {
  readonly action: Action;
  readonly path: string;
  readonly romSetId: string;
  readonly size: number;
  readonly sha256: string;
}

/** Discriminated so the UI must handle each reason rather than print a blob. */
export type BlockReason =
  | { readonly markerConflict: Record<string, never> }
  | { readonly manifestDisagreement: Record<string, never> }
  | { readonly staleInventory: Record<string, never> }
  | { readonly outsideManagedRoot: { readonly path: string } }
  | {
      readonly effectiveCaseCollision: {
        readonly path: string;
        readonly existing: string | null;
      };
    }
  | { readonly invalidTargetPath: { readonly path: string } }
  | { readonly pathOccupiedByDirectory: { readonly path: string } }
  | {
      readonly profileRevisionChanged: {
        readonly recorded: number;
        readonly active: number;
      };
    }
  | { readonly pathConflict: { readonly path: string } }
  | { readonly managedContentChanged: { readonly path: string } }
  | {
      readonly insufficientCapacity: {
        readonly required: number;
        readonly available: number;
      };
    }
  | { readonly unsupportedCapability: { readonly capability: string } };

export interface RomPackChoice {
  readonly romPackId: string;
  readonly revision: number;
  readonly title: string;
  readonly romSetCount: number;
}

export interface MediaTargetChoice {
  readonly targetId: string;
  readonly label: string;
  readonly bindingLocator: string | null;
  readonly connected: boolean;
}

export interface PlanView {
  readonly planDigest: string;
  readonly targetId: string;
  readonly bindingLocator: string;
  readonly profileId: string;
  readonly profileRevision: number;
  readonly romPackRevision: number;
  readonly inventoryFresh: boolean;
  readonly inventoryDigest: string;
  readonly transportLimitations: readonly string[];
  readonly actions: readonly PlanAction[];
  readonly preservedUnknowns: readonly string[];
  readonly preservedDuplicates: readonly string[];
  readonly preservedUnrepresentable: readonly string[];
  readonly missingManaged: readonly string[];
  readonly conflicts: readonly BlockReason[];
  readonly peakCapacityRequired: number;
  readonly safetyMargin: number;
  readonly permanentRemovalCount: number;
  readonly executable: boolean;
}

export type Phase = "preparing" | "writing" | "verifying" | "removing" | "publishing";
export type CancellationState = "running" | "requested" | "stopped";

export interface Progress {
  readonly phase: Phase;
  readonly bytesDone: number;
  readonly bytesTotal: number;
  readonly artifactsDone: number;
  readonly artifactsTotal: number;
  readonly currentRomSet: string | null;
  readonly verifying: boolean;
  readonly cancellation: CancellationState;
  readonly durablyRecorded: boolean;
}

export type OutcomeKind = "completed" | "cancelled" | "incomplete" | "indeterminate";

export interface OutcomeView {
  readonly kind: OutcomeKind;
  readonly reason: string | null;
  readonly performed: readonly string[];
  readonly notAttempted: readonly string[];
  readonly uncertain: readonly string[];
  readonly residue: readonly string[];
  readonly refreshRequired: boolean;
}

/** The authority. Every command returns one; the UI replaces what it holds. */
export interface Snapshot {
  readonly step: WizardStep;
  readonly romPack: RomPackChoice | null;
  readonly mediaTarget: MediaTargetChoice | null;
  readonly plan: PlanView | null;
  readonly progress: Progress | null;
  readonly outcome: OutcomeView | null;
  readonly recoveryDisclosure: readonly string[];
}

export type AppEvent =
  | { readonly event: "progressChanged"; readonly data: Progress }
  | { readonly event: "stateChanged"; readonly data: Snapshot };

/**
 * The complete command surface. Coarse by design — each call is one user
 * intention and returns the whole resulting state.
 *
 * Note what is absent: no path, no query, no URL, no command string. Selection
 * is by identifier only.
 */
export interface Commands {
  /** Re-reads authoritative state. Called on startup and after any doubt. */
  loadSnapshot(): Promise<Snapshot>;
  selectRomPack(romPackId: string, revision: number): Promise<Snapshot>;
  selectMediaTarget(targetId: string): Promise<Snapshot>;
  /**
   * Opens the operating system's own folder picker and remembers the choice.
   *
   * Takes no arguments, deliberately. The frontend says "the user wants to add
   * one"; the OS picker decides which directory. That is how this boundary can
   * have a file picker at all without a path ever crossing it — cancelling
   * simply returns the unchanged state.
   */
  pickMediaTarget(): Promise<Snapshot>;
  /** As above, for a folder to look for ROMs in. Remembering is not scanning. */
  pickImportFolder(): Promise<Snapshot>;
  /**
   * Reads every remembered folder and gathers what it finds into ROM Packs.
   *
   * Explicit, and separate from remembering: the application never walks the
   * user's disks on its own schedule.
   */
  scanImportFolders(): Promise<Snapshot>;
  /**
   * Claims a device by writing its marker. Separate and confirmed, because
   * this is how the application takes responsibility for a device's contents —
   * a user who plugged in the wrong card should get a question, not a claim.
   */
  initializeTarget(confirmed: boolean): Promise<Snapshot>;
  /** Observes the target afresh. Never called automatically. */
  refreshTarget(): Promise<Snapshot>;
  buildPlan(): Promise<Snapshot>;
  /**
   * Approves and runs. `acknowledgedRemovals` must equal the count the plan
   * displayed; the core rejects any other value.
   */
  approveAndExecute(planDigest: string, acknowledgedRemovals: number): Promise<Snapshot>;
  requestCancellation(): Promise<Snapshot>;
  dismissResult(): Promise<Snapshot>;
}
