/**
 * WCAG 2.2 contrast arithmetic.
 *
 * Contrast ratio is a pure function of two colours — it needs no browser and no
 * rendering. That means the AA thresholds can be a *test* rather than a manual
 * check someone has to remember to repeat, which is the difference between a
 * guarantee and an intention.
 *
 * Formulae: WCAG 2.2 relative luminance and contrast ratio definitions.
 */

export interface Rgb {
  readonly r: number;
  readonly g: number;
  readonly b: number;
}

/** Parses `#rgb` or `#rrggbb`. */
export function parseHex(hex: string): Rgb {
  const value = hex.replace("#", "");
  const expanded =
    value.length === 3
      ? value
          .split("")
          .map((character) => character + character)
          .join("")
      : value;
  if (!/^[0-9a-fA-F]{6}$/.test(expanded)) {
    throw new Error(`not a colour: ${hex}`);
  }
  return {
    r: Number.parseInt(expanded.slice(0, 2), 16),
    g: Number.parseInt(expanded.slice(2, 4), 16),
    b: Number.parseInt(expanded.slice(4, 6), 16),
  };
}

/** WCAG relative luminance. */
export function relativeLuminance({ r, g, b }: Rgb): number {
  const channel = (value: number): number => {
    const normalized = value / 255;
    return normalized <= 0.03928
      ? normalized / 12.92
      : Math.pow((normalized + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** Contrast ratio between two colours, from 1 to 21. */
export function contrastRatio(foreground: string, background: string): number {
  const first = relativeLuminance(parseHex(foreground));
  const second = relativeLuminance(parseHex(background));
  const lighter = Math.max(first, second);
  const darker = Math.min(first, second);
  return (lighter + 0.05) / (darker + 0.05);
}

/** AA thresholds: 4.5:1 for body text, 3:1 for large text and UI boundaries. */
export const AA_BODY = 4.5;
export const AA_LARGE = 3;

export function meetsAA(
  foreground: string,
  background: string,
  large = false,
): boolean {
  return contrastRatio(foreground, background) >= (large ? AA_LARGE : AA_BODY);
}
