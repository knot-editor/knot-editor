import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { buffers } from "knot";
import type { DiffHunk } from "knot";
import * as host from "knot/testing";
import { blocksOf, connectorLinks, decorationsOf, scrollLink, stagedFlags, targetLine } from "../src/hunks.js";

beforeEach(() => host.reset());

const hunk = (old: [number, number], now: [number, number]): DiffHunk => ({
  old: { start: old[0], end: old[1] },
  new: { start: now[0], end: now[1] },
});
// A line changed, a line removed after the second, and a line added.
const hunks = [hunk([1, 2], [1, 2]), hunk([4, 5], [4, 4]), hunk([7, 7], [6, 7])];

test("the strip colors each link by what its hunk does", () => {
  assert.deepEqual(
    connectorLinks(hunks).map((link) => link.color),
    ["diff.modified.background", "diff.removed.background", "diff.added.background"],
  );
  assert.deepEqual(connectorLinks(hunks)[1]!.first, { start: 4, end: 5 });
});

test("the scroll link maps the original's lines to the modified's", () => {
  assert.deepEqual(scrollLink(hunks), {
    first: "original",
    second: "modified",
    map: hunks.map((entry) => ({ first: entry.old, second: entry.new })),
  });
});

test("a side's bands cover the hunk's lines and its wedges sit at the boundary", async () => {
  host.openWindow();
  const old = await buffers.create({ text: "a\nb\nc\nd\ne\n" });
  const now = await buffers.create({ text: "a\nB\nc\n" });
  const changes = [hunk([1, 2], [1, 2]), hunk([3, 5], [3, 3])];
  assert.deepEqual(await decorationsOf(old, changes, "original"), [
    { range: { startByteOffset: 2, endByteOffset: 3 }, decoration: { color: "diff.modified.background", style: "line" } },
    { range: { startByteOffset: 6, endByteOffset: 9 }, decoration: { color: "diff.removed.background", style: "line" } },
  ]);
  assert.deepEqual(await decorationsOf(now, changes, "modified"), [
    { range: { startByteOffset: 2, endByteOffset: 3 }, decoration: { color: "diff.modified.background", style: "line" } },
    { range: { startByteOffset: 6, endByteOffset: 6 }, gutter: { color: "diff.removed", style: "deletion" } },
  ]);
  assert.deepEqual(await decorationsOf(now, [], "modified"), []);
});

test("an added line leaves a wedge on the original side", async () => {
  host.openWindow();
  const old = await buffers.create({ text: "a\nc\n" });
  assert.deepEqual(await decorationsOf(old, [hunk([1, 1], [1, 2])], "original"), [
    { range: { startByteOffset: 2, endByteOffset: 2 }, gutter: { color: "diff.added", style: "deletion" } },
  ]);
});

test("next and previous follow the side's own lines and wrap", () => {
  assert.equal(targetLine(hunks, "modified", 0, "next"), 1);
  assert.equal(targetLine(hunks, "modified", 1, "next"), 4);
  assert.equal(targetLine(hunks, "modified", 6, "next"), 1);
  assert.equal(targetLine(hunks, "modified", 4, "previous"), 1);
  assert.equal(targetLine(hunks, "modified", 1, "previous"), 6);
  // The original's hunks start at other lines.
  assert.equal(targetLine(hunks, "original", 4, "next"), 7);
  assert.equal(targetLine([], "modified", 0, "next"), null);
});

test("the inline rows put each hunk's original lines above the modified line, skipping additions", async () => {
  const original = await buffers.create({ text: "a\nb\nc\nd\ne\nf\ng\nh\n" });
  assert.deepEqual(
    blocksOf(hunks, original).map(({ line, lines, color }) => ({ line, lines, color })),
    [
      { line: 1, lines: { start: 1, end: 2 }, color: "diff.removed.background" },
      { line: 4, lines: { start: 4, end: 5 }, color: "diff.removed.background" },
    ],
  );
});

test("hunks the staged text already holds are staged, and a partly staged one is not", () => {
  const all = [hunk([1, 2], [1, 2]), hunk([4, 5], [4, 5]), hunk([7, 7], [7, 9])];
  // The staged text against the modified one differs only in the second hunk.
  assert.deepEqual(stagedFlags(all, [hunk([4, 5], [4, 5])]), [true, false, true]);
  assert.deepEqual(stagedFlags(all, []), [true, true, true]);
});

test("a removal is staged unless the staged text differs at its boundary", () => {
  const removal = hunk([3, 4], [3, 3]);
  assert.deepEqual(stagedFlags([removal], [hunk([3, 3], [3, 4])]), [false]);
  assert.deepEqual(stagedFlags([removal], [hunk([5, 6], [5, 6])]), [true]);
});

test("staged hunks use the dimmer colors in the strip, the blocks, and the bands", async () => {
  const hunks = [hunk([1, 2], [1, 2]), hunk([4, 5], [4, 4])];
  assert.deepEqual(
    connectorLinks(hunks, [true, false]).map((link) => link.color),
    ["diff.modified.background.staged", "diff.removed.background"],
  );
  const original = await buffers.create({ text: "a\nb\nc\nd\ne\n" });
  assert.deepEqual(
    blocksOf(hunks, original, [false, true]).map((block) => block.color),
    ["diff.removed.background", "diff.removed.background.staged"],
  );
  const modified = await buffers.create({ text: "a\nB\nc\nd\n" });
  const items = await decorationsOf(modified, hunks, "modified", [true, true]);
  assert.deepEqual(
    items.map((item) => item.decoration?.color ?? item.gutter?.color),
    ["diff.modified.background.staged", "diff.removed.staged"],
  );
});
