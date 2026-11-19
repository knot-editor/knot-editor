import assert from "node:assert/strict";
import { test } from "node:test";
import { type Command, EMPTY, feed, type Pending, pendingText } from "../src/keys.js";

/** Feeds keys and returns the command they complete, plus what stays pending. */
function parse(...keys: string[]): { command: Command | null | undefined; pending: Pending } {
  return parseIn(false, keys);
}

/** Feeds keys in visual mode. */
function visual(...keys: string[]): Command | null | undefined {
  return parseIn(true, keys).command;
}

function parseIn(inVisual: boolean, keys: string[]): { command: Command | null | undefined; pending: Pending } {
  let pending = EMPTY;
  for (const key of keys) {
    const fed = feed(pending, key, inVisual);
    if ("pending" in fed) pending = fed.pending;
    else return { command: fed.command, pending: EMPTY };
  }
  return { command: undefined, pending };
}

test("counts prefix motions, and 0 is a motion until a count starts", () => {
  assert.deepEqual(parse("w").command, { kind: "motion", motion: "word", count: null });
  assert.deepEqual(parse("1", "0", "j").command, { kind: "motion", motion: "down", count: 10 });
  assert.deepEqual(parse("0").command, { kind: "motion", motion: "line-start", count: null });
});

test("g starts a two-key motion and shows as pending", () => {
  const { pending } = parse("3", "g");
  assert.equal(pendingText(pending), "3g");
  assert.deepEqual(parse("3", "g", "g").command, { kind: "motion", motion: "first-line", count: 3 });
  assert.deepEqual(parse("g", "j").command, { kind: "motion", motion: "row-down", count: null });
  assert.equal(parse("g", "z").command, null, "an unknown g key cancels");
});

test("Escape and unknown keys cancel pending input", () => {
  assert.equal(parse("2", "<esc>").command, null);
  assert.equal(parse("Q").command, null);
});

test("insert, undo, and redo commands", () => {
  assert.deepEqual(parse("A").command, { kind: "insert", at: "A", count: 1 });
  assert.deepEqual(parse("3", "i").command, { kind: "insert", at: "i", count: 3 });
  assert.deepEqual(parse("3", "u").command, { kind: "undo", count: 3 });
  assert.deepEqual(parse("<c-r>").command, { kind: "redo", count: 1 });
});

test("dot repeats the last change, with an optional count", () => {
  assert.deepEqual(parse(".").command, { kind: "repeat", count: null });
  assert.deepEqual(parse("3", ".").command, { kind: "repeat", count: 3 });
  assert.equal(parse("d", ".").command, null, "an operator waits for a motion");
  assert.equal(parseIn(true, ["."]).command, null, "dot means nothing in visual mode");
});

test("q records into a letter register and @ plays one, with a count", () => {
  assert.deepEqual(parse("q", "a").command, { kind: "record", register: "a" });
  assert.deepEqual(parse("q", "A").command, { kind: "record", register: "A" });
  assert.equal(parse("q", "1").command, null);
  assert.deepEqual(parse("@", "a").command, { kind: "macro", register: "a", count: 1 });
  assert.deepEqual(parse("3", "@", "b").command, { kind: "macro", register: "b", count: 3 });
  assert.deepEqual(parse("@", "@").command, { kind: "macro", register: "@", count: 1 });
  assert.equal(parse("@", "A").command, null);
  assert.equal(parse("d", "q").command, null, "an operator waits for a motion");
  assert.equal(parseIn(true, ["q"]).command, null, "recording starts in normal mode");
});

test("a huge count is clamped", () => {
  const keys = [..."123456789"];
  assert.deepEqual(parse(...keys, "l").command, { kind: "motion", motion: "right", count: 99_999 });
});

test("operators take a motion, a text object, or their doubled line form", () => {
  assert.deepEqual(parse("d", "w").command, {
    kind: "operator", operator: "d", register: null, count: null, target: { kind: "motion", motion: "word" },
  });
  assert.deepEqual(parse("c", "i", "(").command, {
    kind: "operator", operator: "c", register: null, count: null,
    target: { kind: "object", object: "paren", around: false },
  });
  assert.deepEqual(parse("y", "y").command, {
    kind: "operator", operator: "y", register: null, count: null, target: { kind: "line" },
  });
  for (const keys of [["g", "U", "U"], ["g", "U", "g", "U"]]) {
    assert.deepEqual(parse(...keys).command, {
      kind: "operator", operator: "gU", register: null, count: null, target: { kind: "line" },
    });
  }
  assert.equal(parse("d", "y").command, null, "a different operator cancels");
  assert.equal(parse("d", "i", "z").command, null, "an unknown object cancels");
});

