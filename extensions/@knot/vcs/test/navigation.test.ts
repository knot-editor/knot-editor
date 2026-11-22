import assert from "node:assert/strict";
import { test } from "node:test";
import type { DiffHunk } from "knot";
import { targetLine } from "../src/navigation.js";

const hunk = (start: number, end: number): DiffHunk => ({ old: { start, end: start }, new: { start, end } });
// A modification of line 1, a removal before line 4, an insertion of line 6.
const hunks = [hunk(1, 2), hunk(4, 4), hunk(6, 7)];

test("next goes to the first hunk after the caret line", () => {
  assert.equal(targetLine(hunks, 0, "next"), 1);
  assert.equal(targetLine(hunks, 1, "next"), 4);
  assert.equal(targetLine(hunks, 4, "next"), 6);
});

test("next wraps from the last hunk to the first", () => {
  assert.equal(targetLine(hunks, 6, "next"), 1);
  assert.equal(targetLine(hunks, 9, "next"), 1);
});

test("previous goes to the last hunk before the caret line", () => {
  assert.equal(targetLine(hunks, 9, "previous"), 6);
  assert.equal(targetLine(hunks, 6, "previous"), 4);
  assert.equal(targetLine(hunks, 5, "previous"), 4);
});

test("previous wraps from the first hunk to the last", () => {
  assert.equal(targetLine(hunks, 1, "previous"), 6);
  assert.equal(targetLine(hunks, 0, "previous"), 6);
});

test("a document without changes has no target", () => {
  assert.equal(targetLine([], 3, "next"), null);
  assert.equal(targetLine([], 3, "previous"), null);
});
