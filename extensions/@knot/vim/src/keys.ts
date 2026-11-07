// Parses normal- and visual-mode keys into commands. Each key is one typed
// character or a named key such as "<esc>"; `feed` takes the input pending
// so far and returns either more pending input or a complete command.

export type SimpleMotion =
  | "left" | "right" | "left-wrap" | "right-wrap" | "up" | "down" | "row-up" | "row-down"
  | "word" | "WORD" | "back" | "BACK" | "end" | "END" | "back-end" | "BACK-END"
  | "line-start" | "first-non-blank" | "line-end" | "next-line-start" | "previous-line-start"
  | "first-line" | "last-line" | "match-pair" | "paragraph-forward" | "paragraph-back"
  | "repeat-find" | "repeat-find-reverse" | "window-top" | "window-middle" | "window-bottom";

/** A scroll by half a view (`ctrl-d`, `ctrl-u`), a page (`ctrl-f`,
 * `ctrl-b`), or lines (`ctrl-e`, `ctrl-y`). */
export type Scroll = "half-down" | "half-up" | "page-down" | "page-up" | "line-down" | "line-up";

export type FindKind = "f" | "F" | "t" | "T";

/** A mark motion: `'` goes to the mark's line, `` ` `` to the mark itself.
 * The name is a letter, or `'` or `` ` `` for the position before the latest
 * jump. */
export type MarkMotion = Readonly<{ mark: string; exact: boolean }>;

export type Motion = SimpleMotion | Readonly<{ find: FindKind; char: string }> | MarkMotion;

export type Operator = "d" | "c" | "y" | ">" | "<" | "=" | "gu" | "gU" | "g~";

export type TextObject =
  | "word" | "WORD" | "paren" | "brace" | "bracket" | "angle"
  | "dquote" | "squote" | "backtick" | "paragraph";

/** What an operator applies to: a motion, a text object, whole lines from
 * a doubled operator such as `dd`, or the visual selection, taken as whole
 * lines by `D`, `X`, `Y`, `C`, `S`, and `R`. */
export type Target =
  | { readonly kind: "motion"; readonly motion: Motion }
  | { readonly kind: "object"; readonly object: TextObject; readonly around: boolean }
  | { readonly kind: "line" }
  | { readonly kind: "selection"; readonly linewise: boolean };

/** What `ctrl-w` does: moves to the pane in a direction, splits, closes the
 * tab, shows only it, goes to the next recent tab, or resizes. */
export type WindowAction =
  | "focus-left" | "focus-down" | "focus-up" | "focus-right"
  | "split-down" | "split-right" | "close" | "only" | "next" | "previous" | "last-used" | "first" | "last" | "equalize" | "grow" | "shrink";

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
  | { readonly kind: "redo"; readonly count: number }
  /** `m`: sets a mark at the caret. */
  | { readonly kind: "set-mark"; readonly name: string }
  | { readonly kind: "scroll"; readonly scroll: Scroll; readonly count: number | null }
  /** `zt`, `zz`, `zb`, and with `firstNonBlank` `z<cr>`, `z.`, `z-`: scrolls
   * the caret's line, or line `count`, to that place in the view. */
  | {
    readonly kind: "reposition"; readonly at: "top" | "center" | "bottom"; readonly firstNonBlank: boolean;
    readonly count: number | null;
  }
  /** `ctrl-o` (negative) and `ctrl-i`: steps through the jumplist. */
  | { readonly kind: "jump"; readonly count: number }
  /** `v` or `V`: enters, switches, or leaves visual mode. */
  | { readonly kind: "visual"; readonly linewise: boolean }
  /** `gv`: selects the previous visual area again. */
  | { readonly kind: "reselect" }
  /** `o` in visual mode: moves the caret to the selection's other end. */
  | { readonly kind: "swap-ends" }
  | { readonly kind: "leave-visual" }
  /** `ctrl-w` and a key: a window command. */
  | { readonly kind: "window"; readonly action: WindowAction }
  /** `:`: opens the Ex line. */
  | { readonly kind: "ex" }
  /** A text object typed in visual mode, which selects it. */
  | { readonly kind: "select-object"; readonly object: TextObject; readonly around: boolean; readonly count: number };

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
  "<left>": "left", "<right>": "right", "<up>": "up", "<down>": "down",
  "<home>": "line-start", "<end>": "line-end",
  H: "window-top", M: "window-middle", L: "window-bottom",
};

const SCROLLS: Readonly<Record<string, Scroll>> = {
  "<c-d>": "half-down", "<c-u>": "half-up", "<c-f>": "page-down", "<c-b>": "page-up",
  "<c-e>": "line-down", "<c-y>": "line-up",
};

/** The keys after `z` and where each puts the line. */
const REPOSITIONS: Readonly<Record<string, readonly ["top" | "center" | "bottom", boolean]>> = {
  t: ["top", false], z: ["center", false], b: ["bottom", false],
  "<cr>": ["top", true], ".": ["center", true], "-": ["bottom", true],
};

/** The keys after `ctrl-w`. With ctrl held, `h` arrives as Backspace, as in
 * Vim, and the others as their control names. */
