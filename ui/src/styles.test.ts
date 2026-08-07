/**
 * The stylesheet is generated, so what is worth testing is that it stays
 * generated — that no colour reaches the screen without having gone through the
 * palette the contrast tests check.
 */

import { describe, expect, it } from "vitest";
import { stylesheet } from "./styles";
import { light, toCssVariables } from "./tokens";

describe("the stylesheet", () => {
  it("names no colour of its own", () => {
    // A hex literal here would be a second copy of the palette, free to pass
    // the contrast tests while shipping something else.
    const literals = stylesheet().match(/#[0-9a-f]{3,8}\b/gi) ?? [];
    expect(literals).toEqual([]);
  });

  it("uses only custom properties the tokens actually emit", () => {
    const emitted = new Set(
      Array.from(toCssVariables().matchAll(/(--[a-zA-Z-]+):/g), (match) => match[1]),
    );
    const referenced = new Set(
      Array.from(stylesheet().matchAll(/var\((--[a-zA-Z-]+)\)/g), (match) => match[1]),
    );

    for (const name of referenced) {
      expect(emitted, `${name} is styled but never defined`).toContain(name);
    }
    expect(referenced.size).toBeGreaterThan(0);
  });

  it("covers every colour role the palette defines", () => {
    // Not a completeness requirement so much as a drift check: a role added to
    // the palette and never styled is a decision nobody made.
    const referenced = stylesheet();
    for (const role of ["background", "foreground", "line", "accent"] as const) {
      expect(light).toHaveProperty(role);
      expect(referenced).toContain(`var(--color-${role})`);
    }
  });

  it("never removes focus, only restyles it", () => {
    // outline:none with nothing replacing it is how a keyboard user loses
    // their place entirely.
    const css = stylesheet();
    expect(css).toContain(":focus-visible");
    expect(css).not.toMatch(/outline:\s*(none|0)\s*;/);
  });

  it("keeps disabled controls legible", () => {
    // A user who cannot read why a button is unavailable cannot act on the
    // reason printed beside it.
    expect(stylesheet()).toMatch(/button:disabled[^}]*opacity:\s*1/);
  });

  it("honours a reduced-motion preference", () => {
    expect(stylesheet()).toContain("prefers-reduced-motion");
  });

  it("does not require horizontal scrolling for the shell", () => {
    // The tokens say so; this checks the stylesheet did not quietly disagree.
    expect(stylesheet()).not.toMatch(/overflow-x:\s*scroll/);
  });
});
