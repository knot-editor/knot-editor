import { processes } from "knot";

/**
 * The short `git status` of the repository at the folder `uri`. A package that
 * runs programs declares `"entitlements": ["process"]` in `knot.jsonc`.
 * @param {string} uri
 */
export async function gitStatus(uri) {
  const result = await processes.run({ command: "git", args: ["status", "--short"], cwd: uri });
  if (result.exitCode !== 0) throw new Error(result.stderr.trim());
  return result.stdout.split("\n").filter((line) => line !== "");
}

/**
 * Starts a program and calls `onLine` with each complete line of its output.
 * Aborting `signal` kills it, since cancelling a command does not.
 * @param {string} command
 * @param {readonly string[]} args
 * @param {(line: string) => void} onLine
 * @param {{ addEventListener(type: "abort", listener: () => void): void }} signal
 */
export async function followLines(command, args, onLine, signal) {
  const child = await processes.spawn({ command, args });
  // Best effort: the process may already have exited and been released.
  signal.addEventListener("abort", () => { void child.kill().catch(() => {}); });
  let partial = "";
  child.onStdout((text) => {
    const lines = (partial + text).split("\n");
    partial = lines.pop() ?? "";
    lines.forEach(onLine);
  });
  const exit = await child.exited;
  if (partial !== "") onLine(partial);
  await child.dispose();
  return exit;
}
