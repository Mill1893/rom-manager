/**
 * The application's stylesheet, built from the design tokens (issue #31).
 *
 * # Why this is TypeScript rather than a `.css` file
 *
 * The palette has exactly one definition — `tokens.ts` — and the contrast tests
 * check *that* one. A hand-written stylesheet would be a second copy of the
 * same colours, free to pass the tests while shipping something else. Emitting
 * the CSS from the tokens means a colour that was never contrast-checked cannot
 * reach the screen.
 *
 * # What this does not settle
 *
 * The visual direction in [#31] is a larger piece of work with a prototype
 * behind it. This is the honest minimum: the warm archival palette, the three
 * type roles, the declared density, and the reflow and reduced-motion
 * behaviour the tokens already specify. It is not the Cartridge Index visual
 * language, and applying that remains open.
 */

import { motion, responsive } from "./tokens";

export function stylesheet(): string {
  return `
:root {
  color-scheme: light;
  font-family: var(--font-ui);
  /* 100% and 200% scaling both have to work, so nothing is sized in pixels
     that a user's zoom should be able to move. */
  font-size: 100%;
  line-height: 1.5;
  background: var(--color-background);
  color: var(--color-foreground);
}

*, *::before, *::after { box-sizing: border-box; }

body {
  margin: 0;
  background: var(--color-background);
  color: var(--color-foreground);
}

.shell {
  max-width: ${responsive.inspectorCollapse};
  margin: 0 auto;
  padding: var(--density-paneGap);
  /* Core tasks must never need horizontal scrolling, so the shell reflows
     rather than letting content push it wider than the viewport. */
  min-width: ${responsive.minimumContentWidth};
}

h1, h2, h3 {
  font-family: var(--font-display);
  font-weight: 600;
  line-height: 1.2;
  margin: 0 0 var(--density-gutter);
}

h1 { font-size: 1.75rem; }
h2 { font-size: 1.25rem; }
h3 { font-size: 1rem; }

p { margin: 0 0 var(--density-gutter); }

/* Paths, digests, and other technical values. Reserved, not decorative. */
code, .mono {
  font-family: var(--font-mono);
  font-size: 0.9em;
  overflow-wrap: anywhere;
}

section {
  border: var(--density-artFrame) solid var(--color-line);
  border-radius: 4px;
  padding: var(--density-paneGap);
  margin-bottom: var(--density-paneGap);
}

button {
  font: inherit;
  min-height: var(--density-rowHeight);
  padding: 0 var(--density-paneGap);
  margin-right: var(--density-gutter);
  color: var(--color-background);
  background: var(--color-accent);
  border: var(--density-artFrame) solid var(--color-accent);
  border-radius: 4px;
  cursor: pointer;
  transition: opacity ${motion.transition};
}

button:disabled {
  /* Disabled controls stay legible: a user who cannot read why a button is
     unavailable cannot act on the reason beside it. */
  opacity: 1;
  color: var(--color-dim);
  background: transparent;
  border-color: var(--color-line);
  cursor: not-allowed;
}

/* Focus is never removed, only restyled. It must survive both backgrounds. */
:focus-visible {
  outline: 2px solid var(--color-foreground);
  outline-offset: 2px;
}

ul { margin: 0 0 var(--density-gutter); padding-left: 1.25rem; }
li { margin-bottom: var(--density-gutter); }

/* Colour is one channel of three. The border weight and the word "alert" in
   the accessibility tree carry the same information for anyone who cannot
   distinguish rust from the background. */
.failure, [role="alert"] {
  color: var(--color-bad);
  border-left: 3px solid var(--color-bad);
  padding-left: var(--density-gutter);
}

progress { width: 100%; height: var(--density-gutter); }

@media (prefers-reduced-motion: reduce) {
  * { transition-duration: ${motion.reducedMotionTransition} !important; }
}

/* Below this the inspector would collapse. The wizard is single-column
   already, so it only needs to stop being centred on a narrow screen. */
@media (max-width: ${responsive.navigationRail}) {
  .shell { padding: var(--density-gutter); }
}
`.trim();
}
