/**
 * Reflow coverage for the three-pane Library Browser (issue #31).
 *
 * These assert the *rules* — which pane yields, in what order, at what width.
 * Whether the rendered CSS then fits still needs a browser; this is the half
 * that does not.
 */

import { describe, expect, it } from "vitest";

import {
  MINIMUM_TASK_WIDTH_PX,
  effectiveWidth,
  fitsWithoutHorizontalScroll,
  panesFor,
  transitionFor,
} from "./layout";

describe("effective width under scaling", () => {
  it("halves the usable width at 200 percent", () => {
    expect(effectiveWidth(1920, 100)).toBe(1920);
    expect(effectiveWidth(1920, 200)).toBe(960);
  });

  it("rejects a nonsensical scale", () => {
    expect(() => effectiveWidth(1920, 0)).toThrow();
  });
});

describe("pane collapse order", () => {
  it("shows everything on a wide display", () => {
    const panes = panesFor(effectiveWidth(1920, 100));

    expect(panes.navigation).toBe("full");
    expect(panes.inspector).toBe(true);
    expect(panes.content).toBe(true);
  });

  it("collapses the inspector first", () => {
    // 800px: below the 60rem inspector threshold, above the 44rem rail one.
    const panes = panesFor(800);

    expect(panes.inspector).toBe(false);
    expect(panes.navigation).toBe("full");
    expect(panes.content).toBe(true);
  });

  it("turns navigation into a rail only after the inspector has gone", () => {
    const panes = panesFor(600);

    expect(panes.inspector).toBe(false);
    expect(panes.navigation).toBe("rail");
    expect(panes.content).toBe(true);
  });

  it("never hides the content pane", () => {
    // At every width, including absurd ones. It is what the user came for.
    for (const width of [2560, 1440, 1100, 800, 600, 400, 240]) {
      expect(panesFor(width).content, `content hidden at ${width}px`).toBe(true);
    }
  });

  it("keeps the collapse order stable across the 200 percent case", () => {
    // A 1280px laptop at 200% has 640 effective pixels — the case that
    // actually bites, and the one where a mis-collapse would hurt most.
    const panes = panesFor(effectiveWidth(1280, 200));

    expect(panes.inspector).toBe(false);
    expect(panes.navigation).toBe("rail");
    expect(panes.content).toBe(true);
  });
});

describe("horizontal scrolling", () => {
  it("is never required at 100 percent on any reasonable display", () => {
    for (const width of [1920, 1440, 1280, 1024, 800]) {
      expect(fitsWithoutHorizontalScroll(width, 100)).toBe(true);
    }
  });

  it("is never required at 200 percent on a common laptop", () => {
    // Scrolling sideways to reach a confirm button is how destructive actions
    // get mis-clicked.
    expect(fitsWithoutHorizontalScroll(1280, 200)).toBe(true);
    expect(fitsWithoutHorizontalScroll(1366, 200)).toBe(true);
  });

  it("reports honestly when a viewport is genuinely too small", () => {
    // 600px at 200% leaves 300 effective pixels, under the 320px task minimum.
    expect(fitsWithoutHorizontalScroll(600, 200)).toBe(false);
    // Exactly at the minimum still fits — the boundary is inclusive.
    expect(fitsWithoutHorizontalScroll(640, 200)).toBe(true);
    expect(MINIMUM_TASK_WIDTH_PX).toBe(320);
  });
});

describe("reduced motion", () => {
  it("suppresses transitions entirely when asked", () => {
    // Motion here is decorative, so honouring the preference costs nothing.
    expect(transitionFor(true)).toBe("0ms");
    expect(transitionFor(false)).toContain("ms");
  });
});
