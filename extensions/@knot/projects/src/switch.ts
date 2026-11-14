// Switching a window to a project. The source project's windows are suspended
// with their unsaved text retained, closed, and replaced by the destination's
// windows in one native commit.
import { documents, recovery, windows, workbench } from "knot";
import type { CaptureId, DocumentId, SessionEntryOutcome, WindowInfo, WindowTarget } from "knot";
import { syncBindings } from "./bindings.js";
import { describeWindow, documentsById, isReusable, needsCapture, presentedDocuments, viewCount, windowRoots } from "./layout.js";
import { displayName, projectById, sameRoots, suggestedName, type Project } from "./registry.js";
import { describe, errorCode, report } from "./report.js";
import { EMPTY_WINDOW, SESSION_SCHEMA, type Entry, type Session, type SessionWindow } from "./session.js";
import {
  boundWindows,
  isActive,
  matchingWindows,
  recordSelection,
  refreshTree,
  release,
  writeSession,
  type ProjectsState,
} from "./state.js";
import { reconcileCaptures, trackProject } from "./tracking.js";

const SWITCH_ATTEMPTS = 3;

/** Describes a restore outcome worth reporting, or null for a restored entry. */
export function problem(outcome: SessionEntryOutcome, entry: Entry) {
  const name = entry.uri !== undefined ? suggestedName(entry.uri) : "an untitled document";
  if (outcome.kind === "missing") return `${name} no longer exists`;
  if (outcome.kind === "failed") return `${name} could not be restored (${outcome.code})`;
  return null;
}

export type SwitchOptions = {
  /**
   * How the originating window takes part. By default it is replaced with
   * its project's windows, or reused when it can host the project. "adopt"
   * releases a window without roots and carries its tabs into the first
   * destination window; "keep" leaves the origin and its project untouched
   * and opens the destination in new windows.
   */
  origin?: "adopt" | "keep";
  /** A live document to present and focus in the first destination window. */
  present?: DocumentId;
};

/**
 * The first window with the carried entries appended, except those whose file
 * it already lists, which resolve to the same live document.
 */
function withCarried(windows: readonly SessionWindow[], carried: readonly Entry[]): SessionWindow[] {
  const [first, ...rest] = windows;
  const uris = new Set(first!.entries.map((entry) => entry.uri).filter((uri) => uri !== undefined));
  const added = carried.filter((entry) => entry.uri === undefined || !uris.has(entry.uri));
  return [{ ...first!, entries: [...first!.entries, ...added] }, ...rest];
}

/**
 * Switches the originating window to a project. The source project's windows
 * are suspended with their unsaved text retained before the native commit, so
 * a failed, cancelled, or interrupted switch never loses text.
 */
