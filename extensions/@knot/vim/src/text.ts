// Text math on one line, in the UTF-16 columns JavaScript strings index by.
// Columns always sit on a code point boundary.

/** The column after the code point at `col`. */
export function nextColumn(text: string, col: number): number {
  return col + (isHighSurrogate(text.charCodeAt(col)) && col + 1 < text.length ? 2 : 1);
}

/** The column of the code point before `col`. */
export function previousColumn(text: string, col: number): number {
  return col - (col >= 2 && isLowSurrogate(text.charCodeAt(col - 1)) ? 2 : 1);
}

/** The column of the line's last character, or 0 on an empty line. */
export function lastColumn(text: string): number {
  return text.length === 0 ? 0 : previousColumn(text, text.length);
}

/** The first column that is not a space or tab, or the last character's on a
 * blank line. */
export function firstNonBlank(text: string): number {
  const index = text.search(/[^ \t]/);
  return index === -1 ? lastColumn(text) : index;
}

/** The screen column `col` starts at, with tabs advancing to the next stop. */
export function screenColumn(text: string, col: number, tabSize: number): number {
  let cells = 0;
  for (let index = 0; index < col && index < text.length; index = nextColumn(text, index)) {
    cells = text.charCodeAt(index) === 9 ? cells + tabSize - (cells % tabSize) : cells + 1;
  }
  return cells;
}

/** The column of the last character starting at or before screen column
 * `want`, never past the line's last character. */
export function columnAtScreen(text: string, want: number, tabSize: number): number {
  let cells = 0;
  let col = 0;
  for (let index = 0; index < text.length; index = nextColumn(text, index)) {
    if (cells > want) break;
    col = index;
    cells = text.charCodeAt(index) === 9 ? cells + tabSize - (cells % tabSize) : cells + 1;
  }
  return col;
}

/** The UTF-8 length of `text`, the unit of buffer offsets. */
export function utf8Length(text: string): number {
  let length = 0;
  for (let index = 0; index < text.length; index++) {
    const unit = text.charCodeAt(index);
    if (unit < 0x80) length += 1;
    else if (unit < 0x800) length += 2;
    else if (isHighSurrogate(unit) && index + 1 < text.length) {
      length += 4;
      index++;
    } else length += 3;
  }
  return length;
}

/** The column `bytes` UTF-8 bytes into `text`. */
export function columnAtByte(text: string, bytes: number): number {
  let consumed = 0;
  let col = 0;
  while (col < text.length && consumed < bytes) {
    const next = nextColumn(text, col);
    consumed += utf8Length(text.slice(col, next));
    col = next;
  }
  return col;
}

const isHighSurrogate = (unit: number) => unit >= 0xd800 && unit <= 0xdbff;
const isLowSurrogate = (unit: number) => unit >= 0xdc00 && unit <= 0xdfff;
