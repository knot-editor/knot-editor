// Parses Ex command lines: the commands the package runs, `:set` arguments,
// and the paths `:e` opens; and completes them through the shared matcher.
import { completion, type InputCompletion } from "knot";

export type BooleanOption = "number" | "relativenumber" | "wrap" | "expandtab" | "ignorecase" | "smartcase";
export type NumberOption = "tabstop" | "shiftwidth";

/** One `:set` argument: turning a boolean option on, off, or over, giving a
 * number option a value, or showing either's value. */
export type SetArgument =
  | { readonly option: BooleanOption; readonly action: "on" | "off" | "toggle" | "show" }
  | { readonly option: NumberOption; readonly action: "show" }
  | { readonly option: NumberOption; readonly action: "set"; readonly value: number };

/** A line of an Ex range: a base and an offset from it. */
export type Address = {
  readonly base:
    | { readonly kind: "line"; readonly line: number }
    | { readonly kind: "current" }
    | { readonly kind: "last" }
    | { readonly kind: "mark"; readonly name: string };
  readonly offset: number;
};

/** The lines an Ex command runs over; `:%` is the first line through `$`. */
export type ExRange = { readonly first: Address; readonly last: Address };

export type SubstituteFlags = {
  /** Every match on a line, not the first. */
  readonly global: boolean;
  /** `i` ignores case, `I` respects it, null leaves it to the options. */
  readonly ignoreCase: boolean | null;
  /** Count the matches only. */
  readonly count: boolean;
  /** No error when nothing matches. */
  readonly quiet: boolean;
};

export type ExCommand =
  | { readonly kind: "write" }
  | { readonly kind: "quit"; readonly force: boolean }
  /** `:wq`, or with `ifChanged` `:x`, which writes only a changed document. */
  | { readonly kind: "write-quit"; readonly ifChanged: boolean }
  | { readonly kind: "edit"; readonly path: string | null }
  /** `:<line>`, 1-based, or null for `:$`. */
  | { readonly kind: "line"; readonly line: number | null }
  | { readonly kind: "set"; readonly arguments: readonly SetArgument[] }
  /** `:s`, over `range` or the current line. An empty `pattern` is the last
   * search pattern. */
  | {
    readonly kind: "substitute";
    readonly range: ExRange | null;
    readonly pattern: string;
    readonly replacement: string;
    readonly flags: SubstituteFlags;
  }
  /** `:noh`, which hides the search highlights until the next search. */
  | { readonly kind: "nohlsearch" }
  /** An empty line, which does nothing. */
  | { readonly kind: "none" }
  | { readonly kind: "error"; readonly message: string };

/** Command names, each with the shortest abbreviation Vim accepts. */
const NAMES: readonly (readonly [string, number])[] = [
  ["write", 1], ["quit", 1], ["wq", 2], ["xit", 1], ["exit", 3], ["edit", 1], ["set", 2],
  ["nohlsearch", 3], ["substitute", 1],
];

const BOOLEANS: Readonly<Record<string, BooleanOption>> = {
  number: "number", nu: "number",
  relativenumber: "relativenumber", rnu: "relativenumber",
  wrap: "wrap",
  expandtab: "expandtab", et: "expandtab",
  ignorecase: "ignorecase", ic: "ignorecase",
  smartcase: "smartcase", scs: "smartcase",
};

const NUMBERS: Readonly<Record<string, NumberOption>> = {
  tabstop: "tabstop", ts: "tabstop",
  shiftwidth: "shiftwidth", sw: "shiftwidth",
};

/** The largest tab size editors use. */
const MAX_SIZE = 32;

export function parseEx(text: string): ExCommand {
  let line = text.replace(/^[\s:]+/, "").trimEnd();
  if (line === "") return { kind: "none" };
  if (/^\d+$/.test(line)) return { kind: "line", line: Math.max(1, Number(line)) };
  if (line === "$") return { kind: "line", line: null };
  const parsed = parseRange(line);
  if (parsed.range !== null && parsed.rest === "") return error(`E492: Not an editor command: ${line}`);
  const range = parsed.range;
  line = parsed.rest;
  const match = /^([a-zA-Z]+)(!?)\s*(.*)$/.exec(line);
  const name = match === null ? null : fullName(match[1]!);
  if (match === null || name === null) return error(`E492: Not an editor command: ${line}`);
  const force = match[2] === "!";
  const argument = match[3]!;
  if (name === "substitute") return parseSubstitute(range, argument);
  if (range !== null) return error("E481: No range allowed");
  if (name === "set") return parseSet(argument);
  if (name === "edit") {
    return { kind: "edit", path: argument === "" ? null : argument };
  }
  if (argument !== "") return error(`E488: Trailing characters: ${argument}`);
  switch (name) {
    case "write": return { kind: "write" };
    case "nohlsearch": return { kind: "nohlsearch" };
    case "quit": return { kind: "quit", force };
    case "wq": return { kind: "write-quit", ifChanged: false };
    default: return { kind: "write-quit", ifChanged: true };
  }
}