test("counts before and after an operator multiply, and a register comes first", () => {
  assert.deepEqual(parse('"', "a", "2", "d", "3", "w").command, {
    kind: "operator", operator: "d", register: "a", count: 6, target: { kind: "motion", motion: "word" },
  });
  assert.equal(pendingText(parse('"', "a", "2", "d").pending), '"a2d');
  assert.deepEqual(parse('"', '"', "p").command, { kind: "put", before: false, register: null, count: 1 });
  assert.equal(parse('"', "!").command, null);
});

test("finds take the next key as their character", () => {
  assert.deepEqual(parse("2", "t", ")").command, { kind: "motion", motion: { find: "t", char: ")" }, count: 2 });
  assert.deepEqual(parse("f", "<tab>").command, { kind: "motion", motion: { find: "f", char: "\t" }, count: null });
  assert.equal(parse("f", "<cr>").command, null);
  assert.deepEqual(parse("d", "f", "x").command, {
    kind: "operator", operator: "d", register: null, count: null,
    target: { kind: "motion", motion: { find: "f", char: "x" } },
  });
});

test("shorthands are operators", () => {
  assert.deepEqual(parse("3", "x").command, {
    kind: "operator", operator: "d", register: null, count: 3, target: { kind: "motion", motion: "right" },
  });
  assert.deepEqual(parse("C").command, {
    kind: "operator", operator: "c", register: null, count: null, target: { kind: "motion", motion: "line-end" },
  });
  assert.deepEqual(parse("2", "r", "a").command, { kind: "replace", char: "a", count: 2 });
  assert.deepEqual(parse("J").command, { kind: "join", count: 1 });
});

test("r takes Enter as a line break, except in visual mode", () => {
  assert.deepEqual(parse("2", "r", "<cr>").command, { kind: "replace", char: "\n", count: 2 });
  assert.equal(visual("r", "<cr>"), null);
});

test("gJ, gi, ga, gp, gP, and the bracket puts are commands", () => {
  assert.deepEqual(parse("3", "g", "J").command, { kind: "join", count: 3, spaces: false });
  assert.deepEqual(visual("g", "J"), { kind: "join", count: 1, spaces: false });
  assert.deepEqual(parse("2", "g", "i").command, { kind: "insert", at: "gi", count: 2 });
  assert.deepEqual(parse("g", "a").command, { kind: "show-character" });
  assert.deepEqual(parse("g", "p").command, {
    kind: "put", before: false, register: null, count: 1, variant: "caret-after",
  });
  assert.deepEqual(parse('"', "a", "g", "P").command, {
    kind: "put", before: true, register: "a", count: 1, variant: "caret-after",
  });
  assert.deepEqual(parse("]", "p").command, { kind: "put", before: false, register: null, count: 1, variant: "indent" });
  for (const keys of [["[", "p"], ["[", "P"], ["]", "P"]]) {
    assert.deepEqual(parse(...keys).command, { kind: "put", before: true, register: null, count: 1, variant: "indent" });
  }
  assert.equal(parse("d", "g", "J").command, null);
});

test("_ and | are motions", () => {
  assert.deepEqual(parse("2", "_").command, { kind: "motion", motion: "line-down-first-non-blank", count: 2 });
  assert.deepEqual(parse("d", "3", "|").command, {
    kind: "operator", operator: "d", register: null, count: 3, target: { kind: "motion", motion: "column" },
  });
});

test("v, V, and gv start visual modes from normal mode", () => {
  assert.deepEqual(parse("v").command, { kind: "visual", linewise: false });
  assert.deepEqual(parse("V").command, { kind: "visual", linewise: true });
  assert.deepEqual(parse("g", "v").command, { kind: "reselect" });
});

test("in visual mode operators apply to the selection at once", () => {
  const selection = (operator: string, linewise = false, count: number | null = null, register: string | null = null) =>
    ({ kind: "operator", operator, register, count, target: { kind: "selection", linewise } });
  assert.deepEqual(visual("d"), selection("d"));
  assert.deepEqual(visual('"', "a", "y"), selection("y", false, null, "a"));
  assert.deepEqual(visual("3", ">"), selection(">", false, 3));
  assert.deepEqual(visual("g", "U"), selection("gU"));
  assert.deepEqual(visual("U"), selection("gU"));
  assert.deepEqual(visual("~"), selection("g~"));
  assert.deepEqual(visual("x"), selection("d"));
  assert.deepEqual(visual("D"), selection("d", true));
  assert.deepEqual(visual("S"), selection("c", true));
  assert.deepEqual(visual("Y"), selection("y", true));
});

