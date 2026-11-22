// Moving the caret between the changed regions of a document.
import type { DiffHunk } from "knot";

export type Direction = "next" | "previous";

/**
 * The line a move from `caretLine` lands on: the first line of the nearest
 * hunk after (or before) it, wrapping at the ends, or null without hunks.
 * A removal's line is the one that followed the removed lines.
 */
export function targetLine(hunks: readonly DiffHunk[], caretLine: number, direction: Direction): number | null {
  if (hunks.length === 0) return null;
  const starts = hunks.map((hunk) => hunk.new.start);
  if (direction === "next") return starts.find((start) => start > caretLine) ?? starts[0]!;
  const before = starts.filter((start) => start < caretLine);
  return before[before.length - 1] ?? starts[starts.length - 1]!;
}
