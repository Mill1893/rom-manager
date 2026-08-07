/**
 * Cartridge Index design tokens (issue #31).
 *
 * The production expression of the direction settled in issue #24: warm
 * archival, light-first, rust accent, moderate density. Rewritten as tokens
 * rather than promoted from the prototype CSS, which stays throwaway evidence.
 *
 * # Colour never carries meaning alone
 *
 * Every state below pairs a colour with a **shape** and a **label**. A user who
 * cannot distinguish rust from green — or who is reading in a high-contrast
 * mode that flattens the palette — still gets the state from the icon and the
 * words. Colour is the fastest channel, not the only one.
 */

/** The warm archival palette. Light is the primary theme, not an afterthought. */
export const light = {
  background: "#f1eadb",
  /** Fine keyline for framed box art and pane divisions. */
  line: "#cfc0a5",
  foreground: "#27241e",
  /** Secondary text: still body-legible, not decorative. */
  dim: "#675f52",
  /** Tertiary text. Used only at large sizes, where AA allows 3:1. */
  faint: "#6b6052",
  accent: "#a03d17",
  good: "#1f5238",
  warn: "#7a4700",
  bad: "#8f2323",
} as const;

export type Palette = typeof light;

/**
 * Type roles.
 *
 * Fonts must be redistributable with an MIT application or rely on an
 * intentional platform stack. These are platform stacks by design: shipping a
 * licensed serif would put a redistribution obligation on every fork.
 */
export const type = {
  /** Editorial serif, display headings only. */
  display: "Iowan Old Style, Palatino Linotype, Georgia, serif",
  /** The UI face. Everything that is not a heading or a technical value. */
  ui: "Inter, Segoe UI, system-ui, -apple-system, sans-serif",
  /** Monospace, reserved for paths, hashes, and similar technical values. */
  mono: "ui-monospace, SFMono-Regular, Consolas, monospace",
} as const;

/** Moderate density: comfortable to scan, dense enough to manage a library. */
export const density = {
  rowHeight: "2.25rem",
  gutter: "0.75rem",
  paneGap: "1rem",
  /** Box art keyline and offset depth — a physical indexed object. */
  artFrame: "1px",
  artOffset: "2px",
} as const;

/** A state's full expression. Colour is one channel of three. */
export interface StateSemantics {
  readonly label: string;
  /** A shape, so the state survives any colour treatment. */
  readonly shape: string;
  readonly color: keyof Palette;
  /** Whether the border must be emphasised as well. */
  readonly strongKeyline: boolean;
}

export const states = {
  success: {
    label: "Synced",
    shape: "check",
    color: "good",
    strongKeyline: false,
  },
  stale: {
    label: "Needs refresh",
    shape: "clock",
    color: "warn",
    strongKeyline: false,
  },
  blocked: {
    label: "Blocked",
    shape: "octagon",
    color: "bad",
    strongKeyline: true,
  },
  destructive: {
    // Named exactly as issue #24 requires, so a destructive block is never
    // mistaken for an ordinary one.
    label: "ACTION REQUIRED",
    shape: "octagon",
    color: "bad",
    strongKeyline: true,
  },
  indeterminate: {
    label: "Unconfirmed",
    shape: "question",
    color: "warn",
    strongKeyline: true,
  },
} as const satisfies Record<string, StateSemantics>;

export type StateName = keyof typeof states;

/**
 * Layout behaviour under constraint.
 *
 * At narrow widths and 200% scaling the Library stays primary: the inspector
 * collapses before content does, because content is what the user came for.
 * Core tasks must never require horizontal scrolling.
 */
export const responsive = {
  /** Below this, the inspector pane collapses. */
  inspectorCollapse: "60rem",
  /** Below this, navigation becomes a compact overflowable rail. */
  navigationRail: "44rem",
  /** Never required for a core task. */
  horizontalScrollAllowed: false,
  /** Panes reflow rather than shrinking past legibility. */
  minimumContentWidth: "20rem",
} as const;

/** Motion is decorative here, so it is the first thing to go. */
export const motion = {
  transition: "120ms ease-out",
  reducedMotionTransition: "0ms",
} as const;

/** Emits the tokens as CSS custom properties. */
export function toCssVariables(palette: Palette = light): string {
  const entries = [
    ...Object.entries(palette).map(([name, value]) => [`--color-${name}`, value]),
    ...Object.entries(type).map(([name, value]) => [`--font-${name}`, value]),
    ...Object.entries(density).map(([name, value]) => [`--density-${name}`, value]),
  ];
  return entries.map(([name, value]) => `${name}: ${value};`).join("\n");
}
