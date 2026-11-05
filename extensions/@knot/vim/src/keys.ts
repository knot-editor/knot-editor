// Parses normal-mode keys into commands. Each key is one typed character or
// a named key such as "<esc>"; `feed` takes the input pending so far and
// returns either more pending input or a complete command.

export type SimpleMotion =
  | "left" | "right" | "left-wrap" | "right-wrap" | "up" | "down" | "row-up" | "row-down"
  | "word" | "WORD" | "back" | "BACK" | "end" | "END" | "back-end" | "BACK-END"
  | "line-start" | "first-non-blank" | "line-end" | "next-line-start" | "previous-line-start"
  | "first-line" | "last-line" | "match-pair" | "paragraph-forward" | "paragraph-back"
  | "repeat-find" | "repeat-find-reverse";

export type FindKind = "f" | "F" | "t" | "T";

export type Motion = SimpleMotion | Readonly<{ find: FindKind; char: string }>;

export type Operator = "d" | "c" | "y" | ">" | "<" | "=" | "gu" | "gU" | "g~";

export type TextObject =
  | "word" | "WORD" | "paren" | "brace" | "bracket" | "angle"
  | "dquote" | "squote" | "backtick" | "paragraph";

/** What an operator applies to: a motion, a text object, or whole lines
 * from a doubled operator such as `dd`. */
export type Target =
  | { readonly kind: "motion"; readonly motion: Motion }
  | { readonly kind: "object"; readonly object: TextObject; readonly around: boolean }
  | { readonly kind: "line" };

export type InsertAt = "i" | "a" | "I" | "A" | "o" | "O";

export type Command =
  | { readonly kind: "motion"; readonly motion: Motion; readonly count: number | null }
  | {
    readonly kind: "operator"; readonly operator: Operator; readonly register: string | null;
    readonly count: number | null; readonly target: Target;
  }
  | { readonly kind: "put"; readonly before: boolean; readonly register: string | null; readonly count: number }
  | { readonly kind: "replace"; readonly char: string; readonly count: number }
  | { readonly kind: "toggle-case"; readonly count: number }
  | { readonly kind: "join"; readonly count: number }
  | { readonly kind: "insert"; readonly at: InsertAt }
  | { readonly kind: "undo"; readonly count: number }
  | { readonly kind: "redo"; readonly count: number };

/** Input typed so far: a register, a count, an operator with its own count,
 * a key prefix such as "g" or "f", and the keys as typed, for display. */
export type Pending = Readonly<{
  register: string | null;
  count: string;
  operator: Operator | null;
  operatorCount: string;
  prefix: string;
  typed: string;
}>;

export const EMPTY: Pending = { register: null, count: "", operator: null, operatorCount: "", prefix: "", typed: "" };

/** What one key leaves: more pending input, or complete input with nothing
 * pending, whose command is null for a key that cancels or means nothing. */
export type Fed = { readonly pending: Pending } | { readonly command: Command | null };

/** Counts above this are clamped, so a stray run of digits stays bounded. */
const COUNT_LIMIT = 99_999;

const MOTIONS: Readonly<Record<string, SimpleMotion>> = {
  h: "left", l: "right",
  "<bs>": "left-wrap", " ": "right-wrap",
  j: "down", k: "up",
  w: "word", W: "WORD", b: "back", B: "BACK", e: "end", E: "END",
  "0": "line-start", "^": "first-non-blank", $: "line-end",
  "+": "next-line-start", "<cr>": "next-line-start", "-": "previous-line-start",
  G: "last-line", "%": "match-pair", "}": "paragraph-forward", "{": "paragraph-back",
  ";": "repeat-find", ",": "repeat-find-reverse",
};

const G_MOTIONS: Readonly<Record<string, SimpleMotion>> = {
  g: "first-line", j: "row-down", k: "row-up", e: "back-end", E: "BACK-END",
};

const OBJECTS: Readonly<Record<string, TextObject>> = {
  w: "word", W: "WORD",
  "(": "paren", ")": "paren", b: "paren",
  "{": "brace", "}": "brace", B: "brace",
  "[": "bracket", "]": "bracket",
  "<": "angle", ">": "angle",
  '"': "dquote", "'": "squote", "`": "backtick",
  p: "paragraph",
};

const OPERATORS = new Set<string>(["d", "c", "y", ">", "<", "="]);

/** The `g` operators and the key that doubles each, as in `gUU`. */
const G_OPERATORS: Readonly<Record<string, Operator>> = { u: "gu", U: "gU", "~": "g~" };
const DOUBLING: Readonly<Record<string, string>> = { gu: "u", gU: "U", "g~": "~" };

