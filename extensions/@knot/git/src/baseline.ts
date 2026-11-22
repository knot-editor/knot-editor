// The baseline of a file is its text in HEAD as the working tree would have
// it, which `cat-file --filters` produces: line-ending and other smudge
// filters apply, so a file compares cleanly with its checked-out form. A file
// the index holds but HEAD lacks has an empty baseline and shows as added.
import { git } from "./git.js";
import { relative } from "./uri.js";

/** Text with a NUL is binary, which has no baseline. */
function textOf(output: string): string | null {
  return output.includes("\0") ? null : output;
}

/** The text of `file` in the HEAD of `repository`, or null when it has none. */
export async function baseline(repository: string, file: string): Promise<string | null> {
  const path = relative(repository, file);
  if (path === null) return null;
  const head = await git(repository, ["cat-file", "--filters", `HEAD:${path}`]);
  if (head === null) return null;
  if (head.exitCode === 0) return textOf(head.stdout);
  // Not in HEAD, or no HEAD yet: staged files are all added, others have no baseline.
  const staged = await git(repository, ["ls-files", "--stage", "--", path]);
  if (staged === null || staged.exitCode !== 0 || !staged.stdout.startsWith("100")) return null;
  const index = await git(repository, ["cat-file", "--filters", `:${path}`]);
  return index !== null && index.exitCode === 0 && textOf(index.stdout) !== null ? "" : null;
}
