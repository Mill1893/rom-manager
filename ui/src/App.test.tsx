/**
 * The wizard shell (issue #34).
 *
 * What matters here is not that buttons render. It is that the core stays the
 * authority: a refusal is surfaced, a returned snapshot replaces state wholesale
 * rather than being merged, and the UI never invents a state the core did not
 * hand it.
 */

import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type { PlanView, Snapshot, WizardStep } from "./bindings";

const invoke = vi.fn();
const listen = vi.fn().mockResolvedValue(() => {});

function withBridge(): void {
  window.__TAURI__ = { core: { invoke }, event: { listen } };
}

function withoutBridge(): void {
  delete window.__TAURI__;
}

afterEach(() => {
  invoke.mockReset();
  listen.mockClear();
  withoutBridge();
});

function snapshot(step: WizardStep, extra: Partial<Snapshot> = {}): Snapshot {
  return {
    step,
    romPack: null,
    mediaTarget: null,
    plan: null,
    progress: null,
    outcome: null,
    recoveryDisclosure: [],
    ...extra,
  };
}

function plan(overrides: Partial<PlanView> = {}): PlanView {
  return {
    planDigest: "digest-abc",
    targetId: "target-1",
    bindingLocator: "/media/card",
    profileId: "generic-folder",
    profileRevision: 1,
    romPackRevision: 1,
    inventoryFresh: true,
    inventoryDigest: "inv-1",
    transportLimitations: [],
    actions: [],
    preservedUnknowns: [],
    preservedDuplicates: [],
    preservedUnrepresentable: [],
    missingManaged: [],
    conflicts: [],
    peakCapacityRequired: 0,
    safetyMargin: 0,
    permanentRemovalCount: 0,
    executable: true,
    ...overrides,
  };
}

describe("the bridge to the core", () => {
  it("says the core is unreachable rather than rendering a wizard that does nothing", async () => {
    // Returning empty state here would render something that looks functional,
    // and the first thing a user would do with it is try to sync a real device.
    withoutBridge();
    render(<App />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/not reachable/i);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("loads authoritative state on startup", async () => {
    withBridge();
    invoke.mockResolvedValue(snapshot({ step: "selectRomPack" }));

    render(<App />);

    await screen.findByRole("heading", { name: /choose what to sync/i });
    expect(invoke).toHaveBeenCalledWith("load_snapshot", undefined);
  });
});

describe("command failures", () => {
  it("shows a refusal instead of swallowing it", async () => {
    // The core refusing is the safety mechanism working. Hiding it would leave
    // the user pressing a button that appears to do nothing.
    withBridge();
    invoke
      .mockResolvedValueOnce(snapshot({ step: "reviewPlan" }, { plan: null }))
      .mockRejectedValueOnce(new Error("this device changed since the plan was built"));

    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: /build a sync plan/i }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/changed since the plan was built/i);
  });
});

describe("the plan review step", () => {
  it("sends the digest the user was shown, not one it recomputed", async () => {
    // Approving a plan the user did not see is the exact failure the digest
    // binding exists to prevent.
    withBridge();
    invoke
      .mockResolvedValueOnce(snapshot({ step: "reviewPlan" }, { plan: plan() }))
      .mockResolvedValueOnce(snapshot({ step: "executing" }));

    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: /sync/i }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("approve_and_execute", {
        planDigest: "digest-abc",
        acknowledgedRemovals: 0,
      }),
    );
  });

  it("blocks a stale plan and says why, rather than letting it proceed quietly", async () => {
    // The gate is the disabled button plus a reason tied to it by
    // aria-describedby — a warning the user could scroll past would not be one.
    withBridge();
    invoke.mockResolvedValue(
      snapshot({ step: "reviewPlan" }, { plan: plan({ inventoryFresh: false }) }),
    );

    render(<App />);

    const start = await screen.findByRole("button", { name: /start sync/i });
    expect(start).toBeDisabled();

    const reasonId = start.getAttribute("aria-describedby");
    expect(reasonId).not.toBeNull();
    expect(document.getElementById(reasonId ?? "")).toHaveTextContent(/refresh/i);
  });
});

describe("the device step", () => {
  it("shows a disconnected device rather than hiding it", async () => {
    // The user picked this device. "It is not plugged in" is more useful than
    // it vanishing from the list.
    withBridge();
    invoke.mockResolvedValue(
      snapshot(
        { step: "selectMediaTarget" },
        {
          mediaTarget: {
            targetId: "target-1",
            label: "Odin SD card",
            bindingLocator: null,
            connected: false,
          },
        },
      ),
    );

    render(<App />);

    expect(await screen.findByText(/Odin SD card/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /continue/i })).toBeDisabled();
  });
});

describe("the result step", () => {
  it("shows the recovery disclosure the core supplied", async () => {
    withBridge();
    invoke.mockResolvedValue(
      snapshot(
        { step: "result" },
        {
          outcome: {
            kind: "indeterminate",
            reason: "the device disconnected",
            performed: [],
            notAttempted: [],
            uncertain: ["ROMs/nes/Tracers.nes"],
            residue: ["ROMs/nes/Leftover.nes"],
            refreshRequired: true,
          },
          recoveryDisclosure: ["1 action(s) have an uncertain result"],
        },
      ),
    );

    render(<App />);

    expect(await screen.findByText(/uncertain result/i)).toBeInTheDocument();
    expect(screen.getByText("ROMs/nes/Leftover.nes")).toBeInTheDocument();
  });
});

describe("state pushed by the core", () => {
  it("replaces what the UI holds rather than merging into it", async () => {
    // Two sources of truth about what is on a device is how a UI ends up
    // confidently showing a state the device left ten seconds ago.
    withBridge();
    invoke.mockResolvedValue(snapshot({ step: "selectRomPack" }));

    let push: ((message: { payload: unknown }) => void) | undefined;
    listen.mockImplementation((_event: string, handler: (message: { payload: unknown }) => void) => {
      push = handler;
      return Promise.resolve(() => {});
    });

    render(<App />);
    await screen.findByRole("heading", { name: /choose what to sync/i });

    await waitFor(() => expect(push).toBeDefined());
    push?.({ payload: snapshot({ step: "executing" }, { progress: null }) });

    expect(await screen.findByRole("heading", { name: /syncing/i })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /choose what to sync/i })).toBeNull();
  });
});

describe("an empty catalogue", () => {
  it("offers to add a ROM folder rather than showing an empty list", async () => {
    // A wizard with nothing to choose and no way to add anything is a dead end,
    // which is exactly what shipping the empty catalogues without this would be.
    withBridge();
    invoke.mockResolvedValue(snapshot({ step: "selectRomPack" }, { romPack: null }));

    render(<App />);
    const add = await screen.findByRole("button", { name: /add a rom folder/i });

    await userEvent.click(add);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("pick_import_folder", undefined));
  });

  it("offers to add a device, and sends no path when doing so", async () => {
    // The command takes no arguments on purpose: the OS picker decides which
    // directory, so a path never crosses this boundary.
    withBridge();
    invoke.mockResolvedValue(snapshot({ step: "selectMediaTarget" }, { mediaTarget: null }));

    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: /add a device/i }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("pick_media_target", undefined));
    const pathish = invoke.mock.calls.filter(([, payload]) =>
      JSON.stringify(payload ?? {}).match(/[/\\]/),
    );
    expect(pathish).toHaveLength(0);
  });
});
