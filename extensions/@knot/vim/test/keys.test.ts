import assert from "node:assert/strict";
import { test } from "node:test";
import { type Command, EMPTY, feed, type Pending, pendingText } from "../src/keys.js";

/** Feeds keys and returns the command they complete, plus what stays pending. */
function parse(...keys: string[]): { command: Command | null | undefined; pending: Pending } {
  let pending = EMPTY;
  for (const key of keys) {
    const fed = feed(pending, key);
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
