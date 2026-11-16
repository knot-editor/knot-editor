// Go to File: type part of a path to list the matching files of the window's
// roots, recently active ones first; Enter opens the highlighted one in the
// pane the command ran from. Open Recent lists every recently active file. A
// search button at the toolbar's right runs Go to File. Search Everywhere
// lists files and commands under All, Files, and Actions tabs.
import { commands, documents, icons, workbench, workspaces } from "knot";
import type { CommandContext } from "knot";
import { recordActive } from "./recent.js";
import { fileRows } from "./fileRows.js";
import { searchEverywhere } from "./searchEverywhere.js";
import { clearRecent, recentRows } from "./openRecent.js";

export const COMMAND = "workspace.go-to-file";
export const OPEN_RECENT_COMMAND = "workspace.open-recent";
export const SEARCH_EVERYWHERE_COMMAND = "workspace.search-everywhere";
export const CLEAR_RECENT_COMMAND = "workspace.clear-recent-files";
export const SEARCH_ICON = "toolbar.search";

async function goToFile({ target }: CommandContext) {
  const workspace = await workspaces.get(target.windowTarget);
  const roots = workspace?.roots ?? [];
  const uris = roots.map((root) => root.uri);
  const rows = (query: string) => fileRows(target.windowTarget, uris, roots.length > 1, query);

  const result = await workbench.showPicker({ title: "Go to File", onQuery: rows });
  if (result.kind === "selected") await documents.open(result.id);
}

async function openRecent({ target }: CommandContext) {
  const workspace = await workspaces.get(target.windowTarget);
  const items = await recentRows((workspace?.roots ?? []).map((root) => root.uri));
  const result = await workbench.showPicker({ title: "Open Recent", items });
  if (result.kind === "selected") await documents.open(result.id);
}

export async function start() {
  await workbench.observe(recordActive);
  await commands.register(COMMAND, goToFile, {
    title: "Go to File…",
    description: "Open a file by typing part of its path.",
  });
  await commands.register(SEARCH_EVERYWHERE_COMMAND, searchEverywhere, {
    title: "Search Everywhere…",
    description: "Find a file or a command by typing part of its name.",
  });
  await commands.register(OPEN_RECENT_COMMAND, openRecent, {
    title: "Open Recent…",
    description: "Open one of the recently active files.",
  });
  await commands.register(CLEAR_RECENT_COMMAND, () => clearRecent(), {
    title: "Clear Recent Files",
    description: "Forget the recently active files.",
  });
  await icons.define(SEARCH_ICON, { lucide: "search" });
  await workbench.registerStatusItem("go-to-file", {
    text: "",
    icon: SEARCH_ICON,
    tooltip: "Go to File",
    command: { name: COMMAND },
    side: "right",
    placement: "toolbar",
  });
}
