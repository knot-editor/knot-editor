// Validation and URIs of names typed into the explorer.

/** The path segments of a typed name, or why it is not a valid path. */
export function pathSegments(name: string): { readonly segments: string[] } | { readonly problem: string } {
  if (name.startsWith("/")) return { problem: "A name cannot start with a slash" };
  const segments = name.split("/");
  if (segments.some((segment) => segment === "")) return { problem: "A name cannot be empty or end with a slash" };
  if (segments.some((segment) => segment === "." || segment === "..")) {
    return { problem: "A name cannot be . or .." };
  }
  return { segments };
}

/** The URI of the typed path below `directory`. */
export function childUri(directory: string, segments: readonly string[]): string {
  return [directory.replace(/\/+$/, ""), ...segments.map((segment) => encodeURIComponent(segment))].join("/");
}

/** The name of the nth copy of `name` (1 is "name copy", 2 is "name copy 2"),
 * the extension of a file staying last. */
export function copyName(name: string, directory: boolean, n: number): string {
  const dot = name.lastIndexOf(".");
  const split = directory || dot <= 0 ? name.length : dot;
  return `${name.slice(0, split)} copy${n > 1 ? ` ${n}` : ""}${name.slice(split)}`;
}

/** The UTF-16 range of `name` to select when renaming: a file's name without
 * its extension, a folder's whole name. */
export function renameSelection(name: string, directory: boolean): readonly [number, number] {
  const dot = name.lastIndexOf(".");
  return directory || dot <= 0 ? [0, name.length] : [0, dot];
}
