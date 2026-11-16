import type { ThemeChrome } from "knot";

/** Cards with rounded corners floating on the window background. */
export const ISLANDS_CHROME: ThemeChrome = {
  cardRadius: 6,
  cardGap: 8,
  cardInset: 4,
  cardBorder: 1,
  tabBarHeight: 30,
  stripeWidth: 36,
  headerWeight: 600,
  accentWidth: 2,
};

/** Square cards that meet at 1px borders, with no gap or window margin. */
export const FLAT_CHROME: ThemeChrome = {
  cardRadius: 0,
  cardGap: 0,
  cardInset: 0,
  cardBorder: 1,
  tabBarHeight: 30,
  stripeWidth: 36,
  headerWeight: 600,
  accentWidth: 2,
};
