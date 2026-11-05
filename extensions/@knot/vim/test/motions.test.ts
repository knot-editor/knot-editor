import assert from "node:assert/strict";
import { test } from "node:test";
import {
  left, lineStart, linesOf, type Position, right, wordBackward, wordEnd, wordEndBackward, wordForward,
} from "../src/motions.js";
import { columnAtByte, columnAtScreen, firstNonBlank, lastColumn, screenColumn, utf8Length } from "../src/text.js";

/** Applies `motion` repeatedly from the start and lists where it lands. */
async function stops(
  text: string,
  motion: (lines: ReturnType<typeof linesOf>, from: Position) => Promise<Position>,
  from: Position = { line: 0, col: 0 },
  limit = 20,
): Promise<string[]> {
  const lines = linesOf(text);
  const visited: string[] = [];
  let position = from;
  for (let n = 0; n < limit; n++) {
    const next = await motion(lines, position);
    if (next.line === position.line && next.col === position.col) break;
    position = next;
    visited.push(`${next.line}:${next.col}`);
  }
  return visited;
}

test("w stops at each word and punctuation run, and on empty lines", async () => {
  const text = "foo.bar  baz\n\n  qux(1)";
  assert.deepEqual(await stops(text, (lines, from) => wordForward(lines, from, 1, false)), [
    "0:3", "0:4", "0:9", "1:0", "2:2", "2:5", "2:6", "2:7", "2:8",
  ]);
});

test("W stops only after white space", async () => {
  const text = "foo.bar  baz\n\n  qux(1)";
  assert.deepEqual(await stops(text, (lines, from) => wordForward(lines, from, 1, true)), [
    "0:9", "1:0", "2:2", "2:8",
  ]);
});

test("a count repeats a word motion", async () => {
  const lines = linesOf("one two three four");
  assert.deepEqual(await wordForward(lines, { line: 0, col: 0 }, 3, false), { line: 0, col: 14 });
  assert.deepEqual(await wordBackward(lines, { line: 0, col: 14 }, 2, false), { line: 0, col: 4 });
});

test("b stops at word starts backwards, and on empty lines", async () => {
  const text = "foo.bar  baz\n\n  qux(1)";
  assert.deepEqual(
    await stops(text, (lines, from) => wordBackward(lines, from, 1, false), { line: 2, col: 7 }),
    ["2:6", "2:5", "2:2", "1:0", "0:9", "0:4", "0:3", "0:0"],
  );
});

test("e stops at word ends and skips empty lines", async () => {
  const text = "foo.bar  baz\n\n  qux(1)";
  // Past the last word it ends on the line's end, which normal mode clamps.
  assert.deepEqual(await stops(text, (lines, from) => wordEnd(lines, from, 1, false)), [
    "0:2", "0:3", "0:6", "0:11", "2:4", "2:5", "2:6", "2:7", "2:8",
  ]);
});

test("ge stops at previous word ends", async () => {
  const text = "foo bar\n\nbaz";
  assert.deepEqual(
    await stops(text, (lines, from) => wordEndBackward(lines, from, 1, false), { line: 2, col: 2 }),
    ["1:0", "0:6", "0:2", "0:0"],
  );
});

test("words treat letters with marks, digits, and underscores as one class", async () => {
  const lines = linesOf("é_9x, 😀😀 ok");
  assert.deepEqual(await wordForward(lines, { line: 0, col: 0 }, 1, false), { line: 0, col: 4 });
  assert.deepEqual(await wordForward(lines, { line: 0, col: 4 }, 1, false), { line: 0, col: 6 });
  assert.deepEqual(await wordForward(lines, { line: 0, col: 6 }, 1, false), { line: 0, col: 11 });
});

test("h and l stay within the line unless wrapping, and l stops on the last character", async () => {
  const lines = linesOf("ab😀\ncd");
  assert.deepEqual(await right(lines, { line: 0, col: 0 }, 5, false), { line: 0, col: 2 });
  assert.deepEqual(await right(lines, { line: 0, col: 2 }, 1, true), { line: 1, col: 0 });
  assert.deepEqual(await left(lines, { line: 1, col: 0 }, 1, false), { line: 1, col: 0 });
  assert.deepEqual(await left(lines, { line: 1, col: 0 }, 1, true), { line: 0, col: 2 });
  assert.deepEqual(await left(lines, { line: 0, col: 2 }, 1, false), { line: 0, col: 1 });
});

test("line starts skip indentation and clamp to the document", async () => {
  const lines = linesOf("a\n\t  b\n   ");
  assert.deepEqual(await lineStart(lines, 1), { line: 1, col: 3 });
  assert.deepEqual(await lineStart(lines, 9), { line: 2, col: 2 });
  assert.deepEqual(await lineStart(lines, -3), { line: 0, col: 0 });
});

test("screen columns expand tabs to the next stop", () => {
  assert.equal(screenColumn("\tab", 1, 4), 4);
  assert.equal(screenColumn("a\tb", 2, 4), 4);
  assert.equal(columnAtScreen("\tab", 5, 4), 2);
  assert.equal(columnAtScreen("\tab", 2, 4), 0, "inside a tab lands on the tab");
  assert.equal(columnAtScreen("ab", 10, 4), 1, "past the end lands on the last character");
});

test("columns convert to UTF-8 bytes and back on surrogate pairs", () => {
  assert.equal(utf8Length("é😀a"), 7);
  assert.equal(columnAtByte("é😀a", 6), 3);
  assert.equal(lastColumn("a😀"), 1);
  assert.equal(firstNonBlank("  \t"), 2);
});
