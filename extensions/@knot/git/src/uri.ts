// File URI arithmetic. URIs stay strings and are compared lexically, so a
// symlinked directory is never replaced by its real path, which would no
// longer match the document URIs and window roots the provider hands out.

/** The URI without trailing slashes. */
export function trimmed(uri: string): string {
  let end = uri.length;
  while (end > "file:///".length && uri[end - 1] === "/") end -= 1;
  return uri.slice(0, end);
}

/** Whether `file` is `root` or lies below it, at a path boundary. */
export function contains(root: string, file: string): boolean {
  const base = trimmed(root);
  return file === base || file.startsWith(`${base}/`);
}

/** The URI of the directory `levels` segments above `uri`. */
export function ancestor(uri: string, levels: number): string {
  let result = trimmed(uri);
  for (let level = 0; level < levels; level += 1) {
    const cut = result.lastIndexOf("/");
    if (cut < "file://".length + 1) break;
    result = result.slice(0, cut);
  }
  return result;
}

/** The path of `file` below `root`, decoded, or null when it is not below. */
export function relative(root: string, file: string): string | null {
  const base = trimmed(root);
  if (!file.startsWith(`${base}/`)) return null;
  try {
    return decodeURIComponent(file.slice(base.length + 1));
  } catch {
    return null;
  }
}

/** The URI of `path`, a relative path of the segments `a/b`, below `root`. */
export function below(root: string, path: string): string {
  const segments = path.split("/").filter((segment) => segment !== "");
  return [trimmed(root), ...segments.map(encodeURIComponent)].join("/");
}
