// Search Everywhere: one picker over the window's files and the palette's
// commands, with All, Files, and Actions tabs. Choosing a file opens it and
// choosing an action runs it in the window the picker opened from.
import { commands, documents, workbench, workspaces } from "knot";
import type { CommandContext, CommandInfo, PickerRow } from "knot";
import { fileRows } from "./fileRows.js";

export const ACTION_ICON = "search-everywhere.action";
const ACTION_PREFIX = "command:";
/** Actions the All tab shows, so files stay visible beside them. */
const ALL_TAB_ACTIONS = 8;

const TABS = [
  { id: "all", label: "All" },
  { id: "files", label: "Files" },
  { id: "actions", label: "Actions" },
];

/** The actions whose title or name contains `query`, with the title ranges. */
export function actionRows(all: readonly CommandInfo[], query: string): PickerRow[] {
  const needle = query.trim().toLowerCase();
  const rows: PickerRow[] = [];
  for (const command of all) {
    const at = command.title.toLowerCase().indexOf(needle);
    if (needle !== "" && at < 0 && !command.name.toLowerCase().includes(needle)) continue;
    rows.push({
      id: ACTION_PREFIX + command.name,
      label: command.title,
      description: command.description,
      icon: ACTION_ICON,
      labelMatches: needle !== "" && at >= 0 ? [[at, at + needle.length]] : [],
    });
  }
  return rows;
}

export async function searchEverywhere({ target }: CommandContext) {
  const workspace = await workspaces.get(target.windowTarget);
  const roots = workspace?.roots ?? [];
  const uris = roots.map((root) => root.uri);
  const available = await commands.list();

  const rows = async (query: string, tab: string | null): Promise<PickerRow[]> => {
    const found = tab === "actions" ? [] : await fileRows(target.windowTarget, uris, roots.length > 1, query);
    const actions = tab === "files" ? [] : actionRows(available, query);
    // An empty All tab lists recent files; actions join once something is typed.
    if (tab === "all") return [...found, ...(query.trim() === "" ? [] : actions.slice(0, ALL_TAB_ACTIONS))];
    return [...found, ...actions];
  };

  const result = await workbench.showPicker({ title: "Search Everywhere", tabs: TABS, onQuery: rows });
  if (result.kind !== "selected") return;
  if (result.id.startsWith(ACTION_PREFIX)) await commands.invoke(result.id.slice(ACTION_PREFIX.length));
  else await documents.open(result.id);
}
