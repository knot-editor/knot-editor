import type { Disposable, ViewRegistration, WindowTarget } from "knot";

export interface ExplorerState {
  view: ViewRegistration | null;
  /** The folder rows each window has loaded, by row ID. */
  readonly loaded: Map<WindowTarget, Set<string>>;
  /** The recursive watch of each root URI that some window shows. */
  readonly watches: Map<string, Disposable>;
}

export function createState(): ExplorerState {
  return { view: null, loaded: new Map(), watches: new Map() };
}

export function loadedIn(state: ExplorerState, window: WindowTarget): Set<string> {
  let ids = state.loaded.get(window);
  if (ids === undefined) {
    ids = new Set();
    state.loaded.set(window, ids);
  }
  return ids;
}