/** The range at the start of `line` and what follows it. `%` is every line;
 * `,` joins two addresses; an address is a number, `.`, `$` or `'x`, then
 * any `+N` and `-N` offsets, and an offset alone counts from the current
 * line. */
function parseRange(line: string): { readonly range: ExRange | null; readonly rest: string } {
  if (line.startsWith("%")) {
    const first: Address = { base: { kind: "line", line: 1 }, offset: 0 };
    return { range: { first, last: { base: { kind: "last" }, offset: 0 } }, rest: line.slice(1).trimStart() };
  }
  const first = parseAddress(line);
  if (first === null) return { range: null, rest: line };
  let rest = first.rest;
  let last = first.address;
  const comma = /^\s*,/.exec(rest);
  if (comma !== null) {
    const second = parseAddress(rest.slice(comma[0].length));
    if (second === null) return { range: null, rest: line };
    last = second.address;
    rest = second.rest;
  }
  return { range: { first: first.address, last }, rest: rest.trimStart() };
}

function parseAddress(text: string): { readonly address: Address; readonly rest: string } | null {
  const base = /^\s*(?:(\d+)|(\.)|(\$)|'(.))/.exec(text);
  let rest = base === null ? text.trimStart() : text.slice(base[0].length);
  let kind: Address["base"];
  if (base === null) kind = { kind: "current" };
  else if (base[1] !== undefined) kind = { kind: "line", line: Number(base[1]) };
  else if (base[2] !== undefined) kind = { kind: "current" };
  else if (base[3] !== undefined) kind = { kind: "last" };
  else kind = { kind: "mark", name: base[4]! };
  let offset = 0;
  let any = base !== null;
  for (let step = /^\s*([+-])(\d*)/.exec(rest); step !== null; step = /^\s*([+-])(\d*)/.exec(rest)) {
    offset += (step[1] === "-" ? -1 : 1) * (step[2] === "" ? 1 : Number(step[2]));
    rest = rest.slice(step[0].length);
    any = true;
  }
  return any ? { address: { base: kind, offset }, rest } : null;
}

/** `:s/pattern/replacement/flags` with any delimiter a letter, digit, backslash,
 * `"` or `|` is not; a backslash before the delimiter makes it literal. */
function parseSubstitute(range: ExRange | null, text: string): ExCommand {
  const delimiter = text[0];
  if (delimiter === undefined || /[\p{L}\p{N}\\"|\s]/u.test(delimiter)) {
    return error(`E492: Not an editor command: s${text}`);
  }
  let index = 1;
  const field = (): string => {
    let out = "";
    while (index < text.length && text[index] !== delimiter) {
      if (text[index] === "\\" && index + 1 < text.length) {
        if (text[index + 1] !== delimiter) out += "\\";
        index++;
      }
      out += text[index++];
    }
    index++;
    return out;
  };
  const pattern = field();
  const replacement = field();
  const flags = { global: false, ignoreCase: null as boolean | null, count: false, quiet: false };
  for (const flag of text.slice(Math.min(index, text.length)).trim()) {
    if (flag === "g") flags.global = !flags.global;
    else if (flag === "i") flags.ignoreCase = true;
    else if (flag === "I") flags.ignoreCase = false;
    else if (flag === "n") flags.count = true;
    else if (flag === "e") flags.quiet = true;
    else return error(`E488: Trailing characters: ${text.slice(Math.min(index, text.length)).trim()}`);
  }
  return { kind: "substitute", range, pattern, replacement, flags };
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
  ["ignorecase", "ic"], ["smartcase", "scs"],
  ["tabstop", "ts"], ["shiftwidth", "sw"],
];

/** Completions for the Ex line `text`, best first: command names while the
 * line has no space, then `:set` option names for the last word, keeping its
 * `no` or `inv` prefix. Each item's value is the whole new line. */
export async function completeEx(text: string): Promise<InputCompletion[]> {
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
      ...(short === "" ? {} : { description: short }),
      ...(candidates[index] === full ? { matches: ranges } : {}),
    });
  }
  return items;
}

/** The `:e` line `text` lists files for: the line up to the path, and the path
 * typed so far. `:e` alone lists files too; null for any other line. */
export function fileCompletionTarget(text: string): { readonly head: string; readonly query: string } | null {
  const match = /^(\s*:?\s*(?:e|ed|edi|edit)!?)(?:\s+(.*))?$/.exec(text);
  return match === null ? null : { head: match[1]!, query: match[2] ?? "" };
}

/** The text `:e` takes for the resource `uri`: a file's path, or the URI. */
export function pathOfUri(uri: string): string {
  return uri.startsWith("file://") ? decodeURIComponent(uri.slice("file://".length)) : uri;
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
