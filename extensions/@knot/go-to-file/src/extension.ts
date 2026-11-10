// Go to File: type part of a path to list the matching files of the window's
// roots, recently active ones first; Enter opens the highlighted one in the
// pane the command ran from.
import { commands, documents, files, workbench, workspaces } from "knot";
import type { CommandContext, FileSearchHit, PickerRow } from "knot";
import { matchingRecent, recentFiles, recordActive, type RecentFile } from "./recent.js";
import { rootName, rowFor } from "./rows.js";

export const COMMAND = "workspace.go-to-file";

function hitFor(file: RecentFile, ranges: FileSearchHit["ranges"]): FileSearchHit {
  return {
    uri: file.uri,
    directory: file.root,
    path: file.path,
    name: file.path.slice(file.path.lastIndexOf("/") + 1),
    ranges,
  };
}

async function goToFile({ target }: CommandContext) {
  const workspace = await workspaces.get(target.windowTarget);
  const roots = workspace?.roots ?? [];
  const labelled = roots.length > 1;

  const uris = roots.map((root) => root.uri);

  const rows = async (query: string): Promise<PickerRow[]> => {
    if (roots.length === 0) return [];
    const recent = await recentFiles(target.windowTarget, uris);
    const first =
      query.trim() === ""
        ? recent.map((file) => ({ file, ranges: [] }))
        : await matchingRecent(recent, query);
    const seen = new Set(first.map(({ file }) => file.uri));
    const { results } = await files.search(uris, query);
    const hits = [
      ...first.map(({ file, ranges }) => hitFor(file, ranges)),
      ...results.filter((hit) => !seen.has(hit.uri)),
    ];
    return hits.map((hit) => rowFor(hit, labelled ? rootName(hit.directory) : null));
  };

  const result = await workbench.showPicker({ title: "Go to File", onQuery: rows });
  if (result.kind === "selected") await documents.open(result.id);
}

export async function start() {
  await workbench.observe(recordActive);
  await commands.register(COMMAND, goToFile, {
    title: "Go to File…",
    description: "Open a file by typing part of its path.",
  });
}
