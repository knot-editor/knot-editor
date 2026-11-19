// Vim's motions over lines of text, without host calls. Word motions follow
// Vim's own algorithm: a position may sit on the end of a line, just past its
// last character, which counts as white space, and an empty line is a word
// of its own.
import { firstNonBlank, lastColumn, nextColumn, previousColumn } from "./text.js";

/** Lines of a document, fetched on demand. */
export interface Lines {
  count(): Promise<number>;
  text(line: number): Promise<string>;
}

/** A line and a UTF-16 column within it. */
export type Position = Readonly<{ line: number; col: number }>;

/** In-memory lines, for tests and small texts. */
export function linesOf(text: string): Lines {
  const lines = text.split(/\r?\n/);
  return { count: async () => lines.length, text: async (line) => lines[line] ?? "" };
}

/** Vim's character class at `col`: 0 for white space and the line's end, 1
 * for punctuation, 2 for word characters, and 3 for emoji. With `big`,
 * every non-blank character is class 1. */
export function classAt(text: string, col: number, big: boolean): number {
  if (col >= text.length) return 0;
  const character = String.fromCodePoint(text.codePointAt(col)!);
  if (character === " " || character === "\t") return 0;
  if (big) return 1;
  if (/[\p{L}\p{N}_\p{M}]/u.test(character)) return 2;
  if (/\p{Extended_Pictographic}/u.test(character)) return 3;
  return 1;
}

/** A position that walks the document character by character. */
class Cursor {
  constructor(
    private readonly lines: Lines,
    public line: number,
    public col: number,
    public text: string,
  ) {}

  static async at(lines: Lines, { line, col }: Position): Promise<Cursor> {
    return new Cursor(lines, line, col, await lines.text(line));
  }

  get position(): Position {
    return { line: this.line, col: this.col };
  }

  /** Whether the cursor sits on an empty line. */
  get onEmptyLine(): boolean {
    return this.text.length === 0;
  }

  /** Step forward: 0 within the line, 2 onto its end, 1 onto the next line,
   * -1 when already at the end of the last line. */
  async next(): Promise<number> {
    if (this.col < this.text.length) {
      this.col = nextColumn(this.text, this.col);
      return this.col < this.text.length ? 0 : 2;
    }
    if (this.line + 1 >= (await this.lines.count())) return -1;
    this.line += 1;
    this.col = 0;
    this.text = await this.lines.text(this.line);
    return 1;
  }

  /** Step back: 0 within the line, 1 onto the previous line's end, -1 at the
   * start of the document. */
  async previous(): Promise<number> {
    if (this.col > 0) {
      this.col = previousColumn(this.text, this.col);
      return 0;
    }
    if (this.line === 0) return -1;
    this.line -= 1;
    this.text = await this.lines.text(this.line);
    this.col = this.text.length;
    return 1;
  }

  /** Vim's character class under the cursor; see `classAt`. */
  kind(big: boolean): number {
    return classAt(this.text, this.col, big);
  }

  /** Move while the class stays `kind`; true when the document ended. */
  async skip(kind: number, big: boolean, forward: boolean): Promise<boolean> {
    while (this.kind(big) === kind) {
      if ((forward ? await this.next() : await this.previous()) === -1) return true;
    }
    return false;
  }
}

/** `w` and `W`: the start of the count-th next word. With `eol`, as under
 * an operator, the last word stops at its line's end instead of moving on
 * to the next line, so `dw` on a line's last word keeps the line break. */
export async function wordForward(lines: Lines, from: Position, count: number, big: boolean, eol = false) {
  const cursor = await Cursor.at(lines, from);
  for (let n = 0; n < count; n++) {
    const last = n === count - 1;
    const start = cursor.kind(big);
    let step = await cursor.next();
    if (step === -1 || (step >= 1 && eol && last)) break;
    if (start !== 0) {
      let stopped = false;
      while (cursor.kind(big) === start) {
        step = await cursor.next();
        if (step === -1 || (step >= 1 && eol && last)) {
          stopped = true;
          break;
        }
      }
      if (stopped) break;
    }
    let ended = false;
    while (cursor.kind(big) === 0) {
      if (cursor.col === 0 && cursor.onEmptyLine) break;
      step = await cursor.next();
      if (step === -1 || (step >= 1 && eol && last)) {
        ended = true;
        break;
      }
    }
    if (ended) break;
  }
  return cursor.position;
}

