// Go to File: type part of a path to list the matching files of the window's
// roots; Enter opens the highlighted one in the pane the command ran from.
import { commands, documents, files, workbench, workspaces } from "knot";
import type { CommandContext, PickerRow } from "knot";
import { rootName, rowFor } from "./rows.js";

export const COMMAND = "workspace.go-to-file";

async function goToFile({ target }: CommandContext) {
  const workspace = await workspaces.get(target.windowTarget);
  const roots = workspace?.roots ?? [];
  const labelled = roots.length > 1;

  const search = async (query: string): Promise<PickerRow[]> => {
    if (roots.length === 0) return [];
    const { results } = await files.search(roots.map((root) => root.uri), query);
    return results.map((hit) => rowFor(hit, labelled ? rootName(hit.directory) : null));
  };

  const result = await workbench.showPicker({ title: "Go to File", onQuery: search });
  if (result.kind === "selected") await documents.open(result.id);
}

export async function start() {
  await commands.register(COMMAND, goToFile, {
    title: "Go to File…",
    description: "Open a file by typing part of its path.",
  });
}
