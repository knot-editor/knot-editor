// Parsing a typed location and converting between columns and offsets within
// one line. Lines and columns here are 1-based, as the user types them.
import type { TextSnapshot } from "knot";

/** A typed location; a missing column means the start of the line. */
export type Location = { line: number; column: number | null };

/** Reads `N`, `N:C`, or `N,C`, ignoring surrounding whitespace. Lines and
 * columns start at 1; anything else is not a location. */
export function parseLocation(text: string): Location | null {
  const match = /^(\d+)(?:[:,](\d+))?$/.exec(text.trim());
  if (match === null) return null;
  const line = Number(match[1]);
  const column = match[2] === undefined ? null : Number(match[2]);
  if (line < 1 || (column !== null && column < 1)) return null;
  return { line, column };
}

/** The byte offset, relative to the snapshot, of a 1-based column counted in
 * Unicode scalar values with a tab as one; past the end clamps to the end. */
export function columnOffset(line: TextSnapshot, column: number): number {
  let index = 0;
  let remaining = column - 1;
  for (const character of line.text) {
    if (remaining === 0) break;
    index += character.length;
    remaining -= 1;
  }
  return line.byteOffsetAtUtf16(index);
}

/** The 1-based column of a byte offset relative to the snapshot. */
export function columnAt(line: TextSnapshot, offset: number): number {
  return [...line.text.slice(0, line.utf16OffsetAtByte(offset))].length + 1;
}
