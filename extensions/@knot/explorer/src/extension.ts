// The bundled file explorer. Every root of a window is a top-level row; folders
// load when expanded and reload when a recursive watch of their root reports a
// change, so the tree needs no private access to the application.
import { icons, settings, workbench } from "knot";
import { registerCommands } from "./commands.js";
import { followActive } from "./reveal.js";
import { defineSettings, SHOW_IGNORED } from "./settings.js";
import { createState } from "./state.js";
import { registerTree } from "./tree.js";
import { refreshAll, syncWatches } from "./watch.js";

export async function start() {
  await defineSettings();
  await icons.define("tool.builtin.explorer", { lucide: "folder-tree" });
  const state = createState();
  await registerTree(state);
  await registerCommands(state);
  await settings.observe(SHOW_IGNORED, () => refreshAll(state));
  await workbench.observe(async (event) => {
    switch (event.type) {
      case "activeTargetChanged":
        if (event.activeTarget !== null) await followActive(state, event.activeTarget);
        break;
      case "windowClosed":
        state.loaded.delete(event.window);
        await syncWatches(state);
        break;
      case "workspaceChanged":
        state.loaded.delete(event.workspace.window);
        await state.view?.invalidate(null, event.workspace.window).catch(() => {});
        await syncWatches(state);
        break;
      case "windowOpened":
      case "launched":
        await syncWatches(state);
        break;
    }
  });
  await syncWatches(state);
}
