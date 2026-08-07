/**
 * The redundant-status contract (issue #31, under #24).
 *
 * What is worth testing is not that a badge renders, but that the state
 * survives losing a channel: colour removed, shape removed, or both.
 */

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StatusBadge } from "./StatusBadge";
import { type StateName, states } from "./tokens";

const ALL = Object.keys(states) as StateName[];

describe("every declared state", () => {
  it("renders a shape, so the state survives a flattened palette", () => {
    // A high-contrast mode, a monochrome display, or colour blindness all
    // remove the colour channel. The shape has to carry the state without it.
    for (const state of ALL) {
      const { container, unmount } = render(<StatusBadge state={state} />);
      const svg = container.querySelector("svg");
      expect(svg, `${state} drew no shape`).not.toBeNull();
      expect(
        svg?.querySelectorAll("path, circle").length ?? 0,
        `${state} drew an empty shape`,
      ).toBeGreaterThan(0);
      unmount();
    }
  });

  it("renders words, so the state survives losing the shape too", () => {
    for (const state of ALL) {
      const { unmount } = render(<StatusBadge state={state} />);
      expect(screen.getByText(states[state].label)).toBeInTheDocument();
      unmount();
    }
  });

  it("uses a distinct shape per meaning, not one icon recoloured", () => {
    // Three states rendered as the same glyph in three colours would be the
    // colour channel wearing a disguise.
    const shapes = new Set(ALL.map((state) => states[state].shape));
    expect(shapes.size).toBeGreaterThanOrEqual(3);
  });

  it("draws its colour from the palette rather than naming one", () => {
    for (const state of ALL) {
      const { container, unmount } = render(<StatusBadge state={state} />);
      const badge = container.querySelector(".status-badge") as HTMLElement;
      expect(badge.style.color).toContain(`--color-${states[state].color}`);
      unmount();
    }
  });

  it("emphasises the keyline exactly where the tokens say to", () => {
    for (const state of ALL) {
      const { container, unmount } = render(<StatusBadge state={state} />);
      const badge = container.querySelector(".status-badge") as HTMLElement;
      expect(badge.style.borderWidth).toBe(states[state].strongKeyline ? "2px" : "1px");
      unmount();
    }
  });
});

describe("the shape", () => {
  it("is hidden from assistive technology", () => {
    // The label is already read as text. Announcing "octagon" beside it would
    // be noise, not a second channel.
    const { container } = render(<StatusBadge state="blocked" />);
    expect(container.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
  });
});

describe("a destructive state", () => {
  it("is named distinctly so it is never read as an ordinary block", () => {
    render(<StatusBadge state="destructive" />);
    expect(screen.getByText("ACTION REQUIRED")).toBeInTheDocument();
    expect(states.destructive.strongKeyline).toBe(true);
  });
});

describe("a caller-supplied label", () => {
  it("replaces the token wording without changing the state's channels", () => {
    const { container } = render(<StatusBadge state="stale" label="Changed since planning" />);
    expect(screen.getByText("Changed since planning")).toBeInTheDocument();
    expect(container.querySelector("svg")).not.toBeNull();
    expect(container.querySelector(".status-badge")).toHaveAttribute("data-state", "stale");
  });
});
