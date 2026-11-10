// Parses Ex command lines: the commands the package runs, `:set` arguments,
// and the paths `:e` opens; and completes them through the shared matcher.
import { completion, files, type InputCompletion } from "knot";

export type BooleanOption = "number" | "relativenumber" | "wrap" | "expandtab";
export type NumberOption = "tabstop" | "shiftwidth";

/** One `:set` argument: turning a boolean option on, off, or over, giving a
 * number option a value, or showing either's value. */
export type SetArgument =
  | { readonly option: BooleanOption; readonly action: "on" | "off" | "toggle" | "show" }
  | { readonly option: NumberOption; readonly action: "show" }
  | { readonly option: NumberOption; readonly action: "set"; readonly value: number };

export type ExCommand =
  | { readonly kind: "write" }
  | { readonly kind: "quit"; readonly force: boolean }
  /** `:wq`, or with `ifChanged` `:x`, which writes only a changed document. */
  | { readonly kind: "write-quit"; readonly ifChanged: boolean }
  | { readonly kind: "edit"; readonly path: string }
  /** `:<line>`, 1-based, or null for `:$`. */
  | { readonly kind: "line"; readonly line: number | null }
  | { readonly kind: "set"; readonly arguments: readonly SetArgument[] }
  /** An empty line, which does nothing. */
  | { readonly kind: "none" }
  | { readonly kind: "error"; readonly message: string };

/** Command names, each with the shortest abbreviation Vim accepts. */
const NAMES: readonly (readonly [string, number])[] = [
  ["write", 1], ["quit", 1], ["wq", 2], ["xit", 1], ["exit", 3], ["edit", 1], ["set", 2],
];

const BOOLEANS: Readonly<Record<string, BooleanOption>> = {
  number: "number", nu: "number",
  relativenumber: "relativenumber", rnu: "relativenumber",
  wrap: "wrap",
  expandtab: "expandtab", et: "expandtab",
};

const NUMBERS: Readonly<Record<string, NumberOption>> = {
  tabstop: "tabstop", ts: "tabstop",
  shiftwidth: "shiftwidth", sw: "shiftwidth",
};

/** The largest tab size editors use. */
const MAX_SIZE = 32;

export function parseEx(text: string): ExCommand {
  const line = text.replace(/^[\s:]+/, "").trimEnd();
  if (line === "") return { kind: "none" };
  if (/^\d+$/.test(line)) return { kind: "line", line: Math.max(1, Number(line)) };
  if (line === "$") return { kind: "line", line: null };
  const match = /^([a-zA-Z]+)(!?)\s*(.*)$/.exec(line);
  const name = match === null ? null : fullName(match[1]!);
  if (match === null || name === null) return error(`E492: Not an editor command: ${line}`);
  const force = match[2] === "!";
  const argument = match[3]!;
  if (name === "set") return parseSet(argument);
  if (name === "edit") {
    return argument === "" ? error("E32: No file name") : { kind: "edit", path: argument };
  }
  if (argument !== "") return error(`E488: Trailing characters: ${argument}`);
  switch (name) {
    case "write": return { kind: "write" };
    case "quit": return { kind: "quit", force };
    case "wq": return { kind: "write-quit", ifChanged: false };
    default: return { kind: "write-quit", ifChanged: true };
  }
}

function fullName(typed: string): string | null {
  for (const [name, shortest] of NAMES) {
    if (typed.length >= shortest && name.startsWith(typed)) return name;
  }
  return null;
}

function error(message: string): ExCommand {
  return { kind: "error", message };
}

/** `:set` arguments: `opt`, `noopt`, `invopt`, `opt!`, `opt?`, `opt=N`, and
 * `opt:N`. */
function parseSet(text: string): ExCommand {
  const parsed: SetArgument[] = [];
  for (const word of text.split(/\s+/).filter((word) => word !== "")) {
    const match = /^([a-z]+)([!?]|[=:](.*))?$/.exec(word);
    if (match === null) return error(`E518: Unknown option: ${word}`);
    const [, name, suffix, value] = match as unknown as [string, string, string | undefined, string | undefined];
    const number = NUMBERS[name];
    if (number !== undefined) {
      if (suffix === undefined || suffix === "?") {
        parsed.push({ option: number, action: "show" });
      } else if (value !== undefined && /^\d+$/.test(value) && Number(value) >= 1 && Number(value) <= MAX_SIZE) {
        parsed.push({ option: number, action: "set", value: Number(value) });
      } else {
        return error(`E474: Invalid argument: ${word}`);
      }
      continue;
    }
    const plain = BOOLEANS[name];
    const negated = name.startsWith("no") ? BOOLEANS[name.slice(2)] : undefined;
    const inverted = name.startsWith("inv") ? BOOLEANS[name.slice(3)] : undefined;
    const option = plain ?? negated ?? inverted;
    if (option === undefined) {
      return error(`E518: Unknown option: ${name}`);
    }
    if (value !== undefined || (suffix !== undefined && plain === undefined)) {
      return error(`E474: Invalid argument: ${word}`);
    }
    const action = suffix === "?" ? "show" : suffix === "!" || inverted !== undefined ? "toggle"
      : negated !== undefined ? "off" : "on";
    parsed.push({ option, action });
  }
  return { kind: "set", arguments: parsed };
}

