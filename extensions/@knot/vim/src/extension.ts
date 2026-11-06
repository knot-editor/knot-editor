// Vim's modal editing for every editor where `vim.enabled` resolves true.
// Each such editor is followed through a long-lived handle. Normal and
// visual modes are the `vim.normal` key context, where typed text and a few
// named keys feed a parser and its commands run here; insert mode is the
// `vim.insert` context, where typing is native and Escape returns to normal
// mode.
import { commands, documents, keymaps, settings, storage, windows, workbench } from "knot";
import type { CommandContext, DocumentId, EditorHandle, JsonValue, StatusItem, ViewTarget, WindowTarget } from "knot";
import { JumpList } from "./jumps.js";
import { EMPTY, feed, type Pending, pendingText } from "./keys.js";
import { BufferLines } from "./lines.js";
import { lineStart } from "./motions.js";
import {
  INSERT, isVisual, leaveInsert, type Mode, moveOnto, NORMAL, run, setMode, type Shared, type State,
} from "./normal.js";
import { Registers } from "./registers.js";
import { followSelection, runVisual } from "./visual.js";

declare module "knot" {
  interface Settings {
    /** Whether editors use Vim's modal editing. */
    "vim.enabled": boolean;
  }
}

export const ENABLED = "vim.enabled";
/** The storage record holding the registers. */
export const REGISTERS = "registers";
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
  ["ctrl-u", "<c-u>"],
  ["ctrl-f", "<c-f>"],
  ["ctrl-b", "<c-b>"],
  ["ctrl-e", "<c-e>"],
  ["ctrl-y", "<c-y>"],
  ["ctrl-h", "<bs>"],
  ["ctrl-j", "<c-j>"],
  ["ctrl-k", "<c-k>"],
  ["ctrl-o", "<c-o>"],
  // Vim takes ctrl-i and Tab as one key.
  ["ctrl-i", "<tab>"],
  ["left", "<left>"],
  ["right", "<right>"],
  ["up", "<up>"],
  ["down", "<down>"],
  ["home", "<home>"],
  ["end", "<end>"],
];

/** A followed editor and the normal-mode input pending in it. */
type Followed = State & {
  pending: Pending;
  /** Whether the caret shows that an operator is pending. */
  operatorShown: boolean;
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
  const saved = await storage.read(REGISTERS);
  const registers = Registers.fromJson(saved.found ? saved.value : undefined);
  // Writes go in order without holding up the keys that made them.
  let saving = Promise.resolve();
  const shared: Shared = {
    registers,
    lastFind: null,
    saveRegisters() {
      const value = registers.toJson();
      saving = saving.then(() => storage.write(REGISTERS, value)).catch(() => {});
    },
    marks: new Map(),
    globalMarks: new Map(),
    jumpToDocument: (document, offset, exact) => vim.jumpToDocument(document, offset, exact),
  };
  const vim = new Vim(
    shared,
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
      case "documentClosed":
        forgetMarks(shared, event.document);
        break;
    }
  });
  await settings.observe(ENABLED, () => vim.refresh());
  if (observation.snapshot.activeTarget !== null) await vim.activate(observation.snapshot.activeTarget);
}

class Vim {
  private readonly states = new Map<ViewTarget, Followed>();
  private readonly active = new Map<WindowTarget, Active>();

