// Copy, Cut, and Paste of a file or folder.
import { commands, files, keymaps, platform } from "knot";
import type { CommandContext, WindowTarget } from "knot";
import { describe, object, report, rootsOf, rowFor, showCreated } from "./actions.js";
import { childUri, copyName } from "./names.js";
import {
  COPY, CUT, DROP, nameOf, parentOf, parseRowId, PASTE, rowId, trimmed, VIEW_ID,
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
  // A copy or cut made meanwhile replaces what this paste took.
  if (source === null) {
    if (state.taken === taken) state.taken = null;
    return report(`${name} no longer exists`);
  }
  if (await transfer(state, window, row.root, taken.uri, source.kind === "directory", directory, taken.cut)) {
    if (taken.cut && state.taken === taken) state.taken = null;
  }
}

/**
 * Moves or copies `sourceUri` into `directory`, then shows the result under
 * `root`. A copy takes the first free name; a move refuses a taken one.
 * Returns whether something was moved or copied.
 */
async function transfer(
  state: ExplorerState,
  window: WindowTarget,
  root: string,
  sourceUri: string,
  isDirectory: boolean,
  directory: string,
  move: boolean,
): Promise<boolean> {
  const name = nameOf(sourceUri);
  const from = trimmed(sourceUri);
  if (isDirectory && (trimmed(directory) === from || trimmed(directory).startsWith(`${from}/`))) {
    await report(`Cannot put ${name} into itself`);
    return false;
  }
  try {
    if (move) {
      if (parentOf(sourceUri) === trimmed(directory)) return false;
      const target = childUri(directory, [name]);
      if ((await files.stat(target)) !== null) {
        await report(`${name} already exists in the destination`);
        return false;
      }
      await files.rename(sourceUri, target);
      await state.view?.invalidate(rowId(root, parentOf(sourceUri)), window).catch(() => {});
      await showCreated(state, window, rowId(root, trimmed(directory)), target);
      return true;
    }
    const target = childUri(directory, [await freeName(directory, name, isDirectory)]);
    await files.copy(sourceUri, target);
    await showCreated(state, window, rowId(root, trimmed(directory)), target);
    return true;
  } catch (error) {
    await report(`Cannot ${move ? "move" : "copy"} ${name}: ${describe(error)}`);
    return false;
  }
}

/** The tree's `drop`: moves the dragged row into the folder it was dropped
 * on, or beside a file it was dropped on; a copy when Option was held. */
async function drop(state: ExplorerState, { arguments: args, target }: CommandContext) {
  const value = object(args);
  const sourceId = object(value?.source ?? null)?.id;
  const targetId = object(value?.target ?? null)?.id;
  if (typeof sourceId !== "string" || typeof targetId !== "string") return;
  const source = parseRowId(sourceId);
  const destination = parseRowId(targetId);
  const kind = await files.stat(source.uri);
  const destinationKind = await files.stat(destination.uri);
  if (kind === null || destinationKind === null) return report("The file no longer exists");
  const directory = destinationKind.kind === "file" ? parentOf(destination.uri) : destination.uri;
  await transfer(state, target.windowTarget, destination.root, source.uri, kind.kind === "directory", directory, value?.copy !== true);
}

export async function registerClipboard(state: ExplorerState) {
  const definitions: [string, string, string, (ctx: CommandContext, selected: boolean) => Promise<void>][] = [
    [CUT, "Cut", "Cut the selected file or folder of the explorer, to move it with Paste.", (ctx, s) => take(state, ctx, s, true)],
    [COPY, "Copy", "Copy the selected file or folder of the explorer, to duplicate it with Paste.", (ctx, s) => take(state, ctx, s, false)],
    [PASTE, "Paste", "Paste the cut or copied file or folder into the selected folder of the explorer.", (ctx, s) => paste(state, ctx, s)],
  ];
  for (const [name, title, description, run] of definitions) {
    await commands.register(name, (ctx) => run(ctx, false), { title, description });
    await commands.registerForView(VIEW_ID, name, (ctx) => run(ctx, true));
  }
  // The view handler needs the global command of its name.
  await commands.register(DROP, (ctx) => drop(state, ctx), { palette: false });
  await commands.registerForView(VIEW_ID, DROP, (ctx) => drop(state, ctx));
  const mod = platform === "macos" ? "cmd" : "ctrl";
  keymaps.bind(`${mod}-x`, CUT, { view: VIEW_ID });
  keymaps.bind(`${mod}-c`, COPY, { view: VIEW_ID });
  keymaps.bind(`${mod}-v`, PASTE, { view: VIEW_ID });
}
