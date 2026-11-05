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

  /** Vim's character class under the cursor: 0 for white space and line ends,
   * 1 for punctuation, 2 for word characters, and 3 for emoji. With `big`,
   * every non-blank character is class 1. */
  kind(big: boolean): number {
    if (this.col >= this.text.length) return 0;
    const character = String.fromCodePoint(this.text.codePointAt(this.col)!);
    if (character === " " || character === "\t") return 0;
    if (big) return 1;
    if (/[\p{L}\p{N}_\p{M}]/u.test(character)) return 2;
    if (/\p{Extended_Pictographic}/u.test(character)) return 3;
    return 1;
  }

  /** Move while the class stays `kind`; true when the document ended. */
  async skip(kind: number, big: boolean, forward: boolean): Promise<boolean> {
    while (this.kind(big) === kind) {
      if ((forward ? await this.next() : await this.previous()) === -1) return true;
    }
    return false;
  }
}

/** `w` and `W`: the start of the count-th next word. */
export async function wordForward(lines: Lines, from: Position, count: number, big: boolean) {
  const cursor = await Cursor.at(lines, from);
  for (let n = 0; n < count; n++) {
    const start = cursor.kind(big);
    if ((await cursor.next()) === -1) break;
    if (start !== 0 && (await cursor.skip(start, big, true))) break;
    let ended = false;
    while (cursor.kind(big) === 0) {
      if (cursor.col === 0 && cursor.onEmptyLine) break;
      if ((await cursor.next()) === -1) {
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

/** `e` and `E`: the end of the count-th next word. */
export async function wordEnd(lines: Lines, from: Position, count: number, big: boolean) {
  const cursor = await Cursor.at(lines, from);
  for (let n = 0; n < count; n++) {
    const start = cursor.kind(big);
    if ((await cursor.next()) === -1) break;
    if (start !== 0 && cursor.kind(big) === start) {
      if (await cursor.skip(start, big, true)) break;
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
