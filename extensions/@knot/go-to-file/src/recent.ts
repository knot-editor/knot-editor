// Recently active files and documents: recorded when an editor tab becomes
// active, whatever opened it, and ordered for the picker.
import { completion, documents, history, windows } from "knot";
import type { ApplicationEvent, DocumentInfo, ViewTarget, WindowTarget } from "knot";

export const HISTORY = "recent-files";
export const BUFFER_HISTORY = "recent-buffers";

/** The document a tab presents, or null for anything else. */
export async function documentOf(target: ViewTarget): Promise<DocumentInfo | null> {
  const window = await windows.get(target.window);
  const tab = window?.panes.flatMap((pane) => pane.tabs).find((tab) => tab.target === target);
  if (tab === undefined || tab.document === null) return null;
  return documents.get(tab.document);
}

/** The URI of the file a tab presents, or null for anything else. */
async function fileOf(target: ViewTarget): Promise<string | null> {
  return (await documentOf(target))?.resourceUri ?? null;
}

/** Records the document and file of a newly active tab. */
export async function recordActive(event: ApplicationEvent) {
  if (event.type !== "activeTargetChanged" || event.activeTarget === null) return;
  const document = await documentOf(event.activeTarget);
  if (document === null) return;
  await history.record(BUFFER_HISTORY, document.id);
  if (document.resourceUri !== null) await history.record(HISTORY, document.resourceUri);
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
