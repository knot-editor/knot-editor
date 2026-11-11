// New File, New Folder, Rename, and Delete: inline editing on the row they
// act on, then the file operation, then the tree follows.
import { commands, documents, files, keymaps, workbench, workspaces } from "knot";
import type { CommandContext, JsonValue, WindowTarget } from "knot";
import { childUri, pathSegments, renameSelection } from "./names.js";
import { activeFile, revealFile } from "./reveal.js";
import {
  DELETE, idPath, NEW_FILE, NEW_FOLDER, nameOf, parentOf, parseRowId, RENAME, rootOf, rowId, trimmed, VIEW_ID,
} from "./rows.js";
import type { ExplorerState } from "./state.js";

/** How often an edit waits for its row to load before giving up. */
const ATTEMPTS = 20;

export function object(value: JsonValue): Record<string, JsonValue> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, JsonValue>)
    : null;
}

function idArgument(value: JsonValue): string | null {
  const id = object(value)?.id;
  return typeof id === "string" ? id : null;
}

export async function report(message: string) {
  await workbench.showMessage({ title: "Explorer", message });
}

export function describe(error: unknown): string {
  const code = (error as { code?: unknown } | null)?.code;
  return typeof code === "string" ? code : String(error);
}

/** The roots of `window`. */
export async function rootsOf(window: WindowTarget) {
  return (await workspaces.get(window))?.roots ?? [];
}

/** The row a command acts on: the one it names, else the selected one, else
 * the row of the active file. */
export async function rowFor(ctx: CommandContext, selected: boolean): Promise<string | null> {
  const named = idArgument(ctx.arguments);
  if (named !== null) return named;
  if (selected) {
    const id = await workbench.selectedTreeItem(ctx.target);
    if (id !== null) return id;
  }
  const window = ctx.target.windowTarget;
  const uri = await activeFile(window);
  const root = uri === null ? null : rootOf(await rootsOf(window), uri);
  const path = root === null || uri === null ? null : idPath(root, uri);
  return path === null ? null : path[path.length - 1]!;
}

/** Whether the row is a root of the window. */
async function isRoot(window: WindowTarget, id: string): Promise<boolean> {
  const { root, uri } = parseRowId(id);
  return (await rootsOf(window)).some((candidate) => candidate.id === root && trimmed(candidate.uri) === trimmed(uri));
}

/** Run an edit once the row it needs is loaded, showing the explorer first. */
async function whenLoaded<T>(
  state: ExplorerState,
  window: WindowTarget,
  uri: string,
  attempt: () => Promise<T>,
): Promise<T> {
  await state.view?.show(window);
  for (let tries = 1; ; tries++) {
    try {
      return await attempt();
    } catch (error) {
      if ((error as { code?: unknown } | null)?.code !== "TreeItemNotFound" || tries >= ATTEMPTS) throw error;
      await revealFile(state, window, uri);
      await files.stat(uri);
    }
  }
}

/** Reload `parent` and select `uri` in it. */
export async function showCreated(state: ExplorerState, window: WindowTarget, parent: string, uri: string) {
  await state.view?.invalidate(parent, window).catch(() => {});
  await revealFile(state, window, uri);
}

async function create(state: ExplorerState, ctx: CommandContext, directory: boolean, selected: boolean) {
  const window = ctx.target.windowTarget;
  const roots = await rootsOf(window);
  let id = await rowFor(ctx, selected);
  if (id === null) {
    const [first] = roots;
    if (first === undefined) return report("Open a folder first");
    id = rowId(first.id, first.uri);
  }
  const { root, uri: row } = parseRowId(id);
  // A file's new sibling goes beside it.
  const parentUri = (await files.stat(row))?.kind === "file" ? parentOf(row) : row;
  const parent = rowId(root, parentUri);
  const taken = async (name: string) => {
    const path = pathSegments(name);
    if ("problem" in path) return path.problem;
    return (await files.stat(childUri(parentUri, path.segments))) === null ? null : `${name} already exists here`;
  };
  const result = await whenLoaded(state, window, parentUri, () =>
    state.view!.editNewItem(parent, {
      icon: directory ? "folder" : "file",
      validate: (value) => (value === "" ? null : taken(value)),
    }),
  );
  if (result.kind !== "accepted") return;
  const path = pathSegments(result.value);
  if ("problem" in path) return;
  const uri = childUri(parentUri, path.segments);
  try {
    if (directory) await files.createDirectory(uri);
    else await files.createFile(uri);
  } catch (error) {
    return report(`Cannot create ${result.value}: ${describe(error)}`);
  }
  await showCreated(state, window, parent, uri);
  if (!directory) await documents.open(uri, { target: window });
}

