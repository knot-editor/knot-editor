// Vim's modal editing for every editor where `vim.enabled` resolves true.
// Each such editor is followed through a long-lived handle. Normal mode is
// the `vim.normal` key context, where typed text and a few named keys feed a
// parser and its commands run here; insert mode is the `vim.insert` context,
// where typing is native and Escape returns to normal mode.
import { commands, documents, keymaps, languages, settings, windows, workbench } from "knot";
import type {
  Buffer, CommandContext, Disposable, DocumentId, EditorHandle, JsonValue, StatusItem,
  ViewTarget, WindowTarget,
} from "knot";
import { type Command, EMPTY, feed, type InsertAt, type Motion, type Pending, pendingText } from "./keys.js";
import { BufferLines } from "./lines.js";
import {
  left, lineStart, type Position, right, wordBackward, wordEnd, wordEndBackward, wordForward,
} from "./motions.js";
import { columnAtScreen, firstNonBlank, lastColumn, nextColumn, previousColumn, screenColumn } from "./text.js";

declare module "knot" {
  interface Settings {
    /** Whether editors use Vim's modal editing. */
    "vim.enabled": boolean;
  }
}

export const ENABLED = "vim.enabled";
const NORMAL = "vim.normal";
const INSERT = "vim.insert";
const normal = { view: "editor", context: NORMAL } as const;
const insert = { view: "editor", context: INSERT } as const;

/** Named keys normal mode handles, as gpui names them and as the parser
 * names them. Keys that would edit natively are bound too, so normal mode
 * never inserts or deletes text by accident; unhandled ones do nothing. */
const NAMED_KEYS: readonly (readonly [string, string])[] = [
  ["escape", "<esc>"],
  ["enter", "<cr>"],
  ["backspace", "<bs>"],
  ["delete", "<del>"],
  ["tab", "<tab>"],
  ["shift-tab", "<s-tab>"],
  ["ctrl-r", "<c-r>"],
  ["ctrl-d", "<c-d>"],
  ["ctrl-h", "<bs>"],
  ["ctrl-j", "<c-j>"],
  ["ctrl-k", "<c-k>"],
];

type Mode = "normal" | "insert";

/** One followed editor. */
type State = {
  readonly target: ViewTarget;
  readonly handle: EditorHandle;
  readonly buffer: Buffer;
  readonly document: DocumentId;
  language: string;
  mode: Mode;
  pending: Pending;
  /** The screen column `j` and `k` aim for, valid while the caret is still
   * where this package last put it; Infinity after `$`. */
  want: number | null;
  caret: number | null;
  /** The undo group of the current insert session. */
  group: Disposable | null;
  /** Whether insert mode began by opening a line, whose indentation alone
   * is removed again on Escape. */
  opened: boolean;
};

/** The tab a window last activated, and the document and language it was
 * last checked against, so a language change rechecks it. */
type Active = { target: ViewTarget; document: DocumentId | null; language: string | null };

export async function start() {
  await settings.define({
    key: ENABLED,
    type: "boolean",
    default: false,
    description: "Edit with Vim's modes and keys. Scoped values turn it off for a language or view.",
  });
  const vim = new Vim(
    await workbench.registerStatusItem("mode", { text: "", priority: 100 }),
    await workbench.registerStatusItem("keys", { text: "", side: "right", priority: 40 }),
  );

  await commands.register("vim.type", async (context) => {
    const text = textArgument(context.arguments);
    if (text === null) return commands.invalidArguments("expected { text }");
    await vim.keys(context, [...text]);
  }, { palette: false });
  await commands.register("vim.key", async (context) => {
    const key = keyArgument(context.arguments);
    if (key === null) return commands.invalidArguments("expected { key }");
    await vim.keys(context, [key]);
  }, { palette: false });
  await commands.register("vim.leave-insert", (context) => vim.leaveInsert(context), {
    palette: false,
  });

  keymaps.bindText("vim.type", normal);
  for (const [key, name] of NAMED_KEYS) {
    keymaps.bind(key, { name: "vim.key", arguments: { key: name } }, normal);
  }
  keymaps.bind("escape", "vim.leave-insert", insert);
  keymaps.bind("ctrl-[", "vim.leave-insert", insert);

  const observation = await workbench.observe(async (event) => {
    switch (event.type) {
      case "activeTargetChanged":
        if (event.activeTarget !== null) await vim.activate(event.activeTarget);
        break;
      case "windowClosed":
        vim.closeWindow(event.window);
        break;
      case "documentChanged":
        await vim.languageMayHaveChanged(event.document.id, event.document.language);
        break;
    }
  });
  await settings.observe(ENABLED, () => vim.refresh());
  if (observation.snapshot.activeTarget !== null) await vim.activate(observation.snapshot.activeTarget);
}

