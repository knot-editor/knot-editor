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
  assert.deepEqual(parse("A").command, { kind: "insert", at: "A" });
  assert.deepEqual(parse("3", "u").command, { kind: "undo", count: 3 });
  assert.deepEqual(parse("<c-r>").command, { kind: "redo", count: 1 });
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
