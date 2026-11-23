// Watching repository metadata. A commit, checkout, reset, or `git add` made
// outside Knot changes HEAD, a ref, `packed-refs`, or the index, so those are
// watched for each repository the backend has served and `vcs` is told when
// one moves. The directories are watched with `includeHidden`, since `.git`
// is hidden, and the object store is left alone: it is large and busy, and
// nothing the baselines depend on is found there. The working tree is watched
// too, since an edit on disk changes the status; a batch of only ignored
// files, such as a build's output, is skipped.
import { files } from "knot";
import type { Disposable } from "knot";
import { vcs } from "@knot/vcs";
import { git } from "./git.js";
import { below, trimmed } from "./uri.js";

/** What a repository's watches hold. */
type Watched = { readonly disposables: Disposable[] };

const watched = new Map<string, Promise<Watched | null>>();

/** The URI of an absolute POSIX path. */
function toUri(path: string): string {
  return `file://${path.split("/").map((segment) => encodeURIComponent(segment)).join("/")}`;
}

/** Whether a changed URI is one a baseline depends on: HEAD, the index, packed refs, or a ref. */
function matters(uri: string, directories: readonly string[]): boolean {
  if (directories.includes(trimmed(uri))) return true;
  return /\/(HEAD|index|packed-refs)$/.test(uri) || uri.includes("/refs/");
}

/** Whether any of the changed URIs is one the ignore policy keeps. */
async function anyTracked(uris: readonly string[]): Promise<boolean> {
  for (const uri of uris) {
    if (!(await files.isIgnored(uri).catch(() => false))) return true;
  }
  return false;
}

async function start(repository: string): Promise<Watched | null> {
  const result = await git(repository, [
    "rev-parse", "--absolute-git-dir", "--path-format=absolute", "--git-common-dir",
  ]);
  if (result === null || result.exitCode !== 0) return null;
  const [gitDir, commonDir] = result.stdout.split("\n");
  if (!gitDir?.startsWith("/") || !commonDir?.startsWith("/")) return null;
  const own = toUri(gitDir);
  const common = toUri(commonDir);
  const directories = [...new Set([own, common])];
  const refs = below(common, "refs");
  const everything = [...directories, refs];
  const disposables: Disposable[] = [];
  const stop = async () => {
    for (const disposable of disposables.splice(0)) await disposable[Symbol.asyncDispose]().catch(() => {});
    watched.delete(repository);
  };
  const listener = async (uris: readonly string[]) => {
    if (!uris.some((uri) => matters(uri, everything))) return;
    // A deleted repository ends its watches, and the files in it have no baseline.
    const gone = (await files.stat(own).catch(() => null)) === null;
    if (gone) await stop();
    try {
      await (await vcs.client()).changed({ backend: "git", repository });
    } catch {
      // `@knot/vcs` stopped: the next lookup watches again.
    }
  };
  const tree = async (uris: readonly string[]) => {
    if (!(await anyTracked(uris))) return;
    try {
      await (await vcs.client()).statusChanged({ backend: "git", repository });
    } catch {
      // `@knot/vcs` stopped: the next lookup watches again.
    }
  };
  try {
    for (const directory of directories) {
      disposables.push(await files.watch(directory, listener, { includeHidden: true }));
    }
    disposables.push(await files.watch(refs, listener, { recursive: true, includeHidden: true }));
    disposables.push(await files.watch(repository, tree, { recursive: true }));
  } catch {
    await stop();
    return null;
  }
  return { disposables };
}

/** Starts watching the repository's metadata unless it already is. Resolves once the watches run. */
export async function watch(repository: string): Promise<void> {
  const key = trimmed(repository);
  if (!watched.has(key)) watched.set(key, start(key).catch(() => null));
  await watched.get(key);
}
