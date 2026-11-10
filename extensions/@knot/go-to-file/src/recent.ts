// Recently active files: recorded when an editor tab becomes active, whatever
// opened it, and ordered for the picker.
import { completion, documents, history, windows } from "knot";
import type { ApplicationEvent, ViewTarget, WindowTarget } from "knot";

export const HISTORY = "recent-files";

/** The URI of the file a tab presents, or null for anything else. */
async function fileOf(target: ViewTarget): Promise<string | null> {
  const window = await windows.get(target.window);
  const tab = window?.panes.flatMap((pane) => pane.tabs).find((tab) => tab.target === target);
  if (tab === undefined || tab.document === null) return null;
  return (await documents.get(tab.document))?.resourceUri ?? null;
}

/** Records the file of a newly active tab. */
export async function recordActive(event: ApplicationEvent) {
  if (event.type !== "activeTargetChanged" || event.activeTarget === null) return;
  const uri = await fileOf(event.activeTarget);
  if (uri !== null) await history.record(HISTORY, uri);
}

/** A recent file under one of the roots, with its path below that root. */
export interface RecentFile {
  readonly uri: string;
  readonly root: string;
  readonly path: string;
}

/**
 * The recent files under `roots`, most recent first, without the file the
 * window's last focused editor shows.
 */
export async function recentFiles(window: WindowTarget, roots: readonly string[]): Promise<RecentFile[]> {
  const info = await windows.get(window);
  const current = info?.statusTarget ? await fileOf(info.statusTarget) : null;
  const found: RecentFile[] = [];
  for (const uri of await history.recent(HISTORY)) {
    if (uri === current) continue;
    const root = roots.find((root) => uri.startsWith(`${root.replace(/\/+$/, "")}/`));
    if (root === undefined) continue;
    found.push({ uri, root, path: uri.slice(root.replace(/\/+$/, "").length + 1) });
  }
  return found;
}

/** The recent files matching `query` on their path, best first. */
export async function matchingRecent(
  recent: readonly RecentFile[],
  query: string,
): Promise<{ file: RecentFile; ranges: readonly (readonly [number, number])[] }[]> {
  const matches = await completion.match(query, recent.map((file) => file.path));
  return matches.map((match) => ({ file: recent[match.index]!, ranges: match.ranges }));
}
