/**
 * Accessibility and safety coverage for the plan review step (issue #34).
 *
 * These check what a keyboard or screen-reader user actually gets: that the
 * destructive control is unreachable until it is authorized, that the reason is
 * announced rather than left to be inferred from a greyed-out button, and that
 * every figure the plan carries is on screen.
 *
 * Contrast and 100/200 percent scaling are **not** covered here — those need a
 * real browser, and jsdom has no layout.
 */

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { PlanReview } from "./PlanReview";
import type { PlanView } from "./bindings";

function aPlan(overrides: Partial<PlanView> = {}): PlanView {
  return {
    planDigest: "abc123",
    targetId: "target-fixture-001",
    bindingLocator: "wpd://odin/storage",
    profileId: "generic-folder",
    profileRevision: 1,
    romPackRevision: 3,
    inventoryFresh: true,
    inventoryDigest: "def456",
    transportLimitations: ["this connection cannot publish atomically"],
    actions: [
      { action: "add", path: "ROMs/nes/Tracers.nes", romSetId: "rs-1", size: 24, sha256: "aa" },
    ],
    preservedUnknowns: ["ROMs/nes/Someone-Elses.nes"],
    preservedDuplicates: [],
    preservedUnrepresentable: [],
    missingManaged: [],
    conflicts: [],
    peakCapacityRequired: 65560,
    safetyMargin: 65536,
    permanentRemovalCount: 0,
    executable: true,
    ...overrides,
  };
}

describe("PlanReview", () => {
  it("shows the identity and figures the user needs to judge the plan", () => {
    render(<PlanReview plan={aPlan()} onExecute={vi.fn()} onRefresh={vi.fn()} />);

    expect(screen.getByText("target-fixture-001")).toBeInTheDocument();
    expect(screen.getByText("wpd://odin/storage")).toBeInTheDocument();
    expect(screen.getByText(/generic-folder revision 1/)).toBeInTheDocument();
    expect(screen.getByText(/65560 bytes/)).toBeInTheDocument();
    expect(screen.getByText(/this connection cannot publish atomically/)).toBeInTheDocument();
    // Content the app will keep is shown, not silently dropped.
    expect(screen.getByText("ROMs/nes/Someone-Elses.nes")).toBeInTheDocument();
  });

  it("starts a sync with no removals to acknowledge", async () => {
    const onExecute = vi.fn();
    render(<PlanReview plan={aPlan()} onExecute={onExecute} onRefresh={vi.fn()} />);

    await userEvent.click(screen.getByRole("button", { name: "Start sync" }));
    expect(onExecute).toHaveBeenCalledWith(0);
  });

  it("will not start until permanent removals are acknowledged", async () => {
    const onExecute = vi.fn();
    render(
      <PlanReview plan={aPlan({ permanentRemovalCount: 3 })} onExecute={onExecute} onRefresh={vi.fn()} />,
    );

    const start = screen.getByRole("button", { name: "Start sync" });
    expect(start).toBeDisabled();
    // The reason is announced, not left to be inferred from a greyed button.
    expect(screen.getByRole("status")).toHaveTextContent(
      "Confirm that 3 files will be permanently removed.",
    );

    await userEvent.click(
      screen.getByRole("checkbox", { name: /Permanently remove 3 file\(s\)/ }),
    );
    expect(start).toBeEnabled();

    await userEvent.click(start);
    expect(onExecute).toHaveBeenCalledWith(3);
  });

  it("is fully operable by keyboard", async () => {
    const onExecute = vi.fn();
    render(
      <PlanReview plan={aPlan({ permanentRemovalCount: 1 })} onExecute={onExecute} onRefresh={vi.fn()} />,
    );

    // Tab reaches the acknowledgement, space toggles it, tab reaches the
    // action, Enter fires it — no pointer anywhere.
    await userEvent.tab();
    expect(screen.getByRole("checkbox")).toHaveFocus();
    await userEvent.keyboard(" ");
    await userEvent.tab();
    expect(screen.getByRole("button", { name: "Start sync" })).toHaveFocus();
    await userEvent.keyboard("{Enter}");

    expect(onExecute).toHaveBeenCalledWith(1);
  });

  it("refuses a stale plan and says why", () => {
    render(
      <PlanReview plan={aPlan({ inventoryFresh: false })} onExecute={vi.fn()} onRefresh={vi.fn()} />,
    );

    expect(screen.getByRole("button", { name: "Start sync" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent(/Refresh and build a new plan/);
  });

  it("announces conflicts rather than hiding them in a list", () => {
    render(
      <PlanReview
        plan={aPlan({
          executable: false,
          conflicts: [{ pathOccupiedByDirectory: { path: "ROMs/nes/Tracers.nes" } }],
        })}
        onExecute={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );

    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("ROMs/nes/Tracers.nes is a folder. It will not be removed.");
    expect(screen.getByRole("button", { name: "Start sync" })).toBeDisabled();
  });

  it("ties the blocked reason to the control it blocks", () => {
    render(
      <PlanReview plan={aPlan({ inventoryFresh: false })} onExecute={vi.fn()} onRefresh={vi.fn()} />,
    );

    const start = screen.getByRole("button", { name: "Start sync" });
    const describedBy = start.getAttribute("aria-describedby");
    expect(describedBy).not.toBeNull();
    expect(document.getElementById(describedBy as string)).toHaveTextContent(
      /Refresh and build a new plan/,
    );
  });

  it("offers a refresh as the way out of a blocked plan", async () => {
    const onRefresh = vi.fn();
    render(
      <PlanReview plan={aPlan({ inventoryFresh: false })} onExecute={vi.fn()} onRefresh={onRefresh} />,
    );

    await userEvent.click(screen.getByRole("button", { name: "Refresh device" }));
    expect(onRefresh).toHaveBeenCalled();
  });
});
