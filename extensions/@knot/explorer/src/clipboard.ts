// Copy, Cut, and Paste of a file or folder, and the texts and file manager
// actions of a row: its path, relative path, and name.
import { commands, files, keymaps, platform, workbench } from "knot";
import type { CommandContext, WorkspaceRoot } from "knot";
import { describe, report, rootsOf, rowFor, showCreated } from "./actions.js";
import { childUri, copyName } from "./names.js";
import {
  COPY, COPY_NAME, COPY_PATH, COPY_RELATIVE_PATH, CUT, displayPath, nameOf, parentOf, parseRowId, PASTE, REVEAL, REVEAL_LABEL,
  rowId, rootOf, trimmed, VIEW_ID,
} from "./rows.js";
import type { ExplorerState } from "./state.js";

/** The row a command acts on and its URI, or null when nothing is selected. */
async function rowOf(ctx: CommandContext, selected: boolean) {
  const id = await rowFor(ctx, selected);
  return id === null ? null : { id, ...parseRowId(id) };
}

async function take(state: ExplorerState, ctx: CommandContext, selected: boolean, cut: boolean) {
  const row = await rowOf(ctx, selected);
  const roots = await rootsOf(ctx.target.windowTarget);
  if (row === null || roots.some((root) => trimmed(root.uri) === trimmed(row.uri))) return commands.unavailable();
  state.taken = { uri: row.uri, cut };
}

/** The first free name for `name` in `directory`: itself, else its copies. */
async function freeName(directory: string, name: string, isDirectory: boolean): Promise<string> {
  for (let n = 0; ; n++) {
    const candidate = n === 0 ? name : copyName(name, isDirectory, n);
    if ((await files.stat(childUri(directory, [candidate]))) === null) return candidate;
  }
}

async function paste(state: ExplorerState, ctx: CommandContext, selected: boolean) {
  const taken = state.taken;
  if (taken === null) return commands.unavailable();
  const window = ctx.target.windowTarget;
  const roots = await rootsOf(window);
  let row = await rowOf(ctx, selected);
  if (row === null) {
    const [first] = roots;
    if (first === undefined) return report("Open a folder first");
    row = { id: rowId(first.id, first.uri), root: first.id, uri: first.uri };
  }
  // A file's pasted sibling goes beside it.
  const directory = (await files.stat(row.uri))?.kind === "file" ? parentOf(row.uri) : row.uri;
  const source = await files.stat(taken.uri);
  const name = nameOf(taken.uri);
  if (source === null) {
    state.taken = null;
    return report(`${name} no longer exists`);
  }
  try {
    if (taken.cut) {
      if (parentOf(taken.uri) === trimmed(directory)) return;
      const target = childUri(directory, [name]);
      if ((await files.stat(target)) !== null) return report(`${name} already exists in the destination`);
      await files.rename(taken.uri, target);
      state.taken = null;
      await state.view?.invalidate(rowId(row.root, parentOf(taken.uri)), window).catch(() => {});
      await showCreated(state, window, rowId(row.root, trimmed(directory)), target);
      return;
    }
    const target = childUri(directory, [await freeName(directory, name, source.kind === "directory")]);
    await files.copy(taken.uri, target);
    await showCreated(state, window, rowId(row.root, trimmed(directory)), target);
  } catch (error) {
    await report(`Cannot paste ${name}: ${describe(error)}`);
  }
}

/** The path of `uri` below its root, "." for the root itself. */
function relativePath(roots: readonly WorkspaceRoot[], rootId: string, uri: string): string {
  const root = roots.find((candidate) => candidate.id === rootId) ?? rootOf(roots, uri);
  if (root === undefined || root === null) return displayPath(uri);
  const base = trimmed(root.uri);
  const rest = uri === base ? "" : uri.slice(base.length + 1);
  if (rest === "") return ".";
  return rest.split("/").map((segment) => {
    try {
      return decodeURIComponent(segment);
    } catch {
      return segment;
    }
  }).join("/");
}

async function copyText(ctx: CommandContext, selected: boolean, text: (row: { root: string; uri: string }, ctx: CommandContext) => Promise<string>) {
  const row = await rowOf(ctx, selected);
  if (row === null) return commands.unavailable();
  await workbench.writeClipboardText(await text(row, ctx));
}

export async function registerClipboard(state: ExplorerState) {
  const definitions: [string, string, string, (ctx: CommandContext, selected: boolean) => Promise<void>][] = [
    [CUT, "Cut", "Cut the selected file or folder of the explorer, to move it with Paste.", (ctx, s) => take(state, ctx, s, true)],
    [COPY, "Copy", "Copy the selected file or folder of the explorer, to duplicate it with Paste.", (ctx, s) => take(state, ctx, s, false)],
    [PASTE, "Paste", "Paste the cut or copied file or folder into the selected folder of the explorer.", (ctx, s) => paste(state, ctx, s)],
    [COPY_PATH, "Copy Path", "Copy the path of the selected file or folder of the explorer.",
      (ctx, s) => copyText(ctx, s, async (row) => displayPath(row.uri))],
    [COPY_RELATIVE_PATH, "Copy Relative Path", "Copy the path of the selected file or folder below its root.",
      (ctx, s) => copyText(ctx, s, async (row, c) => relativePath(await rootsOf(c.target.windowTarget), row.root, row.uri))],
    [COPY_NAME, "Copy File Name", "Copy the name of the selected file or folder of the explorer.",
      (ctx, s) => copyText(ctx, s, async (row) => nameOf(row.uri))],
    [REVEAL, REVEAL_LABEL, "Show the selected file or folder of the explorer in the file manager.", async (ctx, s) => {
      const row = await rowOf(ctx, s);
      if (row === null) return commands.unavailable();
      try {
        await files.reveal(row.uri);
      } catch (error) {
        await report(`Cannot reveal ${nameOf(row.uri)}: ${describe(error)}`);
      }
    }],
  ];
  for (const [name, title, description, run] of definitions) {
    await commands.register(name, (ctx) => run(ctx, false), { title, description });
    await commands.registerForView(VIEW_ID, name, (ctx) => run(ctx, true));
  }
  const mod = platform === "macos" ? "cmd" : "ctrl";
  keymaps.bind(`${mod}-x`, CUT, { view: VIEW_ID });
  keymaps.bind(`${mod}-c`, COPY, { view: VIEW_ID });
  keymaps.bind(`${mod}-v`, PASTE, { view: VIEW_ID });
}
