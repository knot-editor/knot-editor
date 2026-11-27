// The actions of the Changes view and `rollback`. Each acts on the exact
// files that a group lists below the row's path, so a folder or a group header
// never touches files that belong to another group, and runs one `git` call
// per kind of change.
import { commands, workbench } from "knot";
import { git, gitLogged } from "./git.js";
import { parseGroups, type Groups } from "./status.js";
import { relative } from "./uri.js";

type Arguments = { readonly repository: string; readonly group: string; readonly path: string };

/** The paths of `groups` at or below `path` (the repository itself when it is `repository`), as Git lists them. */
async function listed(repository: string, path: string, groups: readonly (keyof Groups)[]): Promise<string[]> {
  const below = relative(repository, path);
  const output = await git(repository, ["status", "--porcelain=v2", "-z", "--untracked-files=all"]);
  if (output === null || output.exitCode !== 0) return [];
  const found = parseGroups(output.stdout);
  const inside = (candidate: string) => {
    if (below === null) return true;
    const bare = candidate.replace(/\/+$/, "");
    return bare === below || bare.startsWith(`${below}/`);
  };
  return [...new Set(groups.flatMap((group) => found[group]).filter(inside))];
}

async function run(repository: string, args: readonly string[]) {
  const output = await gitLogged(repository, args);
  if (output.exitCode !== 0) throw new Error(`git ${args[0]} failed`);
}

/** The paths of `paths` that HEAD has. */
async function inHead(repository: string, paths: readonly string[]): Promise<Set<string>> {
  const output = await git(repository, ["ls-tree", "-r", "--name-only", "-z", "HEAD", "--", ...paths]);
  return new Set(output === null ? [] : output.stdout.split("\0"));
}

const stage = (repository: string, paths: readonly string[]) => run(repository, ["add", "-A", "--", ...paths]);
const unstage = (repository: string, paths: readonly string[]) =>
  run(repository, ["restore", "--staged", "--", ...paths]);

/** Restores the paths to HEAD in the index and the working tree; an added file is only unstaged. */
async function restore(repository: string, paths: readonly string[]) {
  const known = await inHead(repository, paths);
  const tracked = paths.filter((path) => known.has(path));
  const added = paths.filter((path) => !known.has(path));
  if (tracked.length > 0) await run(repository, ["restore", "--source=HEAD", "--staged", "--worktree", "--", ...tracked]);
  if (added.length > 0) await unstage(repository, added);
}

/** Restores `path` to HEAD, as `vcs.rollbackFile` asks. */
export async function rollback(repository: string, path: string) {
  const paths = await listed(repository, path, ["staged", "unstaged"]);
  if (paths.length > 0) await restore(repository, paths);
}

async function confirmed(title: string, message: string, confirmLabel: string) {
  return (await workbench.showConfirmation({ title, message, confirmLabel })).kind === "confirmed";
}

/** Registers `git.stage`, `git.unstage`, `git.discard`, and `git.rollback`. */
export async function registerActions() {
  const act = (
    name: string,
    title: string,
    description: string,
    groups: (group: string) => (keyof Groups)[],
    perform: (repository: string, paths: string[], group: string) => Promise<void>,
    confirm?: (what: string, group: string) => { title: string; message: string; confirmLabel: string },
  ) =>
    commands.register(
      name,
      async ({ arguments: given }) => {
        const { repository, group, path } = (given ?? {}) as Partial<Arguments>;
        if (typeof repository !== "string" || typeof group !== "string" || typeof path !== "string") commands.unavailable();
        const paths = await listed(repository, path, groups(group));
        if (paths.length === 0) return;
        if (confirm !== undefined) {
          const what = paths.length === 1 ? paths[0]!.replace(/\/+$/, "").split("/").pop()! : `${paths.length} files`;
          const { title, message, confirmLabel } = confirm(what, group);
          if (!(await confirmed(title, message, confirmLabel))) return;
        }
        await perform(repository, paths, group);
      },
      { title, description, palette: false },
    );
  const own = (group: string) => [group as keyof Groups];
  return [
    await act("git.stage", "Git: Stage", "Stage the changes of a file, folder, or group.", own, stage),
    await act("git.unstage", "Git: Unstage", "Unstage the changes of a file, folder, or group.", own, unstage),
    await act(
      "git.discard",
      "Git: Discard Changes",
      "Discard the unstaged changes of a file, folder, or group, or delete untracked files.",
      own,
      async (repository, paths, group) => {
        if (group === "untracked") await run(repository, ["clean", "-f", "-d", "--", ...paths]);
        else await run(repository, ["restore", "--", ...paths]);
      },
      (what, group) => ({
        title: `${group === "untracked" ? "Delete" : "Discard changes of"} ${what}?`,
        message: `This cannot be undone.`,
        confirmLabel: group === "untracked" ? "Delete" : "Discard",
      }),
    ),
    await act(
      "git.rollback",
      "Git: Rollback",
      "Restore a file, folder, or group to the last commit, in the index and on disk.",
      own,
      restore,
      (what) => ({
        title: `Roll back ${what}?`,
        message: "Staged and unstaged changes are discarded and cannot be recovered.",
        confirmLabel: "Roll Back",
      }),
    ),
  ];
}