test("visual mode keys: motions, objects, ends, and leaving", () => {
  assert.deepEqual(visual("2", "w"), { kind: "motion", motion: "word", count: 2 });
  assert.deepEqual(visual("i", "w"), { kind: "select-object", object: "word", around: false, count: 1 });
  assert.deepEqual(visual("a", "("), { kind: "select-object", object: "paren", around: true, count: 1 });
  assert.deepEqual(visual("o"), { kind: "swap-ends" });
  assert.deepEqual(visual("<esc>"), { kind: "leave-visual" });
  assert.deepEqual(visual("p"), { kind: "put", before: false, register: null, count: 1 });
  assert.deepEqual(visual("r", "x"), { kind: "replace", char: "x", count: 1 });
  assert.equal(visual("I"), null, "inserting keys mean nothing in visual mode");
});

test("m sets a mark, and ' and ` jump to one as motions and operator targets", () => {
  assert.deepEqual(parse("m", "a").command, { kind: "set-mark", name: "a" });
  assert.equal(parse("m", "1").command, null, "digits are not marks");
  assert.deepEqual(parse("'", "A").command, { kind: "motion", motion: { mark: "A", exact: false }, count: null });
  assert.deepEqual(parse("`", "`").command, { kind: "motion", motion: { mark: "`", exact: true }, count: null });
  assert.deepEqual(parse("d", "'", "a").command, {
    kind: "operator", operator: "d", register: null, count: null,
    target: { kind: "motion", motion: { mark: "a", exact: false } },
  });
  assert.deepEqual(visual("`", "b"), { kind: "motion", motion: { mark: "b", exact: true }, count: null });
});

test("ctrl-o steps back through the jumplist and Tab forward, with counts", () => {
  assert.deepEqual(parse("<c-o>").command, { kind: "jump", count: -1 });
  assert.deepEqual(parse("3", "<tab>").command, { kind: "jump", count: 3 });
});

test("ctrl keys scroll with counts, in normal and visual mode but not after an operator", () => {
  assert.deepEqual(parse("<c-d>").command, { kind: "scroll", scroll: "half-down", count: null });
  assert.deepEqual(parse("5", "<c-e>").command, { kind: "scroll", scroll: "line-down", count: 5 });
  assert.deepEqual(visual("<c-b>"), { kind: "scroll", scroll: "page-up", count: null });
  assert.equal(parse("d", "<c-d>").command, null);
});

test("z repositions the caret's line, or a counted line", () => {
  const { pending } = parse("z");
  assert.equal(pendingText(pending), "z");
  assert.deepEqual(parse("z", "z").command, { kind: "reposition", at: "center", firstNonBlank: false, count: null });
  assert.deepEqual(parse("1", "2", "z", "<cr>").command, {
    kind: "reposition", at: "top", firstNonBlank: true, count: 12,
  });
  assert.deepEqual(parse("z", "-").command, { kind: "reposition", at: "bottom", firstNonBlank: true, count: null });
  assert.equal(parse("z", "q").command, null);
  assert.equal(parse("d", "z", "z").command, null, "only fold motions follow an operator");
});

test("z keys open, close, and create folds", () => {
  assert.deepEqual(parse("3", "z", "o").command, { kind: "fold", action: "open", count: 3 });
  assert.deepEqual(parse("z", "C").command, { kind: "fold", action: "close-all", count: null });
  assert.deepEqual(parse("z", "R").command, { kind: "fold", action: "open-every", count: null });
  assert.deepEqual(parse("z", "F").command, { kind: "fold", action: "create-lines", count: null });
  assert.deepEqual(visual("z", "f"), { kind: "fold", action: "create-selection", count: null });
  const { pending } = parse("z", "f");
  assert.equal(pending.operator, "zf", "zf waits for a target");
  assert.deepEqual(parse("z", "f", "j").command, {
    kind: "operator", operator: "zf", register: null, count: null, target: { kind: "motion", motion: "down" },
  });
});

