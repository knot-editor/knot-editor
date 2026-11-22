// Finding repositories. The root of the repository containing a directory
// comes from `git rev-parse --show-prefix`, which names the directory's path
// below the root, so the root's URI is the directory's with those segments
// removed. Git's own absolute path would differ through a symlink.
import { files } from "knot";
import { git } from "./git.js";
import { ancestor, below, contains, trimmed } from "./uri.js";

/** How deep below a root that is not in a repository the search for repositories goes. */
const DEPTH = 2;
/** How many directories that search lists, in all. */
const LISTED = 200;

/** The root URI of the repository containing the directory, or null. */
export async function repositoryOf(directory: string): Promise<string | null> {
  const result = await git(directory, ["rev-parse", "--is-inside-work-tree", "--show-prefix"]);
  if (result === null || result.exitCode !== 0) return null;
  const [inside, prefix = ""] = result.stdout.split("\n");
  if (inside !== "true") return null;
  const levels = prefix.split("/").filter((segment) => segment !== "").length;
  return ancestor(directory, levels);
}

async function isRepository(directory: string): Promise<boolean> {
  return (await files.stat(`${trimmed(directory)}/.git`).catch(() => null)) !== null;
}

/** The directories of the repository that hold a repository of their own: submodules and untracked clones. */
async function nestedIn(repository: string): Promise<string[]> {
  const candidates = new Set<string>();
  const modules = await git(repository, [
    "config", "-z", "--file", ".gitmodules", "--get-regexp", "^submodule\\..*\\.path$",
  ]);
  if (modules !== null && modules.exitCode === 0) {
    for (const entry of modules.stdout.split("\0")) {
      const path = entry.slice(entry.indexOf("\n") + 1);
      if (path !== "") candidates.add(path);
    }
  }
  const untracked = await git(repository, ["ls-files", "-z", "--others", "--exclude-standard", "--directory"]);
  if (untracked !== null && untracked.exitCode === 0) {
    for (const path of untracked.stdout.split("\0")) if (path.endsWith("/")) candidates.add(path);
  }
  const found: string[] = [];
  for (const path of candidates) {
    const uri = below(repository, path);
    if (await isRepository(uri)) found.push(uri);
  }
  return found;
}

/** Repositories below a directory that is not in one, found by listing a few levels. */
async function searchBelow(root: string): Promise<string[]> {
  const found: string[] = [];
  let listed = 0;
  const visit = async (directory: string, depth: number): Promise<void> => {
    if (listed >= LISTED) return;
    listed += 1;
    const entries = await files.list(directory).catch(() => []);
    if (await isRepository(directory)) {
      found.push(trimmed(directory));
      return;
    }
    if (depth >= DEPTH) return;
    for (const entry of entries) {
      if (entry.kind === "directory" && !entry.ignored) await visit(entry.uri, depth + 1);
    }
  };
  for (const entry of await files.list(root).catch(() => [])) {
    if (entry.kind === "directory" && !entry.ignored) await visit(entry.uri, 1);
  }
  return found;
}

/** The repositories covering a root: the one containing it and those nested below it. */
export async function repositories(root: string): Promise<string[]> {
  const outer = await repositoryOf(root);
  if (outer === null) return searchBelow(root);
  const nested = (await nestedIn(outer)).filter((uri) => contains(root, uri));
  return [outer, ...nested];
}
