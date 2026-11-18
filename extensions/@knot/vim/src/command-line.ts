// Runs the Ex line `:` opens in one followed editor, over native commands
// and document-scoped settings, and returns the message it leaves.
import { commands, documents, providers, settings, workbench, workspaces } from "knot";
import type { IconRef, InputCompletion } from "knot";
import {
  type Address, type BooleanOption, completeEx, directoryOf, type ExCommand, fileCompletionTarget, parseEx, pathOfUri, resolvePath,
  type ExRange, type SetArgument,
} from "./ex.js";
import { BufferLines } from "./lines.js";
import { grouped, markOffset, moveTo, prompted, run, type State } from "./normal.js";
import { translateReplacement } from "./pattern.js";
import { compile, IGNORE_CASE, invalidPattern, searchSettings, SMART_CASE } from "./search.js";
import { firstNonBlank } from "./text.js";

/** A row of the `workspace.files` provider's list, the contract Vim expects
 * of it; the row's ID is the file's URI. */
interface FileRow {
  readonly id: string;
  readonly label: string;
  readonly description?: string | null;
  readonly hint?: string | null;
  readonly icon?: IconRef | null;
  readonly labelMatches?: readonly (readonly [number, number])[] | null;
  readonly descriptionMatches?: readonly (readonly [number, number])[] | null;
}

/** The files `query` lists, or none when no package provides the list or its
 * call fails, as when it is restarting. */
async function fileRows(query: string): Promise<readonly FileRow[]> {
  try {
    const provider = await providers.get<{ rows(query: string): Promise<FileRow[]> }>("workspace.files");
    return provider === null ? [] : await provider.rows(query);
  } catch {
    return [];
  }
}

/** The completion that puts `row`'s path in the line that began with `head`. */
function fileCompletion(head: string, row: FileRow): InputCompletion {
  return {
    label: row.label,
    value: `${head} ${pathOfUri(row.id)}`,
    ...(row.description == null ? {} : { description: row.description }),
    ...(row.hint == null ? {} : { hint: row.hint }),
    ...(row.icon == null ? {} : { icon: row.icon }),
    ...(row.labelMatches == null ? {} : { matches: row.labelMatches }),
    ...(row.descriptionMatches == null ? {} : { descriptionMatches: row.descriptionMatches }),
  };
}

/** Asks for a command line and runs it; returns a message to show, or null.
 * Under `:e`, the list is the files provider's, and Enter on a row opens it. */
export async function commandLine(state: State, initial: string): Promise<string | null> {
  let files: readonly FileRow[] = [];
  let highlighted: number | null = null;
  const value = await prompted(state, async () => {
    const input = await workbench.showInput({
      title: "Ex Command",
      prompt: ":",
      value: initial,
      history: state.shared.registers.entries("command"),
      onChange: async (line) => {
        const target = fileCompletionTarget(line);
        if (target === null) {
          files = [];
          return { items: await completeEx(line) };
        }
        files = await fileRows(target.query);
        return { items: files.map((row) => fileCompletion(target.head, row)) };
      },
    });
    if (input.kind === "dismissed") return null;
    highlighted = input.highlighted;
    return input.value;
  });
  if (value === null) return null;
  const row = highlighted === null ? undefined : files[highlighted];
  if (row !== undefined) return open(row.id, row.label);
  const command = parseEx(value);
  if (command.kind === "none") return null;
  const line = value.replace(/^[\s:]+/, "").trimEnd();
  state.shared.registers.set(":", { text: line, linewise: false });
  state.shared.registers.remember("command", line);
  state.shared.saveRegisters();
  return runEx(state, command);
}

async function runEx(state: State, command: ExCommand): Promise<string | null> {
  switch (command.kind) {
    case "none":
      return null;
    case "error":
      return command.message;
    case "line":
      // `:N` goes where `NG` does, and is a jump too.
      await run(state, { kind: "motion", motion: "last-line", count: command.line });
      return null;
    case "write":
      await commands.invoke("file.save");
      return null;
    case "quit":
      await workbench.close(state.target, { discard: command.force });
      return null;
    case "write-quit": {
      const dirty = (await documents.get(state.document))?.dirty ?? false;
      if (!command.ifChanged || dirty) {
        if ((await commands.invoke("file.save")).kind !== "completed") return null;
      }
      await workbench.close(state.target);
      return null;
    }
    case "edit":
      return edit(state, command.path);
    case "set":
      return set(state, command.arguments);
    case "substitute":
      return substitute(state, command);
    case "nohlsearch":
      await state.shared.highlights.hide();
      return null;
  }
}