  constructor(
    private readonly shared: Shared,
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

  private async attach(active: Active): Promise<Followed | null> {
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
    const state: Followed = {
      target, handle, buffer: tab.buffer, document: tab.document, shared: this.shared,
      language: info.language, mode: "normal", pending: EMPTY, operatorShown: false,
      want: null, caret: null, group: null, opened: false, visual: null, selected: null, previous: null,
      jumps: new JumpList(), context: null, scroll: null,
    };
    this.states.set(target, state);
    handle.onDidClose(() => {
      this.states.delete(target);
      disposeMarks(state);
    });
    await handle.onDidChangeSelection(async () => {
      if (this.states.get(target) !== state || state.mode === "insert") return;
      const mode = state.mode;
      await whileOpen(() => followSelection(state));
      if (state.mode !== mode) await this.showStatus(target.window);
    });
    await setMode(state, "normal");
    await whileOpen(() => followSelection(state));
    return state;
  }

  private async detach(state: Followed) {
    this.states.delete(state.target);
    await state.group?.dispose();
    disposeMarks(state);
    await state.handle.setKeyContext(NORMAL, false);
    await state.handle.setKeyContext(INSERT, false);
    await state.handle.setCursorStyle(null);
    await state.handle.dispose();
  }

  /** The followed editor a command was invoked in: the window's focused tab. */
  private async stateOf(context: CommandContext): Promise<Followed | null> {
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

  /** Shows `document` in the command's window, as a global mark in another
   * document jumps, with the caret at `offset` or its line's first non-blank. */
  async jumpToDocument(document: DocumentId, offset: number, exact: boolean) {
    const { presentation } = await documents.present(document);
    const active: Active = { target: presentation.target, document: null, language: null };
    this.active.set(presentation.target.window, active);
    const state = await this.attach(active);
    if (state !== null) {
      const lines = new BufferLines(state.buffer);
      const position = await lines.position(offset);
      state.want = null;
      await moveOnto(state, lines, exact ? position : await lineStart(lines, position.line));
    }
    await this.showStatus(presentation.target.window);
  }

  async keys(context: CommandContext, keys: readonly string[]) {
    const state = await this.stateOf(context);
    if (state === null) return;
    for (const key of keys) {
      if (state.mode === "insert") break;
      const visual = isVisual(state.mode);
      const fed = feed(state.pending, key, visual);
      if ("pending" in fed) {
        state.pending = fed.pending;
      } else {
        state.pending = EMPTY;
        await showOperator(state, false);
        const command = fed.command;
        if (command === null) continue;
        if (visual || command.kind === "visual" || command.kind === "reselect") {
          await runVisual(state, command);
        } else {
          await run(state, command);
        }
      }
    }
    await showOperator(state, state.mode === "normal" && state.pending.operator !== null);
    await this.showStatus(state.target.window);
  }

  async leaveInsert(context: CommandContext) {
    const state = await this.stateOf(context);
    if (state === null || state.mode !== "insert") return;
    await leaveInsert(state);
    // A selection made while inserting carries on in visual mode.
    await followSelection(state);
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
    await this.modeItem.update({ text: MODE_TEXT[state.mode], priority: 100 }, { window });
    await this.keysItem.update({ text: pendingText(state.pending), side: "right", priority: 40 }, { window });
  }
}

/** Disposes the marks an editor's state holds for itself, which a closed
 * buffer already dropped. */
function disposeMarks(state: State) {
  const marks = [state.previous?.anchor, state.previous?.caret, state.context, ...state.jumps.entries];
  for (const mark of marks) mark?.dispose().catch(() => {});
}

/** Drops the marks of a closed document, whose buffer holds them no more. */
function forgetMarks(shared: Shared, document: DocumentId) {
  shared.marks.delete(document);
  for (const [name, global] of shared.globalMarks) {
    if (global.document === document) shared.globalMarks.delete(name);
  }
}

const MODE_TEXT: Readonly<Record<Mode, string>> = {
  normal: "", insert: "-- INSERT --", visual: "-- VISUAL --", "visual-line": "-- VISUAL LINE --",
};

/** Runs `body`, ignoring the failure of an editor or document that closed
 * meanwhile. */
async function whileOpen(body: () => Promise<void>) {
  try {
    await body();
  } catch (error) {
    const code = (error as { code?: string }).code;
    if (code !== "InvalidTarget" && code !== "BufferClosed") throw error;
  }
}

async function enabledFor(document: DocumentId, language: string): Promise<boolean> {
  return (await settings.get(ENABLED, { document, language, view: "editor" })) === true;
}

/** Draws the caret as an underline while an operator waits for its motion. */
async function showOperator(state: Followed, pending: boolean) {
  if (state.operatorShown === pending) return;
  state.operatorShown = pending;
  if (state.mode === "normal") await state.handle.setCursorStyle(pending ? "underline" : "block");
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
