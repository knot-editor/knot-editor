// What staging a hunk means for the index text. The functions take the hunks of
// the extension API's `diff` as data, so they need no Git.
import type { DiffHunk } from "knot";

type Lines = DiffHunk["new"];

/** The lines of `text`, each with its terminator; hunks index these lines. */
export function splitLines(text: string): string[] {
  return text.match(/[^\n]*\n|[^\n]+$/g) ?? [];
}

/** Whether two line ranges share a line, or an empty one lies within or beside the other. */
function overlap(left: Lines, right: Lines): boolean {
  if (left.start === left.end || right.start === right.end) return left.start <= right.end && right.start <= left.end;
  return left.start < right.end && right.start < left.end;
}

/** The hunk whose new lines contain `line`, else the removal at it. */
export function hunkAt(hunks: readonly DiffHunk[], line: number, lastLine: number): DiffHunk | null {
  const containing = hunks.find((hunk) => hunk.new.start <= line && line < hunk.new.end);
  if (containing !== undefined) return containing;
  return hunks.find((hunk) => hunk.new.end === hunk.new.start && Math.min(hunk.new.start, lastLine) === line) ?? null;
}

/** `text` with the lines of each of `replacements`' first range replaced by its second text. */
function replaced(text: string, replacements: readonly { range: Lines; text: string }[]): string {
  const lines = splitLines(text);
  for (const { range, text: insert } of [...replacements].reverse()) {
    lines.splice(range.start, range.end - range.start, insert);
  }
  return lines.join("");
}

/**
 * The index text after staging `target`, a hunk of HEAD against the buffer:
 * the index's lines that differ from the buffer there become the buffer's.
 * `unstaged` are the hunks of the index against the buffer.
 */
export function stagedText(index: string, buffer: string, unstaged: readonly DiffHunk[], target: DiffHunk): string {
  const lines = splitLines(buffer);
  return replaced(
    index,
    unstaged
      .filter((hunk) => overlap(hunk.new, target.new))
      .map((hunk) => ({ range: hunk.old, text: lines.slice(hunk.new.start, hunk.new.end).join("") })),
  );
}

/**
 * The index text after unstaging `target`, a hunk of HEAD against the buffer:
 * the index's lines that differ from HEAD there become HEAD's. `staged` are
 * the hunks of HEAD against the index.
 */
export function unstagedText(index: string, head: string, staged: readonly DiffHunk[], target: DiffHunk): string {
  const lines = splitLines(head);
  return replaced(
    index,
    staged
      .filter((hunk) => overlap(hunk.old, target.old))
      .map((hunk) => ({ range: hunk.new, text: lines.slice(hunk.old.start, hunk.old.end).join("") })),
  );
}
