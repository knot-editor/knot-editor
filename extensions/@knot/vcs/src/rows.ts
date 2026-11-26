// The Changes view's rows: identities, ordering, and the tree items of a
// repository, a group, and a folder, in the tree and list layouts.
import { platform } from "knot";
import type { JsonValue, TreeAction, TreeItem } from "knot";
import { contains, type Repository } from "./repositories.js";
import type { Group, GroupAction } from "./status.js";

export const VIEW_ID = "vcs.changes";
export const SHOW_FILE_CHANGES = "vcs.showFileChanges";
export const OPEN_FILE = "vcs.changes.openFile";
export const NO_REPOSITORY = "vcs.changes.none";
export const NO_CHANGES = "vcs.changes.clean";

export type Layout = "tree" | "list";

/** What a row stands for; the row's ID is its JSON. */
export type RowId =
  | readonly ["repository", backend: string, root: string]
  | readonly ["group", backend: string, root: string, group: string]
  | readonly ["folder", backend: string, root: string, group: string, uri: string]
  | readonly ["file", backend: string, root: string, group: string, uri: string];

export const idOf = (id: RowId): string => JSON.stringify(id);

export function parseId(id: string): RowId | null {
  try {
    const parsed: unknown = JSON.parse(id);
    return Array.isArray(parsed) ? (parsed as unknown as RowId) : null;
  } catch {
    return null;
  }
}

// The native commands on a file's path and location.
const NATIVE = [
  ["Copy Path", "file.copy-path"],
  ["Copy Relative Path", "file.copy-relative-path"],
  ["Copy File Name", "file.copy-name"],
] as const;
const REVEAL = "file.reveal";
const REVEAL_LABEL =
  platform === "macos" ? "Reveal in Finder" : platform === "windows" ? "Reveal in File Explorer" : "Reveal in File Manager";

const trimmed = (uri: string) => uri.replace(/\/+$/, "");

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

/** The path of `uri` below `root`, decoded, without a trailing slash. */
function relative(root: string, uri: string): string {
  const base = trimmed(root);
  const path = trimmed(uri).slice(base.length + 1);
  try {
    return decodeURIComponent(path);
  } catch {
    return path;
  }
}

const compare = (left: string, right: string) => left.localeCompare(right, undefined, { numeric: true, sensitivity: "base" });

/** The backend's actions that appear on a row of `on`, run with the repository, group, and path. */
function backendActions(
  actions: readonly GroupAction[],
  on: "group" | "folder" | "file",
  repository: Repository,
  group: Group,
  path: string,
): TreeAction[] {
  return actions
    .filter((action) => action.on.includes(on))
    .map((action) => ({
      label: action.label,
      command: action.command,
      arguments: { repository: repository.root, group: group.id, path },
      ...(action.icon === undefined ? {} : { icon: action.icon }),
      ...(action.menuOnly === undefined ? {} : { menuOnly: action.menuOnly }),
      ...(action.showOnHover === undefined ? {} : { showOnHover: action.showOnHover }),
    }));
}

const native = (uri: string): TreeAction[] =>
  [...NATIVE, [REVEAL_LABEL, REVEAL] as const].map(([label, command]) => ({
    label,
    command,
    arguments: { uri },
    menuOnly: true,
  }));

/** The arguments of the command that opens a file's changes. */
export const changesArguments = (repository: Repository, group: string, file: string): JsonValue => ({
  backend: repository.backend,
  repository: repository.root,
  group,
  file,
});

export function repositoryItem(repository: Repository): TreeItem {
  const name = nameOf(repository.root);
  return {
    id: idOf(["repository", repository.backend, repository.root]),
    label: name === "" ? repository.root : name,
    icon: { folder: name },
    collapsibleState: "expanded",
    resource: repository.root,
  };
}

export function groupItem(repository: Repository, group: Group): TreeItem {
  return {
    id: idOf(["group", repository.backend, repository.root, group.id]),
    label: group.label,
    statusText: String(group.files.length),
    collapsibleState: "expanded",
    actions: backendActions(group.actions, "group", repository, group, repository.root),
  };
}

