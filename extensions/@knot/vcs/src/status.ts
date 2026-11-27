// File status: each known repository's changed files are pulled from its
// backend and painted on resources as one decoration source. A file shows its
// status as a color, a letter badge, and a tooltip; each folder above a change
// takes the color of its most important change, up to the repository root.
import { decorations, themes } from "knot";
import type { DecorationSource, ResourceDecoration } from "knot";
import { contains, directoryOf, isGone, type Repository } from "./repositories.js";

export type Status = "added" | "modified" | "deleted" | "renamed" | "untracked" | "conflicted";
export type Entry = { readonly file: string; readonly status: Status };

/** An action a group offers on its rows; `command` is one of the backend's own. */
export type GroupAction = {
  readonly label: string;
  readonly icon?: string;
  readonly command: string;
  readonly on: readonly ("group" | "folder" | "file")[];
  readonly menuOnly?: boolean;
  readonly showOnHover?: boolean;
};

/** A list of changed files the backend shows together, as file URIs. */
export type Group = {
  readonly id: string;
  readonly label: string;
  readonly files: readonly string[];
  readonly actions: readonly GroupAction[];
};

/** What a backend reports of a repository's status. */
export type Report = { readonly files: readonly Entry[]; readonly groups: readonly Group[] };

/** A repository and what its backend reported as changed. */
export type Snapshot = { readonly repository: Repository; readonly entries: readonly Entry[]; readonly groups: readonly Group[] };

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
    renamed: { description: "A file renamed against the version control baseline.", dark: "#a78bfa", light: "#6d4bc4" },
    untracked: { description: "A file version control does not track yet.", dark: "#d6b25e", light: "#8a6a00" },
    conflicted: { description: "A file with an unresolved merge conflict.", dark: "#f0629f", light: "#c2256f" },
  };
  for (const status of PRIORITY) {
    const { description, dark, light } = colors[status];
    await themes.defineColor({ key: colorOf(status), description, defaults: { dark, light } });
  }
}

/** Defines the dimmer gutter colors of staged hunks, each a faded form of its diff color. */
export async function defineStagedColors() {
  const colors = [
    { key: "diff.added.staged", description: "Staged added lines, in the gutter.", dark: "#5fb86566", light: "#36965066" },
    { key: "diff.modified.staged", description: "Staged changed lines, in the gutter.", dark: "#548af766", light: "#3574f066" },
    { key: "diff.removed.staged", description: "Staged removed lines, in the gutter.", dark: "#f7546466", light: "#db5c5c66" },
  ];
  for (const { key, description, dark, light } of colors) {
    await themes.defineColor({ key, description, defaults: { dark, light } });
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
  /** The status of one repository. */
  fetch(repository: Repository): Promise<Report>;
  /** A backend whose contract no longer binds. */
  gone(backend: string): void;
  /** A repository's snapshot was replaced, or null when the shown repositories changed. */
  changed(repository: Repository | null): void;
};

export type Statuses = {
  /**
   * The known repositories, or one repository's status, may have changed;
   * `key` is `keyOf` of that repository. Without a key, the repositories are
   * found again and those not shown yet are pulled.
   */
  request(key?: string): void;
  /** What the backend last reported of a repository, if it has been pulled. */
  snapshot(repository: Repository): Snapshot | undefined;
  [Symbol.asyncDispose](): Promise<void>;
};

/** Identifies a repository; its root may be spelled with or without a trailing slash. */
export const keyOf = (repository: Repository) => `${repository.backend}\0${repository.root.replace(/\/+$/, "")}`;

/** One repository's pulls: one in flight and at most one more queued. */
type Slot = { running: boolean; pending: boolean };

/** Keeps the decorations in step with the known repositories' status. */
export function createStatuses(deps: StatusDeps): Statuses {
  const snapshots = new Map<string, Snapshot>();
  const slots = new Map<string, Slot>();
  /** The repositories found by the last discovery. */
  let current: Repository[] = [];
  /** What the source holds now, as JSON by URI. */
  let published = new Map<string, string>();
  let source: DecorationSource | null = null;
  /** Publishes run one after another, each from the state it finds. */
  let chain: Promise<void> = Promise.resolve();
  let discovering = false;
  let rediscover = false;
  let closed = false;

  async function publish() {
    if (closed) return;
    const wanted = decorate(current.flatMap((repository) => snapshots.get(keyOf(repository)) ?? []));
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

  const rebuild = () => {
    chain = chain.then(publish).catch(() => {});
  };

  async function fetch(repository: Repository) {
    const key = keyOf(repository);
    try {
      const { files, groups } = await deps.fetch(repository);
      snapshots.set(key, { repository, entries: files, groups });
    } catch (error) {
      // A repository whose status failed keeps what it showed; one that never had any shows none.
      if (!isGone(error)) return;
      snapshots.delete(key);
      deps.gone(repository.backend);
    }
  }

  async function pull(repository: Repository) {
    const key = keyOf(repository);
    const slot = slots.get(key) ?? { running: false, pending: false };
    slots.set(key, slot);
    if (slot.running) {
      slot.pending = true;
      return;
    }
    slot.running = true;
    try {
      do {
        slot.pending = false;
        if (closed) break;
        await fetch(repository);
        rebuild();
        deps.changed(repository);
      } while (slot.pending);
    } finally {
      slot.running = false;
    }
  }

  async function discover() {
    if (discovering) {
      rediscover = true;
      return;
    }
    discovering = true;
    try {
      do {
        rediscover = false;
        if (closed) break;
        current = await deps.known();
        const keys = new Set(current.map(keyOf));
        for (const key of [...snapshots.keys()]) if (!keys.has(key)) snapshots.delete(key);
        for (const key of [...slots.keys()]) if (!keys.has(key) && !slots.get(key)?.running) slots.delete(key);
        rebuild();
        deps.changed(null);
        await Promise.all(current.filter((repository) => !snapshots.has(keyOf(repository))).map(pull));
      } while (rediscover);
    } finally {
      discovering = false;
    }
  }

  return {
    snapshot: (repository) => snapshots.get(keyOf(repository)),
    request(key) {
      const known = key === undefined ? undefined : current.find((repository) => keyOf(repository) === key);
      void (known === undefined ? discover() : pull(known)).catch(() => {});
    },
    async [Symbol.asyncDispose]() {
      closed = true;
      await chain;
      await source?.[Symbol.asyncDispose]().catch(() => {});
      source = null;
    },
  };
}
