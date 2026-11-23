// The changed files of a repository against HEAD, from Git's two-column
// status collapsed into one HEAD-relative status. `status --porcelain=v2 -z`
// lists tracked entries with an index column X and a worktree column Y, and
// untracked files one by one, up to a limit; ignored files are not listed.
import { git, gitWhile } from "./git.js";
import { below } from "./uri.js";

export type Status = "added" | "modified" | "deleted" | "renamed" | "untracked" | "conflicted";
export type Change = { readonly file: string; readonly status: Status };

/** The status of a tracked entry with the columns `index` and `tree`, or null when it equals HEAD. */
function collapse(index: string, tree: string): Status | null {
  if (index === "A" || index === "C") return tree === "D" ? null : "added";
  if (tree === "A") return "added"; // intent to add
  if (index === "D" || tree === "D") return "deleted";
  if (index === "R") return "renamed";
  if (index === "M" || index === "T" || tree === "M" || tree === "T") return "modified";
  return null;
}

/** The entries of `git status --porcelain=v2 -z`, as relative paths with their status. */
export function parse(output: string): Array<{ path: string; status: Status }> {
  const parts = output.split("\0");
  const found = new Map<string, Status>();
  for (let at = 0; at < parts.length; at += 1) {
    const entry = parts[at] ?? "";
    const fields = entry.split(" ");
    const index = fields[1]?.[0] ?? ".";
    const tree = fields[1]?.[1] ?? ".";
    if (entry.startsWith("? ")) {
      const path = entry.slice(2);
      // Recreated after a staged removal: HEAD has it and the disk differs.
      found.set(path, found.get(path) === "deleted" ? "modified" : "untracked");
    } else if (entry.startsWith("u ")) {
      found.set(fields.slice(10).join(" "), "conflicted");
    } else if (entry.startsWith("1 ")) {
      const status = collapse(index, tree);
      if (status !== null) found.set(fields.slice(8).join(" "), status);
    } else if (entry.startsWith("2 ")) {
      const origin = parts[(at += 1)] ?? "";
      const status = collapse(index, tree);
      if (status === null) continue;
      // Renamed and then removed from disk: the old path is what is gone.
      if (tree === "D") found.set(origin, "deleted");
      else found.set(fields.slice(9).join(" "), status);
    }
  }
  return [...found].map(([path, status]) => ({ path, status }));
}

/**
 * Untracked files listed one by one, at most. Past that a repository with a
 * huge untracked folder, such as a dependency folder nobody ignored, would
 * cost more to list and paint than it is worth, so each untracked folder is
 * one entry instead.
 */
export const UNTRACKED_LIMIT = 2000;

const manyUntracked = (stdout: string) => (stdout.match(/(?:^|\0)\? /g) ?? []).length > UNTRACKED_LIMIT;

/** The changed files of `repository` against its HEAD. */
export async function status(repository: string): Promise<Change[]> {
  const base = ["status", "--porcelain=v2", "-z"];
  // Tracked changes come first and untracked files last, so a long listing is cut short.
  let result = await gitWhile(repository, [...base, "--untracked-files=all"], manyUntracked);
  result ??= await git(repository, [...base, "--untracked-files=normal"]);
  if (result === null || result.exitCode !== 0) return [];
  return parse(result.stdout).map(({ path, status }) => ({
    file: below(repository, path.replace(/\/+$/, "")),
    status,
  }));
}
