// The current ref: one status bar item that shows, in each window, what is
// checked out in the repository of the window's focused editor's file, or of
// its first root when no editor has a file. A window with neither shows nothing.
import { icons, windows, workbench } from "knot";
import type { StatusItem, WindowInfo, WindowTarget } from "knot";
import { isGone, type Repository } from "./repositories.js";

export type RefDeps = {
  /** The document URI of a window's focused editor, or null. */
  fileOf(window: WindowInfo): Promise<string | null>;
  /** The repository of a file URI, given the window's roots. */
  find(file: string, roots: readonly string[]): Promise<Repository | null>;
  /** What the backend shows for a repository. */
  fetch(repository: Repository): Promise<{ label: string; tooltip?: string } | null>;
  /** A backend whose contract no longer binds. */
  gone(backend: string): void;
};

export type Refs = {
  /** The repositories or what they have checked out may have changed. */
  refresh(): void;
  [Symbol.asyncDispose](): Promise<void>;
};

const ID = "vcs.ref";

/** Keeps the item in step with every window. */
export async function createRefs(deps: RefDeps): Promise<Refs> {
  await icons.define("vcs.branch", { lucide: "git-branch" });
  const item: StatusItem = await workbench.registerStatusItem(ID, { text: "" });
  /** The text each window shows now, so an unchanged value sends nothing. */
  const shown = new Map<WindowTarget, string>();
  let running = false;
  let pending = false;
  let closed = false;

  async function show(window: WindowInfo) {
    const roots = window.workspace.roots.map((root) => root.uri);
    const file = await deps.fileOf(window);
    let repository = file === null ? null : await deps.find(file, roots);
    if (repository === null && roots[0] !== undefined) repository = await deps.find(roots[0], roots);
    let ref: { label: string; tooltip?: string } | null = null;
    if (repository !== null) {
      try {
        ref = await deps.fetch(repository);
      } catch (error) {
        if (!isGone(error)) throw error;
        deps.gone(repository.backend);
      }
    }
    const key = JSON.stringify(ref);
    if (shown.get(window.target) === key) return;
    shown.set(window.target, key);
    await item.update(
      ref === null
        ? { text: "" }
        : { text: ref.label, icon: "vcs.branch", ...(ref.tooltip === undefined ? {} : { tooltip: ref.tooltip }) },
      { window: window.target },
    );
  }

  async function run() {
    if (running) {
      pending = true;
      return;
    }
    running = true;
    try {
      do {
        pending = false;
        if (closed) break;
        const all = await windows.list();
        for (const gone of [...shown.keys()]) if (!all.some((window) => window.target === gone)) shown.delete(gone);
        for (const window of all) {
          // A window closed meanwhile has nothing to update.
          await show(window).catch((error) => {
            const code = (error as { code?: string }).code;
            if (code !== "InvalidTarget") throw error;
          });
        }
      } while (pending);
    } finally {
      running = false;
    }
  }

  return {
    refresh: () => void run().catch(() => {}),
    async [Symbol.asyncDispose]() {
      closed = true;
      await item[Symbol.asyncDispose]().catch(() => {});
    },
  };
}