/** Options completed by `:set`, each under its full name with its short form. */
const OPTIONS: readonly (readonly [string, string])[] = [
  ["number", "nu"], ["relativenumber", "rnu"], ["wrap", ""], ["expandtab", "et"],
  ["tabstop", "ts"], ["shiftwidth", "sw"],
];

/** Completions for the Ex line `text`, best first: command names while the
 * line has no space, then `:set` option names for the last word, keeping its
 * `no` or `inv` prefix, and for `:e` the entries of the typed directory,
 * resolved against `directory` as `:e` opens them. Each item's value is the
 * whole new line. */
export async function completeEx(text: string, directory: string | null): Promise<InputCompletion[]> {
  const name = /^(\s*:?\s*)([a-zA-Z]*)$/.exec(text);
  if (name !== null) {
    const names = NAMES.map(([full]) => full);
    const matches = await completion.match(name[2]!, names);
    return matches.map(({ index, ranges }) => ({
      label: names[index]!,
      value: `${name[1]}${names[index]}`,
      matches: ranges,
    }));
  }
  const edit = /^(\s*:?\s*e(?:d(?:i(?:t)?)?)?!?\s+)(.*)$/.exec(text);
  if (edit !== null) return completePath(edit[1]!, edit[2]!, directory);
  // No option begins with `no` or `inv`, so they are prefixes to keep.
  const set = /^(\s*:?\s*se(?:t)?\s+(?:.*\s)?(?:no|inv)?)([a-z]*)$/.exec(text);
  if (set === null) return [];
  const head = set[1]!;
  const typed = set[2]!;
  // Full names and short forms are matched, and each option is listed once,
  // under its full name, at its best match.
  const candidates = OPTIONS.flatMap(([full, short]) => (short === "" ? [full] : [full, short]));
  const items: InputCompletion[] = [];
  for (const { index, ranges } of await completion.match(typed, candidates)) {
    const option = OPTIONS.find(([full, short]) => full === candidates[index] || short === candidates[index])!;
    const [full, short] = option;
    if (items.some((item) => item.label === full)) continue;
    items.push({
      label: full,
      value: `${head}${full}`,
      ...(short === "" ? {} : { detail: short }),
      ...(candidates[index] === full ? { matches: ranges } : {}),
    });
  }
  return items;
}

/** The entries of the directory `typed` names up to its last `/`, whose names
 * match the rest, ignored ones left out; directories end in `/`. */
async function completePath(head: string, typed: string, directory: string | null): Promise<InputCompletion[]> {
  const slash = typed.lastIndexOf("/") + 1;
  const prefix = typed.slice(0, slash);
  const uri = resolvePath(prefix, directory);
  if (uri === null) return [];
  let entries;
  try {
    entries = (await files.list(uri)).filter((entry) => !entry.ignored);
  } catch {
    return [];
  }
  entries.sort((a, b) => a.name.localeCompare(b.name));
  const names = entries.map((entry) => entry.name);
  const matches = await completion.match(typed.slice(slash), names);
  return matches.map(({ index, ranges }) => {
    const entry = entries[index]!;
    const label = entry.kind === "directory" ? `${entry.name}/` : entry.name;
    return { label, value: `${head}${prefix}${label}`, matches: ranges };
  });
}

/** The URI `:e` opens for `path`: a URI as it is, an absolute path as a
 * file URI, and a relative one in `directory`, a directory URI, or null
 * when there is none. */
export function resolvePath(path: string, directory: string | null): string | null {
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(path)) return path;
  const segments = path.split("/").map((segment) => encodeURIComponent(segment));
  if (path.startsWith("/")) return `file://${normalize(segments).join("/")}`;
  if (directory === null) return null;
  const base = /^([a-zA-Z][a-zA-Z0-9+.-]*:\/\/[^/]*)(.*)$/.exec(directory);
  if (base === null) return null;
  return `${base[1]}${normalize([...base[2]!.split("/"), ...segments]).join("/")}`;
}

/** The directory URI holding the resource at `uri`. */
export function directoryOf(uri: string): string {
  return uri.slice(0, uri.lastIndexOf("/"));
}

/** Path segments without `.`, `..`, or empty ones, after a leading empty
 * segment, so they join into an absolute path. */
function normalize(segments: readonly string[]): string[] {
  const kept: string[] = [];
  for (const segment of segments) {
    if (segment === "" || segment === ".") continue;
    if (segment === "..") kept.pop();
    else kept.push(segment);
  }
  return ["", ...kept];
}
