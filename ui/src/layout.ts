/**
 * Three-pane layout decisions (issue #31).
 *
 * Extracted as pure functions so the *reflow rules* can be tested without a
 * browser. What still needs real rendering is whether the resulting CSS
 * actually fits — but which pane collapses, at what width, and in what order is
 * logic, and logic can be asserted.
 *
 * # The rule underneath all of it
 *
 * The Library is the primary pane. A user opened this application to look at
 * their games, so when space runs out the inspector goes first, then navigation
 * becomes a rail, and the content pane is the last thing to give ground. Core
 * tasks must never require horizontal scrolling — at 200% scaling on a small
 * laptop that would mean scrolling sideways to reach a confirm button, which is
 * how destructive actions get mis-clicked.
 */

/** Which panes are showing. */
export interface PaneVisibility {
  readonly navigation: "full" | "rail";
  readonly content: true;
  readonly inspector: boolean;
}

/** Effective width in CSS pixels, after the user's scaling is applied. */
export function effectiveWidth(viewportPx: number, scalePercent: number): number {
  if (scalePercent <= 0) {
    throw new Error("scale must be positive");
  }
  return (viewportPx * 100) / scalePercent;
}

const REM = 16;
const INSPECTOR_COLLAPSE_REM = 60;
const NAVIGATION_RAIL_REM = 44;

/**
 * Decides pane visibility for an effective width.
 *
 * The inspector collapses before navigation becomes a rail, and content is
 * never hidden — it is what the user came for.
 */
export function panesFor(effectiveWidthPx: number): PaneVisibility {
  return {
    navigation: effectiveWidthPx < NAVIGATION_RAIL_REM * REM ? "rail" : "full",
    content: true,
    inspector: effectiveWidthPx >= INSPECTOR_COLLAPSE_REM * REM,
  };
}

/**
 * The minimum width a core task needs, so a caller can assert no horizontal
 * scrolling is required.
 */
export const MINIMUM_TASK_WIDTH_PX = 20 * REM;

/** Whether a core task can complete without scrolling sideways. */
export function fitsWithoutHorizontalScroll(
  viewportPx: number,
  scalePercent: number,
): boolean {
  return effectiveWidth(viewportPx, scalePercent) >= MINIMUM_TASK_WIDTH_PX;
}

/**
 * Whether motion should be suppressed.
 *
 * Motion here is decorative, so honouring the preference costs nothing and
 * ignoring it can cause real harm to someone with a vestibular disorder.
 */
export function transitionFor(prefersReducedMotion: boolean): string {
  return prefersReducedMotion ? "0ms" : "120ms ease-out";
}