/** `b` and `B`: the start of the count-th previous word. */
export async function wordBackward(lines: Lines, from: Position, count: number, big: boolean) {
  const cursor = await Cursor.at(lines, from);
  outer: for (let n = 0; n < count; n++) {
    if ((await cursor.previous()) === -1) break;
    while (cursor.kind(big) === 0) {
      if (cursor.col === 0 && cursor.onEmptyLine) continue outer;
      if ((await cursor.previous()) === -1) break outer;
    }
    if (await cursor.skip(cursor.kind(big), big, false)) break;
    await cursor.next();
  }
  return cursor.position;
}

/** `e` and `E`: the end of the count-th next word. With `stop`, as `cw`
 * uses it, a caret already on a word's end counts that word as the first. */
export async function wordEnd(lines: Lines, from: Position, count: number, big: boolean, stop = false) {
  const cursor = await Cursor.at(lines, from);
  for (let n = 0; n < count; n++) {
    const start = cursor.kind(big);
    if ((await cursor.next()) === -1) break;
    if (start !== 0 && cursor.kind(big) === start) {
      if (await cursor.skip(start, big, true)) break;
    } else if (stop && n === 0 && start !== 0) {
      // Already on the word's end.
    } else {
      let ended = false;
      while (cursor.kind(big) === 0) {
        if ((await cursor.next()) === -1) {
          ended = true;
          break;
        }
      }
      if (ended || (await cursor.skip(cursor.kind(big), big, true))) break;
    }
    await cursor.previous();
  }
  return cursor.position;
}

/** `ge` and `gE`: the end of the count-th previous word. */
export async function wordEndBackward(lines: Lines, from: Position, count: number, big: boolean) {
  const cursor = await Cursor.at(lines, from);
  outer: for (let n = 0; n < count; n++) {
    const start = cursor.kind(big);
    if ((await cursor.previous()) === -1) break;
    if (start !== 0) {
      while (cursor.kind(big) === start) {
        if ((await cursor.previous()) === -1) break outer;
      }
    }
    while (cursor.kind(big) === 0) {
      if (cursor.col === 0 && cursor.onEmptyLine) break;
      if ((await cursor.previous()) === -1) break outer;
    }
  }
  return cursor.position;
}

/** `h`, or with `wrap` Backspace: count characters left. */
export async function left(lines: Lines, from: Position, count: number, wrap: boolean) {
  let { line, col } = from;
  let text = await lines.text(line);
  for (let n = 0; n < count; n++) {
    if (col > 0) col = previousColumn(text, col);
    else if (wrap && line > 0) {
      line -= 1;
      text = await lines.text(line);
      col = lastColumn(text);
    } else break;
  }
  return { line, col };
}

/** `l`, or with `wrap` Space: count characters right, never onto a line's end. */
export async function right(lines: Lines, from: Position, count: number, wrap: boolean) {
  let { line, col } = from;
  let text = await lines.text(line);
  const total = await lines.count();
  for (let n = 0; n < count; n++) {
    if (col < lastColumn(text)) col = nextColumn(text, col);
    else if (wrap && line + 1 < total) {
      line += 1;
      text = await lines.text(line);
      col = 0;
    } else break;
  }
  return { line, col };
}

/** The first non-blank character of a line, clamped to the document. */
export async function lineStart(lines: Lines, line: number): Promise<Position> {
  const target = Math.max(0, Math.min(line, (await lines.count()) - 1));
  return { line: target, col: firstNonBlank(await lines.text(target)) };
}

/** `f`, `F`, `t`, and `T`: the column of the count-th `char` after or
 * before `col` on the line, or just before or after it for `t` and `T`, or
 * null when the line has too few. With `repeat`, as `;` and `,` repeat a
 * `t`, a target right next to the caret is skipped so the caret moves on. */
