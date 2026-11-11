import { commands, documents, settings, workspaces } from "knot";
import type { CommandContext, JsonValue } from "knot";
import { activeFile, revealFile } from "./reveal.js";
import { ACTIVATE, OPEN_FOLDER, VIEW_ID } from "./rows.js";
import { SHOW_IGNORED } from "./settings.js";
import type { ExplorerState } from "./state.js";

function object(value: JsonValue): Record<string, JsonValue> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, JsonValue>)
    : null;
}

/** The gesture a tree row's command was activated with, or null for a call
 * that did not come from a row. */
function gestureOf(args: JsonValue): string | null {
  const gesture = object(args)?.gesture;
  return typeof gesture === "string" ? gesture : null;
}

function uriOf(args: JsonValue): string | null {
  const payload = object(args)?.arguments;
  const uri = payload === undefined || payload === null ? null : object(payload)?.uri;
  return typeof uri === "string" ? uri : null;
}

async function openRow({ target, arguments: args }: CommandContext) {
  const uri = uriOf(args);
  if (uri === null) return;
  const window = target.windowTarget;
  switch (gestureOf(args)) {
    case "click":
      await documents.open(uri, { target: window, preview: true });
      break;
    case "doubleClick":
    case "enter":
      await documents.open(uri, { target: window });
      break;
    case "middleClick":
      await documents.open(uri, { target: window, focus: false });
      break;
  }
}

async function openFolder({ target, arguments: args }: CommandContext) {
  const gesture = gestureOf(args);
  if (gesture !== null && gesture !== "click" && gesture !== "enter") return;
  const picked = await workspacesPick(target.windowTarget);
  if (picked !== null) await workspaces.openFolder(picked, { target: target.windowTarget });
}

async function workspacesPick(window: CommandContext["target"]["windowTarget"]): Promise<string | null> {
  const result = await workspaces.pickFolder({ target: window });
  return result.kind === "selected" ? result.uri : null;
}

export async function registerCommands(state: ExplorerState) {
  // The view handler of a row command needs the global command of its name,
  // which also serves the call without a row.
  await commands.register(ACTIVATE, async ({ arguments: args }) => {
    const uri = typeof args === "string" ? args : (object(args)?.uri ?? null);
    if (typeof uri === "string") await documents.open(uri);
  }, { palette: false });
  await commands.registerForView(VIEW_ID, ACTIVATE, openRow);
  await commands.register(OPEN_FOLDER, openFolder, {
    title: "Open Folder…",
    description: "Choose a folder to show in this window.",
    palette: false,
  });

  await commands.register("explorer.reveal-active-file", async ({ target }) => {
    const uri = await activeFile(target.windowTarget);
    if (uri === null || state.view === null) return;
    await state.view.show(target.windowTarget);
    if (await revealFile(state, target.windowTarget, uri)) await state.view.focus(target.windowTarget);
  }, { title: "Reveal Active File", description: "Select the active file in the explorer." });

  await commands.register("explorer.toggle", async ({ target }) => {
    await state.view?.toggle(target.windowTarget);
  }, { title: "Toggle Explorer", description: "Show and focus the file explorer, or hide it when focused." });

  await commands.register("explorer.toggle-ignored", async () => {
    await settings.set(SHOW_IGNORED, !(await settings.get(SHOW_IGNORED)));
  }, { title: "Toggle Ignored Files in Explorer", description: "Show or hide the files that ignore rules match." });
}
