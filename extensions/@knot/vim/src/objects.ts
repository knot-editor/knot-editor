// Vim's text objects over lines of text, without host calls.
import type { TextObject } from "./keys.js";
import { classAt, type Lines, type Position, scanBracket } from "./motions.js";
import { firstNonBlank, nextColumn, previousColumn } from "./text.js";

/** Text an operator applies to. A characterwise region runs from `start` up
 * to `end`, exclusive; a linewise one covers the lines from `start.line`
 * through `end.line`. */
export type Region = Readonly<{ start: Position; end: Position; linewise: boolean }>;

const PAIRS: Readonly<Partial<Record<TextObject, readonly [string, string]>>> = {
  paren: ["(", ")"], brace: ["{", "}"], bracket: ["[", "]"], angle: ["<", ">"],
};

const QUOTES: Readonly<Partial<Record<TextObject, string>>> = { dquote: '"', squote: "'", backtick: "`" };

/** The region of a text object at `at`, or null where there is none. */
export async function textObject(
  lines: Lines,
  at: Position,
  object: TextObject,
  around: boolean,
  count: number,
): Promise<Region | null> {
  if (object === "word" || object === "WORD") {
    return wordObject(await lines.text(at.line), at, count, object === "WORD", around);
  }
  if (object === "paragraph") return paragraphObject(lines, at.line, count, around);
  const pair = PAIRS[object];
  if (pair !== undefined) return bracketObject(lines, at, pair, count, around);
  return quoteObject(await lines.text(at.line), at, QUOTES[object]!, around);
}

/** The end of the run of one character class that starts at or spans `col`. */
function runEnd(text: string, col: number, big: boolean): number {
  const kind = classAt(text, col, big);
  let end = col;
  while (end < text.length && classAt(text, end, big) === kind) end = nextColumn(text, end);
  return end;
}

function runStart(text: string, col: number, big: boolean): number {
  const kind = classAt(text, col, big);
  let start = col;
  while (start > 0) {
    const before = previousColumn(text, start);
    if (classAt(text, before, big) !== kind) break;
    start = before;
  }
  return start;
}

/** `iw` and `aw`, and `iW` and `aW`, within the caret's line. `iw` takes
 * count runs of words or white space; `aw` takes count words with the white
 * space after each, or before the first when none follows the last, unless
 * that is the line's indentation. */
function wordObject(text: string, at: Position, count: number, big: boolean, around: boolean): Region | null {
  if (text.length === 0) return null;
  const line = at.line;
  let start = runStart(text, at.col, big);
  let end = runEnd(text, at.col, big);
  if (!around) {
    for (let n = 1; n < count && end < text.length; n++) end = runEnd(text, end, big);
  } else if (classAt(text, at.col, big) === 0) {
    for (let n = 0; n < count && end < text.length; n++) {
      end = runEnd(text, end, big);
      if (n < count - 1 && end < text.length) end = runEnd(text, end, big);
    }
  } else {
    let trailing = false;
    for (let n = 0; n < count; n++) {
      if (n > 0 && end < text.length) end = runEnd(text, end, big);
      trailing = end < text.length && classAt(text, end, big) === 0;
      if (trailing) end = runEnd(text, end, big);
    }
    if (!trailing && start > 0) {
      const white = runStart(text, previousColumn(text, start), big);
      if (white > 0) start = white;
    }
  }
  return { start: { line, col: start }, end: { line, col: end }, linewise: false };
}

/** `ip` and `ap`: count runs of non-blank or blank lines, where `ap` adds
 * the blank lines after each paragraph, or before it when none follow, or
 * starting on blank lines, the paragraph after them. */