export function findInLine(
  text: string,
  col: number,
  kind: "f" | "F" | "t" | "T",
  char: string,
  count: number,
  repeat = false,
): number | null {
  const forward = kind === "f" || kind === "t";
  const till = kind === "t" || kind === "T";
  let at = col;
  if (repeat && till) {
    const adjacent = forward ? nextColumn(text, at) : at > 0 ? previousColumn(text, at) : -1;
    if (adjacent >= 0 && text.startsWith(char, adjacent)) at = adjacent;
  }
  for (let n = 0; n < count; n++) {
    const found = forward ? text.indexOf(char, nextColumn(text, at)) : at > 0 ? text.lastIndexOf(char, at - 1) : -1;
    if (found === -1) return null;
    at = found;
  }
  if (!till) return at;
  return forward ? previousColumn(text, at) : nextColumn(text, at);
}

/** `}` and `{`: the count-th next or previous empty line after a non-empty
 * one, or the document's last character or first line when none is left.
 * `inclusive` is true for that last character, which an operator includes.
 * Null when the document ends before the count runs out. */
export async function paragraph(
  lines: Lines,
  from: Position,
  count: number,
  forward: boolean,
): Promise<{ position: Position; inclusive: boolean } | null> {
  const total = await lines.count();
  let line = from.line;
  for (let n = 0; n < count; n++) {
    let skipped = false;
    for (let first = true; ; first = false) {
      const empty = (await lines.text(line)).length === 0;
      if (!empty) skipped = true;
      if (!first && skipped && empty) break;
      const next = line + (forward ? 1 : -1);
      if (next < 0 || next >= total) {
        if (n < count - 1) return null;
        break;
      }
      line = next;
    }
  }
  if (forward && line === total - 1) {
    const text = await lines.text(line);
    if (text.length > 0) return { position: { line, col: lastColumn(text) }, inclusive: true };
  }
  return { position: { line, col: 0 }, inclusive: false };
}

/** A sentence of a block of lines: its text runs from `start` to `end`, and
 * the white space after it up to `next`, the next sentence's start. Offsets
 * index the block's text, whose lines are joined by "\n". */
export type Sentence = Readonly<{ start: number; end: number; next: number }>;

/** The run of empty lines, or of non-empty ones, around a line. A run of
 * empty lines is one sentence boundary; a run of non-empty lines splits into
 * sentences, each ending at `.`, `!`, or `?` with any closing `)`, `]`, `"`,
 * or `'` after it, followed by white space or the end of the line. */
export type SentenceBlock = Readonly<{
  first: number;
  last: number;
  blank: boolean;
  sentences: readonly Sentence[];
  /** The start of each sentence, or of the empty lines. */
  starts: readonly Position[];
  /** The position of an offset into the sentences' text, and the reverse. */
  position(offset: number): Position;
  offset(at: Position): number;
}>;

export async function sentenceBlock(lines: Lines, line: number): Promise<SentenceBlock> {
  const total = await lines.count();
  const blank = (await lines.text(line)).length === 0;
  const same = async (index: number) => ((await lines.text(index)).length === 0) === blank;
  let first = line;
  while (first > 0 && (await same(first - 1))) first--;
  let last = line;
  while (last + 1 < total && (await same(last + 1))) last++;
  if (blank) {
    const start = { line: first, col: 0 };
    return { first, last, blank, sentences: [], starts: [start], position: () => start, offset: () => 0 };
  }
  const rows: string[] = [];
  const rowStarts: number[] = [];
  let length = 0;
  for (let index = first; index <= last; index++) {
    const row = await lines.text(index);
    rowStarts.push(length);
    rows.push(row);
    length += row.length + 1;
  }
  const text = rows.join("\n");
  const position = (offset: number): Position => {
    let row = rowStarts.length - 1;
    while (rowStarts[row]! > offset) row--;
    return { line: first + row, col: offset - rowStarts[row]! };
  };
  const bounds = text.trimEnd().length;
  const skipWhite = (from: number) => {
    let index = from;
    while (index < text.length && /\s/.test(text[index]!)) index++;
    return index;
  };
  const sentences: Sentence[] = [];
  let start = skipWhite(0);
  const ends = /[.!?][)\]"']*(?=[ \t\n]|$)/g;
  while (start < bounds) {
    ends.lastIndex = start;
    const found = ends.exec(text);
    const end = found === null ? bounds : found.index + found[0].length;
    const next = end >= bounds ? text.length : skipWhite(end);
    sentences.push({ start, end, next });
    start = next;
  }
  return {
    first, last, blank, sentences, starts: sentences.map((sentence) => position(sentence.start)), position,
    offset: (position) => rowStarts[position.line - first]! + position.col,
  };
}

