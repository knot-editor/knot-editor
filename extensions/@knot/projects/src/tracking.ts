// Live tracking of active sessions. Commands and listeners are serial roots
// of this package, so tracking never records the intermediate state of a
// switch. Native hot exit restores product windows at startup; tracking waits
// for launch so restored windows rebind by root set first.
import { documents, recovery, windows } from "knot";
import type { ApplicationEvent, CaptureId, DocumentId, DocumentInfo, WindowInfo } from "knot";
import { syncBindings } from "./bindings.js";
import { describeWindow, documentsById, windowRoots } from "./layout.js";
import { newProjectId, projectWithRoots, withProject, type Project } from "./registry.js";
import { switchTo } from "./switch.js";
import { prune } from "./prune.js";
import { describe } from "./report.js";
import {
  SESSION_SCHEMA,
  capturesOf,
  sameSession,
  suspendedWithoutCaptures,
  type Session,
} from "./session.js";
import {
  boundWindows,
  commit,
  isActive,
  refreshTree,
  release,
  writeSession,
  type ProjectsState,
} from "./state.js";

/**
 * Records an active project's current membership. Losing its last window
 * suspends it with its last membership and no captures.
 */
export async function trackProject(
  state: ProjectsState,
  project: Project,
  list: readonly WindowInfo[],
  byId: Map<DocumentId, DocumentInfo>,
) {
  const session = state.sessions.get(project.id);
  const bound = boundWindows(state, project, list);
  const next: Session =
    bound.length === 0
      ? suspendedWithoutCaptures(session)
      : {
          schema: SESSION_SCHEMA,
          state: "active",
          windows: await Promise.all(bound.map((window) => describeWindow(window, byId, new Map()))),
        };
  if (session === undefined || !sameSession(session, next)) await writeSession(state, project.id, next);
}

/**
 * Remembers each root set that unbound windows show and no project has: an
 * unnamed active project is created and the windows bind to it. Windows
 * without roots are never remembered.
 */
async function rememberUnbound(state: ProjectsState, list: readonly WindowInfo[]) {
  let created = false;
  for (const window of list) {
    const roots = windowRoots(window);
    if (roots.length === 0 || state.bindings.has(window.target)) continue;
    if (projectWithRoots(state.registry, roots) !== null) continue;
    const now = Date.now();
    const project = { id: newProjectId(), name: null, roots, createdAt: now, lastOpenedAt: now };
    await writeSession(state, project.id, { schema: SESSION_SCHEMA, state: "active", windows: [] });
    await commit(state, withProject(state.registry, project));
    created = true;
  }
  if (!created) return;
  await prune(state);
  await syncBindings(state, list);
}

/**
 * Restores a suspended project in each unbound window showing exactly its
 * roots. The switch replaces the window when it holds nothing beyond an empty
 * document and otherwise opens the project in a new window.
 */
async function restoreUnbound(state: ProjectsState, list: readonly WindowInfo[]) {
  for (const window of list) {
    if (state.bindings.has(window.target) || state.restoreTried.has(window.target)) continue;
    const project = projectWithRoots(state.registry, windowRoots(window));
    if (project === null || isActive(state, project) || state.unreadable.has(project.id)) continue;
    state.restoreTried.add(window.target);
    await switchTo(state, project, window.target);
  }
}

/** Live tracking: rewrite active records that changed. Suspended records are never written. */
async function track(state: ProjectsState) {
  if (!state.launched) return;
  try {
    await syncBindings(state, await windows.list());
    await restoreUnbound(state, await windows.list());
    await rememberUnbound(state, await windows.list());
    const [list, docs] = await Promise.all([windows.list(), documents.list()]);
    await syncBindings(state, list);
    const byId = documentsById(docs);
    for (const project of state.registry.projects.filter((project) => isActive(state, project))) {
      await trackProject(state, project, list, byId);
    }
  } catch {
    // Tracking is retried on the next change; quit stops storage first, so a
    // shutdown never records windows closing.
  }
}

/**
 * Releases this package's captures that no session record references. An
 * unreadable record may reference any capture, so nothing is released while
 * one exists.
 */
export async function reconcileCaptures(state: ProjectsState) {
  if (state.unreadable.size > 0) return;
  const referenced = new Set<CaptureId>();
  for (const session of state.sessions.values()) {
    for (const capture of capturesOf(session)) referenced.add(capture);
  }
  const listed = await recovery.list();
  await release(listed.map((capture) => capture.id).filter((id) => !referenced.has(id)));
}

/**
 * Marks every active session without a bound window as suspended, keeping
 * its entries. Only state is written; nothing is released.
 */
async function suspendUnbound(state: ProjectsState) {
  const list = await windows.list();
  await syncBindings(state, list);
  for (const project of state.registry.projects) {
    const session = state.sessions.get(project.id);
    if (session?.state !== "active" || boundWindows(state, project, list).length > 0) continue;
    await writeSession(state, project.id, { ...session, state: "suspended" }, { keepCaptures: true });
  }
}

/**
 * Once startup windows exist, rebinds restored windows by root set, suspends
 * the remaining active sessions, and remembers the other windows' folders.
 */
export async function afterLaunch(state: ProjectsState) {
  state.launched = true;
  try {
    await suspendUnbound(state);
    await restoreUnbound(state, await windows.list());
    await rememberUnbound(state, await windows.list());
    await track(state);
    await reconcileCaptures(state);
  } catch (error) {
    state.loadFailure = state.loadFailure ?? describe(error);
  }
  await refreshTree(state);
}

/** Whether a document's membership-relevant metadata changed since last seen. */
function membershipChanged(state: ProjectsState, document: DocumentInfo) {
  const key = `${document.kind}|${document.resourceUri}|${document.dirty}`;
  if (state.tracked.get(document.id) === key) return false;
  state.tracked.set(document.id, key);
  return true;
}

export async function onApplicationEvent(state: ProjectsState, event: ApplicationEvent) {
  if (event.type === "launched") {
    await afterLaunch(state);
    return;
  }
  if (event.type === "documentChanged" && !membershipChanged(state, event.document)) return;
  if (event.type === "documentOpened") membershipChanged(state, event.document);
  if (event.type === "documentClosed") state.tracked.delete(event.document);
  await track(state);
  if (event.type === "workspaceChanged" || event.type === "windowClosed") await refreshTree(state);
}
