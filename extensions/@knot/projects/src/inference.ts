// Project inference: the nearest `.git` ancestor of a document that falls
// outside every open root. Pure decision logic over explicit inputs, so tests
// exercise it against the fake host's `files` without opening anything; the
// confirmation and window mutation land with the offer wiring.
import { files } from "knot";

export type InferenceInput = {
  /** The document's stored resource URI, or null for untitled documents and destinations without one. */
  documentUri: string | null;
  /** Roots of the window presenting the document. */
  windowRoots: readonly string[];
  /** Roots of every open window. */
  allRoots: readonly string[];
  /** The `homeUri` host fact, or null when no home directory is known. */
  homeUri: string | null;
  /** Normalized dismissed roots. */
  dismissed: ReadonlySet<string>;
};

/** The URI without trailing slashes, mirroring `trimmed` in the explorer's rows. */
export function trimmedUri(uri: string): string {
  return uri.replace(/\/+$/, "");
}

/** The comparison and storage form for a root URI. */
export function normalizeRoot(uri: string): string {
  return trimmedUri(uri);
}

/** Whether `uri` is `base` or below it, by string prefix like the explorer's `idPath`. */
function within(uri: string, base: string): boolean {
  const target = trimmedUri(uri);
  const root = trimmedUri(base);
  return target === root || target.startsWith(`${root}/`);
}

function sameRoot(left: string, right: string): boolean {
  return normalizeRoot(left) === normalizeRoot(right);
}

function splitFileUri(uri: string): { origin: string; path: string } | null {
  const match = /^file:\/\/([^/]*)(\/.*)$/i.exec(trimmedUri(uri));
  if (match === null) return null;
  return { origin: `file://${match[1]}`, path: match[2]! };
}

/**
 * The nearest ancestor directory of `documentUri` containing a `.git` entry,
 * in root form with a trailing slash. A `.git` file (worktree, submodule)
 * counts like a directory; the walk stops at the first match, so nested
 * repositories infer their inner root. Null when the URI is not a stored
 * `file:` URI, when no ancestor carries the marker, or when a stat fails, so
 * a later document may retry.
 */
export async function inferRoot(documentUri: string | null): Promise<string | null> {
  if (documentUri === null) return null;
  const split = splitFileUri(documentUri);
  if (split === null) return null;
  let directory = split.path.slice(0, split.path.lastIndexOf("/"));
  if (directory === "") directory = "/";
  for (;;) {
    const candidate = `${split.origin}${directory === "/" ? "" : directory}/`;
    let marker;
    try {
      marker = await files.stat(`${candidate}.git`);
    } catch {
      return null;
    }
    if (marker !== null) return candidate;
    if (directory === "/") return null;
    directory = directory.slice(0, directory.lastIndexOf("/")) || "/";
  }
}

/**
 * The root to offer for `input`, or null when no offer applies: an unstored
 * or non-`file:` document, a document already under one of its window's
 * roots, no `.git` ancestor, the filesystem root, the home directory, a root
 * already open in some window, or a dismissed root.
 */
export async function inferProjectRoot(input: InferenceInput): Promise<string | null> {
  if (input.documentUri === null) return null;
  if (input.windowRoots.some((root) => within(input.documentUri!, root))) return null;
  const inferred = await inferRoot(input.documentUri);
  if (inferred === null) return null;
  const split = splitFileUri(input.documentUri);
  if (split !== null && sameRoot(inferred, `${split.origin}/`)) return null;
  if (input.homeUri !== null && sameRoot(inferred, input.homeUri)) return null;
  if (input.allRoots.some((root) => sameRoot(inferred, root))) return null;
  if (input.dismissed.has(normalizeRoot(inferred))) return null;
  return inferred;
}