function compare(a: Position, b: Position): number {
  return a.line - b.line || a.col - b.col;
}

/** The sentence start after or before `from`, or null at the document's end. */
async function adjacentSentence(lines: Lines, from: Position, forward: boolean): Promise<Position | null> {
  const block = await sentenceBlock(lines, from.line);
  const near = forward
    ? block.starts.find((start) => compare(start, from) > 0)
    : block.starts.filter((start) => compare(start, from) < 0).pop();
  if (near !== undefined) return near;
  const line = forward ? block.last + 1 : block.first - 1;
  if (line < 0 || line >= (await lines.count())) return null;
  const starts = (await sentenceBlock(lines, line)).starts;
  return (forward ? starts[0] : starts[starts.length - 1]) ?? null;
}

/** `)` and `(`: the count-th next or previous sentence start. An empty line
 * is a sentence of its own, and the document's end stops a motion short, at
 * its last character, which an operator includes. Null when nothing moves. */
export async function sentence(
  lines: Lines,
  from: Position,
  count: number,
  forward: boolean,
): Promise<{ position: Position; inclusive: boolean } | null> {
  let at = from;
  let inclusive = false;
  for (let n = 0; n < count; n++) {
    const next = await adjacentSentence(lines, at, forward);
    if (next !== null) {
      at = next;
      continue;
    }
    if (forward) {
      const line = (await lines.count()) - 1;
      const text = await lines.text(line);
      const edge = { line, col: lastColumn(text) };
      if (compare(edge, at) > 0) {
        at = edge;
        inclusive = text.length > 0;
      }
    } else if (compare(at, { line: 0, col: 0 }) > 0) {
      at = { line: 0, col: 0 };
    }
    break;
  }
  return compare(at, from) === 0 ? null : { position: at, inclusive };
}

const OPENERS = "([{";
const CLOSERS = ")]}";

/** The first bracket at or after `col` on the line, as `%` looks for one. */
export function bracketFrom(text: string, col: number): number | null {
  for (let index = col; index < text.length; index++) {
    if (OPENERS.includes(text[index]!) || CLOSERS.includes(text[index]!)) return index;
  }
  return null;
}

/** The partner of the bracket at `at` by counting brackets in the text, as
 * `%` does in a document without syntax. */
export async function matchBracketText(lines: Lines, at: Position): Promise<Position | null> {
  const bracket = (await lines.text(at.line))[at.col]!;
  const opener = OPENERS.indexOf(bracket);
  return opener !== -1
    ? scanBracket(lines, at, bracket, CLOSERS[opener]!, true)
    : scanBracket(lines, at, bracket, OPENERS[CLOSERS.indexOf(bracket)]!, false);
}

/** The first unbalanced `pair` after or before `at`, counting `bracket`
 * against it, or null when the document ends first. */
export async function scanBracket(
  lines: Lines,
  at: Position,
  bracket: string,
  pair: string,
  forward: boolean,
): Promise<Position | null> {
  let text = await lines.text(at.line);
  const total = await lines.count();
  let depth = 0;
  let { line, col } = at;
  for (;;) {
    col += forward ? 1 : -1;
    while (col < 0 || col >= text.length) {
      line += forward ? 1 : -1;
      if (line < 0 || line >= total) return null;
      text = await lines.text(line);
      col = forward ? 0 : text.length - 1;
    }
    const character = text[col];
    if (character === bracket) depth++;
    else if (character === pair) {
      if (depth === 0) return { line, col };
      depth--;
    }
  }
}
