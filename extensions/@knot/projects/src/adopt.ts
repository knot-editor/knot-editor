// Adopting an inferred repository as a window's project, automatically for a
// rootless window opened for one file, or through the Open Containing
// Repository command.
import { documents, windows, workspaces } from "knot";
import type { DocumentId, DocumentInfo, WindowInfo } from "knot";
import { automaticRoot } from "./inference.js";
import { windowRoots } from "./layout.js";
import { projectWithRoots } from "./registry.js";
import type { ProjectsState } from "./state.js";
import { switchTo } from "./switch.js";

/**
 * Makes `root` the project of `document`, which `window` presents. A known
 * project is switched to and restores its session with the document in it:
 * a rootless window is adopted into it, a rooted one is kept. A new root
 * becomes the rootless window's root in place, or opens in a new window with
 * the document beside a rooted one; tracking then remembers it.
 */
export async function adopt(state: ProjectsState, root: string, window: WindowInfo, document: DocumentId) {
  const rootless = windowRoots(window).length === 0;
  const project = projectWithRoots(state.registry, [root]);
  if (project !== null && !state.unreadable.has(project.id)) {
    await switchTo(state, project, window.target, { origin: rootless ? "adopt" : "keep", present: document });
    return;
  }
  if (rootless) {
    await workspaces.replaceRoots([root], { target: window.target });
    return;
  }
  const opened = await windows.open({ roots: [root] });
  await documents.present(document, { target: opened.target });
}

/**
 * Adopts the repository of a document just opened or saved to a new file
 * when its window qualifies (see `automaticRoot`). Failures leave the
 * document where it opened.
 */
export async function adoptAutomatically(state: ProjectsState, document: DocumentInfo) {
  try {
    const list = await windows.list();
    const window = list.find((window) =>
      window.panes.some((pane) => pane.tabs.some((tab) => tab.document === document.id)),
    );
    if (window === undefined) return;
    const root = await automaticRoot(document, window, list);
    if (root !== null) await adopt(state, root, window, document.id);
  } catch {
    // Inference is a convenience; the document stays where it opened.
  }
}
