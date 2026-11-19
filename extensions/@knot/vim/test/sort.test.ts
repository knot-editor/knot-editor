import assert from "node:assert/strict";
import { test } from "node:test";
import { sortLines } from "../src/sort.js";

const none = { reverse: false, ignoreCase: false, numeric: false, unique: false };

test("lines sort by text", () => {
  assert.deepEqual(sortLines(["b", "B", "a", ""], none), ["", "B", "a", "b"]);
});

test("reverse flips the order and keeps equal lines in theirs", () => {
  assert.deepEqual(sortLines(["b", "a", "b"], { ...none, reverse: true }), ["b", "b", "a"]);
  assert.deepEqual(
    sortLines(["x1", "y1", "z0"], { ...none, reverse: true, numeric: true }),
    ["x1", "y1", "z0"],
  );
});

test("ignoring case keeps equal lines in their order", () => {
  assert.deepEqual(sortLines(["b", "A", "a", "B"], { ...none, ignoreCase: true }), ["A", "a", "b", "B"]);
});

test("numeric sorts by the first number, lines without one first in order", () => {
  const sorted = sortLines(["a10", "z", "a2 9", "b-3", "y"], { ...none, numeric: true });
  assert.deepEqual(sorted, ["z", "y", "b-3", "a2 9", "a10"]);
});

test("unique keeps the first of equal lines, ignoring case with i", () => {
  assert.deepEqual(sortLines(["b", "a", "b"], { ...none, unique: true }), ["a", "b"]);
  assert.deepEqual(sortLines(["b", "A", "a"], { ...none, unique: true, ignoreCase: true }), ["A", "b"]);
});
