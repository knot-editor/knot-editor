import type { ThemeChrome } from "knot";

/** Cards with rounded corners floating on the window background. */
export const ISLANDS_CHROME: ThemeChrome = {
  cardRadius: 8,
  cardGap: 4,
  cardInset: 4,
  cardBorder: 1,
  tabBarHeight: 40,
  stripeWidth: 36,
  accentWidth: 0,
  tabInset: 6,
  tabRadius: 5.5,
};

/** Square cards that meet at 1px borders, with no gap or window margin. */
export const FLAT_CHROME: ThemeChrome = {
  cardRadius: 0,
  cardGap: 0,
  cardInset: 0,
  cardBorder: 1,
  tabBarHeight: 30,
  stripeWidth: 36,
  accentWidth: 2,
  tabInset: 0,
  tabRadius: 0,
};

/** `color`, a `#rrggbb` value, at `percent` opacity: a band behind text. */
export function tint(color: string, percent: number): string {
  const alpha = Math.round((percent / 100) * 255).toString(16).padStart(2, "0");
  return `${color}${alpha}`;
}