async function paragraphObject(lines: Lines, line: number, count: number, around: boolean): Promise<Region> {
  const total = await lines.count();
  const blank = async (index: number) => /^[ \t]*$/.test(await lines.text(index));
  const runAfter = async (from: number) => {
    const kind = await blank(from);
    let end = from;
    while (end + 1 < total && (await blank(end + 1)) === kind) end++;
    return end;
  };
  const startsBlank = await blank(line);
  let start = line;
  while (start > 0 && (await blank(start - 1)) === startsBlank) start--;
  let end = await runAfter(line);
  const runs = around ? count * 2 : count;
  for (let n = 1; n < runs && end + 1 < total; n++) end = await runAfter(end + 1);
  if (around && !startsBlank && !(await blank(end)) && start > 0) {
    // No blank lines follow, so the ones before the paragraph go with it.
    start -= 1;
    while (start > 0 && (await blank(start - 1))) start--;
  }
  return { start: { line: start, col: 0 }, end: { line: end, col: 0 }, linewise: true };
}

/** `i(` and `a(` and the other pairs: the count-th pair around the caret,
 * matched by counting brackets in the text. `i(` leaves out an opener that
 * ends its line and the indentation before a closer on its own line, so an
 * operator on a block's lines takes them whole. */
async function bracketObject(
  lines: Lines,
  at: Position,
  [open, close]: readonly [string, string],
  count: number,
  around: boolean,
): Promise<Region | null> {
  const under = (await lines.text(at.line))[at.col];
  // A closer under the caret is skipped by the backward scan, which finds
  // its own opener.
  let opener = under === open ? at : await scanBracket(lines, at, close, open, false);
  for (let n = 1; n < count && opener !== null; n++) opener = await scanBracket(lines, opener, close, open, false);
  if (opener === null) return null;
  const closer = await scanBracket(lines, opener, open, close, true);
  if (closer === null) return null;
  if (around) {
    return { start: opener, end: { line: closer.line, col: closer.col + 1 }, linewise: false };
  }
  let start: Position = { line: opener.line, col: opener.col + 1 };
  if (start.col >= (await lines.text(opener.line)).length && opener.line < closer.line) {
    start = { line: opener.line + 1, col: 0 };
  }
  const closerText = await lines.text(closer.line);
  let end: Position = closer.col <= firstNonBlank(closerText) && closer.line > opener.line
    ? { line: closer.line, col: 0 }
    : closer;
  if (end.line < start.line || (end.line === start.line && end.col < start.col)) end = start;
  return { start, end, linewise: false };
}

/** The columns of the line's quote characters, skipping escaped ones. */
function quoteColumns(text: string, quote: string): number[] {
  const columns: number[] = [];
  for (let index = 0; index < text.length; index++) {
    if (text[index] === "\\") index++;
    else if (text[index] === quote) columns.push(index);
  }
  return columns;
}

/** `i"` and `a"` and the other quotes, within the caret's line. On a quote,
 * counting quotes from the line's start tells an opening one from a
 * closing one; elsewhere the quotes nearest the caret are used, or the next
 * quoted text after it. `a"` adds the white space after the closing quote,
 * or before the opening one when none follows. */
function quoteObject(text: string, at: Position, quote: string, around: boolean): Region | null {
  const quotes = quoteColumns(text, quote);
  let open: number | undefined;
  let close: number | undefined;
  const on = quotes.indexOf(at.col);
  if (on !== -1) {
    [open, close] = on % 2 === 0 ? [quotes[on], quotes[on + 1]] : [quotes[on - 1], quotes[on]];
  } else {
    const before = quotes.filter((column) => column < at.col).pop();
    open = before ?? quotes.find((column) => column > at.col);
    close = open === undefined ? undefined : quotes.find((column) => column > open!);
  }
  if (open === undefined || close === undefined) return null;
  const line = at.line;
  if (!around) return { start: { line, col: open + 1 }, end: { line, col: close }, linewise: false };
  let start = open;
  let end = close + 1;
  if (end < text.length && /[ \t]/.test(text[end]!)) {
    while (end < text.length && /[ \t]/.test(text[end]!)) end++;
  } else {
    while (start > 0 && /[ \t]/.test(text[start - 1]!)) start--;
  }
  return { start: { line, col: start }, end: { line, col: end }, linewise: false };
}