class Vim {
  private readonly states = new Map<ViewTarget, State>();
  private readonly active = new Map<WindowTarget, Active>();

  constructor(
    private readonly modeItem: StatusItem,
    private readonly keysItem: StatusItem,
  ) {}

  /** Follows a newly active tab when Vim applies to it. */
  async activate(target: ViewTarget) {
    const active: Active = { target, document: null, language: null };
    this.active.set(target.window, active);
    await this.attach(active);
    await this.showStatus(target.window);
  }

  closeWindow(window: WindowTarget) {
    this.active.delete(window);
  }

  /** Rechecks every editor after `vim.enabled` changed. */
  async refresh() {
    for (const state of [...this.states.values()]) {
      if (!(await enabledFor(state.document, state.language))) await this.detach(state);
    }
    for (const active of this.active.values()) await this.attach(active);
    for (const window of this.active.keys()) await this.showStatus(window);
  }

  /** Rechecks the editors of a document whose language changed, since a
   * language-scoped value may turn Vim on or off for them. */
  async languageMayHaveChanged(document: DocumentId, language: string) {
    for (const state of [...this.states.values()]) {
      if (state.document !== document || state.language === language) continue;
      state.language = language;
      if (!(await enabledFor(document, language))) await this.detach(state);
      await this.showStatus(state.target.window);
    }
    for (const active of this.active.values()) {
      if (active.document !== document || active.language === language) continue;
      await this.attach(active);
      await this.showStatus(active.target.window);
    }
  }

  private async attach(active: Active): Promise<State | null> {
    const { target } = active;
    const existing = this.states.get(target);
    if (existing !== undefined) return existing;
    const tab = (await windows.get(target.window))?.panes
      .flatMap((pane) => pane.tabs)
      .find((candidate) => candidate.target === target);
    if (tab?.document == null || tab.buffer === null) return null;
    const info = await documents.get(tab.document);
    if (info == null) return null;
    active.document = tab.document;
    active.language = info.language;
    if (!(await enabledFor(tab.document, info.language))) return null;
    let handle: EditorHandle;
    try {
      handle = await workbench.editor(target);
    } catch (error) {
      // The tab may have closed meanwhile.
      if ((error as { code?: string }).code === "InvalidTarget") return null;
      throw error;
    }
    const state: State = {
      target, handle, buffer: tab.buffer, document: tab.document, language: info.language,
      mode: "normal", pending: EMPTY, want: null, caret: null, group: null, opened: false,
    };
    this.states.set(target, state);
    handle.onDidClose(() => {
      this.states.delete(target);
    });
    await handle.onDidChangeSelection(async () => {
      if (this.states.get(target) === state && state.mode === "normal") await clampCaret(state);
    });
    await setMode(state, "normal");
    await clampCaret(state);
    return state;
  }

  private async detach(state: State) {
    this.states.delete(state.target);
    await state.group?.dispose();
    await state.handle.setKeyContext(NORMAL, false);
    await state.handle.setKeyContext(INSERT, false);
    await state.handle.setCursorStyle(null);
    await state.handle.dispose();
  }

  /** The followed editor a command was invoked in: the window's focused tab. */
  private async stateOf(context: CommandContext): Promise<State | null> {
    const window = context.target.windowTarget;
    const current = (await windows.get(window))?.currentTarget ?? null;
    if (current === null) return null;
    const known = this.states.get(current);
    if (known !== undefined) return known;
    // A key can arrive before its tab's activation is observed.
    const active: Active = { target: current, document: null, language: null };
    this.active.set(window, active);
    const attached = await this.attach(active);
    await this.showStatus(window);
    return attached;
  }

  async keys(context: CommandContext, keys: readonly string[]) {
    const state = await this.stateOf(context);
    if (state === null) return;
    for (const key of keys) {
      if (state.mode !== "normal") break;
      const fed = feed(state.pending, key);
      if ("pending" in fed) {
        state.pending = fed.pending;
      } else {
        state.pending = EMPTY;
        if (fed.command !== null) await run(state, fed.command);
      }
    }
    await this.showStatus(state.target.window);
  }

  async leaveInsert(context: CommandContext) {
    const state = await this.stateOf(context);
    if (state === null || state.mode !== "insert") return;
    await leaveInsert(state);
    await this.showStatus(state.target.window);
  }