/** Shorthands for an operator over a motion or the line, as `x` is `dl`. */
const SHORTHANDS: Readonly<Record<string, readonly [Operator, Target]>> = {
  x: ["d", { kind: "motion", motion: "right" }],
  X: ["d", { kind: "motion", motion: "left" }],
  s: ["c", { kind: "motion", motion: "right" }],
  S: ["c", { kind: "line" }],
  D: ["d", { kind: "motion", motion: "line-end" }],
  C: ["c", { kind: "motion", motion: "line-end" }],
  Y: ["y", { kind: "line" }],
};

const INSERTS = new Set<string>(["i", "a", "I", "A", "o", "O"]);

/** Registers a `"` prefix may name: `"` itself is the unnamed register. */
const REGISTER = /^[a-zA-Z0-9"+*_%:-]$/;

/** The text a status display shows for pending input. */
export function pendingText(pending: Pending): string {
  return pending.typed;
}

/** The character a key stands for after `f`, `t`, or `r`, or null for a
 * named key that stands for none. */
function characterOf(key: string): string | null {
  if (key === "<tab>") return "\t";
  return key.length > 1 && key.startsWith("<") ? null : key;
}

export function feed(pending: Pending, key: string): Fed {
  const done = (command: Command | null): Fed => ({ command });
  if (key === "<esc>") return done(null);
  const next = (changes: Partial<Pending>): Fed => ({
    pending: { ...pending, ...changes, typed: pending.typed + displayed(key) },
  });
  const count = totalCount(pending);
  const { operator, register } = pending;
  const target = (target: Target): Fed => done(operator === null
    ? null
    : { kind: "operator", operator, register, count, target });
  const motion = (motion: Motion): Fed => operator === null
    ? done({ kind: "motion", motion, count })
    : target({ kind: "motion", motion });

  switch (pending.prefix) {
    case '"':
      if (!REGISTER.test(key)) return done(null);
      return next({ register: key === '"' ? null : key, prefix: "" });
    case "f": case "F": case "t": case "T": {
      const char = characterOf(key);
      return char === null ? done(null) : motion({ find: pending.prefix, char });
    }
    case "r": {
      const char = characterOf(key);
      return char === null ? done(null) : done({ kind: "replace", char, count: count ?? 1 });
    }
    case "i": case "a": {
      const object = OBJECTS[key];
      return object === undefined
        ? done(null)
        : target({ kind: "object", object, around: pending.prefix === "a" });
    }
    case "g": {
      if (operator !== null && DOUBLING[operator] === key) return target({ kind: "line" });
      const gOperator = G_OPERATORS[key];
      if (gOperator !== undefined) {
        if (operator === null) return next({ operator: gOperator, prefix: "" });
        return done(null);
      }
      const gMotion = G_MOTIONS[key];
      return gMotion === undefined ? done(null) : motion(gMotion);
    }
  }

  if (/^[1-9]$/.test(key) || (key === "0" && (operator === null ? pending.count : pending.operatorCount) !== "")) {
    return operator === null
      ? next({ count: pending.count + key })
      : next({ operatorCount: pending.operatorCount + key });
  }
  if (key === "g") return next({ prefix: "g" });
  if (key === "f" || key === "F" || key === "t" || key === "T") return next({ prefix: key });
  if (OPERATORS.has(key)) {
    if (operator === null) return next({ operator: key as Operator });
    return operator === key ? target({ kind: "line" }) : done(null);
  }
  if (operator !== null && DOUBLING[operator] === key) return target({ kind: "line" });
  const simple = MOTIONS[key];
  if (simple !== undefined) return motion(simple);
  if (operator !== null) {
    return key === "i" || key === "a" ? next({ prefix: key }) : done(null);
  }
  if (key === '"') return next({ prefix: '"' });
  if (key === "r") return next({ prefix: "r" });
  const shorthand = SHORTHANDS[key];
  if (shorthand !== undefined) {
    return done({ kind: "operator", operator: shorthand[0], register, count, target: shorthand[1] });
  }
  if (key === "p" || key === "P") return done({ kind: "put", before: key === "P", register, count: count ?? 1 });
  if (key === "~") return done({ kind: "toggle-case", count: count ?? 1 });
  if (key === "J") return done({ kind: "join", count: count ?? 1 });
  if (INSERTS.has(key)) return done({ kind: "insert", at: key as InsertAt });
  if (key === "u") return done({ kind: "undo", count: count ?? 1 });
  if (key === "<c-r>") return done({ kind: "redo", count: count ?? 1 });
  return done(null);
}

/** The count of the command: the product of the counts before and after
 * the operator, as `2d3w` deletes six words, or null when neither was typed. */
function totalCount(pending: Pending): number | null {
  if (pending.count === "" && pending.operatorCount === "") return null;
  const product = Number(pending.count || "1") * Number(pending.operatorCount || "1");
  return Math.min(product, COUNT_LIMIT);
}

/** How a key shows among pending keys. */
function displayed(key: string): string {
  switch (key) {
    case "<cr>": return "^M";
    case "<bs>": return "^H";
    case "<tab>": return "^I";
    default: return key.length > 1 && key.startsWith("<") ? "" : key;
  }
}
