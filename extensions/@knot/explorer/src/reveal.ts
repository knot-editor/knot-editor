// Revealing a file in the explorer.
import { documents, settings, windows, workspaces } from "knot";
import type { ViewTarget, WindowTarget } from "knot";
import { idPath, rootOf } from "./rows.js";
import { AUTO_REVEAL } from "./settings.js";
import type { ExplorerState } from "./state.js";

/** The URI of the file a tab shows, or null for any other tab. */
export async function fileOf(target: ViewTarget): Promise<string | null> {
  const window = await windows.get(target.window);
  const tab = window?.panes.flatMap((pane) => pane.tabs).find((tab) => tab.target === target);
  if (tab === undefined || tab.document === null) return null;
  return (await documents.get(tab.document))?.resourceUri ?? null;
}

/** The file of the editor `window` last focused, or null. */
export async function activeFile(window: WindowTarget): Promise<string | null> {
  const info = await windows.get(window);
  return info?.statusTarget ? fileOf(info.statusTarget) : null;
}

/** Select `uri` in the explorer of `window`, expanding the folders above it.
 * Resolves false when no root of the window contains it. */
export async function revealFile(state: ExplorerState, window: WindowTarget, uri: string): Promise<boolean> {
  const roots = (await workspaces.get(window))?.roots ?? [];
  const root = rootOf(roots, uri);
  const path = root === null ? null : idPath(root, uri);
  if (path === null || state.view === null) return false;
  try {
    await state.view.reveal(path, window);
  } catch {
    // A window that shows no explorer has nothing to reveal in.
    return false;
  }
  return true;
}

/** Follow a newly active editor tab when `explorer.autoReveal` is on. */
export async function followActive(state: ExplorerState, target: ViewTarget) {
  if (!(await settings.get(AUTO_REVEAL))) return;
  const uri = await fileOf(target);
  if (uri !== null) await revealFile(state, target.window, uri);
}