/** The 0-based first and last line of `range`, or the current line without
 * one; a message when an address is outside the buffer or a mark is unset. */
async function resolveRange(
  state: State,
  lines: BufferLines,
  range: ExRange | null,
): Promise<readonly [number, number] | string> {
  const total = await lines.count();
  const current = (await lines.position((await state.handle.selection()).caret)).line;
  const address = async ({ base, offset }: Address): Promise<number | string> => {
    switch (base.kind) {
      case "line": return base.line - 1 + offset;
      case "current": return current + offset;
      case "last": return total - 1 + offset;
      case "mark": {
        const at = await markOffset(state, base.name);
        return at === null ? "E20: Mark not set" : (await lines.position(at)).line + offset;
      }
    }
  };
  if (range === null) return [current, current];
  const first = await address(range.first);
  const last = await address(range.last);
  if (typeof first === "string") return first;
  if (typeof last === "string") return last;
  if (Math.min(first, last) < 0 || Math.max(first, last) >= total) return "E16: Invalid range";
  return [Math.min(first, last), Math.max(first, last)];
}

/** The most matches one substitution reads. */
const SUBSTITUTE_LIMIT = 100_000;

/** The most substitutions made without a report, as Vim's `report`. */
const REPORT = 2;

const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? "" : "s"}`;

/** `:s`: replaces the matches in the range in one native replace, so one undo
 * step. The pattern becomes the last search pattern, and the caret goes to
 * the first non-blank of the last line substituted. */
async function substitute(state: State, command: Extract<ExCommand, { kind: "substitute" }>): Promise<string | null> {
  const lines = new BufferLines(state.buffer);
  const resolved = await resolveRange(state, lines, command.range);
  if (typeof resolved === "string") return resolved;
  const { registers, highlights } = state.shared;
  let typed = command.pattern;
  let smart = true;
  if (typed === "") {
    const last = registers.get("/")?.text;
    if (last === undefined) return "E35: No previous regular expression";
    typed = last;
    smart = state.shared.search.smart;
  } else {
    registers.set("/", { text: typed, linewise: false });
    registers.remember("search", typed);
    state.shared.saveRegisters();
    state.shared.search = { forward: state.shared.search.forward, smart };
  }
  await highlights.show();
  const compiled = await compile(state, typed, smart);
  if ("error" in compiled) return compiled.error;
  const [first, last] = resolved;
  // A range reaches into the line break after its last line, so a pattern
  // may match it, as `\n` does to join lines.
  const total = await lines.count();
  const rangeEnd = last + 1 < total
    ? (await state.buffer.lineRange(last + 1)).startByteOffset
    : (await state.buffer.lineRange(last)).endByteOffset;
  const options = {
    regex: true,
    caseSensitive: command.flags.ignoreCase === null ? compiled.caseSensitive : !command.flags.ignoreCase,
    perLine: !command.flags.global,
    range: {
      startByteOffset: (await state.buffer.lineRange(first)).startByteOffset,
      endByteOffset: rangeEnd,
    },
  } as const;
  try {
    const read = await state.buffer.find(compiled.query, { ...options, limit: SUBSTITUTE_LIMIT });
    // A match at the range's very end, which `^` makes, is on the next line.
    const outside = read.matches.at(-1)?.startByteOffset === rangeEnd && last + 1 < total;
    const found = { ...read, matches: outside ? read.matches.slice(0, -1) : read.matches };
    if (found.matches.length === 0) return command.flags.quiet ? null : `E486: Pattern not found: ${typed}`;
    const matched = new Set<number>();
    for (const match of found.matches) matched.add(await state.buffer.lineAt(match.startByteOffset));
    const count = found.matches.length;
    if (command.flags.count) return `${count} match${count === 1 ? "" : "es"} on ${plural(matched.size, "line")}`;
    const lastLine = await state.buffer.lineAt(found.matches.at(-1)!.endByteOffset);
    const { syntax } = await searchSettings(state);
    await grouped(state, async () => {
      await state.buffer.replace(compiled.query, translateReplacement(command.replacement, syntax), {
        ...options, limit: found.matches.length, ifRevision: found.revision,
      });
    });
    // What follows the last match is untouched, so the line is as far from
    // the end as it was.
    const fresh = new BufferLines(state.buffer);
    const line = Math.max(0, (await fresh.count()) - 1 - (total - 1 - lastLine));
    await moveTo(state, fresh, { line, col: firstNonBlank(await fresh.text(line)) });
    return count > REPORT ? `${plural(count, "substitution")} on ${plural(matched.size, "line")}` : null;
  } catch (error) {
    const code = (error as { code?: string }).code;
    if (code === "RevisionConflict") return "The buffer changed during the substitution";
    const message = invalidPattern(error);
    if (message === null) throw error;
    return message;
  }
}

/** The directory `:e` resolves relative paths in: the document's, or for an
 * untitled document the window's first root. */
async function baseDirectory(state: State): Promise<string | null> {
  const uri = (await documents.get(state.document))?.resourceUri ?? null;
  if (uri !== null) return directoryOf(uri);
  return (await workspaces.get(state.target.window))?.roots[0]?.uri ?? null;
}

/** `:e`: opens `path` in the base directory; without one it does nothing. */
async function edit(state: State, path: string | null): Promise<string | null> {
  if (path === null) return null;
  const resolved = resolvePath(path, await baseDirectory(state));
  if (resolved === null) return `No directory to open ${path} in`;
  return open(resolved, path);
}

/** Opens the resource `uri`, named `name` in a failure message. */
async function open(uri: string, name: string): Promise<string | null> {
  try {
    await documents.open(uri);
  } catch (error) {
    return `Cannot open ${name}: ${(error as { code?: string }).code ?? String(error)}`;
  }
  return null;
}

/** The settings each option writes. */
const BOOLEAN_KEYS = {
  wrap: "editor.softWrap", expandtab: "editor.insertSpaces", ignorecase: IGNORE_CASE, smartcase: SMART_CASE,
} as const;
/** Knot has one indentation size, so `tabstop` and `shiftwidth` both set it. */
const SIZE = "editor.tabSize";
const LINE_NUMBERS = "editor.lineNumbers";

/** `:set`: writes each option scoped to the document; returns the values
 * shown, if any. */
async function set(state: State, args: readonly SetArgument[]): Promise<string | null> {
  const scope = { document: state.document };
  const shown: string[] = [];
  for (const argument of args) {
    if (argument.option === "tabstop" || argument.option === "shiftwidth") {
      if (argument.action === "set") await settings.set(SIZE, argument.value, scope);
      else shown.push(`${argument.option}=${await settings.get(SIZE, { ...scope, view: "editor" })}`);
      continue;
    }
    const current = await booleanValue(state, argument.option);
    if (argument.action === "show") {
      shown.push(current ? argument.option : `no${argument.option}`);
      continue;
    }
    const value = argument.action === "toggle" ? !current : argument.action === "on";
    if (argument.option === "number" || argument.option === "relativenumber") {
      const lines = await settings.get(LINE_NUMBERS, { ...scope, view: "editor" });
      let number = lines !== "off";
      let relative = lines === "relative";
      if (argument.option === "number") number = value;
      else relative = value;
      await settings.set(LINE_NUMBERS, relative ? "relative" : number ? "on" : "off", scope);
    } else {
      await settings.set(BOOLEAN_KEYS[argument.option], value, scope);
    }
  }
  return shown.length === 0 ? null : shown.join("  ");
}

/** A boolean option's value for the document. Relative line numbers count
 * as `number` too, as Vim's hybrid numbering shows the caret's line. */
async function booleanValue(state: State, option: BooleanOption): Promise<boolean> {
  const scope = { document: state.document, view: "editor" } as const;
  switch (option) {
    case "number": return (await settings.get(LINE_NUMBERS, scope)) !== "off";
    case "relativenumber": return (await settings.get(LINE_NUMBERS, scope)) === "relative";
    default: return (await settings.get(BOOLEAN_KEYS[option], scope)) === true;
  }
}
