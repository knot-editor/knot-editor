// File watching: one recursive watch per root that a window shows, and the
// refresh of the loaded folders a batch of changes touches.
import { files, windows } from "knot";
import { parentOf, parseRowId, trimmed } from "./rows.js";
import type { ExplorerState } from "./state.js";

/** Whether `uri` is the watched root or below it. */
function within(root: string, uri: string): boolean {
  const base = trimmed(root);
  const target = trimmed(uri);
  return target === base || target.startsWith(`${base}/`);
}

/** Reload the loaded folders of `root` that contain a changed entry. A batch
 * naming the root itself means events were lost, so every loaded folder of the
 * root reloads. */
export async function refreshChanged(state: ExplorerState, root: string, changed: readonly string[]) {
  const view = state.view;
  if (view === null) return;
  const rescan = changed.some((uri) => trimmed(uri) === trimmed(root));
  const parents = new Set(changed.map((uri) => parentOf(uri)));
  for (const [window, ids] of state.loaded) {
    for (const id of ids) {
      const folder = parseRowId(id).uri;
      if (!within(root, folder) || !(rescan || parents.has(trimmed(folder)))) continue;
      await view.invalidate(id, window).catch(() => {});
    }
  }
}

/** Reload every loaded folder, after a setting changed what they show. */
export async function refreshAll(state: ExplorerState) {
  const view = state.view;
  if (view === null) return;
  for (const [window, ids] of state.loaded) {
    for (const id of ids) await view.invalidate(id, window).catch(() => {});
  }
}

/** Watch exactly the roots some window shows. */
export async function syncWatches(state: ExplorerState) {
  const wanted = new Set<string>();
  for (const window of await windows.list()) {
    for (const root of window.workspace.roots) wanted.add(root.uri);
  }
  for (const [uri, watch] of [...state.watches]) {
    if (wanted.has(uri)) continue;
    state.watches.delete(uri);
    await watch.dispose();
  }
  for (const uri of wanted) {
    if (state.watches.has(uri)) continue;
    try {
      state.watches.set(
        uri,
        await files.watch(uri, (changed) => refreshChanged(state, uri, changed), { recursive: true }),
      );
    } catch {
      // A root that cannot be watched is browsed without live updates.
    }
  }
}
