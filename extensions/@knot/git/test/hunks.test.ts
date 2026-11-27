import assert from "node:assert/strict";
import { test } from "node:test";
import type { DiffHunk } from "knot";
import { hunkAt, stagedText, unstagedText } from "../src/hunks.js";

const hunk = (old: [number, number], now: [number, number]): DiffHunk => ({
  old: { start: old[0], end: old[1] },
  new: { start: now[0], end: now[1] },
});

const HEAD = "a\nb\nc\nd\n";

test("staging a hunk puts the buffer's lines in the index and leaves the others", () => {
  const hunks = [hunk([1, 2], [1, 2]), hunk([3, 4], [3, 4])];
  assert.equal(stagedText(HEAD, "a\nB\nc\nD\n", hunks, hunks[0]!), "a\nB\nc\nd\n");
  assert.equal(stagedText(HEAD, "a\nB\nc\nD\n", hunks, hunks[1]!), "a\nb\nc\nD\n");
});

test("staging a partly staged hunk completes it", () => {
  assert.equal(stagedText("a\nB\nc\nd\n", "a\nBB\nc\nd\n", [hunk([1, 2], [1, 2])], hunk([1, 2], [1, 2])), "a\nBB\nc\nd\n");
});

test("staging a removal removes the lines from the index", () => {
  const removal = hunk([2, 3], [2, 2]);
  assert.equal(stagedText(HEAD, "a\nb\nd\n", [removal], removal), "a\nb\nd\n");
});

test("staging a hunk that adds lines after a last line without a newline", () => {
  const added = hunk([1, 2], [1, 3]);
  assert.equal(stagedText("a\nb", "a\nb\nc", [added], added), "a\nb\nc");
});

test("staging a hunk whose lines the index holds elsewhere changes nothing", () => {
  assert.equal(stagedText(HEAD, "a\nb\nc\nd\ne\n", [hunk([4, 4], [4, 5])], hunk([1, 2], [1, 2])), HEAD);
});

test("unstaging a hunk puts HEAD's lines back in the index and leaves the others", () => {
  const staged = [hunk([1, 2], [1, 2]), hunk([3, 4], [3, 4])];
  assert.equal(unstagedText("a\nB\nc\nD\n", HEAD, staged, hunk([1, 2], [1, 2])), "a\nb\nc\nD\n");
});

test("unstaging an insertion removes its lines from the index", () => {
  const added = hunk([1, 1], [1, 2]);
  assert.equal(unstagedText("a\nx\nb\n", "a\nb\n", [added], added), "a\nb\n");
});

test("unstaging a removal restores its lines in the index", () => {
  const removed = hunk([2, 3], [2, 2]);
  assert.equal(unstagedText("a\nb\nd\n", HEAD, [removed], removed), "a\nb\nc\nd\n");
});

test("the hunk at a line is the one containing it, else the removal there", () => {
  const hunks = [hunk([1, 2], [1, 2]), hunk([3, 4], [3, 3])];
  assert.equal(hunkAt(hunks, 1, 5), hunks[0]);
  assert.equal(hunkAt(hunks, 3, 5), hunks[1]);
  assert.equal(hunkAt(hunks, 2, 5), null);
});
