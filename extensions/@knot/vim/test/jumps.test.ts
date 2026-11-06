import assert from "node:assert/strict";
import { test } from "node:test";
import { JumpList, LIMIT } from "../src/jumps.js";

/** An entry that knows its line, or null once its text is gone. */
type Entry = { line: number | null };
const at = (line: number): Entry => ({ line });
const lineOf = (entry: Entry) => entry.line;
const lines = (list: JumpList<Entry>) => list.entries.map(lineOf);

function listOf(...entries: number[]): JumpList<Entry> {
  const list = new JumpList<Entry>();
  for (const line of entries) list.push(at(line), line, lineOf);
  return list;
}

test("a jump appends the position it left", () => {
  const list = listOf(1, 5, 9);
  assert.deepEqual(lines(list), [1, 5, 9]);
  assert.equal(list.index, 3);
});

test("an earlier entry on the same line is dropped", () => {
  const list = listOf(1, 5, 9);
  const removed = list.push(at(5), 5, lineOf);
  assert.deepEqual(lines(list), [1, 9, 5]);
  assert.deepEqual(removed.map(lineOf), [5]);
});

test("the oldest entries go past the limit", () => {
  const list = listOf(...Array.from({ length: LIMIT + 2 }, (_, line) => line));
  assert.equal(list.entries.length, LIMIT);
  assert.deepEqual(lines(list).slice(0, 2), [2, 3]);
});

test("entries whose text is gone are dropped", () => {
  const list = listOf(1, 5, 9);
  list.entries[1]!.line = null;
  list.push(at(12), 12, lineOf);
  assert.deepEqual(lines(list), [1, 9, 12]);
});

test("stepping back from the end records the current position", () => {
  const list = listOf(1, 5);
  const current = at(9);
  const { target } = list.step(-1, current, 9, lineOf);
  assert.equal(target?.line, 5);
  assert.deepEqual(lines(list), [1, 5, 9]);
  // ctrl-i then comes back to where the steps began.
  assert.equal(list.step(1, at(5), 5, lineOf).target, current);
});

test("a count steps several entries", () => {
  const list = listOf(1, 5, 9);
  assert.equal(list.step(-3, at(12), 12, lineOf).target?.line, 1);
  assert.equal(list.step(2, at(1), 1, lineOf).target?.line, 9);
});

test("stepping past either end fails and keeps the list", () => {
  const list = listOf(1, 5);
  const current = at(9);
  const back = list.step(-3, current, 9, lineOf);
  assert.equal(back.target, null);
  assert.ok(back.removed.includes(current));
  assert.deepEqual(lines(list), [1, 5]);
  assert.equal(list.step(1, at(9), 9, lineOf).target, null);
});

test("stepping back from a line already listed moves its entry to the end", () => {
  const list = listOf(1, 5);
  const { target } = list.step(-1, at(5), 5, lineOf);
  assert.equal(target?.line, 1);
  assert.deepEqual(lines(list), [1, 5]);
});

test("a jump after stepping back appends at the end", () => {
  const list = listOf(1, 5, 9);
  list.step(-2, at(12), 12, lineOf);
  list.push(at(5), 5, lineOf);
  assert.deepEqual(lines(list), [1, 9, 12, 5]);
  assert.equal(list.index, 4);
});
