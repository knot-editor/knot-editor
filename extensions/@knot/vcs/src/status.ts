// File status: each known repository's changed files are pulled from its
// backend and painted on resources as one decoration source. A file shows its
// status as a color, a letter badge, and a tooltip; each folder above a change
// takes the color of its most important change, up to the repository root.
import { decorations, themes } from "knot";
import type { DecorationSource, ResourceDecoration } from "knot";
import { contains, directoryOf, isGone, type Repository } from "./repositories.js";

export type Status = "added" | "modified" | "deleted" | "renamed" | "untracked" | "conflicted";
export type Entry = { readonly file: string; readonly status: Status };

/** A repository and the files its backend reported as changed. */
export type Snapshot = { readonly repository: Repository; readonly entries: readonly Entry[] };

/** Most important first: the status a folder takes when its changes differ. */
const PRIORITY: readonly Status[] = ["conflicted", "deleted", "modified", "renamed", "added", "untracked"];

const LABELS: Record<Status, { badge: string; tooltip: string }> = {
  added: { badge: "A", tooltip: "Added" },
  modified: { badge: "M", tooltip: "Modified" },
  deleted: { badge: "D", tooltip: "Deleted" },
  renamed: { badge: "R", tooltip: "Renamed" },
  untracked: { badge: "U", tooltip: "Untracked" },
  conflicted: { badge: "C", tooltip: "Conflicted" },
};

const colorOf = (status: Status) => `vcs.${status}`;

/** Defines the color keys; those without a counterpart in the diff colors pick their own. */
export async function defineColors() {
  const colors: Record<Status, { description: string; dark: string; light: string }> = {
    added: { description: "A file added to version control.", dark: "diff.added", light: "diff.added" },
    modified: { description: "A file that differs from the version control baseline.", dark: "diff.modified", light: "diff.modified" },
    deleted: { description: "A file removed from the working tree.", dark: "diff.removed", light: "diff.removed" },
    renamed: { description: "A file renamed against the version control baseline.", dark: "#b392f0", light: "#7e4fc1" },
    untracked: { description: "A file version control does not track yet.", dark: "#4fc1b0", light: "#1a8a7c" },
    conflicted: { description: "A file with an unresolved merge conflict.", dark: "#e8934a", light: "#c4661a" },
  };
  for (const status of PRIORITY) {
    const { description, dark, light } = colors[status];
    await themes.defineColor({ key: colorOf(status), description, defaults: { dark, light } });
  }
}

const rank = (status: Status) => PRIORITY.indexOf(status);

/** The folders from the one holding `file` up to and including `root`. */
function ancestors(root: string, file: string): string[] {
  const top = root.length > 1 && root.endsWith("/") ? root.slice(0, -1) : root;
  const folders: string[] = [];
  let folder = directoryOf(file);
  while (contains(top, folder)) {
    folders.push(folder);
    const next = directoryOf(folder);
    if (folder === top || next === folder) break;
    folder = next;
  }
  return folders;
}

/**
 * What each resource shows. Where repositories nest, a file belongs to the
 * deepest one, so an outer repository's entries inside a nested one, such as
 * the nested folder it reports as untracked, are dropped.
 */
export function decorate(snapshots: readonly Snapshot[]): Map<string, ResourceDecoration> {
  const files = new Map<string, Status>();
  const folders = new Map<string, Status>();
  for (const { repository, entries } of snapshots) {
    const inner = snapshots
      .map((other) => other.repository.root)
      .filter((root) => root.length > repository.root.length && contains(repository.root, root));
    for (const { file, status } of entries) {
      if (inner.some((root) => contains(root, file)) || files.has(file)) continue;
      files.set(file, status);
      for (const folder of ancestors(repository.root, file)) {
        const current = folders.get(folder);
        if (current === undefined || rank(status) < rank(current)) folders.set(folder, status);
      }
    }
  }
  const result = new Map<string, ResourceDecoration>();
  for (const [folder, status] of folders) result.set(folder, { color: colorOf(status) });
  for (const [file, status] of files) {
    const { badge, tooltip } = LABELS[status];
    result.set(file, { color: colorOf(status), badge: { text: badge }, tooltip });
  }
  return result;
}

export type StatusDeps = {
  /** Every repository whose status is shown. */
  known(): Promise<Repository[]>;
  /** The changed files of one repository. */
  fetch(repository: Repository): Promise<readonly Entry[]>;
  /** A backend whose contract no longer binds. */
  gone(backend: string): void;
};

export type Statuses = {
  /** The known repositories or one repository's status may have changed; `key` is `keyOf` of that repository. */
  request(key?: string): void;
  [Symbol.asyncDispose](): Promise<void>;
};

export const keyOf = (repository: Repository) => `${repository.backend}\0${repository.root}`;

/** Keeps the decorations in step with the known repositories' status. */
export function createStatuses(deps: StatusDeps): Statuses {
  const snapshots = new Map<string, Snapshot>();
  /** Repositories whose status is pulled again. */
  const dirty = new Set<string>();
  /** What the source holds now, as JSON by URI. */
  let published = new Map<string, string>();
  let source: DecorationSource | null = null;
  let running = false;
  let pending = false;
  let closed = false;

  async function publish(wanted: Map<string, ResourceDecoration>) {
    const changes: Record<string, ResourceDecoration | null> = {};
    const next = new Map<string, string>();
    for (const [uri, decoration] of wanted) {
      const json = JSON.stringify(decoration);
      next.set(uri, json);
      if (published.get(uri) !== json) changes[uri] = decoration;
    }
    for (const uri of published.keys()) if (!next.has(uri)) changes[uri] = null;
    if (Object.keys(changes).length === 0) return;
    source ??= await decorations.createSource();
    await source.set(changes);
    published = next;
  }

  async function pass() {
    const known = await deps.known();
    const keys = new Set(known.map(keyOf));
    for (const key of [...snapshots.keys()]) if (!keys.has(key)) snapshots.delete(key);
    for (const repository of known) {
      const key = keyOf(repository);
      if (snapshots.has(key) && !dirty.has(key)) continue;
      dirty.delete(key);
      try {
        snapshots.set(key, { repository, entries: await deps.fetch(repository) });
      } catch (error) {
        // A repository whose status failed keeps what it showed; one that never had any shows none.
        if (!isGone(error)) continue;
        snapshots.delete(key);
        deps.gone(repository.backend);
      }
    }
    const current = known.flatMap((repository) => snapshots.get(keyOf(repository)) ?? []);
    await publish(decorate(current));
  }

  async function run() {
    if (running) {
      pending = true;
      return;
    }
    running = true;
    try {
      do {
        pending = false;
        if (closed) break;
        await pass();
      } while (pending);
    } finally {
      running = false;
    }
  }

  return {
    request(key) {
      if (key !== undefined) dirty.add(key);
      void run().catch(() => {});
    },
    async [Symbol.asyncDispose]() {
      closed = true;
      await source?.[Symbol.asyncDispose]().catch(() => {});
      source = null;
    },
  };
}
