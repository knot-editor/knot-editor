// Running Git. Every call sets GIT_OPTIONAL_LOCKS=0 so that reading never
// takes the index lock a user's own command may need, and clears the
// variables that would point Git at another repository than the directory.
import { processes } from "knot";

export type Output = { readonly exitCode: number | null; readonly stdout: string };

/**
 * Runs `git` in the directory `cwd`. Resolves null when the directory does
 * not exist; rejects `ProgramNotFound` when Git is not installed.
 */
export async function git(cwd: string, args: readonly string[]): Promise<Output | null> {
  try {
    const { exitCode, stdout } = await processes.run({
      command: "git",
      args: ["--literal-pathspecs", ...args],
      cwd,
      env: { GIT_OPTIONAL_LOCKS: "0", GIT_DIR: null, GIT_WORK_TREE: null, GIT_INDEX_FILE: null },
    });
    return { exitCode, stdout };
  } catch (error) {
    if ((error as { code?: string }).code === "WorkingDirectoryNotFound") return null;
    throw error;
  }
}

export function isMissingProgram(error: unknown): boolean {
  return (error as { code?: string }).code === "ProgramNotFound";
}

/**
 * Runs `git` like `git` does, but kills it as soon as `enough(stdout so far)`
 * holds, so output that would be thrown away is never read. Resolves null
 * when it was killed.
 */
export async function gitWhile(
  cwd: string,
  args: readonly string[],
  enough: (stdout: string) => boolean,
): Promise<Output | null> {
  const child = await processes.spawn({
    command: "git",
    args: ["--literal-pathspecs", ...args],
    cwd,
    env: { GIT_OPTIONAL_LOCKS: "0", GIT_DIR: null, GIT_WORK_TREE: null, GIT_INDEX_FILE: null },
  });
  let stdout = "";
  let killed = false;
  child.onStdout(async (text) => {
    if (killed) return;
    stdout += text;
    if (enough(stdout)) {
      killed = true;
      await child.kill().catch(() => {});
    }
  });
  const [{ exitCode }] = await Promise.all([child.exited, child.stdoutEnded]);
  return killed ? null : { exitCode, stdout };
}
