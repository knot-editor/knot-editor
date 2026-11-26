// The two sides of a file's diff in each status group. Texts are read with
// `cat-file --filters`, like baselines, so they compare cleanly with the
// checked-out form. A side the file lacks, such as HEAD for a new file or the
// working file once deleted, is empty.
import { git } from "./git.js";
import { parseGroups, type Groups } from "./status.js";
import { relative } from "./uri.js";

type Side = { readonly label: string; readonly text: string };
export type Comparison = { readonly original: Side; readonly modified: "document" | Side };

/** The text of `spec` (`HEAD:path` or `:path`): "" when it is missing, null when it is binary. */
async function read(repository: string, spec: string): Promise<string | null> {
  const output = await git(repository, ["cat-file", "--filters", spec]);
  if (output === null || output.exitCode !== 0) return "";
  return output.stdout.includes("\0") ? null : output.stdout;
}

/** The path `path` had in HEAD before the index renamed it, or `path` itself. */
async function origin(repository: string, path: string): Promise<string> {
  const output = await git(repository, ["diff", "--cached", "--name-status", "-M", "-z"]);
  if (output === null || output.exitCode !== 0) return path;
  const parts = output.stdout.split("\0");
  for (let at = 0; at < parts.length; at += 1) {
    if (!parts[at]?.startsWith("R")) continue;
    if (parts[at + 2] === path) return parts[at + 1] ?? path;
    at += 2;
  }
  return path;
}

/** The working file as a side, or `"document"` while it exists. */
async function working(repository: string, path: string): Promise<"document" | Side> {
  const gone = await git(repository, ["ls-files", "--deleted", "-z", "--", path]);
  return gone !== null && gone.stdout.split("\0").includes(path) ? { label: "Working Copy", text: "" } : "document";
}

/** The group lists of one path. */
async function groupsOfPath(repository: string, path: string): Promise<Groups | null> {
  const output = await git(repository, ["status", "--porcelain=v2", "-z", "--untracked-files=all", "--", path]);
  return output === null || output.exitCode !== 0 ? null : parseGroups(output.stdout);
}

/** The diff of `file` in `group` of `repository`, or null when the file has none there. */
export async function compare(repository: string, group: string, file: string): Promise<Comparison | null> {
  const path = relative(repository, file);
  if (path === null) return null;
  const found = await groupsOfPath(repository, path);
  if (found === null || !found[group as keyof Groups]?.includes(path)) return null;

  const side = (label: string, text: string | null) => (text === null ? null : { label, text });
  let original: Side | null;
  let modified: "document" | Side | null;
  if (group === "staged") {
    original = side("HEAD", await read(repository, `HEAD:${await origin(repository, path)}`));
    modified = side("Index", await read(repository, `:${path}`));
  } else if (group === "unstaged") {
    original = side("Index", await read(repository, `:${path}`));
    modified = await working(repository, path);
  } else if (group === "conflicts") {
    original = side("HEAD", await read(repository, `HEAD:${path}`));
    modified = await working(repository, path);
  } else {
    original = { label: "Empty", text: "" };
    modified = "document";
  }
  return original === null || modified === null ? null : { original, modified };
}
