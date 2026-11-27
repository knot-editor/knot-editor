// Running Git. Every call sets GIT_OPTIONAL_LOCKS=0 so that reading never
// takes the index lock a user's own command may need, and clears the
// variables that would point Git at another repository than the directory.
import { processes } from "knot";
import { vcs } from "@knot/vcs";

export type Output = { readonly exitCode: number | null; readonly stdout: string; readonly stderr: string };

/**
 * Runs `git` in the directory `cwd`, feeding `stdin` when given. Resolves null
 * when the directory does not exist; rejects `ProgramNotFound` when Git is not
 * installed.
 */
export async function git(cwd: string, args: readonly string[], stdin?: string): Promise<Output | null> {
  try {
    const { exitCode, stdout, stderr } = await processes.run({
      command: "git",
      args: ["--literal-pathspecs", ...args],
      cwd,
      ...(stdin === undefined ? {} : { stdin }),
      env: { GIT_OPTIONAL_LOCKS: "0", GIT_DIR: null, GIT_WORK_TREE: null, GIT_INDEX_FILE: null },
    });
    return { exitCode, stdout, stderr };
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
  return killed ? null : { exitCode, stdout, stderr: "" };
}

const quoted = (arg: string) => (/^[\w@%+=:,./-]+$/.test(arg) ? arg : `'${arg.replace(/'/g, `'\\''`)}'`);

/**
 * Runs `git` for a command the user started, adding it and its output to the
 * Version Control output. Rejects when Git cannot run in `repository`.
 */
export async function gitLogged(repository: string, args: readonly string[], stdin?: string): Promise<Output> {
  const output = await git(repository, args, stdin);
  if (output === null) throw new Error("The repository is gone");
  await (await vcs.client())
    .log({
      backend: "git",
      repository,
      command: `git ${args.map(quoted).join(" ")}`,
      output: output.stdout + output.stderr,
      failed: output.exitCode !== 0,
    })
    .catch(() => {});
  return output;
}