  /** Shows the mode and pending keys of the window's active editor, or
   * nothing when Vim does not follow it. */
  private async showStatus(window: WindowTarget) {
    const target = this.active.get(window)?.target;
    const state = target === undefined ? undefined : this.states.get(target);
    if (state === undefined) {
      await this.modeItem.update(null, { window });
      await this.keysItem.update(null, { window });
      return;
    }
    await this.modeItem.update({ text: state.mode === "insert" ? "-- INSERT --" : "", priority: 100 }, { window });
    await this.keysItem.update({ text: pendingText(state.pending), side: "right", priority: 40 }, { window });
  }
}

async function enabledFor(document: DocumentId, language: string): Promise<boolean> {
  return (await settings.get(ENABLED, { document, language, view: "editor" })) === true;
}

async function setMode(state: State, mode: Mode) {
  state.mode = mode;
  await state.handle.setKeyContext(NORMAL, mode === "normal");
  await state.handle.setKeyContext(INSERT, mode === "insert");
  await state.handle.setCursorStyle(mode === "normal" ? "block" : null);
}

/** Puts a caret that sits past its line's last character back on it, as
 * normal mode never rests on a line's end. A selection is left alone. */
async function clampCaret(state: State) {
  const { anchor, caret } = await state.handle.selection();
  if (anchor !== caret) return;
  const lines = new BufferLines(state.buffer);
  const position = await lines.position(caret);
  const last = lastColumn(await lines.text(position.line));
  if (position.col > last) await moveTo(state, lines, { line: position.line, col: last });
}

async function moveTo(
  state: State,
  lines: BufferLines,
  position: Position,
  reveal: "nearest" | "center" = "nearest",
) {
  const offset = await lines.offset(position);
  state.caret = offset;
  await state.handle.setSelection({ anchor: offset, caret: offset }, { reveal });
}

async function run(state: State, command: Command) {
  switch (command.kind) {
    case "motion":
      await motion(state, command.motion, command.count);
      break;
    case "insert":
      await enterInsert(state, command.at);
      break;
    case "undo":
    case "redo":
      for (let n = 0; n < command.count; n++) await commands.invoke(`editor.${command.kind}`);
      state.want = null;
      await clampCaret(state);
      break;
  }
}

async function motion(state: State, name: Motion, count: number | null) {
  const n = count ?? 1;
  if (name === "row-up" || name === "row-down") {
    const native = name === "row-up" ? "editor.move-up" : "editor.move-down";
    for (let step = 0; step < n; step++) await commands.invoke(native);
    state.want = null;
    await clampCaret(state);
    return;
  }
  const lines = new BufferLines(state.buffer);
  const { caret } = await state.handle.selection();
  const from = await lines.position(caret);
  const total = await lines.count();
  let to: Position;
  let want: number | null = null;
  let reveal: "nearest" | "center" = "nearest";
  switch (name) {
    case "left": to = await left(lines, from, n, false); break;
    case "right": to = await right(lines, from, n, false); break;
    case "left-wrap": to = await left(lines, from, n, true); break;
    case "right-wrap": to = await right(lines, from, n, true); break;
    case "up":
    case "down": {
      const tabSize = await tabSizeOf(state);
      want = state.caret === caret && state.want !== null
        ? state.want
        : screenColumn(await lines.text(from.line), from.col, tabSize);
      const line = Math.max(0, Math.min(total - 1, from.line + (name === "up" ? -n : n)));
      const text = await lines.text(line);
      to = { line, col: want === Infinity ? lastColumn(text) : columnAtScreen(text, want, tabSize) };
      break;
    }
    case "word": to = await wordForward(lines, from, n, false); break;
    case "WORD": to = await wordForward(lines, from, n, true); break;
    case "back": to = await wordBackward(lines, from, n, false); break;
    case "BACK": to = await wordBackward(lines, from, n, true); break;
    case "end": to = await wordEnd(lines, from, n, false); break;
    case "END": to = await wordEnd(lines, from, n, true); break;
    case "back-end": to = await wordEndBackward(lines, from, n, false); break;
    case "BACK-END": to = await wordEndBackward(lines, from, n, true); break;
    case "line-start": to = { line: from.line, col: 0 }; break;
    case "first-non-blank": to = { line: from.line, col: firstNonBlank(await lines.text(from.line)) }; break;
    case "line-end": {
      const line = Math.min(total - 1, from.line + n - 1);
      to = { line, col: lastColumn(await lines.text(line)) };
      want = Infinity;
      break;
    }
    case "next-line-start": to = await lineStart(lines, from.line + n); break;
    case "previous-line-start": to = await lineStart(lines, from.line - n); break;
    case "first-line":
      to = await lineStart(lines, count === null ? 0 : count - 1);
      reveal = "center";
      break;
    case "last-line":
      to = await lineStart(lines, count === null ? total - 1 : count - 1);
      reveal = "center";
      break;
  }
  const clamped = { line: to.line, col: Math.min(to.col, lastColumn(await lines.text(to.line))) };
  if (want === null) want = screenColumn(await lines.text(clamped.line), clamped.col, await tabSizeOf(state));
  await moveTo(state, lines, clamped, reveal);
  state.want = want;
}

