/**
 * Contrast and semantics coverage for the Cartridge Index tokens (issue #31).
 *
 * Contrast ratio is arithmetic, so WCAG 2.2 AA is verified here rather than
 * left as a manual check someone has to remember to repeat. What still needs a
 * browser is *layout* — reflow and scaling — not colour.
 */

import { describe, expect, it } from "vitest";

import { AA_BODY, AA_LARGE, contrastRatio, meetsAA, parseHex } from "./contrast";
import { light, responsive, states, toCssVariables, type StateName } from "./tokens";

describe("contrast arithmetic", () => {
  it("computes the known extremes", () => {
    expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 1);
    expect(contrastRatio("#ffffff", "#ffffff")).toBeCloseTo(1, 5);
  });

  it("parses both hex forms", () => {
    expect(parseHex("#fff")).toEqual({ r: 255, g: 255, b: 255 });
    expect(parseHex("#b7471d")).toEqual({ r: 183, g: 71, b: 29 });
    expect(() => parseHex("nonsense")).toThrow();
  });
});

describe("Cartridge Index palette", () => {
  it("meets AA for body text on the page background", () => {
    for (const role of ["foreground", "dim"] as const) {
      const ratio = contrastRatio(light[role], light.background);
      expect(
        ratio,
        `${role} on background is ${ratio.toFixed(2)}:1, below ${AA_BODY}`,
      ).toBeGreaterThanOrEqual(AA_BODY);
    }
  });

  it("meets AA for every state colour used as text", () => {
    // A blocked state the user cannot read is worse than no state at all.
    for (const role of ["accent", "good", "warn", "bad"] as const) {
      const ratio = contrastRatio(light[role], light.background);
      expect(
        ratio,
        `${role} on background is ${ratio.toFixed(2)}:1, below ${AA_BODY}`,
      ).toBeGreaterThanOrEqual(AA_BODY);
    }
  });

  it("meets the large-text threshold for tertiary text", () => {
    // faint is only ever used at large sizes, where AA allows 3:1.
    expect(meetsAA(light.faint, light.background, true)).toBe(true);
  });

  it("keeps the keyline visible as a UI boundary", () => {
    expect(
      contrastRatio(light.line, light.background),
      "a keyline nobody can see is not a frame",
    ).toBeGreaterThanOrEqual(1.2);
  });
});

describe("state semantics", () => {
  it("never lets colour carry meaning alone", () => {
    // Every state must survive being read in greyscale.
    for (const [name, state] of Object.entries(states)) {
      expect(state.label.length, `${name} has no label`).toBeGreaterThan(0);
      expect(state.shape.length, `${name} has no shape`).toBeGreaterThan(0);
    }
  });

  it("gives every state a distinct label and shape pairing", () => {
    const pairs = Object.values(states).map(
      (state) => `${state.label}|${state.shape}`,
    );
    // success/stale/blocked must be distinguishable; destructive and blocked
    // share a shape but differ by label, which is the point.
    expect(new Set(pairs).size).toBe(pairs.length);
  });

  it("labels a destructive block exactly as the decision requires", () => {
    expect(states.destructive.label).toBe("ACTION REQUIRED");
    expect(states.destructive.strongKeyline).toBe(true);
  });

  it("emphasises the keyline for every state that stops the user", () => {
    for (const name of ["blocked", "destructive", "indeterminate"] as StateName[]) {
      expect(states[name].strongKeyline, `${name} must be unmissable`).toBe(true);
    }
  });
});

describe("responsive behaviour", () => {
  it("never requires horizontal scrolling for a core task", () => {
    expect(responsive.horizontalScrollAllowed).toBe(false);
  });

  it("collapses the inspector before the content pane", () => {
    // Content is what the user came for.
    const inspector = Number.parseFloat(responsive.inspectorCollapse);
    const navigation = Number.parseFloat(responsive.navigationRail);
    expect(inspector).toBeGreaterThan(navigation);
  });
});

describe("token emission", () => {
  it("emits every palette, type, and density token as a custom property", () => {
    const css = toCssVariables();

    expect(css).toContain("--color-accent: #a03d17;");
    expect(css).toContain("--font-mono:");
    expect(css).toContain("--density-rowHeight:");
  });

  it("uses platform font stacks rather than bundled faces", () => {
    // Shipping a licensed serif would put a redistribution obligation on every
    // fork of an MIT application.
    const css = toCssVariables();
    expect(css).toContain("serif");
    expect(css).toContain("system-ui");
  });
});

describe("AA thresholds", () => {
  it("uses the WCAG 2.2 values", () => {
    expect(AA_BODY).toBe(4.5);
    expect(AA_LARGE).toBe(3);
  });
});