export async function switchTo(
  state: ProjectsState,
  project: Project,
  origin: WindowTarget,
  options: SwitchOptions = {},
) {
  if (state.unreadable.has(project.id)) {
    await report(`${displayName(project)} has a session record this version cannot read; it is kept unchanged`);
    return;
  }
  if (project.roots.length === 0) {
    await report(`${displayName(project)} has no root`);
    return;
  }
  const initialList = await windows.list();
  await syncBindings(state, initialList);
  if (isActive(state, project)) {
    const [bound] = boundWindows(state, project, initialList);
    if (bound !== undefined) {
      await windows.focus(bound.target);
      await recordSelection(state, project);
      await refreshTree(state);
      return;
    }
  }
  const destination = state.sessions.get(project.id)?.windows ?? [];
  const stored = destination.length > 0 ? destination : [EMPTY_WINDOW];
  // The source is fixed before the first attempt suspends its record.
  const sourceId = options.origin === undefined ? state.bindings.get(origin) : undefined;
  const source = sourceId === undefined ? null : projectById(state.registry, sourceId);
  for (let attempt = 1; ; attempt += 1) {
    const [list, docs] = await Promise.all([windows.list(), documents.list()]);
    const byId = documentsById(docs);
    const originWindow = list.find((window) => window.target === origin) ?? null;
    let sources: WindowInfo[] = [];
    let targets = stored;
    if (options.origin === "keep") {
      // No sources: the origin and its project stay as they are.
    } else if (options.origin === "adopt") {
      if (originWindow !== null && windowRoots(originWindow).length === 0) {
        sources = [originWindow];
        targets = withCarried(stored, (await describeWindow(originWindow, byId, new Map())).entries);
      }
    } else if (source !== null && originWindow !== null && sameRoots(windowRoots(originWindow), source.roots)) {
      sources = [originWindow, ...boundWindows(state, source, list).filter((window) => window !== originWindow)];
    } else if (originWindow !== null && (await isReusable(originWindow, byId, project.roots))) {
      sources = [originWindow];
    }
    const presented = presentedDocuments(sources);
    const captures = new Map<DocumentId, CaptureId>();
    try {
      // Carried documents stay live in the destination and need no capture.
      for (const id of options.origin === "adopt" ? [] : presented) {
        const document = byId.get(id);
        if (document === undefined || viewCount(sources, id) < viewCount(list, id)) continue;
        if (await needsCapture(document)) captures.set(id, (await recovery.retain(id)).id);
      }
      if (source !== null) {
        await writeSession(state, source.id, {
          schema: SESSION_SCHEMA,
          state: "suspended",
          windows: await Promise.all(sources.map((window) => describeWindow(window, byId, captures))),
        });
      }
    } catch (error) {
      await release(captures.values());
      await report(`cannot switch to ${displayName(project)}: ${describe(error)}`);
      return;
    }
    let result;
    try {
      result = await workbench.replaceWindows({
        suspend: presented.map((id) => ({ document: id, capture: captures.get(id) ?? null })),
        windows: targets.map((window) => ({
          roots: project.roots,
          entries: window.entries,
          selected: window.selected,
          layout: window.layout,
        })),
        release: sources.map((window) => window.target),
        focus: 0,
      });
    } catch (error) {
      if (errorCode(error) === "SessionConflict" && attempt < SWITCH_ATTEMPTS) continue;
      // The source is unchanged and still active; recording it again drops the
      // captures retained for this attempt.
      if (source !== null) {
        const [list, docs] = await Promise.all([windows.list(), documents.list()]);
        const suspended = state.sessions.get(source.id);
        if (suspended !== undefined) state.sessions.set(source.id, { ...suspended, state: "active" });
        await trackProject(state, source, list, documentsById(docs));
      }
      await report(`cannot switch to ${displayName(project)}: ${describe(error)}`);
      return;
    }
    const [after, afterDocs] = await Promise.all([windows.list(), documents.list()]);
    const byIdAfter = documentsById(afterDocs);
    const next: Session = {
      schema: SESSION_SCHEMA,
      state: "active",
      windows: await Promise.all(
        matchingWindows(project, after).map((window) => describeWindow(window, byIdAfter, new Map())),
      ),
    };
    // Captures restored without durable automatic protection stay retained
    // until a later reconciliation, after automatic recovery has caught up.
    await writeSession(state, project.id, next, { keepCaptures: !result.protected });
    await syncBindings(state, after);
    const opened = result.windows[0]?.window;
    if (result.protected) {
      await reconcileCaptures(state);
    } else {
      await report(`${displayName(project)} opened, but its recovered text is not yet protected`, opened);
    }
    await recordSelection(state, project);
    await refreshTree(state);
    if (options.present !== undefined && opened !== undefined) {
      try {
        await documents.present(options.present, { target: opened });
      } catch {
        // The project is open; the document stays where it was.
      }
    }
    const problems = [];
    for (const [index, window] of result.windows.entries()) {
      const entries = targets[index]?.entries ?? [];
      for (const [position, outcome] of window.entries.entries()) {
        const described = problem(outcome, entries[position] ?? {});
        if (described !== null) problems.push(described);
      }
    }
    if (problems.length > 0) await report(`Opened ${displayName(project)}. ${problems.join("; ")}.`, opened);
    return;
  }
}