async function tabSizeOf(state: State): Promise<number> {
  const size = await settings.get("editor.tabSize", {
    document: state.document, language: state.language, view: "editor",
  });
  return typeof size === "number" && size >= 1 ? size : 4;
}

async function enterInsert(state: State, at: InsertAt) {
  try {
    state.group = await state.handle.beginUndoGroup();
  } catch (error) {
    // Another view of the buffer is inserting; its group takes this edit too.
    if ((error as { code?: string }).code !== "OperationInProgress") throw error;
    state.group = null;
  }
  state.opened = at === "o" || at === "O";
  await setMode(state, "insert");
  const lines = new BufferLines(state.buffer);
  const from = await lines.position((await state.handle.selection()).caret);
  const text = await lines.text(from.line);
  switch (at) {
    case "i":
      break;
    case "a":
      if (text.length > 0) await moveTo(state, lines, { line: from.line, col: nextColumn(text, from.col) });
      break;
    case "I":
      await moveTo(state, lines, { line: from.line, col: firstNonBlank(text) });
      break;
    case "A":
      await moveTo(state, lines, { line: from.line, col: text.length });
      break;
    case "o":
      await moveTo(state, lines, { line: from.line, col: text.length });
      await commands.invoke("editor.insert-newline");
      break;
    case "O":
      if (from.line > 0 && (await languages.get(state.language))?.queries.indents !== undefined) {
        // Breaking after the previous line lets the indent query place the
        // new line.
        await moveTo(state, lines, { line: from.line - 1, col: (await lines.text(from.line - 1)).length });
        await commands.invoke("editor.insert-newline");
      } else {
        // Without a query, or above the first line, the new line takes the
        // current line's indentation, as Vim's autoindent does.
        const indent = /^[ \t]*/.exec(text)![0];
        const start = await lines.offset({ line: from.line, col: 0 });
        const { revision } = await state.buffer.snapshot({ startByteOffset: start, endByteOffset: start });
        await state.buffer.applyEdits(
          [{ range: { startByteOffset: start, endByteOffset: start }, text: `${indent}\n` }],
          { ifRevision: revision },
        );
        await moveTo(state, new BufferLines(state.buffer), { line: from.line, col: indent.length });
      }
      break;
  }
}

async function leaveInsert(state: State) {
  const lines = new BufferLines(state.buffer);
  const { anchor, caret } = await state.handle.selection();
  const from = await lines.position(caret);
  const text = await lines.text(from.line);
  if (state.opened && text.length > 0 && /^[ \t]*$/.test(text)) {
    // An opened line left empty keeps no indentation, as in Vim.
    const start = await lines.offset({ line: from.line, col: 0 });
    const end = await lines.offset({ line: from.line, col: text.length });
    const { revision } = await state.buffer.snapshot({ startByteOffset: start, endByteOffset: end });
    await state.buffer.applyEdits([{ range: { startByteOffset: start, endByteOffset: end }, text: "" }], {
      ifRevision: revision,
    });
    await moveTo(state, new BufferLines(state.buffer), { line: from.line, col: 0 });
  } else if (anchor === caret) {
    await moveTo(state, lines, { line: from.line, col: from.col > 0 ? previousColumn(text, from.col) : 0 });
  }
  const group = state.group;
  state.group = null;
  state.opened = false;
  state.want = null;
  await group?.dispose();
  await setMode(state, "normal");
}

function textArgument(value: JsonValue): string | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const text = (value as { readonly [key: string]: JsonValue }).text;
  return typeof text === "string" ? text : null;
}

function keyArgument(value: JsonValue): string | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const key = (value as { readonly [key: string]: JsonValue }).key;
  return typeof key === "string" ? key : null;
}