export const noRepositoryItem = (): TreeItem => ({
  id: NO_REPOSITORY,
  label: "No repository",
  collapsibleState: "none",
  muted: true,
});

export const noChangesItem = (): TreeItem => ({
  id: NO_CHANGES,
  label: "No changes",
  collapsibleState: "none",
  muted: true,
});

function fileItem(repository: Repository, group: Group, uri: string, detail: string | null): TreeItem {
  const name = nameOf(uri);
  const id = idOf(["file", repository.backend, repository.root, group.id, uri]);
  return {
    id,
    label: name,
    icon: { file: name },
    collapsibleState: "none",
    resource: uri,
    ...(detail === null || detail === "" ? {} : { detail }),
    command: SHOW_FILE_CHANGES,
    arguments: changesArguments(repository, group.id, uri),
    actions: [
      ...backendActions(group.actions, "file", repository, group, uri),
      { label: "Show Diff", command: SHOW_FILE_CHANGES, arguments: changesArguments(repository, group.id, uri), menuOnly: true },
      { label: "Open File", command: OPEN_FILE, arguments: { uri }, menuOnly: true },
      ...native(uri),
    ],
  };
}

/** A folder row; `leaf` marks one the backend lists as a single entry, which has no children. */
function folderItem(repository: Repository, group: Group, uri: string, leaf: boolean): TreeItem {
  const name = nameOf(uri);
  const folder = trimmed(uri);
  return {
    id: idOf(["folder", repository.backend, repository.root, group.id, folder]),
    label: name,
    icon: { folder: name },
    collapsibleState: leaf ? "none" : "expanded",
    resource: folder,
    actions: [...backendActions(group.actions, "folder", repository, group, folder), ...native(folder)],
  };
}

/** The rows of `group` that are directly below the folder `base`: folders first, then files. */
export function folderChildren(repository: Repository, group: Group, base: string): TreeItem[] {
  const parent = trimmed(base);
  const folders = new Map<string, boolean>();
  const files: string[] = [];
  for (const uri of group.files) {
    if (!contains(parent, trimmed(uri)) || trimmed(uri) === parent) continue;
    const rest = uri.slice(parent.length + 1);
    const slash = rest.indexOf("/");
    if (slash < 0) {
      files.push(uri);
      continue;
    }
    // A slash only at the end names a folder the backend lists as one entry.
    const folder = `${parent}/${rest.slice(0, slash)}`;
    folders.set(folder, slash === rest.length - 1 && folders.get(folder) !== false);
  }
  const rows: TreeItem[] = [...folders]
    .sort(([left], [right]) => compare(nameOf(left), nameOf(right)))
    .map(([uri, leaf]) => folderItem(repository, group, uri, leaf));
  for (const uri of files.sort((left, right) => compare(nameOf(left), nameOf(right)))) {
    rows.push(fileItem(repository, group, uri, null));
  }
  return rows;
}

/** Every file of `group` as a flat list ordered by path, each with its directory as detail. */
export function listChildren(repository: Repository, group: Group): TreeItem[] {
  return [...group.files]
    .sort((left, right) => compare(relative(repository.root, left), relative(repository.root, right)))
    .map((uri) => {
      const path = relative(repository.root, uri);
      const cut = path.lastIndexOf("/");
      return fileItem(repository, group, uri, cut < 0 ? null : path.slice(0, cut));
    });
}

/** The rows below `id` for a layout, or null for an ID that names no parent. */
export function childrenOf(id: RowId, group: Group | undefined, layout: Layout): TreeItem[] | null {
  if (id[0] === "group") {
    if (group === undefined) return [];
    const repository = { backend: id[1], root: id[2] };
    return layout === "list" ? listChildren(repository, group) : folderChildren(repository, group, repository.root);
  }
  if (id[0] === "folder") {
    if (group === undefined) return [];
    return folderChildren({ backend: id[1], root: id[2] }, group, id[4]);
  }
  return null;
}
