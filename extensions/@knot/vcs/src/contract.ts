// What `@knot/vcs` defines for version control backends, in both directions
// because no callbacks cross a contract and `client()` binds only the latest
// registration. A backend imports this module from "@knot/vcs" and lists
// `@knot/vcs` in its manifest's `requires`. This module is pure schema code,
// so it imports only "knot/contract".
import { defineContract, s } from "knot/contract";

/** Served by `@knot/vcs`, called by backends. */
export const vcs = defineContract("vcs", {
  /** Announces the backend whose `backendContract(id)` the caller serves. */
  register: s.fn(s.string(), s.void()),
  /** Withdraws a backend; one that stops without calling this is dropped when next asked. */
  unregister: s.fn(s.string(), s.void()),
  /** The baselines of the repository, as a URI of its root, changed. */
  changed: s.fn(s.object({ backend: s.string(), repository: s.string() }), s.void()),
  /**
   * Files in the repository's working tree changed, so its status may have;
   * its baselines did not.
   */
  statusChanged: s.fn(s.object({ backend: s.string(), repository: s.string() }), s.void()),
  /**
   * Appends to the Version Control output a command the user started: its
   * `command` line, the `output` of both its streams, and whether it `failed`.
   */
  log: s.fn(
    s.object({
      backend: s.string(),
      repository: s.string(),
      command: s.string(),
      output: s.string(),
      failed: s.boolean(),
    }),
    s.void(),
  ),
  /** The IDs of the registered backends that are still being served. */
  backends: s.fn(s.void(), s.array(s.string())),
});

/** One side of a diff: a label and the text shown under it. */
const side = s.object({ label: s.string(), text: s.string() });

/**
 * What a group offers on its rows. `command` is one of the backend's own
 * commands, run with `{ repository, group, path }` where `path` is the file,
 * the folder, or the repository root for a group header.
 */
const action = s.object({
  label: s.string(),
  icon: s.optional(s.string()),
  command: s.string(),
  on: s.array(s.enum(["group", "folder", "file"])),
  menuOnly: s.optional(s.boolean()),
  showOnHover: s.optional(s.boolean()),
});

/**
 * What a backend adds to the hunk popup's toolbar for a hunk in one of the
 * states `on`. `command` is one of the backend's own commands, run in the
 * editor with `{ repository, file, line }`: the hunk's first current line, or
 * the line at a removal's boundary.
 */
const hunkAction = s.object({
  label: s.string(),
  icon: s.string(),
  command: s.string(),
  on: s.array(s.enum(["staged", "unstaged"])),
});

const backends = new Map<string, ReturnType<typeof defineBackend>>();

function defineBackend(id: string) {
  return defineContract(`backend.${id}`, {
    /** The repositories covering a root URI: one containing it and those nested below it, as root URIs. */
    repositories: s.fn(s.string(), s.array(s.string())),
    /** The root URI of the repository of a file URI, or null when it has none. */
    repositoryOf: s.fn(s.string(), s.nullable(s.string())),
    /**
     * The text of a file URI in a repository's baseline, or null when it has
     * none. `staged` is the text between the baseline and the working file,
     * such as Git's index entry; it is omitted when it equals `text` or the
     * backend has no such text.
     */
    baseline: s.fn(
      s.object({ repository: s.string(), file: s.string() }),
      s.nullable(s.object({ text: s.string(), staged: s.optional(s.string()) })),
    ),
    /**
     * A snapshot of the repository: `files` are those that differ from its
     * baseline, as file URIs (one it does not list is unchanged, ignored files
     * are never listed, and `renamed` is the new path); `groups` are the lists
     * the backend shows them in, in its order. A file can be in several groups.
     */
    status: s.fn(
      s.string(),
      s.object({
        files: s.array(
          s.object({
            file: s.string(),
            status: s.enum(["added", "modified", "deleted", "renamed", "untracked", "conflicted"]),
          }),
        ),
        groups: s.array(
          s.object({
            id: s.string(),
            label: s.string(),
            files: s.array(s.string()),
            actions: s.array(action),
          }),
        ),
      }),
    ),
    /**
     * The sides of a file's diff in a group, or null when it has none there.
     * `modified` is `"document"` for the working file, opened editable.
     */
    compare: s.fn(
      s.object({ repository: s.string(), group: s.string(), file: s.string() }),
      s.nullable(
        s.object({
          original: side,
          modified: s.union([s.literal("document"), side]),
        }),
      ),
    ),
    /** Restores the path, a file or folder URI, to its baseline on disk, after confirming with the user. */
    rollback: s.fn(s.object({ repository: s.string(), path: s.string() }), s.void()),
    /** The toggles the commit panel offers for the repository, all off by default. */
    commitOptions: s.fn(
      s.string(),
      s.array(s.object({ id: s.string(), label: s.string(), tooltip: s.optional(s.string()) })),
    ),
    /** The message the options suggest, or null when they suggest none. */
    commitMessage: s.fn(s.object({ repository: s.string(), options: s.array(s.string()) }), s.nullable(s.string())),
    /**
     * Commits with the options on, asking the user for what it needs. Null means
     * the user cancelled; a failure rejects.
     */
    commit: s.fn(
      s.object({ repository: s.string(), message: s.string(), options: s.array(s.string()) }),
      s.nullable(s.object({ summary: s.string() })),
    ),
    /** The actions the hunk popup offers for a repository's hunks. */
    hunkActions: s.fn(s.string(), s.array(hunkAction)),
    /** What is checked out in the repository, or null when the backend has nothing to show. */
    ref: s.fn(s.string(), s.nullable(s.object({ label: s.string(), tooltip: s.optional(s.string()) }))),
  });
}

/**
 * The contract of the backend `id`, served by that backend and called by
 * `@knot/vcs`. One contract per backend lets several run at once. The same
 * `id` yields the same contract.
 */
export function backendContract(id: string) {
  if (typeof id !== "string" || id === "") throw new TypeError("backendContract requires a backend ID");
  let contract = backends.get(id);
  if (contract === undefined) {
    contract = defineBackend(id);
    backends.set(id, contract);
  }
  return contract;
}
