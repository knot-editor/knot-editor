import assert from "node:assert/strict";
import { test } from "node:test";
import type { ByteRange, DiffHunk } from "knot";
import { baselineText, hunkAt, revertEdit } from "../src/hunk.js";

const hunk = (old: [number, number], now: [number, number]): DiffHunk => ({
  old: { start: old[0], end: old[1] },
  new: { start: now[0], end: now[1] },
});

/** A buffer over `text`, with lines split at `\n` as `Buffer` does. */
function lines(text: string) {
  const ranges: ByteRange[] = [];
  let start = 0;
  for (const line of text.split("\n")) {
    ranges.push({ startByteOffset: start, endByteOffset: start + line.length });
    start += line.length + 1;
  }
  return { lineCount: async () => ranges.length, lineRange: async (line: number) => ranges[line]! };
}

/** Applies the revert of `hunk` to `current` and returns the resulting text. */
async function reverted(baseline: string, current: string, h: DiffHunk) {
  const { range, text } = await revertEdit(baseline, h, lines(current));
  return current.slice(0, range.startByteOffset) + text + current.slice(range.endByteOffset);
}

test("a line belongs to the hunk whose new lines contain it", () => {
  const hunks = [hunk([1, 2], [1, 3]), hunk([5, 5], [6, 8])];
  assert.equal(hunkAt(hunks, 2, 9), hunks[0]);
  assert.equal(hunkAt(hunks, 7, 9), hunks[1]);
  assert.equal(hunkAt(hunks, 3, 9), null);
});

test("a removal belongs to the line that followed it, clamped to the last line", () => {
  const removal = hunk([2, 4], [2, 2]);
  assert.equal(hunkAt([removal], 2, 5), removal);
  assert.equal(hunkAt([removal], 1, 5), null);
  assert.equal(hunkAt([hunk([2, 4], [3, 3])], 2, 2) !== null, true);
});

test("the baseline text is the replaced lines with their terminators", () => {
  assert.equal(baselineText("a\nb\nc\n", hunk([1, 3], [1, 2])), "b\nc\n");
  assert.equal(baselineText("a\nb\nc", hunk([2, 3], [2, 3])), "c");
  assert.equal(baselineText("a\nb\n", hunk([2, 2], [2, 3])), "");
});

test("reverting a modification restores the baseline", async () => {
  assert.equal(await reverted("a\nb\nc\n", "a\nX\nY\nc\n", hunk([1, 2], [1, 3])), "a\nb\nc\n");
});

test("reverting an insertion deletes its lines", async () => {
  assert.equal(await reverted("a\nc\n", "a\nb\nc\n", hunk([1, 1], [1, 2])), "a\nc\n");
  assert.equal(await reverted("a\n", "a\nb\n", hunk([1, 1], [1, 2])), "a\n");
});

test("reverting a removal puts the lines back", async () => {
  assert.equal(await reverted("a\nb\nc\n", "a\nc\n", hunk([1, 2], [1, 1])), "a\nb\nc\n");
  assert.equal(await reverted("a\nb\nc\n", "b\nc\n", hunk([0, 1], [0, 0])), "a\nb\nc\n");
  assert.equal(await reverted("a\nb\nc\n", "a\n", hunk([1, 3], [1, 1])), "a\nb\nc\n");
});

test("reverting a removal of the last line", async () => {
  assert.equal(await reverted("a\nb\n", "a\n", hunk([1, 2], [1, 1])), "a\nb\n");
});

test("a missing final newline on the current side is restored", async () => {
  assert.equal(await reverted("a\nb\n", "a\nb", hunk([1, 2], [1, 2])), "a\nb\n");
  assert.equal(await reverted("a\nb\n", "a\nX", hunk([1, 2], [1, 2])), "a\nb\n");
});

test("a missing final newline on the baseline side is restored", async () => {
  assert.equal(await reverted("a\nb", "a\nb\n", hunk([1, 2], [1, 2])), "a\nb");
  assert.equal(await reverted("a\nb", "a\nX\nY\n", hunk([1, 2], [1, 3])), "a\nb");
});

test("reverting an insertion at the end of a text without a final newline", async () => {
  assert.equal(await reverted("a\n", "a\nb", hunk([1, 1], [1, 2])), "a\n");
});