const WINDOW_ACTIONS: Readonly<Record<string, WindowAction>> = {
  h: "focus-left", "<bs>": "focus-left", j: "focus-down", "<c-j>": "focus-down",
  k: "focus-up", "<c-k>": "focus-up", l: "focus-right", "<c-l>": "focus-right",
  s: "split-down", v: "split-right", c: "close", q: "close",
  "=": "equalize", "+": "grow", ">": "grow", "-": "shrink", "<": "shrink",
  o: "only", "<c-o>": "only", w: "next", "<c-w>": "next", W: "previous",
  p: "last-used", t: "first", b: "last", "<c-b>": "last",
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

/** Visual-mode shorthands for an operator over the selection. */
const VISUAL_SHORTHANDS: Readonly<Record<string, readonly [Operator, boolean]>> = {
  x: ["d", false], X: ["d", true], D: ["d", true],
  s: ["c", false], S: ["c", true], C: ["c", true], R: ["c", true],
  Y: ["y", true],
  u: ["gu", false], U: ["gU", false], "~": ["g~", false],
};

const INSERTS = new Set<string>(["i", "a", "I", "A", "o", "O"]);

/** Registers a `"` prefix may name: `"` itself is the unnamed register. */
const REGISTER = /^[a-zA-Z0-9"+*_%:-]$/;

/** Marks `'` and `` ` `` may jump to, and `m` may set. */
const MARK = /^[a-zA-Z'`]$/;

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

/** Parses one key; `visual` is whether a visual mode is active, where
 * operators apply to the selection at once. */
export function feed(pending: Pending, key: string, visual = false): Fed {
  const done = (command: Command | null): Fed => ({ command });
  if (key === "<esc>") return done(visual ? { kind: "leave-visual" } : null);
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
  const selection = (operator: Operator, linewise: boolean): Fed =>
    done({ kind: "operator", operator, register, count, target: { kind: "selection", linewise } });

  switch (pending.prefix) {
    case '"':
      if (!REGISTER.test(key)) return done(null);
      return next({ register: key === '"' ? null : key, prefix: "" });
    case "f": case "F": case "t": case "T": {
      const char = characterOf(key);
      return char === null ? done(null) : motion({ find: pending.prefix, char });
    }
    case "'": case "`":
      return MARK.test(key) ? motion({ mark: key, exact: pending.prefix === "`" }) : done(null);
    case "m":
      return MARK.test(key) ? done({ kind: "set-mark", name: key }) : done(null);
    case "r": {
      const char = characterOf(key);
      return char === null ? done(null) : done({ kind: "replace", char, count: count ?? 1 });
    }
    case "i": case "a": {
      const object = OBJECTS[key];
      const around = pending.prefix === "a";
      if (object === undefined) return done(null);
      if (visual) return done({ kind: "select-object", object, around, count: count ?? 1 });
      return target({ kind: "object", object, around });
    }
    case "<c-w>": {
      const action = WINDOW_ACTIONS[key];
      return action === undefined ? done(null) : done({ kind: "window", action });
    }
    case "z": {
      const reposition = REPOSITIONS[key];
      if (reposition === undefined) return done(null);
      return done({ kind: "reposition", at: reposition[0], firstNonBlank: reposition[1], count });
    }
    case "g": {
      if (operator !== null && DOUBLING[operator] === key) return target({ kind: "line" });
      if (key === "v") return done({ kind: "reselect" });
      const gOperator = G_OPERATORS[key];
      if (gOperator !== undefined) {
        if (visual) return selection(gOperator, false);
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
  if (key === "f" || key === "F" || key === "t" || key === "T" || key === "'" || key === "`") {
    return next({ prefix: key });
  }
  if (OPERATORS.has(key)) {
    if (visual) return selection(key as Operator, false);
    if (operator === null) return next({ operator: key as Operator });
    return operator === key ? target({ kind: "line" }) : done(null);
  }
  if (operator !== null && DOUBLING[operator] === key) return target({ kind: "line" });
  const simple = MOTIONS[key];
  if (simple !== undefined) return motion(simple);
  if (operator !== null) {
    return key === "i" || key === "a" ? next({ prefix: key }) : done(null);
  }
  const scroll = SCROLLS[key];
  if (scroll !== undefined) return done({ kind: "scroll", scroll, count });
  if (key === "z") return next({ prefix: "z" });
  if (key === "<c-w>" && operator === null) return next({ prefix: "<c-w>" });
  if (key === '"') return next({ prefix: '"' });
  if (key === "r") return next({ prefix: "r" });
  if (key === "m" && !visual) return next({ prefix: "m" });
  if ((key === "<c-o>" || key === "<tab>") && !visual) {
    return done({ kind: "jump", count: (key === "<c-o>" ? -1 : 1) * (count ?? 1) });
  }
  if (key === "v" || key === "V") return done({ kind: "visual", linewise: key === "V" });
  if (key === ":" && !visual) return done({ kind: "ex" });
  if (visual) {
    if (key === "i" || key === "a") return next({ prefix: key });
    if (key === "o" || key === "O") return done({ kind: "swap-ends" });
    const shorthand = VISUAL_SHORTHANDS[key];
    if (shorthand !== undefined) return selection(shorthand[0], shorthand[1]);
    if (key === "p" || key === "P") return done({ kind: "put", before: key === "P", register, count: count ?? 1 });
    if (key === "J") return done({ kind: "join", count: count ?? 1 });
    return done(null);
  }
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
    case "<c-w>": return "^W";
    default: return key.length > 1 && key.startsWith("<") ? "" : key;
  }
}
