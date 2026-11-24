// Revealing a file in the explorer.
import { documents, settings, windows, workspaces } from "knot";
import type { CommandContext, DocumentInfo, ViewNodeTarget, ViewTarget, WindowTarget } from "knot";
import { idPath, parentOf, rootOf, rowId } from "./rows.js";
import { AUTO_REVEAL } from "./settings.js";
import type { ExplorerState } from "./state.js";

/** The URI of the file a tab or editor view shows, or null for any other view. */
export async function fileOf(target: ViewTarget | ViewNodeTarget): Promise<string | null> {
  const window = await windows.get(target.window);
  const document = window === null ? null : windows.findView(window, target)?.view.document;
  if (document == null) return null;
  return (await documents.get(document))?.resourceUri ?? null;
}

/** The file of the editor `window` last focused, or null. */
export async function activeFile(window: WindowTarget): Promise<string | null> {
  const info = await windows.get(window);
  return info?.statusView ? fileOf(info.statusView) : null;
}

/** The URI of the document the command's captured tab shows, or null for any
 * other surface or an untitled document. Buffer wrappers are interned per
 * buffer, so the captured buffer is identical to its document's. */
export async function capturedUri(ctx: CommandContext): Promise<string | null> {
  if (ctx.buffer === null) return null;
  for (const document of await documents.list()) {
    if (document.buffer === ctx.buffer) return document.resourceUri;
  }
  return null;
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

/** Follow a document that moved to another file, as Save As does, in the
 * windows whose active editor shows it, as `explorer.autoReveal` follows a new
 * active tab. */
export async function followMoved(state: ExplorerState, document: DocumentInfo) {
  if (document.resourceUri === null || !(await settings.get(AUTO_REVEAL))) return;
  for (const window of await windows.list()) {
    if (window.statusView !== null && (await fileOf(window.statusView)) === document.resourceUri) {
      // The file is new to the tree, so its folder reloads first.
      const root = rootOf((await workspaces.get(window.target))?.roots ?? [], document.resourceUri);
      if (root !== null) {
        await state.view?.invalidate(rowId(root.id, parentOf(document.resourceUri)), window.target).catch(() => {});
      }
      await revealFile(state, window.target, document.resourceUri);
    }
  }
}