async function rename(state: ExplorerState, ctx: CommandContext, selected: boolean) {
  const window = ctx.target.windowTarget;
  const id = await rowFor(ctx, selected);
  if (id === null || (await isRoot(window, id))) return commands.unavailable();
  const { root, uri } = parseRowId(id);
  const stat = await files.stat(uri);
  if (stat === null) return report(`${nameOf(uri)} no longer exists`);
  const name = nameOf(uri);
  const directory = parentOf(uri);
  const problem = async (value: string) => {
    if (value === name) return null;
    if (value === "") return "A name must be provided";
    if (value.includes("/")) return "A name cannot contain a slash";
    if (value === "." || value === "..") return "A name cannot be . or ..";
    // A change of case alone names the same entry on a case-folding disk.
    if (value.toLowerCase() === name.toLowerCase()) return null;
    return (await files.stat(childUri(directory, [value]))) === null ? null : `${value} already exists here`;
  };
  const result = await whenLoaded(state, window, uri, () =>
    state.view!.editLabel(id, {
      value: name,
      selection: renameSelection(name, stat.kind === "directory"),
      validate: problem,
    }),
  );
  if (result.kind !== "accepted" || result.value === name) return;
  const target = childUri(directory, [result.value]);
  try {
    await files.rename(uri, target);
  } catch (error) {
    return report(`Cannot rename ${name}: ${describe(error)}`);
  }
  await showCreated(state, window, rowId(root, directory), target);
}

async function remove(state: ExplorerState, ctx: CommandContext, selected: boolean) {
  const window = ctx.target.windowTarget;
  const id = await rowFor(ctx, selected);
  if (id === null || (await isRoot(window, id))) return commands.unavailable();
  const { root, uri } = parseRowId(id);
  const name = nameOf(uri);
  const stat = await files.stat(uri);
  if (stat === null) return report(`${name} no longer exists`);
  const answer = await workbench.showConfirmation({
    title: `Delete ${name}?`,
    message:
      stat.kind === "directory"
        ? `${name} and everything in it will move to the Trash.`
        : `${name} will move to the Trash.`,
    confirmLabel: "Move to Trash",
  });
  if (answer.kind !== "confirmed") return;
  try {
    await files.delete(uri);
  } catch (error) {
    return report(`Cannot delete ${name}: ${describe(error)}`);
  }
  await state.view?.invalidate(rowId(root, parentOf(uri)), window).catch(() => {});
}

export async function registerActions(state: ExplorerState) {
  const definitions: [string, string, string, (ctx: CommandContext, selected: boolean) => Promise<void>][] = [
    [NEW_FILE, "New File…", "Create a file in the selected folder of the explorer.", (ctx, selected) => create(state, ctx, false, selected)],
    [NEW_FOLDER, "New Folder…", "Create a folder in the selected folder of the explorer.", (ctx, selected) => create(state, ctx, true, selected)],
    [RENAME, "Rename…", "Rename the selected file or folder of the explorer.", (ctx, selected) => rename(state, ctx, selected)],
    [DELETE, "Delete…", "Move the selected file or folder of the explorer to the Trash.", (ctx, selected) => remove(state, ctx, selected)],
  ];
  for (const [name, title, description, run] of definitions) {
    // The global handler serves the palette, acting on the active file; the
    // view's handler acts on the selected row.
    await commands.register(name, (ctx) => run(ctx, false), { title, description });
    await commands.registerForView(VIEW_ID, name, (ctx) => run(ctx, true));
  }
  keymaps.bind("f2", RENAME, { view: VIEW_ID });
  keymaps.bind("delete", DELETE, { view: VIEW_ID });
  keymaps.bind("cmd-backspace", DELETE, { view: VIEW_ID });
}
