// The commands that stage and unstage the hunk at a line: they compute the new
// index text (see hunks.ts) and write it with `hash-object` and `update-index`.
import { commands, diff } from "knot";
import type { CommandContext } from "knot";
import { hunkAt, stagedText, unstagedText } from "./hunks.js";
import { git, gitLogged } from "./git.js";
import { relative } from "./uri.js";

type Arguments = { readonly repository: string; readonly file: string; readonly line: number };

/** The text of `spec` as the working tree would have it, or null when there is none. */
async function textAt(repository: string, spec: string): Promise<string | null> {
  const output = await git(repository, ["cat-file", "--filters", spec]);
  if (output === null) throw new Error("The repository is gone");
  return output.exitCode === 0 ? output.stdout : null;
}

/** The mode of the file's index entry, else HEAD's. */
async function modeOf(repository: string, path: string): Promise<string> {
  for (const args of [["ls-files", "--stage", "--", path], ["ls-tree", "HEAD", "--", path]]) {
    const output = await git(repository, args);
    const mode = output?.exitCode === 0 ? /^(\d{6}) /.exec(output.stdout)?.[1] : undefined;
    if (mode !== undefined) return mode;
  }
  return "100644";
}

/** Replaces the index entry of `path` with `text`, as the clean filters make it. */
async function writeIndex(repository: string, path: string, text: string) {
  const mode = await modeOf(repository, path);
  const hash = await gitLogged(repository, ["hash-object", "-w", "--stdin", "--path", path], text);
  if (hash.exitCode !== 0) throw new Error("git hash-object failed");
  const entry = `${mode},${hash.stdout.trim()},${path}`;
  const update = await gitLogged(repository, ["update-index", "--add", "--cacheinfo", entry]);
  if (update.exitCode !== 0) throw new Error("git update-index failed");
}

/** Stages or unstages the hunk at `line` of the buffer's file. */
const act = (staging: boolean) => async ({ buffer, arguments: given }: CommandContext) => {
  const { repository, file, line } = (given ?? {}) as Partial<Arguments>;
  if (buffer === null || typeof repository !== "string" || typeof file !== "string" || typeof line !== "number") {
    commands.unavailable();
  }
  const path = relative(repository, file);
  if (path === null) commands.unavailable();
  const index = await textAt(repository, `:${path}`);
  if (index === null) commands.unavailable();
  // A file the index holds and HEAD lacks is all added.
  const head = (await textAt(repository, `HEAD:${path}`)) ?? "";
  const { text } = await buffer.snapshot();
  const hunk = hunkAt((await diff(head, text)).hunks, line, (await buffer.lineCount()) - 1);
  if (hunk === null) commands.unavailable();
  const next = staging
    ? stagedText(index, text, (await diff(index, text)).hunks, hunk)
    : unstagedText(index, head, (await diff(head, index)).hunks, hunk);
  if (next !== index) await writeIndex(repository, path, next);
};

/** Registers `git.stageHunk` and `git.unstageHunk`. */
export async function registerStaging() {
  return [
    await commands.register("git.stageHunk", act(true), {
      title: "Git: Stage Hunk",
      description: "Stage the changes of the hunk at the cursor.",
    }),
    await commands.register("git.unstageHunk", act(false), {
      title: "Git: Unstage Hunk",
      description: "Unstage the changes of the hunk at the cursor.",
    }),
  ];
}

/** What the hunk popup offers: stage an unstaged hunk, unstage a staged one. */
export const hunkActions = [
  { label: "Stage Hunk", icon: "git.stageHunk", command: "git.stageHunk", on: ["unstaged" as const] },
  { label: "Unstage Hunk", icon: "git.unstageHunk", command: "git.unstageHunk", on: ["staged" as const] },
];
