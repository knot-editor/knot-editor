// `:sort` over line snapshots, without host calls.
import type { SortOptions } from "./ex.js";

/** The first decimal number of a line, with one leading minus, or null. */
function numberOf(line: string): number | null {
  const match = /-?\d+/.exec(line);
  return match === null ? null : Number(match[0]);
}

/** `lines` in order: by text, or with `numeric` by their first number, lines
 * without one first. `reverse` flips the order and equal lines keep theirs;
 * `unique` keeps the first of lines of equal text. */
export function sortLines(lines: readonly string[], options: SortOptions): string[] {
  const fold = (line: string) => (options.ignoreCase ? line.toLowerCase() : line);
  const keyed = lines.map((text, index) => ({
    text,
    index,
    key: options.numeric ? (numberOf(text) ?? -Infinity) : fold(text),
  }));
  keyed.sort((a, b) => {
    const order = a.key < b.key ? -1 : a.key > b.key ? 1 : 0;
    return (options.reverse ? -order : order) || a.index - b.index;
  });
  const sorted = keyed.map(({ text }) => text);
  return options.unique ? sorted.filter((line, at) => at === 0 || fold(line) !== fold(sorted[at - 1]!)) : sorted;
}
