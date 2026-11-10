// Bounds the unnamed projects the registry remembers.
import { settings, storage } from "knot";
import { withoutProject, type Project } from "./registry.js";
import { capturesOf, sessionKey } from "./session.js";
import { commit, isActive, type ProjectsState } from "./state.js";

declare module "knot" {
  interface Settings {
    /** How many unnamed projects the registry remembers. */
    "projects.rememberedLimit": number;
  }
}

export const REMEMBERED_LIMIT = "projects.rememberedLimit";
const DEFAULT_LIMIT = 30;

export async function defineSettings() {
  await settings.define({
    key: REMEMBERED_LIMIT,
    type: "integer",
    default: DEFAULT_LIMIT,
    description:
      "How many unnamed projects to remember. The least recently opened are forgotten first; named and active projects, and projects holding unsaved text, are kept.",
  });
}

function lastOpened(project: Project) {
  return project.lastOpenedAt ?? project.createdAt ?? 0;
}

/** Whether forgetting the project could lose a session or unsaved text. */
function protectedFromPruning(state: ProjectsState, project: Project) {
  return (
    project.name !== null ||
    isActive(state, project) ||
    state.unreadable.has(project.id) ||
    capturesOf(state.sessions.get(project.id)).size > 0
  );
}

/** Forgets the least recently opened unprotected projects beyond the limit. */
export async function prune(state: ProjectsState) {
  const limit = Math.max(0, (await settings.get(REMEMBERED_LIMIT)) ?? DEFAULT_LIMIT);
  const unnamed = state.registry.projects
    .filter((project) => project.name === null)
    .sort((left, right) => lastOpened(left) - lastOpened(right));
  let excess = unnamed.length - limit;
  const forgotten = [];
  for (const project of unnamed) {
    if (excess <= 0) break;
    if (protectedFromPruning(state, project)) continue;
    forgotten.push(project);
    excess -= 1;
  }
  if (forgotten.length === 0) return;
  let next = state.registry;
  for (const project of forgotten) next = withoutProject(next, project.id);
  await commit(state, next);
  for (const project of forgotten) {
    await storage.delete(sessionKey(project.id));
    state.sessions.delete(project.id);
  }
}
