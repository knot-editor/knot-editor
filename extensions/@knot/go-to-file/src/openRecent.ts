// Open Recent: every recently active file, most recent first, wherever it
// lives; files that no longer exist are dropped and forgotten.
import { files, history } from "knot";
import type { PickerRow } from "knot";
import { HISTORY } from "./recent.js";
import { rootName, rowFor } from "./rows.js";

const parentOf = (uri: string) => uri.slice(0, uri.lastIndexOf("/"));

/** The root containing `uri`, if any. */
function rootOf(roots: readonly string[], uri: string): string | undefined {
  return roots.find((root) => uri.startsWith(`${root.replace(/\/+$/, "")}/`));
}

/** The URIs among `uris` that exist. A provider failure keeps the entry. */
async function existing(uris: readonly string[], roots: readonly string[]): Promise<Set<string>> {
  // The workspace index answers listings but hides ignored entries, so a name
  // in a listing proves existence and its absence proves nothing.
  const confirmed = new Set<string>();
  const directories = new Set(uris.filter((uri) => rootOf(roots, uri) !== undefined).map(parentOf));
  await Promise.all(
    [...directories].map(async (directory) => {
      try {
        for (const entry of await files.list(directory)) confirmed.add(entry.uri);
      } catch {
        // Left to the stat below.
      }
    }),
  );
  const present = new Set(confirmed);
  await Promise.all(
    uris
      .filter((uri) => !confirmed.has(uri))
      .map(async (uri) => {
        try {
          if ((await files.stat(uri)) !== null) present.add(uri);
        } catch {
          present.add(uri);
        }
      }),
  );
  return present;
}

function decoded(text: string): string {
  try {
    return decodeURIComponent(text);
  } catch {
    return text;
  }
}

function rowOf(uri: string, roots: readonly string[], labelled: boolean): PickerRow {
  const root = rootOf(roots, uri);
  const name = decoded(uri.slice(uri.lastIndexOf("/") + 1));
  if (root !== undefined) {
    const path = decoded(uri.slice(root.replace(/\/+$/, "").length + 1));
    return rowFor({ uri, directory: root, path, name, ranges: [] }, labelled ? rootName(root) : null);
  }
  const directory = decoded(parentOf(uri).replace(/^[a-z][a-z0-9+.-]*:\/\/[^/]*/i, ""));
  return { id: uri, label: name, description: directory === "" ? null : directory };
}

/** The rows of the recent files that still exist, forgetting the others. */
export async function recentRows(roots: readonly string[]): Promise<PickerRow[]> {
  const recorded = await history.recent(HISTORY);
  const present = await existing(recorded, roots);
  await Promise.all(recorded.filter((uri) => !present.has(uri)).map((uri) => history.forget(HISTORY, uri)));
  return recorded.filter((uri) => present.has(uri)).map((uri) => rowOf(uri, roots, roots.length > 1));
}

/** Empties the list of recent files. */
export async function clearRecent() {
  await history.forget(HISTORY);
}
