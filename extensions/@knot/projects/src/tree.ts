// The Projects tool: a contributed tree listing every project.
import { icons, workbench } from "knot";
import type { TreeItem, WindowTarget } from "knot";
import { displayName, rootsPath } from "./registry.js";
import type { ProjectsState } from "./state.js";

export const VIEW_ID = "builtin.projects";

const ACTION_ICONS = {
  "action.rename": "pencil",
  "action.remove": "trash",
  "action.new-project": "plus",
  "action.new-window": "app-window",
  "action.refresh": "refresh-cw",
} as const;

/** The tree rows for one window: every project, marking the window's own. */
export function projectItems(state: ProjectsState, window: WindowTarget): TreeItem[] {
  if (state.loadFailure !== null) {
    return [
      {
        id: "storage-error",
        label: "Projects storage unavailable",
        description: state.loadFailure,
        collapsibleState: "none",
        badge: "error",
      },
    ];
  }
  return state.registry.projects.map((project) => {
    const active = state.bindings.get(window) === project.id;
    const broken = state.unreadable.has(project.id);
    return {
      id: project.id,
      label: displayName(project),
      description: broken ? "session record unreadable" : null,
      detail: rootsPath(project),
      icon: "folder",
      collapsibleState: "none",
      statusText: broken ? "unreadable" : active ? "active" : null,
      badge: broken ? "error" : active ? "success" : null,
      command: "projects.select",
      actions: [
        { label: "Rename", icon: "action.rename", command: "projects.rename", arguments: { id: project.id } },
        { label: "Remove", icon: "action.remove", command: "projects.remove", arguments: { id: project.id } },
      ],
    };
  });
}

export async function registerTree(state: ProjectsState) {
  await Promise.all(
    Object.entries(ACTION_ICONS).map(([name, lucide]) => icons.define(name, { lucide })),
  );
  state.view = await workbench.registerView({
    id: VIEW_ID,
    title: "Projects",
    content: {
      kind: "tree",
      provider: {
        async getChildren(parentId, context) {
          return parentId === null ? projectItems(state, context.window) : [];
        },
      },
    },
    placement: { location: "left", visible: false },
    actions: [
      { label: "New Project", icon: "action.new-project", command: "projects.create" },
      { label: "New Window", icon: "action.new-window", command: "projects.new-window" },
      { label: "Refresh", icon: "action.refresh", command: "projects.refresh" },
    ],
  });
}
