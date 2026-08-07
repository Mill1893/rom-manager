/**
 * A state rendered on all three channels (issue #31, under #24).
 *
 * `tokens.ts` pairs every state with a **colour**, a **shape**, and a **label**,
 * and says why: "A user who cannot distinguish rust from green — or who is
 * reading in a high-contrast mode that flattens the palette — still gets the
 * state from the icon and the words. Colour is the fastest channel, not the
 * only one."
 *
 * Until now only the colour channel existed. The shape was declared in the
 * tokens and drawn nowhere, which meant the redundancy was a claim rather than
 * a property. This renders all three.
 *
 * The shape is `aria-hidden` on purpose. It is a redundant channel for people
 * reading the screen; a screen reader gets the label as text, and announcing
 * "octagon" alongside it would be noise, not information.
 */

import { type StateName, states } from "./tokens";

export interface StatusBadgeProps {
  readonly state: StateName;
  /** Overrides the token label when the situation has a more specific name. */
  readonly label?: string;
}

/**
 * The shapes, drawn rather than typed.
 *
 * A font glyph would be simpler and would disappear on a host missing that
 * font — which is exactly the reader this channel exists for.
 */
function Shape({ shape }: { readonly shape: string }): React.JSX.Element {
  const common = {
    width: 14,
    height: 14,
    viewBox: "0 0 16 16",
    "aria-hidden": true,
    focusable: false,
    style: { flex: "none", verticalAlign: "-2px" },
  } as const;

  switch (shape) {
    case "check":
      return (
        <svg {...common}>
          <path
            d="M2 8.5 L6 12.5 L14 3.5"
            fill="none"
            stroke="currentColor"
            strokeWidth="2.5"
            strokeLinecap="square"
          />
        </svg>
      );
    case "clock":
      return (
        <svg {...common}>
          <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" strokeWidth="1.75" />
          <path d="M8 4.25 V8.25 L11 10" fill="none" stroke="currentColor" strokeWidth="1.75" />
        </svg>
      );
    case "octagon":
      return (
        <svg {...common}>
          <path
            d="M5.2 1 H10.8 L15 5.2 V10.8 L10.8 15 H5.2 L1 10.8 V5.2 Z"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.75"
          />
        </svg>
      );
    case "question":
      return (
        <svg {...common}>
          <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" strokeWidth="1.75" />
          <path
            d="M5.9 6 A2.2 2.2 0 1 1 8 9 V10"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.75"
          />
          <circle cx="8" cy="12.2" r="0.9" fill="currentColor" />
        </svg>
      );
    default:
      return <svg {...common} />;
  }
}

export function StatusBadge({ state, label }: StatusBadgeProps): React.JSX.Element {
  const semantics = states[state];
  return (
    <span
      className="status-badge"
      data-state={state}
      data-shape={semantics.shape}
      style={{
        color: `var(--color-${semantics.color})`,
        borderColor: `var(--color-${semantics.color})`,
        borderWidth: semantics.strongKeyline ? 2 : 1,
      }}
    >
      <Shape shape={semantics.shape} />
      {label ?? semantics.label}
    </span>
  );
}
