// Runs the Ex line `:` opens in one followed editor, over native commands
// and document-scoped settings, and returns the message it leaves.
import { commands, documents, providers, settings, workbench, workspaces } from "knot";
import type { IconRef, InputCompletion } from "knot";
import {
  type BooleanOption, completeEx, directoryOf, type ExCommand, fileCompletionTarget, parseEx, pathOfUri, resolvePath,
  type SetArgument,
} from "./ex.js";
import { run, type State } from "./normal.js";
import { IGNORE_CASE, SMART_CASE } from "./search.js";

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
export async function commandLine(state: State): Promise<string | null> {
  let files: readonly FileRow[] = [];
  const input = await workbench.showInput({
    title: "Ex Command",
    prompt: ":",
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
  const row = input.highlighted === null ? undefined : files[input.highlighted];
  if (row !== undefined) return open(row.id, row.label);
  const command = parseEx(input.value);
  if (command.kind === "none") return null;
  const line = input.value.replace(/^[\s:]+/, "").trimEnd();
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
