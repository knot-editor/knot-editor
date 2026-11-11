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

/** The UTF-16 range of `name` to select when renaming: a file's name without
 * its extension, a folder's whole name. */
export function renameSelection(name: string, directory: boolean): readonly [number, number] {
  const dot = name.lastIndexOf(".");
  return directory || dot <= 0 ? [0, name.length] : [0, dot];
}
