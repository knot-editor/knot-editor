import assert from "node:assert/strict";
import { test } from "node:test";
import {
  bracketFrom, findInLine, linesOf, matchBracketText, paragraph, type Position, sentence,
  wordBackward, wordEnd, wordEndBackward, wordForward,
} from "../src/motions.js";
import { columnAtByte, firstNonBlank, lastColumn, utf8Length } from "../src/text.js";

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

test("columns convert to UTF-8 bytes and back on surrogate pairs", () => {
  assert.equal(utf8Length("é😀a"), 7);
  assert.equal(columnAtByte("é😀a", 6), 3);
  assert.equal(lastColumn("a😀"), 1);
  assert.equal(firstNonBlank("  \t"), 2);
});

test("f and t find on the line, and repeating t moves past an adjacent match", () => {
  assert.equal(findInLine("a.b.c", 0, "f", ".", 1), 1);
  assert.equal(findInLine("a.b.c", 0, "f", ".", 2), 3);
  assert.equal(findInLine("a.b.c", 0, "f", ".", 3), null);
  assert.equal(findInLine("a.b.c", 4, "F", ".", 1), 3);
  assert.equal(findInLine("a.b.c", 0, "t", ".", 1), 0);
  assert.equal(findInLine("a.b.c", 0, "t", ".", 1, true), 2);
  assert.equal(findInLine("a.b.c", 4, "T", ".", 1), 4);
  assert.equal(findInLine("a.b.c", 4, "T", ".", 1, true), 2);
});

test("{ and } stop at empty lines, not blank ones", async () => {
  const lines = linesOf("a\n \nb\n\nc");
  assert.deepEqual(await paragraph(lines, { line: 0, col: 0 }, 1, true), { position: { line: 3, col: 0 }, inclusive: false });
  assert.deepEqual(await paragraph(lines, { line: 3, col: 0 }, 1, true), { position: { line: 4, col: 0 }, inclusive: true });
  assert.equal(await paragraph(lines, { line: 0, col: 0 }, 3, true), null);
  assert.deepEqual(await paragraph(lines, { line: 4, col: 0 }, 1, false), { position: { line: 3, col: 0 }, inclusive: false });
  assert.deepEqual(await paragraph(lines, { line: 2, col: 0 }, 1, false), { position: { line: 0, col: 0 }, inclusive: false });
});

test("brackets are matched by counting when there is no syntax", async () => {
  const lines = linesOf("f(a, (b),\n  c)");
  assert.equal(bracketFrom("x = f(a)", 0), 5);
  assert.deepEqual(await matchBracketText(lines, { line: 0, col: 1 }), { line: 1, col: 3 });
  assert.deepEqual(await matchBracketText(lines, { line: 1, col: 3 }), { line: 0, col: 1 });
  assert.equal(await matchBracketText(linesOf("(("), { line: 0, col: 0 }), null);
});

test("w under an operator stops at the end of the line's last word", async () => {
  const lines = linesOf("foo bar\nbaz");
  assert.deepEqual(await wordForward(lines, { line: 0, col: 4 }, 1, false), { line: 1, col: 0 });
  assert.deepEqual(await wordForward(lines, { line: 0, col: 4 }, 1, false, true), { line: 0, col: 7 });
  assert.deepEqual(await wordForward(lines, { line: 0, col: 0 }, 2, false, true), { line: 0, col: 7 });
});

test("e with stop keeps a word end the caret is on", async () => {
  const lines = linesOf("foo bar");
  assert.deepEqual(await wordEnd(lines, { line: 0, col: 2 }, 1, false), { line: 0, col: 6 });
  assert.deepEqual(await wordEnd(lines, { line: 0, col: 2 }, 1, false, true), { line: 0, col: 2 });
  assert.deepEqual(await wordEnd(lines, { line: 0, col: 0 }, 1, false, true), { line: 0, col: 2 });
});

/** The caret positions `)` visits from the start, as "line:col". */
async function sentenceStops(text: string, forward: boolean, from: Position): Promise<string[]> {
  const lines = linesOf(text);
  const visited: string[] = [];
  let at = from;
  for (let n = 0; n < 20; n++) {
    const found = await sentence(lines, at, 1, forward);
    if (found === null) break;
    at = found.position;
    visited.push(`${at.line}:${at.col}`);
  }
  return visited;
}

test(") and ( stop at sentence starts and empty lines", async () => {
  const text = "One. Two!  Three?\n  Four.\n\n\nFive e.g. six.";
  assert.deepEqual(await sentenceStops(text, true, { line: 0, col: 0 }), [
    "0:5", "0:11", "1:2", "2:0", "4:0", "4:10", "4:13",
  ]);
  assert.deepEqual(await sentenceStops(text, false, { line: 4, col: 13 }), [
    "4:10", "4:0", "2:0", "1:2", "0:11", "0:5", "0:0",
  ]);
});

test("closing quotes and brackets stay with their sentence, and a period needs white space after", async () => {
  assert.deepEqual(await sentenceStops('Say "hi." Then (go.) Next a.b c.', true, { line: 0, col: 0 }), ["0:10", "0:21", "0:31"]);
});

test(") with a count, and at the document's end", async () => {
  const lines = linesOf("A. B. C.");
  assert.deepEqual(await sentence(lines, { line: 0, col: 0 }, 2, true), { position: { line: 0, col: 6 }, inclusive: false });
  assert.deepEqual(await sentence(lines, { line: 0, col: 6 }, 1, true), { position: { line: 0, col: 7 }, inclusive: true });
  assert.equal(await sentence(lines, { line: 0, col: 7 }, 1, true), null);
  assert.equal(await sentence(lines, { line: 0, col: 0 }, 1, false), null);
});
