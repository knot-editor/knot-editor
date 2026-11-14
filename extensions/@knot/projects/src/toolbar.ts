// The toolbar's project widget: the name of the project a window is bound to,
// as that window's own value. A click runs Switch Project. A window bound to
// no project shows nothing.
import { icons, windows, workbench } from "knot";
import type { StatusItemOptions } from "knot";
import { displayName, projectById } from "./registry.js";
import type { ProjectsState } from "./state.js";

export const PROJECT_ICON = "toolbar.project";
export const SWITCH_COMMAND = "projects.select";

// The shared value shows nothing, so a window shows the widget only where it
// sets its own.
const HIDDEN: StatusItemOptions = { text: "", placement: "toolbar" };

const SHOWN: StatusItemOptions = {
  text: "",
  icon: PROJECT_ICON,
  tooltip: "Switch Project",
  command: { name: SWITCH_COMMAND },
  placement: "toolbar",
};

export async function registerToolbar(state: ProjectsState) {
  await icons.define(PROJECT_ICON, { lucide: "folder" });
  const item = await workbench.registerStatusItem("project", HIDDEN);
  state.toolbar = {
    async sync() {
      for (const window of await windows.list()) {
        const id = state.bindings.get(window.target);
        const project = id === undefined ? null : projectById(state.registry, id);
        await item.update(
          project === null ? null : { ...SHOWN, text: displayName(project) },
          { window: window.target },
        );
      }
    },
  };
  await state.toolbar.sync();
}
