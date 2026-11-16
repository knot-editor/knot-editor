// The rows of the files a query finds in a window's roots: recently active
// files first, then the file search's other hits.
import { files } from "knot";
import type { FileSearchHit, PickerRow, WindowTarget } from "knot";
import { matchingRecent, recentFiles, type RecentFile } from "./recent.js";
import { rootName, rowFor } from "./rows.js";

function hitFor(file: RecentFile, ranges: FileSearchHit["ranges"]): FileSearchHit {
  return {
    uri: file.uri,
    directory: file.root,
    path: file.path,
    name: file.path.slice(file.path.lastIndexOf("/") + 1),
    ranges,
  };
}

/** The rows for `query` over the roots `uris`; with `labelled`, each
 * description starts with its root's name. */
export async function fileRows(
  window: WindowTarget,
  uris: readonly string[],
  labelled: boolean,
  query: string,
): Promise<PickerRow[]> {
  if (uris.length === 0) return [];
  const recent = await recentFiles(window, uris);
  const first =
    query.trim() === ""
      ? recent.map((file) => ({ file, ranges: [] }))
      : await matchingRecent(recent, query);
  const seen = new Set(first.map(({ file }) => file.uri));
  const { results } = await files.search(uris, query);
  const hits = [
    ...first.map(({ file, ranges }) => hitFor(file, ranges)),
    ...results.filter((hit) => !seen.has(hit.uri)),
  ];
  return hits.map((hit) => rowFor(hit, labelled ? rootName(hit.directory) : null));
}
