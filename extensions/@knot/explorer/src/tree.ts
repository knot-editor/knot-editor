import { files, icons, settings, workbench, workspaces } from "knot";
import { NEW_FILE, NEW_FOLDER } from "./actions.js";
import { entryItem, compareEntries, openFolderItem, parseRowId, rootItem, VIEW_ID } from "./rows.js";
import { SHOW_IGNORED } from "./settings.js";
import { loadedIn, type ExplorerState } from "./state.js";

/** Registers the explorer tree, shown in the left dock. */
export async function registerTree(state: ExplorerState) {
  await icons.define("explorer.new-file", { lucide: "file-plus" });
  await icons.define("explorer.new-folder", { lucide: "folder-plus" });
  state.view = await workbench.registerView({
    id: VIEW_ID,
    title: "Explorer",
    content: {
      kind: "tree",
      provider: {
        async getChildren(parentId, context) {
          if (parentId === null) {
            const roots = (await workspaces.get(context.window))?.roots ?? [];
            return roots.length === 0 ? [openFolderItem()] : roots.map(rootItem);
          }
          const { root, uri } = parseRowId(parentId);
          loadedIn(state, context.window).add(parentId);
          const showIgnored = (await settings.get(SHOW_IGNORED)) ?? true;
          const entries = (await files.list(uri)).filter((entry) => showIgnored || !entry.ignored);
          return entries.sort(compareEntries).map((entry) => entryItem(root, entry));
        },
      },
    },
    placement: { location: "left", order: 0 },
    actions: [
      { label: "New File", icon: "explorer.new-file", command: NEW_FILE },
      { label: "New Folder", icon: "explorer.new-folder", command: NEW_FOLDER },
    ],
  });
}
