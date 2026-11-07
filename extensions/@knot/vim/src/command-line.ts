// Runs the Ex line `:` opens in one followed editor, over native commands
// and document-scoped settings, and returns the message it leaves.
import { commands, documents, settings, workbench, workspaces } from "knot";
import { type BooleanOption, directoryOf, type ExCommand, parseEx, resolvePath, type SetArgument } from "./ex.js";
import { run, type State } from "./normal.js";

/** Asks for a command line and runs it; returns a message to show, or null. */
export async function commandLine(state: State): Promise<string | null> {
  const input = await workbench.showInput({ title: "Ex Command", prompt: ":" });
  if (input.kind === "dismissed") return null;
  const command = parseEx(input.value);
  if (command.kind === "none") return null;
  state.shared.registers.set(":", { text: input.value.replace(/^[\s:]+/, "").trimEnd(), linewise: false });
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

/** `:e`: opens `path`, relative to the document's directory, or for an
 * untitled document to the window's first root. */
async function edit(state: State, path: string): Promise<string | null> {
  const uri = (await documents.get(state.document))?.resourceUri ?? null;
  const root = uri === null ? (await workspaces.get(state.target.window))?.roots[0]?.uri ?? null : null;
  const resolved = resolvePath(path, uri === null ? root : directoryOf(uri));
  if (resolved === null) return `No directory to open ${path} in`;
  try {
    await documents.open(resolved);
  } catch (error) {
    return `Cannot open ${path}: ${(error as { code?: string }).code ?? String(error)}`;
  }
  return null;
}

/** The settings each option writes. */
const BOOLEAN_KEYS = { wrap: "editor.softWrap", expandtab: "editor.insertSpaces" } as const;
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
