/**
 * The bridge from the WebView to the Rust core.
 *
 * # Why the injected global rather than `@tauri-apps/api`
 *
 * The npm package would be the conventional choice and is deliberately not used.
 * The privacy evidence for this project rests partly on `dependencies` in
 * `ui/package.json` being empty — every runtime dependency is a package that
 * could, at some future version, decide to fetch something. Tauri injects
 * `window.__TAURI__` when `withGlobalTauri` is set, which gives the same
 * `invoke` with nothing added to the supply chain.
 *
 * # A missing bridge is reported, never faked
 *
 * When the global is absent the app is running outside Tauri — a browser, a
 * test, a broken build. Returning empty state there would render a wizard that
 * looks functional and silently does nothing, and the first thing a user would
 * do with it is try to sync a real device. So the absence throws.
 */

import type { Commands, Snapshot } from "./bindings";

type Invoke = (command: string, payload?: Record<string, unknown>) => Promise<unknown>;

interface TauriGlobal {
  readonly core?: { readonly invoke?: Invoke };
  readonly event?: {
    readonly listen?: (
      event: string,
      handler: (message: { payload: unknown }) => void,
    ) => Promise<() => void>;
  };
}

declare global {
  interface Window {
    __TAURI__?: TauriGlobal;
  }
}

export class BridgeUnavailable extends Error {
  constructor() {
    super(
      "The application core is not reachable. This build must run inside the " +
        "ROM Manager desktop application.",
    );
    this.name = "BridgeUnavailable";
  }
}

function bridge(): Invoke {
  const invoke = window.__TAURI__?.core?.invoke;
  if (typeof invoke !== "function") {
    throw new BridgeUnavailable();
  }
  return invoke;
}

/** Whether the core is reachable, for a caller that wants to say so calmly. */
export function bridgeIsAvailable(): boolean {
  return typeof window.__TAURI__?.core?.invoke === "function";
}

async function call(command: string, payload?: Record<string, unknown>): Promise<Snapshot> {
  // Every command returns the whole resulting state, so the UI never has to
  // reconstruct what changed — it replaces what it holds.
  return (await bridge()(command, payload)) as Snapshot;
}

export const commands: Commands = {
  loadSnapshot: () => call("load_snapshot"),
  selectRomPack: (romPackId, revision) => call("select_rom_pack", { romPackId, revision }),
  selectMediaTarget: (targetId) => call("select_media_target", { targetId }),
  refreshTarget: () => call("refresh_target"),
  buildPlan: () => call("build_plan"),
  approveAndExecute: (planDigest, acknowledgedRemovals) =>
    call("approve_and_execute", { planDigest, acknowledgedRemovals }),
  requestCancellation: () => call("request_cancellation"),
  dismissResult: () => call("dismiss_result"),
};

/**
 * Subscribes to core-pushed state. Returns a disposer.
 *
 * Progress arrives as events rather than polling because the core is the only
 * thing that knows how far a write has got, and a UI that guessed would show a
 * bar that does not correspond to bytes on the device.
 */
export async function subscribe(
  onSnapshot: (snapshot: Snapshot) => void,
): Promise<() => void> {
  const listen = window.__TAURI__?.event?.listen;
  if (typeof listen !== "function") {
    return () => {};
  }
  const disposers = await Promise.all([
    listen("state-changed", (message) => onSnapshot(message.payload as Snapshot)),
    listen("progress-changed", (message) => onSnapshot(message.payload as Snapshot)),
  ]);
  return () => {
    for (const dispose of disposers) {
      dispose();
    }
  };
}
