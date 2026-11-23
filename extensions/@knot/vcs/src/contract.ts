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
  /** The IDs of the registered backends that are still being served. */
  backends: s.fn(s.void(), s.array(s.string())),
});

const backends = new Map<string, ReturnType<typeof defineBackend>>();

function defineBackend(id: string) {
  return defineContract(`backend.${id}`, {
    /** The repositories covering a root URI: one containing it and those nested below it, as root URIs. */
    repositories: s.fn(s.string(), s.array(s.string())),
    /** The root URI of the repository of a file URI, or null when it has none. */
    repositoryOf: s.fn(s.string(), s.nullable(s.string())),
    /** The text of a file URI in a repository's baseline, or null when it has none. */
    baseline: s.fn(s.object({ repository: s.string(), file: s.string() }), s.nullable(s.string())),
    /**
     * A snapshot of the repository's files that differ from its baseline, as
     * file URIs: one it does not list is unchanged, and ignored files are never
     * listed. `renamed` is the new path.
     */
    status: s.fn(
      s.string(),
      s.array(
        s.object({
          file: s.string(),
          status: s.enum(["added", "modified", "deleted", "renamed", "untracked", "conflicted"]),
        }),
      ),
    ),
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
