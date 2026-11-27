// Committing the index: the options the panel offers, the message Amend
// suggests, and `git commit` with the message on stdin.
import { workbench } from "knot";
import type { WindowTarget } from "knot";
import { git, gitLogged } from "./git.js";

export const commitOptions = [
  { id: "amend", label: "Amend", tooltip: "Replace the last commit" },
  { id: "signoff", label: "Sign-off", tooltip: "Add a Signed-off-by line" },
] as const;

/** HEAD's message while amending, null otherwise. */
export async function commitMessage(repository: string, options: readonly string[]): Promise<string | null> {
  if (!options.includes("amend")) return null;
  const output = await git(repository, ["log", "-1", "--format=%B"]);
  return output === null || output.exitCode !== 0 ? null : output.stdout.trimEnd();
}

/** Whether `args` (a `diff` of the index or the working tree) finds a difference. */
async function differs(repository: string, args: readonly string[]): Promise<boolean> {
  const output = await git(repository, ["diff", "--quiet", ...args]);
  return output !== null && output.exitCode === 1;
}

/** The first line of a failed command's output that says why. */
function reason(output: string): string {
  return output.split("\n").find((line) => line.trim() !== "") ?? "git commit failed";
}

/**
 * Commits what is staged. With nothing staged, it asks to stage every tracked
 * change first; untracked files are never included. Resolves null when the
 * user declines; the prompt shows in the caller's `window`.
 */
export async function commit(
  repository: string,
  message: string,
  options: readonly string[],
  window: WindowTarget | null,
): Promise<{ summary: string } | null> {
  const amend = options.includes("amend");
  const args = ["commit", "-F", "-"];
  if (amend) args.push("--amend");
  if (options.includes("signoff")) args.push("--signoff");
  if (!amend && !(await differs(repository, ["--cached"])) && (await differs(repository, []))) {
    const answer = await workbench.showConfirmation({
      title: "Stage all changes and commit?",
      message: "Nothing is staged. Every changed tracked file will be staged and committed.",
      confirmLabel: "Commit",
      ...(window === null ? {} : { target: window }),
    });
    if (answer.kind !== "confirmed") return null;
    args.push("-a");
  }
  const output = await gitLogged(repository, args, message);
  if (output.exitCode !== 0) throw new Error(reason(output.stderr + output.stdout));
  const head = await git(repository, ["log", "-1", "--format=%h: %s"]);
  return { summary: head?.stdout.trim() ?? "" };
}