test("fold motions are motions and operator targets", () => {
  assert.deepEqual(parse("2", "z", "j").command, { kind: "motion", motion: "fold-next", count: 2 });
  assert.deepEqual(parse("z", "k").command, { kind: "motion", motion: "fold-previous", count: null });
  assert.deepEqual(parse("[", "z").command, { kind: "motion", motion: "fold-start", count: null });
  assert.deepEqual(parse("]", "z").command, { kind: "motion", motion: "fold-end", count: null });
  assert.deepEqual(parse("d", "z", "j").command, {
    kind: "operator", operator: "d", register: null, count: null, target: { kind: "motion", motion: "fold-next" },
  });
  assert.equal(parse("[", "x").command, null);
});

test("H, M, and L are motions and operator targets", () => {
  assert.deepEqual(parse("3", "H").command, { kind: "motion", motion: "window-top", count: 3 });
  assert.deepEqual(parse("d", "L").command, {
    kind: "operator", operator: "d", register: null, count: null, target: { kind: "motion", motion: "window-bottom" },
  });
  assert.deepEqual(visual("M"), { kind: "motion", motion: "window-middle", count: null });
});

test(": opens the Ex line in normal and visual modes, not after an operator", () => {
  assert.deepEqual(parse(":").command, { kind: "ex" });
  assert.equal(parse("d", ":").command, null);
  assert.deepEqual(visual(":"), { kind: "ex" });
});

test("ctrl-w takes a window key, with or without ctrl", () => {
  assert.equal(pendingText(parse("<c-w>").pending), "^W");
  assert.deepEqual(parse("<c-w>", "h").command, { kind: "window", action: "focus-left", count: null });
  assert.deepEqual(parse("<c-w>", "<bs>").command, { kind: "window", action: "focus-left", count: null });
  assert.deepEqual(parse("<c-w>", "<c-j>").command, { kind: "window", action: "focus-down", count: null });
  assert.deepEqual(parse("<c-w>", "<c-w>").command, { kind: "window", action: "next", count: null });
  assert.deepEqual(visual("<c-w>", "v"), { kind: "window", action: "split-right", count: null });
  assert.deepEqual(parse("<c-w>", "L").command, { kind: "window", action: "move-right", count: null });
  assert.deepEqual(parse("<c-w>", "q").command, { kind: "window", action: "close", count: null });
  assert.deepEqual(parse("<c-w>", "=").command, { kind: "window", action: "equalize", count: null });
  assert.deepEqual(parse("<c-w>", ">").command, { kind: "window", action: "wider", count: null });
  assert.deepEqual(parse("<c-w>", "-").command, { kind: "window", action: "shorter", count: null });
  assert.equal(parse("<c-w>", "x").command, null, "an unsupported key cancels");
  assert.equal(parse("<c-w>", "<esc>").command, null);
  assert.equal(parse("d", "<c-w>").command, null, "ctrl-w is no operator target");
});

test("a count may precede or follow ctrl-w", () => {
  assert.deepEqual(parse("3", "<c-w>", "+").command, { kind: "window", action: "taller", count: 3 });
  assert.deepEqual(parse("<c-w>", "1", "0", ">").command, { kind: "window", action: "wider", count: 10 });
  assert.equal(pendingText(parse("<c-w>", "2").pending), "^W2");
});

test("search keys are motions, with a count and after an operator", () => {
  assert.deepEqual(parse("/").command, {
    kind: "motion", motion: { search: "prompt", forward: true }, count: null,
  });
  assert.deepEqual(parse("3", "?").command, {
    kind: "motion", motion: { search: "prompt", forward: false }, count: 3,
  });
  assert.deepEqual(parse("n").command, { kind: "motion", motion: { search: "repeat", reverse: false }, count: null });
  assert.deepEqual(parse("N").command, { kind: "motion", motion: { search: "repeat", reverse: true }, count: null });
  assert.deepEqual(parse("*").command, {
    kind: "motion", motion: { search: "word", forward: true, whole: true }, count: null,
  });
  assert.deepEqual(parse("g", "#").command, {
    kind: "motion", motion: { search: "word", forward: false, whole: false }, count: null,
  });
  assert.deepEqual(parse("d", "/").command, {
    kind: "operator", operator: "d", register: null, count: null,
    target: { kind: "motion", motion: { search: "prompt", forward: true } },
  });
  assert.deepEqual(visual("n"), { kind: "motion", motion: { search: "repeat", reverse: false }, count: null });
});

test("the search register can be named", () => {
  assert.deepEqual(parse('"', "/", "p").command, { kind: "put", before: false, register: "/", count: 1 });
});
