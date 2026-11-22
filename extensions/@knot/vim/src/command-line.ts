// Runs the Ex line `:` opens in one followed editor, over native commands
// and document-scoped settings, and returns the message it leaves.
import { type FileRow, workspace } from "@knot/go-to-file";
import { commands, completion, documents, files, settings, windows, workbench, workspaces } from "knot";
import type { InputCompletion, ViewTarget } from "knot";
import {
  type Address, type BooleanOption, bufferCompletionTarget, completeEx, directoryOf, type ExCommand, fileCompletionTarget, parseEx, pathOfUri, resolvePath,
  type ExRange, type SetArgument,
} from "./ex.js";
import { BufferLines } from "./lines.js";
import { applyOperator, grouped, join, markOffset, moveTo, prompted, record, regionText, replace, run, type State } from "./normal.js";
import { translateReplacement } from "./pattern.js";
import { sortLines } from "./sort.js";
import { compile, IGNORE_CASE, invalidPattern, searchSettings, SMART_CASE } from "./search.js";
import { firstNonBlank } from "./text.js";

/** The files `query` lists, or none when no package serves the list or its
 * call fails, as when it is restarting. */
async function fileRows(query: string): Promise<readonly FileRow[]> {
  try {
    return await (await workspace.client()).files(query);
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

/** The open documents `query` matches, each completing the `:b` line that
 * began with `head`. */
async function bufferCompletions(head: string, query: string): Promise<InputCompletion[]> {
  const titles = (await documents.list()).map((document) => document.title);
  const unique = [...new Set(titles)];
  return (await completion.match(query, unique)).map(({ index, ranges }) => ({
    label: unique[index]!,
    value: `${head} ${unique[index]}`,
    matches: ranges,
  }));
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
        const buffers = bufferCompletionTarget(line);
        if (buffers !== null) {
          files = [];
          return { items: await bufferCompletions(buffers.head, buffers.query) };
        }
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
      return command.path === null ? save(state) : writeTo(state, command.path, command.force);
    case "save-as":
      return saveAs(state, command.path, command.force);
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
    case "write-all":
      return (await commands.invoke("file.save-all")).kind === "completed" ? null : "E37: Write failed";
    case "quit-all":
      await commands.invoke("application.quit");
      return null;
    case "write-quit-all":
      if ((await commands.invoke("file.save-all")).kind !== "completed") return "E37: Write failed";
      await commands.invoke("application.quit");
      return null;
    case "edit":
      return edit(state, command.path, command.force);
    case "set":
      return set(state, command.arguments);
    case "substitute":
      return substitute(state, command);
    case "split":
      return split(state, command);
    case "enew":
      await documents.createUntitled({ target: state.target });
      return null;
    case "cycle-buffer":
      await commands.invoke(command.forward ? "workbench.next-tab" : "workbench.previous-tab");
      return null;
    case "buffer":
      return switchTo(state, command.name);
    case "only":
      await commands.invoke("workbench.close-other-panes");
      return null;
    case "undo":
    case "redo":
      await run(state, { kind: command.kind, count: 1 });
      return null;
    case "pwd": {
      const directory = await baseDirectory(state);
      return directory === null ? "No directory" : pathOfUri(directory);
    }
    case "delete":
    case "yank":
      return deleteOrYank(state, command);
    case "move":
    case "copy":
      return moveOrCopy(state, command);
    case "shift":
      return shift(state, command);
    case "join":
      return joinLines(state, command);
    case "sort":
      return sort(state, command);
    case "nohlsearch":
      await state.shared.highlights.hide();
      return null;
  }
}

/** The 0-based line `address` names; it is -1 for `0`, and may lie outside
 * the buffer. A message when a mark is unset. */
async function addressLine(
  state: State,
  lines: BufferLines,
  { base, offset }: Address,
  current: number,
  total: number,
): Promise<number | string> {
  switch (base.kind) {
    case "line": return base.line - 1 + offset;
    case "current": return current + offset;
    case "last": return total - 1 + offset;
    case "mark": {
      const at = await markOffset(state, base.name);
      return at === null ? "E20: Mark not set" : (await lines.position(at)).line + offset;
    }
  }
}

async function currentLine(state: State, lines: BufferLines): Promise<number> {
  return (await lines.position((await state.handle.selection()).caret)).line;
}

/** The 0-based first and last line of `range`, or the current line without
 * one; a message when an address is outside the buffer or a mark is unset. */
async function resolveRange(
  state: State,
  lines: BufferLines,
  range: ExRange | null,
): Promise<readonly [number, number] | string> {
  const total = await lines.count();
  const current = await currentLine(state, lines);
  if (range === null) return [current, current];
  const first = await addressLine(state, lines, range.first, current, total);
  const last = await addressLine(state, lines, range.last, current, total);
  if (typeof first === "string") return first;
  if (typeof last === "string") return last;
  if (Math.min(first, last) < 0 || Math.max(first, last) >= total) return "E16: Invalid range";
  return [Math.min(first, last), Math.max(first, last)];
}

/** The lines of `range`, or with `count` that many from its last line, as far
 * as the buffer goes. */
async function countedRange(
  state: State,
  lines: BufferLines,
  range: ExRange | null,
  count: number | null,
): Promise<readonly [number, number] | string> {
  const resolved = await resolveRange(state, lines, range);
  if (typeof resolved === "string" || count === null) return resolved;
  const last = resolved[1];
  return [last, Math.min((await lines.count()) - 1, last + count - 1)];
}

/** The linewise region of the lines `first` to `last`. */
const wholeLines = (first: number, last: number) =>
  ({ start: { line: first, col: 0 }, end: { line: last, col: 0 }, linewise: true }) as const;

/** `:d` and `:y`: fill the registers as `dd` and `yy` do; `:y` leaves the caret. */
async function deleteOrYank(
  state: State,
  command: Extract<ExCommand, { kind: "delete" | "yank" }>,
): Promise<string | null> {
  const lines = new BufferLines(state.buffer);
  const resolved = await countedRange(state, lines, command.range, command.count);
  if (typeof resolved === "string") return resolved;
  const region = wholeLines(...resolved);
  if (command.kind === "yank") {
    await record(state, command.register, await regionText(lines, region), "yank");
  } else {
    const from = await lines.position((await state.handle.selection()).caret);
    await applyOperator(state, lines, "d", command.register, region, from, false);
  }
  return null;
}

/** `:m` and `:t`: one edit over the lines the move or copy touches, with the
 * caret on the first non-blank of the last line placed. */
async function moveOrCopy(
  state: State,
  command: Extract<ExCommand, { kind: "move" | "copy" }>,
): Promise<string | null> {
  const lines = new BufferLines(state.buffer);
  const resolved = await resolveRange(state, lines, command.range);
  if (typeof resolved === "string") return resolved;
  const [first, last] = resolved;
  const total = await lines.count();
  const destination = await addressLine(state, lines, command.destination, await currentLine(state, lines), total);
  if (typeof destination === "string") return destination;
  if (destination < -1 || destination >= total) return "E16: Invalid range";
  const copying = command.kind === "copy";
  if (!copying && destination >= first && destination < last) return "E134: Cannot move a range of lines into itself";
  const texts = async (from: number, to: number) => {
    const out: string[] = [];
    for (let line = from; line <= to; line++) out.push(await lines.text(line));
    return out;
  };
  const block = await texts(first, last);
  let landed: number;
  await grouped(state, async () => {
    if (copying) {
      // Below the destination line, or above the first line for `0`.
      const at = destination < 0 ? 0 : await lines.offset({ line: destination, col: (await lines.text(destination)).length });
      await replace(state, at, at, destination < 0 ? `${block.join("\n")}\n` : `\n${block.join("\n")}`);
      landed = destination + block.length;
    } else if (destination > last) {
      const between = await texts(last + 1, destination);
      await replaceLines(state, lines, first, destination, [...between, ...block]);
      landed = destination;
    } else if (destination < first - 1) {
      const between = await texts(destination + 1, first - 1);
      await replaceLines(state, lines, destination + 1, last, [...block, ...between]);
      landed = destination + block.length;
    } else {
      landed = last;
    }
  });
  const fresh = new BufferLines(state.buffer);
  await moveTo(state, fresh, { line: landed!, col: firstNonBlank(await fresh.text(landed!)) });
  return null;
}

/** Replaces the lines `first` to `last` with `texts`, joined by line breaks. */
async function replaceLines(state: State, lines: BufferLines, first: number, last: number, texts: readonly string[]) {
  const start = await lines.offset({ line: first, col: 0 });
  const end = await lines.offset({ line: last, col: (await lines.text(last)).length });
  await replace(state, start, end, texts.join("\n"));
}

/** `:>` and `:<`, as `>>` and `<<` over the lines. */
async function shift(state: State, command: Extract<ExCommand, { kind: "shift" }>): Promise<string | null> {
  const lines = new BufferLines(state.buffer);
  const resolved = await countedRange(state, lines, command.range, command.count);
  if (typeof resolved === "string") return resolved;
  const from = await lines.position((await state.handle.selection()).caret);
  await applyOperator(state, lines, command.direction, null, wholeLines(...resolved), from, false, command.amount);
  return null;
}

/** `:j`: joins the lines of the range, or the current line and the next; a
 * range of one line does nothing. With `!` no spaces are added or removed. */
async function joinLines(state: State, command: Extract<ExCommand, { kind: "join" }>): Promise<string | null> {
  const lines = new BufferLines(state.buffer);
  let resolved = await countedRange(state, lines, command.range, command.count);
  if (typeof resolved === "string") return resolved;
  if (command.range === null && command.count === null) {
    const next = Math.min(resolved[0] + 1, (await lines.count()) - 1);
    resolved = [resolved[0], next];
  }
  const [first, last] = resolved;
  if (first === last) return null;
  if (command.force) {
    await grouped(state, async () => {
      const texts: string[] = [];
      for (let line = first; line <= last; line++) texts.push(await lines.text(line));
      await replaceLines(state, lines, first, last, [texts.join("")]);
    });
    const fresh = new BufferLines(state.buffer);
    await moveTo(state, fresh, { line: first, col: 0 });
    return null;
  }
  await moveTo(state, lines, { line: first, col: 0 });
  await join(state, last - first + 1);
  return null;
}

/** `:sort`: replaces the lines with their sorted order in one edit. */
async function sort(state: State, command: Extract<ExCommand, { kind: "sort" }>): Promise<string | null> {
  const lines = new BufferLines(state.buffer);
  const total = await lines.count();
  const resolved = command.range === null ? ([0, total - 1] as const) : await resolveRange(state, lines, command.range);
  if (typeof resolved === "string") return resolved;
  const [first, last] = resolved;
  const before: string[] = [];
  for (let line = first; line <= last; line++) before.push(await lines.text(line));
  const after = sortLines(before, command.options);
  if (after.length !== before.length || after.some((text, at) => text !== before[at])) {
    await grouped(state, () => replaceLines(state, lines, first, last, after));
  }
  const fresh = new BufferLines(state.buffer);
  await moveTo(state, fresh, { line: first, col: firstNonBlank(await fresh.text(first)) });
  return null;
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

/** The code of a failed host call, or its text. */
function failure(error: unknown): string {
  return (error as { code?: string }).code ?? String(error);
}

async function save(state: State): Promise<null> {
  await commands.invoke("file.save");
  return null;
}

/** The URI `path` names for a write, or a message when it cannot be resolved
 * or exists and `force` is off, as Vim's `E13`. */
async function writeTarget(state: State, path: string, force: boolean): Promise<{ uri: string } | string> {
  const uri = resolvePath(path, await baseDirectory(state));
  if (uri === null) return `No directory to write ${path} in`;
  if (!force && (await files.stat(uri)) !== null && uri !== (await documents.get(state.document))?.resourceUri) {
    return "E13: File exists (add ! to override)";
  }
  return { uri };
}

/** `:w <file>`: writes a copy and leaves a document that has a file as it is;
 * an untitled document becomes the file's. */
async function writeTo(state: State, path: string, force: boolean): Promise<string | null> {
  const target = await writeTarget(state, path, force);
  if (typeof target === "string") return target;
  const document = await documents.get(state.document);
  if (document === null) return null;
  try {
    if (target.uri === document.resourceUri) return save(state);
    if (document.resourceUri === null) await documents.save(state.document, { uri: target.uri });
    else await files.writeText(target.uri, (await state.buffer.snapshot()).text);
  } catch (error) {
    return `Cannot write ${path}: ${failure(error)}`;
  }
  return null;
}

/** `:sav <file>`: writes the document to the file, which it then is. */
async function saveAs(state: State, path: string, force: boolean): Promise<string | null> {
  const target = await writeTarget(state, path, force);
  if (typeof target === "string") return target;
  try {
    await documents.save(state.document, { uri: target.uri });
  } catch (error) {
    return `Cannot write ${path}: ${failure(error)}`;
  }
  return null;
}

/** `:e`: opens `path` in the base directory; without one it does nothing. `:e!`
 * discards the document's changes: it reverts to its file, or is closed once
 * `path` is open. */
async function edit(state: State, path: string | null, force: boolean): Promise<string | null> {
  if (path === null) return force ? revert(state) : null;
  const resolved = resolvePath(path, await baseDirectory(state));
  if (resolved === null) return `No directory to open ${path} in`;
  if (!force) return open(resolved, path);
  if (resolved === (await documents.get(state.document))?.resourceUri) return revert(state);
  const message = await open(resolved, path);
  if (message === null) await workbench.close(state.target, { discard: true });
  return message;
}

/** `:e!` alone: the document's text becomes its file's, undoably. */
async function revert(state: State): Promise<string | null> {
  try {
    await documents.reload(state.document);
  } catch (error) {
    return (error as { code?: string }).code === "ResourceNotFound" ? "E32: No file name" : `Cannot reload: ${failure(error)}`;
  }
  return null;
}

/** `:sp`, `:vs`, `:new` and `:vnew`: splits the pane, then replaces the tab the
 * split copies with the file, or an untitled document, when there is one. */
async function split(state: State, command: Extract<ExCommand, { kind: "split" }>): Promise<string | null> {
  const resolved = command.path === null ? null : resolvePath(command.path, await baseDirectory(state));
  if (command.path !== null && resolved === null) return `No directory to open ${command.path} in`;
  await commands.invoke(command.direction === "down" ? "workbench.split-vertical" : "workbench.split-horizontal");
  if (resolved === null && !command.blank) return null;
  const copy: ViewTarget | null = (await windows.get(state.target.window))?.currentTarget ?? null;
  if (copy === null) return null;
  let message: string | null = null;
  if (resolved !== null) message = await open(resolved, command.path!, copy);
  else await documents.createUntitled({ target: copy });
  if (message === null) await workbench.close(copy);
  return message;
}

/** `:b <name>`: shows the open document titled `name`, else the only one
 * whose title or path contains it. */
async function switchTo(state: State, name: string): Promise<string | null> {
  const all = await documents.list();
  const exact = all.filter((document) => document.title === name);
  const found = exact.length > 0 ? exact : all.filter((document) =>
    document.title.includes(name) || (document.resourceUri !== null && pathOfUri(document.resourceUri).includes(name)));
  if (found.length === 0) return `E94: No matching buffer for ${name}`;
  if (found.length > 1) return `E93: More than one match for ${name}`;
  await documents.present(found[0]!.id, { target: state.target });
  return null;
}

/** Opens the resource `uri`, in the pane of `target` if given, named `name` in a
 * failure message. */
async function open(uri: string, name: string, target?: ViewTarget): Promise<string | null> {
  try {
    await documents.open(uri, target === undefined ? undefined : { target });
  } catch (error) {
    return `Cannot open ${name}: ${failure(error)}`;
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
