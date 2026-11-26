// The explorer's rows: identities, ordering, and the tree items of a folder.
import { platform } from "knot";
import type { FileEntry, TreeAction, TreeItem, WorkspaceRoot } from "knot";

export const VIEW_ID = "builtin.explorer";
export const ACTIVATE = "explorer.activate";
export const REVEAL_ACTIVE = "explorer.reveal-active-file";
export const OPEN_FOLDER = "explorer.open-folder";
export const OPEN_FOLDER_ROW = "explorer.open-folder-row";
export const NEW_FILE = "explorer.new-file";
export const NEW_FOLDER = "explorer.new-folder";
export const RENAME = "explorer.rename";
export const DELETE = "explorer.delete";
export const DELETE_NOW = "explorer.deleteNow";
export const CUT = "explorer.cut";
export const COPY = "explorer.copy";
export const PASTE = "explorer.paste";
export const DROP = "explorer.drop";

// The native commands on a file's path and location.
const FILE_COPY_PATH = "file.copy-path";
const FILE_COPY_RELATIVE_PATH = "file.copy-relative-path";
const FILE_COPY_NAME = "file.copy-name";
const FILE_REVEAL = "file.reveal";

/** The name of the platform's file manager action. */
export const REVEAL_LABEL =
  platform === "macos" ? "Reveal in Finder" : platform === "windows" ? "Reveal in File Explorer" : "Reveal in File Manager";

/** What a row's context menu offers: new items in a folder, clipboard and
 * rename and delete for anything but a root, and copied paths. */
function menuActions(id: string, uri: string, folder: boolean, root: boolean): TreeAction[] {
  const action = (label: string, command: string, args: Record<string, string> = { id }): TreeAction => ({
    label,
    command,
    arguments: args,
    menuOnly: true,
  });
  const file = (label: string, command: string) => action(label, command, { uri });
  return [
    ...(folder ? [action("New File…", NEW_FILE), action("New Folder…", NEW_FOLDER)] : []),
    ...(root ? [] : [action("Cut", CUT), action("Copy", COPY)]),
    action("Paste", PASTE),
    file("Copy Path", FILE_COPY_PATH),
    file("Copy Relative Path", FILE_COPY_RELATIVE_PATH),
    file("Copy File Name", FILE_COPY_NAME),
    ...(root ? [] : [action("Rename…", RENAME), action("Delete…", DELETE)]),
    file(REVEAL_LABEL, FILE_REVEAL),
  ];
}

/**
 * A row is identified by the root it is shown under and its URI, so a folder
 * inside two overlapping roots has independent rows.
 */
export function rowId(root: string, uri: string): string {
  return `${root}|${uri}`;
}

export function parseRowId(id: string): { readonly root: string; readonly uri: string } {
  const split = id.indexOf("|");
  return { root: id.slice(0, split), uri: id.slice(split + 1) };
}

/** The URI without trailing slashes. */
export function trimmed(uri: string): string {
  return uri.replace(/\/+$/, "");
}

/** The directory URI holding `uri`. */
export function parentOf(uri: string): string {
  const base = trimmed(uri);
  return base.slice(0, base.lastIndexOf("/"));
}

/** The decoded last segment of a URI. */
export function nameOf(uri: string): string {
  const base = trimmed(uri);
  const name = base.slice(base.lastIndexOf("/") + 1);
  try {
    return decodeURIComponent(name);
  } catch {
    return name;
  }
}

/** Folders before files, then by name ignoring case, with numbers in natural order. */
export function compareEntries(left: FileEntry, right: FileEntry): number {
  if (left.kind !== right.kind) return left.kind === "directory" ? -1 : 1;
  return left.name.localeCompare(right.name, undefined, { numeric: true, sensitivity: "base" });
}

export function entryItem(root: string, entry: FileEntry): TreeItem {
  const directory = entry.kind === "directory";
  const id = rowId(root, entry.uri);
  return {
    id,
    actions: menuActions(id, entry.uri, directory, false),
    label: entry.name,
    icon: directory ? { folder: entry.name } : { file: entry.name },
    collapsibleState: directory ? "collapsed" : "none",
    muted: entry.ignored,
    draggable: true,
    dropTarget: true,
    resource: entry.uri,
    ...(directory ? {} : { command: ACTIVATE, arguments: { uri: entry.uri } }),
  };
}

/** A root's row, expanded, labelled by its folder name and described by its path. */
export function rootItem(root: WorkspaceRoot): TreeItem {
  const name = nameOf(root.uri);
  const id = rowId(root.id, root.uri);
  return {
    id,
    actions: menuActions(id, root.uri, true, true),
    label: name === "" ? root.uri : name,
    description: displayPath(root.uri),
    icon: { folder: name },
    collapsibleState: "expanded",
    dropTarget: true,
    resource: root.uri,
  };
}

/** The row of a window without roots. */
export function openFolderItem(): TreeItem {
  return {
    id: OPEN_FOLDER_ROW,
    label: "Open Folder…",
    icon: "folder",
    collapsibleState: "none",
    command: OPEN_FOLDER,
  };
}

/** The path of a file URI, else the URI. */
export function displayPath(uri: string): string {
  const match = /^file:\/\/[^/]*(\/.*)$/.exec(trimmed(uri));
  if (match === null) return uri;
  try {
    return decodeURIComponent(match[1]!);
  } catch {
    return match[1]!;
  }
}

/**
 * The IDs from `root`'s row down to the row of `uri`, or null when `uri` is not
 * below the root.
 */
export function idPath(root: WorkspaceRoot, uri: string): string[] | null {
  const base = trimmed(root.uri);
  if (uri !== base && !uri.startsWith(`${base}/`)) return null;
  const path = [rowId(root.id, root.uri)];
  let current = base;
  for (const segment of uri.slice(base.length).split("/").filter((part) => part !== "")) {
    current = `${current}/${segment}`;
    path.push(rowId(root.id, current));
  }
  return path;
}

/** The deepest root containing `uri`. */
export function rootOf(roots: readonly WorkspaceRoot[], uri: string): WorkspaceRoot | null {
  let found: WorkspaceRoot | null = null;
  for (const root of roots) {
    const base = trimmed(root.uri);
    if (uri !== base && !uri.startsWith(`${base}/`)) continue;
    if (found === null || base.length > trimmed(found.uri).length) found = root;
  }
  return found;
}
