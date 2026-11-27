// The baseline of a file is its text in HEAD as the working tree would have
// it, which `cat-file --filters` produces: line-ending and other smudge
// filters apply, so a file compares cleanly with its checked-out form. A file
// the index holds but HEAD lacks has an empty baseline and shows as added. The
// staged text is the index entry's, read the same way.
import { git } from "./git.js";
import { relative } from "./uri.js";

/** Text with a NUL is binary, which has no baseline. */
function textOf(output: string): string | null {
  return output.includes("\0") ? null : output;
}

/** The index entry of `path` as the working tree would have it, or null when it has none or is binary. */
async function indexText(repository: string, path: string): Promise<string | null> {
  const index = await git(repository, ["cat-file", "--filters", `:${path}`]);
  return index !== null && index.exitCode === 0 ? textOf(index.stdout) : null;
}

/** The baseline of `file` in the HEAD of `repository` and its staged text, or null when it has no baseline. */
export async function baseline(repository: string, file: string): Promise<{ text: string; staged?: string } | null> {
  const path = relative(repository, file);
  if (path === null) return null;
  const head = await git(repository, ["cat-file", "--filters", `HEAD:${path}`]);
  if (head === null) return null;
  let text: string;
  if (head.exitCode === 0) {
    const found = textOf(head.stdout);
    if (found === null) return null;
    text = found;
  } else {
    // Not in HEAD, or no HEAD yet: staged files are all added, others have no baseline.
    const entry = await git(repository, ["ls-files", "--stage", "--", path]);
    if (entry === null || entry.exitCode !== 0 || !entry.stdout.startsWith("100")) return null;
    if ((await indexText(repository, path)) === null) return null;
    text = "";
  }
  const staged = await indexText(repository, path);
  return staged === null || staged === text ? { text } : { text, staged };
}
