// The "knot" module seen by extension code under test. Tests control it
// through "knot/testing".
import { api, homeUri } from "./host.js";

// `homeUri` re-exports the fake's binding rather than a snapshot, so tests
// that set it after extension modules load still change what extensions see.
export { homeUri };

export const {
  buffers,
  commands,
  completion,
  completionContract,
  decorations,
  diff,
  documents,
  editor,
  files,
  history,
  icons,
  keymaps,
  languages,
  menus,
  pasteContract,
  platform,
  processes,
  recovery,
  settings,
  storage,
  terminals,
  themes,
  windows,
  workbench,
  workspaces,
} = api;

// Fails the build when the fake's exports and the public declaration diverge
// in either direction, as the real facade's do.
import type * as Declared from "knot";
// (Values of a symbol type are the declaration's private brands.)
type ExportNames<T> = { [K in keyof T]: T[K] extends symbol ? never : K }[keyof T];
type Matches<A, B> = [A] extends [B] ? ([B] extends [A] ? true : never) : never;
const exportsMatch: Matches<ExportNames<typeof import("./knot")>, ExportNames<typeof Declared>> = true;
void exportsMatch;
