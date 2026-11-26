// The changed files of a repository against HEAD, from Git's two-column
// status collapsed into one HEAD-relative status. `status --porcelain=v2 -z`
// lists tracked entries with an index column X and a worktree column Y, and
// untracked files one by one, up to a limit; ignored files are not listed.
import { git, gitWhile } from "./git.js";
import { below } from "./uri.js";

export type Status = "added" | "modified" | "deleted" | "renamed" | "untracked" | "conflicted";

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

export type Groups = Record<"staged" | "unstaged" | "conflicts" | "untracked", string[]>;

/**
 * The entries of `git status --porcelain=v2 -z` by the list that shows them:
 * those with the index column set are staged, those with the worktree column
 * set unstaged, so a staged file edited again is in both. A renamed entry
 * lists its new path.
 */
export function parseGroups(output: string): Groups {
  const parts = output.split("\0");
  const groups: Groups = { staged: [], unstaged: [], conflicts: [], untracked: [] };
  for (let at = 0; at < parts.length; at += 1) {
    const entry = parts[at] ?? "";
    const fields = entry.split(" ");
    const index = fields[1]?.[0] ?? ".";
    const tree = fields[1]?.[1] ?? ".";
    let path: string;
    if (entry.startsWith("? ")) {
      groups.untracked.push(entry.slice(2));
      continue;
    } else if (entry.startsWith("u ")) {
      groups.conflicts.push(fields.slice(10).join(" "));
      continue;
    } else if (entry.startsWith("1 ")) {
      path = fields.slice(8).join(" ");
    } else if (entry.startsWith("2 ")) {
      path = fields.slice(9).join(" ");
      at += 1; // the old path
    } else {
      continue;
    }
    if (index !== ".") groups.staged.push(path);
    if (tree !== ".") groups.unstaged.push(path);
  }
  return groups;
}

/**
 * Untracked files listed one by one, at most. Past that a repository with a
 * huge untracked folder, such as a dependency folder nobody ignored, would
 * cost more to list and paint than it is worth, so each untracked folder is
 * one entry instead.
 */
export const UNTRACKED_LIMIT = 2000;

const manyUntracked = (stdout: string) => (stdout.match(/(?:^|\0)\? /g) ?? []).length > UNTRACKED_LIMIT;

const action = (label: string, icon: string, command: string, on: Action["on"], extra: Partial<Action> = {}): Action => ({
  label,
  icon,
  command,
  on,
  ...extra,
});

type Action = {
  readonly label: string;
  readonly icon: string;
  readonly command: string;
  readonly on: readonly ("group" | "folder" | "file")[];
  readonly menuOnly?: boolean;
  readonly showOnHover?: boolean;
};

const STAGE = [
  action("Stage", "git.stage", "git.stage", ["folder", "file"], { showOnHover: true }),
  action("Stage All", "git.stage", "git.stage", ["group"], { showOnHover: true }),
];
const UNSTAGE = [
  action("Unstage", "git.unstage", "git.unstage", ["folder", "file"], { showOnHover: true }),
  action("Unstage All", "git.unstage", "git.unstage", ["group"], { showOnHover: true }),
];
const DISCARD = [action("Discard", "git.discard", "git.discard", ["group", "folder", "file"], { showOnHover: true })];
const ROLLBACK = [action("Rollback", "git.rollback", "git.rollback", ["group", "folder", "file"], { menuOnly: true })];

/** The groups of the repository, in order; Conflicts is listed only when there are any. */
function groupsOf(repository: string, found: Groups) {
  const uris = (paths: string[]) => paths.map((path) => below(repository, path.replace(/\/+$/, "")));
  const groups = [
    { id: "staged", label: "Staged", files: uris(found.staged), actions: [...UNSTAGE, ...ROLLBACK] },
    { id: "unstaged", label: "Unstaged", files: uris(found.unstaged), actions: [...STAGE, ...DISCARD, ...ROLLBACK] },
    { id: "conflicts", label: "Conflicts", files: uris(found.conflicts), actions: [...STAGE] },
    { id: "untracked", label: "Untracked", files: uris(found.untracked), actions: [...STAGE, ...DISCARD] },
  ];
  return groups.filter((group) => group.id !== "conflicts" || group.files.length > 0);
}

const EMPTY = { files: [], groups: groupsOf("file:///", { staged: [], unstaged: [], conflicts: [], untracked: [] }) };

/** The status of `repository`: its changed files against HEAD, and the groups showing them. */
export async function status(repository: string) {
  const base = ["status", "--porcelain=v2", "-z"];
  // Tracked changes come first and untracked files last, so a long listing is cut short.
  let result = await gitWhile(repository, [...base, "--untracked-files=all"], manyUntracked);
  result ??= await git(repository, [...base, "--untracked-files=normal"]);
  if (result === null || result.exitCode !== 0) return EMPTY;
  return {
    files: parse(result.stdout).map(({ path, status }) => ({
      file: below(repository, path.replace(/\/+$/, "")),
      status,
    })),
    groups: groupsOf(repository, parseGroups(result.stdout)),
  };
}
