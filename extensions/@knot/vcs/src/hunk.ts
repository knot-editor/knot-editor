// What a hunk means for the popup: which hunk a line belongs to, its baseline text, and the edit that reverts it.
import type { Buffer, ByteRange, DiffHunk } from "knot";

/** The buffer reads the revert edit needs. */
export type Lines = Pick<Buffer, "lineCount" | "lineRange">;

/** The hunk whose new lines contain `line`, else the removal at it. */
export function hunkAt(hunks: readonly DiffHunk[], line: number, lastLine: number): DiffHunk | null {
  const containing = hunks.find((hunk) => hunk.new.start <= line && line < hunk.new.end);
  if (containing !== undefined) return containing;
  // A removal after the last line of a text without a final newline sits at
  // the end of that line.
  return hunks.find((hunk) => hunk.new.end === hunk.new.start && Math.min(hunk.new.start, lastLine) === line) ?? null;
}

/** The lines of `text`, each with its terminator; hunks index these lines. */
function splitLines(text: string): string[] {
  return text.match(/[^\n]*\n|[^\n]+$/g) ?? [];
}

/** The baseline's lines that the hunk replaced, terminators included. */
export function baselineText(baseline: string, { old }: DiffHunk): string {
  return splitLines(baseline).slice(old.start, old.end).join("");
}

/**
 * The edit that reverts a hunk: the byte range of its current lines, with
 * their terminators, and the baseline text to put there. A removal has an
 * empty range where the lines were.
 */
export async function revertEdit(
  baseline: string,
  hunk: DiffHunk,
  buffer: Lines,
): Promise<{ range: ByteRange; text: string }> {
  const count = await buffer.lineCount();
  const last = count - 1;
  const { start, end } = hunk.new;
  const startByteOffset = start > last
    ? (await buffer.lineRange(last)).endByteOffset
    : (await buffer.lineRange(start)).startByteOffset;
  const endByteOffset = end === start
    ? startByteOffset
    : end > last
    ? (await buffer.lineRange(last)).endByteOffset
    : (await buffer.lineRange(end)).startByteOffset;
  return { range: { startByteOffset, endByteOffset }, text: baselineText(baseline, hunk) };
}
