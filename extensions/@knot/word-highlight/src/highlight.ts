import type { ByteRange, ViewDecoration } from "knot";

export const WORD_COLOR = "editor.wordHighlight";
export const SELECTION_COLOR = "editor.selectionHighlight";

/** The longest selection, in characters, whose other occurrences are
 * highlighted. */
export const SELECTION_LIMIT = 200;

/** The text whose other occurrences a selection highlights, or null for a
 * selection that is empty, only whitespace, over several lines, or long. */
export function selectionQuery(text: string): string | null {
  if (text.trim() === "" || /[\r\n]/u.test(text) || [...text].length > SELECTION_LIMIT) return null;
  return text;
}

const same = (a: ByteRange, b: ByteRange) =>
  a.startByteOffset === b.startByteOffset && a.endByteOffset === b.endByteOffset;

/** Background decorations for the matches in one color, leaving out `skip`. */
export function decorations(
  matches: readonly ByteRange[],
  color: string,
  skip: ByteRange | null,
): ViewDecoration[] {
  return matches
    .filter((match) => skip === null || !same(match, skip))
    .map((range) => ({ range, decoration: { color, style: "background" as const } }));
}
