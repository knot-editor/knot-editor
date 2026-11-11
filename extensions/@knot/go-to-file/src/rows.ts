// Turns file search hits into picker rows: the file name as the label and the
// directory below its root as the description, with the search's path ranges
// split between the two.
import type { FileSearchHit, PickerRow } from "knot";

type Ranges = readonly (readonly [number, number])[];

/** The ranges inside `[start, end)`, shifted to begin at `start`. */
function within(ranges: Ranges, start: number, end: number): [number, number][] {
  const found: [number, number][] = [];
  for (const [from, to] of ranges) {
    const clippedFrom = Math.max(from, start);
    const clippedTo = Math.min(to, end);
    if (clippedFrom < clippedTo) found.push([clippedFrom - start, clippedTo - start]);
  }
  return found;
}

/** The last segment of a directory URI. */
export function rootName(uri: string): string {
  const trimmed = uri.replace(/\/+$/, "");
  return trimmed.slice(trimmed.lastIndexOf("/") + 1);
}

/**
 * One row for a hit. With `root` set, the description starts with that root's
 * name, as when a window has several roots.
 */
export function rowFor(hit: FileSearchHit, root: string | null): PickerRow {
  const nameStart = hit.path.length - hit.name.length;
  const directory = hit.path.slice(0, Math.max(nameStart - 1, 0));
  const prefix = root === null ? "" : directory === "" ? root : `${root}/`;
  const description = prefix + directory;
  return {
    id: hit.uri,
    label: hit.name,
    icon: { file: hit.name },
    description: description === "" ? null : description,
    labelMatches: within(hit.ranges, nameStart, hit.path.length),
    descriptionMatches: within(hit.ranges, 0, directory.length).map(
      ([from, to]) => [from + prefix.length, to + prefix.length],
    ),
  };
}
