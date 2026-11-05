// Parses normal-mode keys into commands. Each key is one typed character or
// a named key such as "<esc>"; `feed` takes the input pending so far and
// returns either more pending input or a complete command.

export type Motion =
  | "left" | "right" | "left-wrap" | "right-wrap" | "up" | "down" | "row-up" | "row-down"
  | "word" | "WORD" | "back" | "BACK" | "end" | "END" | "back-end" | "BACK-END"
  | "line-start" | "first-non-blank" | "line-end" | "next-line-start" | "previous-line-start"
  | "first-line" | "last-line";

export type InsertAt = "i" | "a" | "I" | "A" | "o" | "O";

export type Command =
  | { readonly kind: "motion"; readonly motion: Motion; readonly count: number | null }
  | { readonly kind: "insert"; readonly at: InsertAt }
  | { readonly kind: "undo"; readonly count: number }
  | { readonly kind: "redo"; readonly count: number };

/** Input typed so far: a count and a key prefix such as "g". */
export type Pending = Readonly<{ count: string; prefix: string }>;

export const EMPTY: Pending = { count: "", prefix: "" };

/** What one key leaves: more pending input, or complete input with nothing
 * pending, whose command is null for a key that cancels or means nothing. */
export type Fed = { readonly pending: Pending } | { readonly command: Command | null };

/** Counts above this are clamped, so a stray run of digits stays bounded. */
const COUNT_LIMIT = 99_999;

const MOTIONS: Readonly<Record<string, Motion>> = {
  h: "left", l: "right",
  "<bs>": "left-wrap", " ": "right-wrap",
  j: "down", k: "up",
  w: "word", W: "WORD", b: "back", B: "BACK", e: "end", E: "END",
  "0": "line-start", "^": "first-non-blank", $: "line-end",
  "+": "next-line-start", "<cr>": "next-line-start", "-": "previous-line-start",
  G: "last-line",
};

const G_MOTIONS: Readonly<Record<string, Motion>> = {
  g: "first-line", j: "row-down", k: "row-up", e: "back-end", E: "BACK-END",
};

const INSERTS = new Set<string>(["i", "a", "I", "A", "o", "O"]);

/** The text a status display shows for pending input. */
export function pendingText(pending: Pending): string {
  return pending.count + pending.prefix;
}

export function feed(pending: Pending, key: string): Fed {
  const done = (command: Command | null): Fed => ({ command });
  if (key === "<esc>") return done(null);
  const count = pending.count === "" ? null : Math.min(Number(pending.count), COUNT_LIMIT);
  if (pending.prefix === "g") {
    const motion = G_MOTIONS[key];
    return done(motion === undefined ? null : { kind: "motion", motion, count });
  }
  if (/^[1-9]$/.test(key) || (key === "0" && pending.count !== "")) {
    return { pending: { count: pending.count + key, prefix: "" } };
  }
  if (key === "g") return { pending: { count: pending.count, prefix: "g" } };
  const motion = MOTIONS[key];
  if (motion !== undefined) return done({ kind: "motion", motion, count });
  if (INSERTS.has(key)) return done({ kind: "insert", at: key as InsertAt });
  if (key === "u") return done({ kind: "undo", count: count ?? 1 });
  if (key === "<c-r>") return done({ kind: "redo", count: count ?? 1 });
  return done(null);
}
