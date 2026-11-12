// The "knot" module seen by extension code under test. Tests control it
// through "knot/testing".
import { api, homeUri } from "./host.js";

// `homeUri` re-exports the fake's binding rather than a snapshot, so tests
// that set it after extension modules load still change what extensions see.
export { homeUri };

export const {
  commands,
  completion,
  documents,
  editor,
  files,
  history,
  icons,
  keymaps,
  languages,
  platform,
  recovery,
  settings,
  storage,
  themes,
  windows,
  workbench,
  workspaces,
} = api;
