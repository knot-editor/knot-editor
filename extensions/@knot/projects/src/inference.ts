// Project inference: the nearest `.git` ancestor of a document, and whether a
// window adopts it automatically. Pure decisions over listed state and
// `files.stat`; adoption itself lives in `adopt.ts`.
import { files, homeUri } from "knot";
import type { DocumentId, WindowInfo } from "knot";
import { windowRoots } from "./layout.js";

function trimmed(uri: string): string {
  return uri.replace(/\/+$/, "");
}

function sameRoot(left: string, right: string): boolean {
  return trimmed(left) === trimmed(right);
}

function splitFileUri(uri: string): { origin: string; path: string } | null {
  const match = /^file:\/\/([^/]*)(\/.*)$/i.exec(trimmed(uri));
  if (match === null) return null;
  return { origin: `file://${match[1]}`, path: match[2]! };
}

/**
 * The nearest ancestor directory of `documentUri` containing a `.git` entry,
 * in root form with a trailing slash. A `.git` file (worktree, submodule)
 * counts like a directory; the walk stops at the first match, so nested
 * repositories infer their inner root. Null for an unstored or non-`file:`
 * document, when no ancestor carries the marker, when the nearest one is the
 * filesystem root or the home directory, or when a stat fails.
 */
export async function repositoryRoot(documentUri: string | null): Promise<string | null> {
  if (documentUri === null) return null;
  const split = splitFileUri(documentUri);
  if (split === null) return null;
  let directory = split.path.slice(0, split.path.lastIndexOf("/")) || "/";
  for (;;) {
    const candidate = `${split.origin}${directory === "/" ? "" : directory}/`;
    let marker;
    try {
      marker = await files.stat(`${candidate}.git`);
    } catch {
      return null;
    }
    if (marker !== null) {
      if (directory === "/") return null;
      if (homeUri !== null && sameRoot(candidate, homeUri)) return null;
      return candidate;
    }
    if (directory === "/") return null;
    directory = directory.slice(0, directory.lastIndexOf("/")) || "/";
  }
}

/** The first window that already has `root` as one of its roots. */
export function windowWithRoot(root: string, list: readonly WindowInfo[]): WindowInfo | undefined {
  return list.find((window) => windowRoots(window).some((open) => sameRoot(open, root)));
}

/**
 * The root `window` adopts automatically for a document it presents: only a
 * window without roots whose every document or terminal tab is that
 * document, and only for a repository no window already has as a root.
 */
export async function automaticRoot(
  document: { id: DocumentId; resourceUri: string | null },
  window: WindowInfo,
  list: readonly WindowInfo[],
): Promise<string | null> {
  if (windowRoots(window).length > 0) return null;
  const tabs = window.panes.flatMap((pane) => pane.tabs).filter((tab) => tab.kind !== "tool");
  if (tabs.length === 0 || tabs.some((tab) => tab.document !== document.id)) return null;
  const root = await repositoryRoot(document.resourceUri);
  if (root === null || windowWithRoot(root, list) !== undefined) return null;
  return root;
}
