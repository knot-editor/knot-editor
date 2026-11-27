// Runs normal-mode commands in one followed editor: motions, operators over
// motions, text objects, and lines, and the commands that edit directly.
// Text math is done here over line snapshots; indentation, joining, bracket
// matching, and visual-row movement are the native editor commands.
import { commands, documents, languages, settings, windows, workbench } from "knot";
import type {
  Buffer, Disposable, DocumentId, EditorHandle, EditorSelection, FoldRange, Mark, RecordedCommand, Recording, ViewNodeTarget, ViewTarget,
} from "knot";
import { JumpList, type LineOf } from "./jumps.js";
import { closedAt, createFold, foldMotion, intoShownLine, lastLine, linesDown, linesUp, runFold, shownLine } from "./folds.js";
import type { Command, FindKind, InsertAt, MarkMotion, Motion, Operator, Pending, Scroll, Target } from "./keys.js";
import { BufferLines } from "./lines.js";
import {
  bracketFrom, findInLine, left, lineStart, matchBracketText, paragraph, type Position, right, sentence,
  wordBackward, wordEnd, wordEndBackward, wordForward,
} from "./motions.js";
import { type Region, textObject } from "./objects.js";
import type { Highlights } from "./highlight.js";
import { searchMotion } from "./search.js";
import type { Register, Registers } from "./registers.js";
import {
  columnAtByte, firstNonBlank, lastColumn, nextColumn, previousColumn, utf8Length,
} from "./text.js";

/** Vim's answer to a change in an editor that refuses edits. */
export const READ_ONLY = "E21: Cannot make changes, 'modifiable' is off";

/** Whether `error` is a buffer refusing an edit. */
export function isReadOnlyError(error: unknown): boolean {
  return (error as { code?: string }).code === "BufferReadOnly";
}

export const NORMAL = "vim.normal";
export const INSERT = "vim.insert";
export const REPLACE = "vim.replace";
/** Held while Escape has something to do in normal mode: a visual mode or
 * pending keys. Without it, Escape is left to what holds the editor. */
export const ESCAPABLE = "vim.escapable";

export type Mode = "normal" | "insert" | "replace" | "visual" | "visual-line";

/** Whether `mode` types text, natively in insert mode and over the text in
 * replace mode. */
export function isTyping(mode: Mode): mode is "insert" | "replace" {
  return mode === "insert" || mode === "replace";
}

export function isVisual(mode: Mode): mode is "visual" | "visual-line" {
  return mode === "visual" || mode === "visual-line";
}

/** State every followed editor shares. */
export type Shared = {
  readonly registers: Registers;
  /** The last `f`, `F`, `t`, or `T`, which `;` and `,` repeat. */
  lastFind: { readonly kind: FindKind; readonly char: string } | null;
  /** The direction of the last search, which `n` follows and `N` reverses,
   * and whether smartcase applied to it. Its pattern is the `/` register. */
  search: { readonly forward: boolean; readonly smart: boolean };
  /** The search highlights of every followed editor. */
  readonly highlights: Highlights;
  /** Persists the registers after they change. */
  saveRegisters(): void;
  /** The last change, which `.` repeats. */
  lastChange: Change | null;
  /** The register `@@` plays again. */
  lastMacro: string | null;
  /** Buffer marks `a`–`z`, by document, or by view for an editor without one. */
  readonly marks: Map<DocumentId | ViewNodeTarget, Map<string, Mark>>;
  /** Global marks `A`–`Z`, each in some open document. */
  readonly globalMarks: Map<string, GlobalMark>;
  /** Shows a document's editor in the command's window with the caret at
   * `offset`, or at its line's first non-blank unless `exact`. */
  jumpToDocument(document: DocumentId, offset: number, exact: boolean): Promise<void>;
  /** How many replayed commands of other packages are running. The keys
   * they send run inside the key that replays them. */
  replayingCommands: number;
};

/** A change `.` repeats: the command that made it and, for one that entered
 * insert mode, the input typed there. */
export type Change = { command: Command; insert: readonly RecordedCommand[] | null };

/** A macro being recorded: the keys so far, and the recording of those after
 * the last prompt, whose answers the editor does not receive. */
export type Macro = { readonly register: string; readonly entries: RecordedCommand[]; recording: Recording };

/** A macro being played. */
export type Player = {
  /** The text a prompt takes as its answer, null to dismiss it. */
  answer: string | null;
  /** The insert session's input so far, null outside one. */
  session: RecordedCommand[] | null;
};

export type GlobalMark = Readonly<{ document: DocumentId; mark: Mark }>;

/** One followed editor. */
export type State = {
  /** The tab of the editor's view. */
  readonly target: ViewTarget;
  readonly view: ViewNodeTarget;
  readonly handle: EditorHandle;
  readonly buffer: Buffer;
  /** The editor's document, null for a buffer outside document identity. */
  readonly document: DocumentId | null;
  readonly shared: Shared;
  language: string;
  mode: Mode;
  /** The keys typed toward a command that is not complete. */
  pending: Pending;
  /** Whether the `ESCAPABLE` context is set. */
  escapable: boolean;
  /** What the last command reported, shown in place of the mode until the
   * next key. */
  message: string | null;
  /** The screen column `j` and `k` aim for, valid while the caret is still
   * where this package last put it; Infinity after `$`. */
  want: number | null;
  caret: number | null;
  /** In replace mode, the text each typed character replaced, empty where
   * it was appended at the line's end, and the caret after the last one;
   * Backspace restores them while the caret stays there. */
  replaced: { readonly originals: string[]; caret: number } | null;
  /** The undo group of the current insert session. */
  group: Disposable | null;
  /** The input of the current insert session, null outside one and while
   * `.` replays it. */
  recording: Recording | null;
  /** The change whose insert session is running, which receives its input. */
  change: Change | null;
  /** While `.` runs, the input it replays, which a counted insert repeats. */
  replaying: readonly RecordedCommand[] | null;
  /** The macro being recorded. */
  macro: Macro | null;
  /** The macro being played. */
  player: Player | null;
  /** Whether a motion failed since a macro step began, which stops the macro. */
  failed: boolean;
  /** How many times the insert session's input runs in all, and whether each
   * further run starts on a new line, as for `o` and `O`. */
  repeat: { count: number; open: boolean };
  /** Whether insert mode began on a line of its own, from `o`, `O`, or a
   * linewise change, whose indentation alone is removed again on Escape. */
  opened: boolean;
  /** In a visual mode, the byte offsets of the characters where the
   * selection began and where its caret is; both are inside it. */
  visual: { anchor: number; caret: number } | null;
  /** The editor selection visual mode last set, so a different one is
   * known to come from elsewhere. */
  selected: EditorSelection | null;
  /** The previous visual area, which `gv` selects again. */
  previous: { readonly anchor: Mark; readonly caret: Mark; readonly linewise: boolean } | null;
  /** The positions jumps left, which `ctrl-o` and `ctrl-i` step through. */
  readonly jumps: JumpList<Mark>;
  /** The position before the latest jump, which `''` returns to. */
  context: Mark | null;
  /** How many lines `ctrl-d` and `ctrl-u` scroll, once a count set it, as
   * Vim's `scroll` option; half the view until then. */
  scroll: number | null;
  /** The folds `zn` expanded, which `zN` collapses again, or null while
   * folding is on. */
  foldsOff: readonly FoldRange[] | null;
};

/** Sets the `ESCAPABLE` context to whether Escape would do something now. */
export async function syncEscape(state: State) {
  const on = !isTyping(state.mode) && (isVisual(state.mode) || state.pending.typed !== "");
  if (on === state.escapable) return;
  state.escapable = on;
  await state.handle.setKeyContext(ESCAPABLE, on);
}

export async function setMode(state: State, mode: Mode) {
  const was = state.mode;
  state.mode = mode;
  if (!isVisual(mode)) {
    state.visual = null;
    state.selected = null;
  }
  if (mode !== "replace") state.replaced = null;
  // Visual modes share normal mode's keys, context, and caret.
  if (!isTyping(was) && !isTyping(mode) && was !== mode) return syncEscape(state);
  if (isTyping(mode) && state.replaying === null) {
    // A playing macro's input is not recorded; it keeps its own.
    if (state.player === null) state.recording = await state.handle.record();
    else state.player.session = [];
  }
  await state.handle.setKeyContext(NORMAL, !isTyping(mode));
  await state.handle.setKeyContext(INSERT, mode === "insert");
  await state.handle.setKeyContext(REPLACE, mode === "replace");
  await state.handle.setCursorStyle(mode === "insert" ? null : mode === "replace" ? "underline" : "block");
  await syncEscape(state);
}

/** Puts a caret that sits past its line's last character back on it, as
 * normal mode never rests on a line's end. A selection is left alone. */
export async function clampCaret(state: State, selection?: EditorSelection) {
  const read = selection ?? await state.handle.selection({ lines: 0 });
  const { anchor, caret } = read;
  if (anchor !== caret) return;
  const lines = new BufferLines(state.buffer, read);
  const position = await lines.position(caret);
  const last = lastColumn(await lines.text(position.line));
  if (position.col > last) await moveTo(state, lines, { line: position.line, col: last });
}

export async function moveTo(
  state: State,
  lines: BufferLines,
  position: Position,
  reveal: "nearest" | "center" = "nearest",
) {
  const offset = await lines.offset(position);
  state.caret = offset;
  await state.handle.setSelection({ anchor: offset, caret: offset }, { reveal });
}

/** Moves to `position`, or the line's last character when it is past it. */
export async function moveOnto(state: State, lines: BufferLines, position: Position) {
  const text = await lines.text(position.line);
  await moveTo(state, lines, { line: position.line, col: Math.min(position.col, lastColumn(text)) });
}

/** The command `.` repeats for `command`, or null when it is no change. A
 * search is repeated with `n`, since the pattern is already set. */
function repeatable(command: Command): Command | null {
  switch (command.kind) {
    case "operator": {
      const { operator, target } = command;
      if (operator === "y" || operator === "zf" || target.kind === "selection") return null;
      const motion = target.kind === "motion" ? target.motion : null;
      if (typeof motion === "object" && motion !== null && "search" in motion && motion.search === "prompt") {
        return { ...command, target: { kind: "motion", motion: { search: "repeat", reverse: false } } };
      }
      return command;
    }
    case "put":
    case "replace":
    case "toggle-case":
    case "join":
    case "insert":
      return command;
    default:
      return null;
  }
}

/** Makes `command` the change `.` repeats, if it is one. */
export function rememberChange(state: State, command: Command) {
  const repeated = repeatable(command);
  if (repeated === null) return;
  state.change = state.shared.lastChange = { command: repeated, insert: null };
}

/** Makes an operator over the visual selection the change `.` repeats, as
 * the same operator over as much text from the caret. A selection over line
 * breaks is not repeated. */
export async function rememberSelection(
  state: State,
  lines: BufferLines,
  operator: Operator,
  register: string | null,
  region: Region,
) {
  let target: Target;
  let count: number;
  if (region.linewise) {
    target = { kind: "line" };
    count = region.end.line - region.start.line + 1;
  } else if (region.start.line === region.end.line) {
    const text = await lines.text(region.start.line);
    count = 0;
    for (let col = region.start.col; col < region.end.col; col = nextColumn(text, col)) count++;
    target = { kind: "motion", motion: "right" };
  } else {
    return;
  }
  rememberChange(state, { kind: "operator", operator, register, count, target });
}

/** `.`: runs the last change again, with its input if it entered insert
 * mode, as one undo step. A count replaces the change's own. */
async function repeatChange(state: State, count: number | null) {
  const change = state.shared.lastChange;
  if (change === null) return;
  if (count !== null) change.command = withCount(change.command, count);
  const entries = change.insert ?? [];
  state.replaying = entries;
  try {
    await run(state, change.command, true);
    if (isTyping(state.mode)) {
      await replay(state, entries);
      await leaveInsert(state);
    }
  } finally {
    state.replaying = null;
  }
}

function withCount(command: Command, count: number): Command {
  switch (command.kind) {
    case "operator":
    case "put":
    case "replace":
    case "toggle-case":
    case "join":
    case "insert":
      return { ...command, count };
    default:
      return command;
  }
}

/** Runs recorded input again. Replace mode's input is this package's own
 * key commands, which run here directly. */
async function replay(state: State, entries: readonly RecordedCommand[]) {
  for (const { command, arguments: args } of entries) {
    if (command === "vim.type") {
      for (const key of (args as { text: string }).text) await replaceKey(state, key);
    } else if (command === "vim.key") {
      await replaceKey(state, (args as { key: string }).key);
    } else {
      await invokeRecorded(state.shared, command, args);
    }
  }
}

/** Runs a recorded command that is not one of this package's keys. */
export async function invokeRecorded(shared: Shared, command: string, args: unknown) {
  shared.replayingCommands++;
  try {
    await commands.invoke(command, args as never);
  } finally {
    shared.replayingCommands--;
  }
}

/** Runs a command; `repeating` is whether `.` runs it again. */
export async function run(state: State, command: Command, repeating = false) {
  if (!repeating) rememberChange(state, command);
  switch (command.kind) {
    case "repeat":
      await repeatChange(state, command.count);
      break;
    case "motion":
      await motion(state, command.motion, command.count);
      break;
    case "operator":
      await operate(state, command.operator, command.register, command.count, command.target);
      break;
    case "put":
      await put(state, command.register, command.count, command.before, command.variant);
      break;
    case "show-character":
      await showCharacter(state);
      break;
    case "change":
      // A diff has its own hunks, from @knot/diff; elsewhere the changes are
      // @knot/vcs's. Without either the command is unavailable and nothing moves.
      for (let step = 0; step < command.count; step += 1) {
        let outcome = await commands.invoke(command.forward ? "diff.nextHunk" : "diff.previousHunk");
        if (outcome.kind === "unavailable") {
          outcome = await commands.invoke(command.forward ? "vcs.nextHunk" : "vcs.previousHunk");
        }
        if (outcome.kind !== "completed") break;
      }
      break;
    case "replace":
      await replaceCharacters(state, command.char, command.count);
      break;
    case "toggle-case":
      await toggleCase(state, command.count);
      break;
    case "join":
      await join(state, command.count, command.spaces !== false);
      break;
    case "insert":
      await enterInsert(state, command.at, command.count);
      break;
    case "set-mark":
      await setMark(state, command.name);
      break;
    case "fold":
      await runFold(state, command.action, command.count);
      break;
    case "jump":
      await stepJumps(state, command.count);
      break;
    case "scroll": {
      const lines = new BufferLines(state.buffer);
      const from = await lines.position((await state.handle.selection()).caret);
      const moved = await scroll(state, lines, from, command.scroll, command.count);
      if (moved !== null) await land(state, lines, moved);
      break;
    }
    case "reposition": {
      const lines = new BufferLines(state.buffer);
      const from = await lines.position((await state.handle.selection()).caret);
      let line = from.line;
      if (command.count !== null || command.firstNonBlank) {
        if (command.count !== null) line = Math.min((await lines.count()) - 1, command.count - 1);
        await land(state, lines, command.firstNonBlank
          ? { to: await lineStart(lines, line), type: "linewise" }
          : await towardWant(state, lines, from, line));
      }
      await reposition(state, line, command.at);
      break;
    }
    case "undo":
    case "redo": {
      for (let n = 0; n < command.count; n++) await commands.invoke(`editor.${command.kind}`);
      // Undoing a change made through a selection restores it; normal mode
      // keeps a bare caret.
      const { anchor, caret } = await state.handle.selection();
      if (anchor !== caret) {
        const start = Math.min(anchor, caret);
        await state.handle.setSelection({ anchor: start, caret: start });
      }
      state.want = null;
      await clampCaret(state);
      break;
    }
  }
}

/** Where a motion goes and how an operator takes the text it crosses. */
export type Moved = {
  readonly to: Position;
  readonly type: "exclusive" | "inclusive" | "linewise";
  /** The screen column `j` and `k` keep aiming for afterwards, when the
   * motion sets one other than where it lands. */
  readonly want?: number;
  readonly reveal?: "nearest" | "center";
  /** Whether the motion is a jump, which the jumplist records. */
  readonly jump?: boolean;
};

/** Resolves a motion from `from`, or null when it fails, as `j` on the last
 * line or `fx` without an x does. With an operator, motions follow Vim's
 * operator-pending forms: `l` may reach the line's end and `w` stops there. */
export async function resolve(
  state: State,
  lines: BufferLines,
  from: Position,
  name: Motion,
  count: number | null,
  operator: Operator | null,
): Promise<Moved | null> {
  const n = count ?? 1;
  const total = await lines.count();
  const text = await lines.text(from.line);
  if (typeof name !== "string" && "mark" in name) return markMotion(state, lines, name);
  if (typeof name !== "string" && "search" in name) return searchMotion(state, lines, from, name, count);
  if (typeof name !== "string") {
    state.shared.lastFind = { kind: name.find, char: name.char };
    return find(text, from, name.find, name.char, n, false);
  }
  switch (name) {
    case "left": return { to: await left(lines, from, n, false), type: "exclusive" };
    case "right": {
      if (operator === null) return { to: await right(lines, from, n, false), type: "exclusive" };
      let col = from.col;
      for (let step = 0; step < n && col < text.length; step++) col = nextColumn(text, col);
      return { to: { line: from.line, col }, type: "exclusive" };
    }
    case "left-wrap": return { to: await left(lines, from, n, true), type: "exclusive" };
    case "right-wrap": return { to: await right(lines, from, n, true), type: "exclusive" };
    case "up":
    case "down": {
      // A collapsed fold counts as one line.
      const folds = await state.handle.folds();
      const line = name === "up" ? linesUp(folds, from.line, n) : linesDown(folds, from.line, n, total);
      if (operator !== null) return line === from.line ? null : { to: { line, col: 0 }, type: "linewise" };
      return towardWant(state, lines, from, line);
    }
    case "row-up":
    case "row-down": {
      const native = name === "row-up" ? "editor.move-up" : "editor.move-down";
      // In visual mode the editor's caret is not on the character.
      const caret = await lines.offset(from);
      await state.handle.setSelection({ anchor: caret, caret });
      for (let step = 0; step < n; step++) await commands.invoke(native);
      const fresh = new BufferLines(state.buffer);
      return { to: await fresh.position((await state.handle.selection()).caret), type: "exclusive" };
    }
    case "word":
    case "WORD": {
      const big = name === "WORD";
      // `cw` on a word changes to its end, as `ce` would, but never past it.
      if (operator === "c" && text.length > 0 && !/[ \t]/.test(text[from.col] ?? " ")) {
        return { to: await wordEnd(lines, from, n, big, true), type: "inclusive" };
      }
      return { to: await wordForward(lines, from, n, big, operator !== null), type: "exclusive" };
    }
    case "back": return { to: await wordBackward(lines, from, n, false), type: "exclusive" };
    case "BACK": return { to: await wordBackward(lines, from, n, true), type: "exclusive" };
    case "end": return { to: await wordEnd(lines, from, n, false), type: "inclusive" };
    case "END": return { to: await wordEnd(lines, from, n, true), type: "inclusive" };
    case "back-end": return { to: await wordEndBackward(lines, from, n, false), type: "inclusive" };
    case "BACK-END": return { to: await wordEndBackward(lines, from, n, true), type: "inclusive" };
    case "line-start": return { to: { line: from.line, col: 0 }, type: "exclusive" };
    case "first-non-blank": return { to: { line: from.line, col: firstNonBlank(text) }, type: "exclusive" };
    case "line-down-first-non-blank": {
      const line = Math.min(total - 1, from.line + n - 1);
      return { to: await lineStart(lines, line), type: "linewise" };
    }
    case "column": {
      const col = columnAtByte(text, await state.handle.columnAtCells(from.line, n - 1));
      return { to: { line: from.line, col: Math.min(col, lastColumn(text)) }, type: "exclusive" };
    }
    case "line-end": {
      const line = Math.min(total - 1, from.line + n - 1);
      return { to: { line, col: lastColumn(await lines.text(line)) }, type: "inclusive", want: Infinity };
    }
    case "next-line-start":
    case "previous-line-start": {
      const line = from.line + (name === "next-line-start" ? n : -n);
      if (operator !== null && (line < 0 || line >= total)) return null;
      return { to: await lineStart(lines, line), type: "linewise" };
    }
    case "first-line":
      return {
        to: await lineStart(lines, count === null ? 0 : count - 1), type: "linewise", reveal: "center", jump: true,
      };
    case "last-line":
      return {
        to: await lineStart(lines, count === null ? total - 1 : count - 1), type: "linewise", reveal: "center",
        jump: true,
      };
    case "match-pair": {
      if (count !== null) {
        if (count > 100) return null;
        const line = Math.ceil((count * total) / 100) - 1;
        return { to: await lineStart(lines, line), type: "linewise", reveal: "center", jump: true };
      }
      const to = await matchingBracket(state, lines, from);
      return to === null ? null : { to, type: "inclusive", reveal: "center", jump: true };
    }
    case "paragraph-forward":
    case "paragraph-back": {
      const found = await paragraph(lines, from, n, name === "paragraph-forward");
      return found === null
        ? null
        : { to: found.position, type: found.inclusive ? "inclusive" : "exclusive", jump: true };
    }
    case "sentence-forward":
    case "sentence-back": {
      const found = await sentence(lines, from, n, name === "sentence-forward");
      return found === null
        ? null
        : { to: found.position, type: found.inclusive ? "inclusive" : "exclusive", jump: true };
    }
    case "repeat-find":
    case "repeat-find-reverse": {
      const last = state.shared.lastFind;
      if (last === null) return null;
      const kind = name === "repeat-find" ? last.kind : REVERSED[last.kind];
      return find(text, from, kind, last.char, n, true);
    }
    case "fold-next":
    case "fold-previous":
    case "fold-start":
    case "fold-end": {
      const line = await foldMotion(state, from.line, name, n);
      return line === null ? null : { to: await lineStart(lines, line), type: "linewise" };
    }
    case "window-top":
    case "window-middle":
    case "window-bottom": {
      const { first, rows } = await state.handle.viewport();
      const row = name === "window-top"
        ? first + Math.min(rows, n) - 1
        : name === "window-bottom"
        ? first + rows - Math.min(rows, n)
        : first + Math.floor((rows - 1) / 2);
      const { line } = await state.handle.rowAt(row);
      return { to: await lineStart(lines, line), type: "linewise", jump: true };
    }
  }
}

/** The display cells the UTF-16 column `col` of buffer line `line`, whose text
 * is `text`, starts at, measured by the editor as soft wrap measures rows. */
export async function cellsAt(state: State, line: number, text: string, col: number): Promise<number> {
  return state.handle.cellsBefore(line, utf8Length(text.slice(0, col)));
}

/** `j` and `k` landing on `line`: the character covering the display cell the
 * caret aims for, which they keep aiming for. */
export async function towardWant(state: State, lines: BufferLines, from: Position, line: number): Promise<Moved> {
  const caret = await lines.offset(from);
  const want = state.caret === caret && state.want !== null
    ? state.want
    : await cellsAt(state, from.line, await lines.text(from.line), from.col);
  const target = await lines.text(line);
  const col = want === Infinity
    ? lastColumn(target)
    : columnAtByte(target, await state.handle.columnAtCells(line, want));
  return { to: { line, col }, type: "linewise", want };
}

/**
 * Scrolls the view for `ctrl-d`, `ctrl-u`, `ctrl-f`, `ctrl-b`, `ctrl-e`, or
 * `ctrl-y` and returns where the caret goes from `from`, or null when it
 * stays. Half-view scrolls move the caret as many rows as the view, so it
 * keeps its place on screen; page scrolls keep two rows of the previous
 * view and move the caret into the new one; line scrolls move it only when
 * it would leave the view. Rows are screen lines, so soft wrap counts.
 */
export async function scroll(
  state: State,
  lines: BufferLines,
  from: Position,
  kind: Scroll,
  count: number | null,
): Promise<Moved | null> {
  const total = await lines.count();
  const before = await state.handle.viewport();
  const height = before.rows;
  const lastRow = (viewport: { first: number; rows: number }) => viewport.first + viewport.rows - 1;
  const caretRow = await rowOfPosition(state, lines, from);
  switch (kind) {
    case "half-down":
    case "half-up": {
      if (count !== null) state.scroll = count;
      const n = state.scroll ?? Math.max(1, Math.floor(height / 2));
      const step = kind === "half-down" ? n : -n;
      const target = Math.max(0, caretRow + step);
      const here = await state.handle.rowAt(caretRow);
      const there = await state.handle.rowAt(target);
      if (here.line === there.line && here.column === there.column) return null;
      // The view stops with the last row at the bottom edge, even where the
      // editor scrolls further; only the caret moves on.
      const lastText = await lines.text(total - 1);
      const room = Math.max(0, (await rowOfPosition(state, lines, { line: total - 1, col: lastText.length })) - lastRow(before));
      await state.handle.scrollBy(step > 0 ? Math.min(step, room) : step);
      return toRow(state, lines, from, target, false);
    }
    case "page-down":
    case "page-up": {
      const down = kind === "page-down";
      await state.handle.scrollBy((down ? 1 : -1) * Math.max(1, height - 2) * (count ?? 1));
      const after = await state.handle.viewport();
      // At the end of the text the view cannot scroll, so the caret goes
      // there instead.
      if (after.first === before.first) {
        const line = down ? total - 1 : 0;
        if (line === from.line) return null;
        return { to: await lineStart(lines, line), type: "linewise" };
      }
      return toRow(state, lines, from, down ? Math.max(caretRow, after.first) : Math.min(caretRow, lastRow(after)), false);
    }
    case "line-down":
    case "line-up": {
      const n = count ?? 1;
      await state.handle.scrollBy(kind === "line-down" ? n : -n);
      const after = await state.handle.viewport();
      if (caretRow < after.first) return toRow(state, lines, from, after.first, true);
      if (caretRow > lastRow(after)) return toRow(state, lines, from, lastRow(after), true);
      return null;
    }
  }
}

/** Where the caret goes to be on document row `row`: that row's start when it
 * is on the caret's line, or else the line's first non-blank, or the column
 * `j` and `k` aim for with `aim`. */
async function toRow(state: State, lines: BufferLines, from: Position, row: number, aim: boolean): Promise<Moved> {
  const start = await state.handle.rowAt(row);
  if (start.line === from.line) {
    const text = await lines.text(start.line);
    return { to: { line: start.line, col: columnAtByte(text, start.column) }, type: "linewise" };
  }
  return aim
    ? towardWant(state, lines, from, start.line)
    : { to: await lineStart(lines, start.line), type: "linewise" };
}

/** The document row the UTF-16 position `at` shows on. */
async function rowOfPosition(state: State, lines: BufferLines, at: Position): Promise<number> {
  const text = await lines.text(at.line);
  return state.handle.rowOf(at.line, utf8Length(text.slice(0, at.col)));
}

/** Scrolls `line` to the top, middle, or bottom of the view. In normal
 * mode the caret is on it, and the editor aligns its edges exactly. */
export async function reposition(state: State, line: number, at: "top" | "center" | "bottom") {
  if (state.mode === "normal") {
    const { caret } = await state.handle.selection();
    await state.handle.setSelection({ anchor: caret, caret }, { reveal: at === "center" ? "always-center" : at });
    return;
  }
  const { first, rows } = await state.handle.viewport();
  const target = at === "top" ? first : at === "bottom" ? first + rows - 1 : first + Math.floor((rows - 1) / 2);
  const row = await state.handle.rowOf(line, 0);
  if (row !== target) await state.handle.scrollBy(row - target);
}

const REVERSED: Readonly<Record<FindKind, FindKind>> = { f: "F", F: "f", t: "T", T: "t" };

function find(text: string, from: Position, kind: FindKind, char: string, count: number, repeat: boolean): Moved | null {
  const col = findInLine(text, from.col, kind, char, count, repeat);
  if (col === null) return null;
  return { to: { line: from.line, col }, type: kind === "f" || kind === "t" ? "inclusive" : "exclusive" };
}

/** `'` and `` ` ``: to a mark in this buffer, or null when it is unset, in
 * another document, or its text was deleted. */
async function markMotion(state: State, lines: BufferLines, motion: MarkMotion): Promise<Moved | null> {
  const offset = await markOffset(state, motion.mark);
  if (offset === null) return null;
  const to = await lines.position(offset);
  if (motion.exact) return { to, type: "exclusive", jump: true };
  return { to: await lineStart(lines, to.line), type: "linewise", jump: true };
}

export async function markOffset(state: State, name: string): Promise<number | null> {
  let mark: Mark | undefined;
  if (name === "<" || name === ">") {
    const { previous } = state;
    if (previous === null) return null;
    const anchor = await previous.anchor.position();
    const caret = await previous.caret.position();
    if (anchor === null || caret === null) return null;
    return name === "<" ? Math.min(anchor, caret) : Math.max(anchor, caret);
  }
  if (name === "'" || name === "`") {
    mark = state.context ?? undefined;
  } else if (name === name.toLowerCase()) {
    mark = state.shared.marks.get(state.document ?? state.view)?.get(name);
  } else {
    const global = state.shared.globalMarks.get(name);
    if (global?.document === state.document) mark = global.mark;
  }
  return mark === undefined ? null : mark.position();
}

/** A mark at `offset`, or null in an empty buffer, which holds none. */
async function markAt(state: State, offset: number): Promise<Mark | null> {
  try {
    return await state.buffer.createMark(offset);
  } catch (error) {
    if ((error as { code?: string }).code === "InvalidRange") return null;
    throw error;
  }
}

/** `m`: sets a buffer or global mark at the caret, or with `'` or `` ` ``
 * records the caret as a jump, which `''` then returns to. */
async function setMark(state: State, name: string) {
  const { caret } = await state.handle.selection();
  if (name === "'" || name === "`") return recordJump(state, caret);
  await setMarkAt(state, name, caret);
}

/** Sets the buffer or global mark `name` at `offset`. */
async function setMarkAt(state: State, name: string, offset: number) {
  const mark = await markAt(state, offset);
  if (mark === null) return;
  const { marks, globalMarks } = state.shared;
  if (name === name.toLowerCase()) {
    const key = state.document ?? state.view;
    let buffer = marks.get(key);
    if (buffer === undefined) marks.set(key, buffer = new Map());
    buffer.get(name)?.[Symbol.asyncDispose]();
    buffer.set(name, mark);
  } else if (state.document !== null) {
    globalMarks.get(name)?.mark[Symbol.asyncDispose]();
    globalMarks.set(name, { document: state.document, mark });
  }
}

/** Records a jump from `offset` in the jumplist, and as the position `''`
 * returns to. */
export async function recordJump(state: State, offset: number) {
  const entry = await markAt(state, offset);
  if (entry === null) return;
  state.context?.[Symbol.asyncDispose]();
  state.context = await markAt(state, offset);
  const line = await state.buffer.lineAt(offset);
  for (const removed of state.jumps.push(entry, line, await linesOf(state))) removed[Symbol.asyncDispose]();
}

/** `ctrl-o` and `ctrl-i`: goes to the jumplist's entry `count` away. */
async function stepJumps(state: State, count: number) {
  const { caret } = await state.handle.selection();
  const current = await markAt(state, caret);
  if (current === null) return;
  const line = await state.buffer.lineAt(caret);
  const { target, removed } = state.jumps.step(count, current, line, await linesOf(state));
  for (const mark of removed) mark[Symbol.asyncDispose]();
  const offset = target === null ? null : await target.position();
  if (offset === null) return;
  const lines = new BufferLines(state.buffer);
  state.want = null;
  await moveOnto(state, lines, await lines.position(offset));
}

/** The line of each jumplist entry now, or null once its text is gone. */
async function linesOf(state: State): Promise<LineOf<Mark>> {
  const entries = state.jumps.entries;
  const lines = await Promise.all(entries.map(async (mark) => {
    const offset = await mark.position();
    return offset === null ? null : state.buffer.lineAt(offset);
  }));
  const byMark = new Map(entries.map((mark, index) => [mark, lines[index]!]));
  return (mark) => byMark.get(mark) ?? null;
}

/** `%`: the partner of the first bracket at or after the caret on its line.
 * The native match is syntax-aware, so brackets in strings and comments are
 * left out; without syntax, brackets are counted in the text. */
async function matchingBracket(state: State, lines: BufferLines, from: Position): Promise<Position | null> {
  const text = await lines.text(from.line);
  const col = bracketFrom(text, from.col);
  if (col === null) return null;
  const at = { line: from.line, col };
  const bracket = text[col]!;
  const partner = { "(": ")", ")": "(", "[": "]", "]": "[", "{": "}", "}": "{" }[bracket]!;
  // The native jump prefers the bracket before the caret, so the caret goes
  // just after the one Vim looks at.
  const after = (await lines.offset(at)) + 1;
  await state.handle.setSelection({ anchor: after, caret: after });
  await commands.invoke("editor.jump-to-matching-bracket");
  const jumped = await lines.position((await state.handle.selection()).caret);
  if ((await lines.text(jumped.line))[jumped.col] === partner) return jumped;
  // An adjacent pair, where the native jump goes back to the opener, or no
  // syntax.
  return matchBracketText(lines, at);
}

async function motion(state: State, name: Motion, count: number | null) {
  // Counts of one or two lines stay within the lines read here.
  const selection = await state.handle.selection({ lines: 1 });
  const { caret } = selection;
  const lines = new BufferLines(state.buffer, selection);
  const from = await lines.position(caret);
  if (typeof name !== "string" && "mark" in name) {
    const global = state.shared.globalMarks.get(name.mark);
    if (global !== undefined && global.document !== state.document) {
      const offset = await global.mark.position();
      if (offset === null) return;
      await recordJump(state, caret);
      await state.shared.jumpToDocument(global.document, offset, name.exact);
      return;
    }
  }
  const moved = await resolve(state, lines, from, name, count, null);
  if (moved === null) {
    // A failed `%` may have moved the native caret.
    state.failed = true;
    await moveTo(state, lines, from);
    return;
  }
  const stuck = name === "left" || name === "right" ? same(moved.to, from) : moved.to.line === from.line;
  if (typeof name === "string" && STEPS.has(name) && stuck) state.failed = true;
  if (moved.jump === true) await recordJump(state, caret);
  await land(state, lines, moved.to.line === from.line ? moved : await intoShownLine(state, lines, moved));
}

/** The motions that fail when they cannot move, as `j` on the last line. */
const STEPS = new Set<Motion>(["left", "right", "up", "down"]);

function same(a: Position, b: Position): boolean {
  return a.line === b.line && a.col === b.col;
}

/** A prompt's answer: the text a playing macro recorded for it, or what
 * `ask` gets, which a recording macro keeps for playing. Null is a dismissed
 * prompt. */
export async function prompted(state: State, ask: () => Promise<string | null>): Promise<string | null> {
  if (state.player !== null) {
    const { answer } = state.player;
    state.player.answer = null;
    return answer;
  }
  const { macro } = state;
  if (macro === null) return ask();
  // The prompt's own typing never reaches the editor. The key that opened it
  // moves after its answer, and what follows is recorded afresh.
  const segment = [...await macro.recording.stop()];
  const opener = segment.pop();
  macro.entries.push(...segment);
  macro.recording = await state.handle.record();
  const text = await ask();
  if (text !== null) macro.entries.push({ command: "vim.answer", arguments: { text } });
  if (opener !== undefined) macro.entries.push(opener);
  return text;
}

/** Moves the caret where a motion went, onto its line's last character at
 * most, and sets the column `j` and `k` aim for. */
async function land(state: State, lines: BufferLines, moved: Moved) {
  const text = await lines.text(moved.to.line);
  const to = { line: moved.to.line, col: Math.min(moved.to.col, lastColumn(text)) };
  await moveTo(state, lines, to, moved.reveal);
  state.want = moved.want ?? await cellsAt(state, to.line, text, to.col);
}

/** The region from `from` to where a motion moved, by its type. */
async function motionRegion(lines: BufferLines, from: Position, moved: Moved): Promise<Region> {
  const [start, end] = before(from, moved.to) ? [from, moved.to] : [moved.to, from];
  if (moved.type === "linewise") return { start, end, linewise: true };
  if (moved.type === "exclusive") return { start, end, linewise: false };
  const text = await lines.text(end.line);
  const after = end.col < text.length ? nextColumn(text, end.col) : text.length;
  return { start, end: { line: end.line, col: after }, linewise: false };
}

function before(a: Position, b: Position): boolean {
  return a.line < b.line || (a.line === b.line && a.col <= b.col);
}

/** Vim's rules for characterwise text ending at a line's start: it ends on
 * the previous line instead, and when it starts in its line's indentation
 * it becomes the whole lines, so `d}` and `ci{` take full lines. A delete
 * that ends with only white space left on its line and starts in the
 * indentation is linewise too. */
async function adjust(lines: BufferLines, region: Region, operator: Operator): Promise<Region> {
  if (region.linewise || region.end.line === region.start.line) return region;
  const inIndent = /^[ \t]*$/.test((await lines.text(region.start.line)).slice(0, region.start.col));
  if (region.end.col === 0) {
    const line = region.end.line - 1;
    if (inIndent) return { start: region.start, end: { line, col: 0 }, linewise: true };
    return { start: region.start, end: { line, col: (await lines.text(line)).length }, linewise: false };
  }
  const rest = (await lines.text(region.end.line)).slice(region.end.col);
  if (operator === "d" && inIndent && /^[ \t]*$/.test(rest)) return { ...region, linewise: true };
  return region;
}

/** Whether a region is empty, so an operator has nothing to act on. */
function empty(region: Region): boolean {
  return !region.linewise && region.start.line === region.end.line && region.start.col >= region.end.col;
}

/** The register text of a region. */
export async function regionText(lines: BufferLines, region: Region): Promise<Register> {
  if (region.linewise) {
    const texts: string[] = [];
    for (let line = region.start.line; line <= region.end.line; line++) texts.push(await lines.text(line));
    return { text: `${texts.join("\n")}\n`, linewise: true };
  }
  const start = await lines.offset(region.start);
  const end = await lines.offset(region.end);
  const { text } = await lines.buffer.snapshot({ startByteOffset: start, endByteOffset: end });
  return { text, linewise: false };
}

/** The byte range a delete of a region removes: whole lines take one line
 * break with them, the one before, or after the first line. Starting before
 * the first line removes a mark at its start, which Vim's line marks lose. */
async function deletedRange(lines: BufferLines, region: Region): Promise<[number, number]> {
  if (!region.linewise) return [await lines.offset(region.start), await lines.offset(region.end)];
  const { start: { line: first }, end: { line: last } } = region;
  const end = await lines.offset({ line: last, col: (await lines.text(last)).length });
  if (first > 0) return [await lines.offset({ line: first - 1, col: (await lines.text(first - 1)).length }), end];
  if (last + 1 < (await lines.count())) return [0, await lines.offset({ line: last + 1, col: 0 })];
  return [0, end];
}

export async function replace(state: State, start: number, end: number, text: string) {
  const { revision } = await state.buffer.snapshot({ startByteOffset: start, endByteOffset: start });
  await state.buffer.applyEdits([{ range: { startByteOffset: start, endByteOffset: end }, text }], {
    ifRevision: revision,
  });
}

/** Opens an undo group, so a change restores the caret from before it when
 * undone, or null while another view of the buffer has one open, which then
 * takes this change too. */
async function openGroup(state: State): Promise<Disposable | null> {
  try {
    return await state.handle.beginUndoGroup();
  } catch (error) {
    if ((error as { code?: string }).code !== "OperationInProgress") throw error;
    return null;
  }
}

export async function grouped(state: State, body: () => Promise<void>) {
  await using _group = await openGroup(state);
  await body();
}

/** Records a yank or delete in the registers and the clipboard. */
export async function record(
  state: State,
  register: string | null,
  value: Register,
  kind: "yank" | "delete",
  big = false,
) {
  const clipboard = state.shared.registers.record(register, value, kind, big);
  if (clipboard !== null) await workbench.writeClipboardText(clipboard);
  state.shared.saveRegisters();
}

async function operate(
  state: State,
  operator: Operator,
  register: string | null,
  count: number | null,
  target: Target,
) {
  const lines = new BufferLines(state.buffer);
  const from = await lines.position((await state.handle.selection()).caret);
  let region: Region | null;
  let big = false;
  switch (target.kind) {
    case "selection":
      return;
    case "line": {
      const folds = await state.handle.folds();
      const total = await lines.count();
      const last = linesDown(folds, from.line, (count ?? 1) - 1, total);
      region = { start: { line: from.line, col: 0 }, end: { line: last, col: 0 }, linewise: true };
      break;
    }
    case "object":
      region = await textObject(lines, from, target.object, target.around, count ?? 1);
      break;
    case "motion": {
      const moved = await resolve(state, lines, from, target.motion, count, operator);
      region = moved === null ? null : await motionRegion(lines, from, moved);
      big = target.motion === "match-pair" || target.motion === "paragraph-forward"
        || target.motion === "paragraph-back"
        || target.motion === "sentence-forward" || target.motion === "sentence-back" || (typeof target.motion !== "string" && "search" in target.motion)
        || (typeof target.motion !== "string" && "mark" in target.motion && target.motion.exact);
      break;
    }
  }
  if (region === null) {
    state.failed = true;
    await moveTo(state, lines, from);
    return;
  }
  region = await adjust(lines, region, operator);
  if (region.linewise) region = await aroundFolds(state, region);
  await applyOperator(state, lines, operator, register, region, from, big);
}

/** Whole lines take the closed folds at their ends along, as `dd` on a
 * closed fold deletes all of it. */
export async function aroundFolds(state: State, region: Region): Promise<Region> {
  const folds = await state.handle.folds();
  const first = shownLine(folds, region.start.line);
  const last = closedAt(folds, region.end.line)?.end ?? region.end.line;
  return { ...region, start: { ...region.start, line: first }, end: { ...region.end, line: last } };
}

/** Applies an operator to a region, from the caret at `from`. `big` makes a
 * small delete shift the numbered registers, as some motions' deletes do,
 * and `shifts` is how many times `>` and `<` indent. */
export async function applyOperator(
  state: State,
  lines: BufferLines,
  operator: Operator,
  register: string | null,
  region: Region,
  from: Position,
  big: boolean,
  shifts = 1,
) {
  state.want = null;
  switch (operator) {
    case "y": {
      if (!empty(region)) await record(state, register, await regionText(lines, region), "yank");
      await moveOnto(state, lines, region.linewise ? { line: region.start.line, col: from.col } : region.start);
      return;
    }
    case "d": {
      if (empty(region)) return moveTo(state, lines, from);
      await record(state, register, await regionText(lines, region), "delete", big);
      const [start, end] = await deletedRange(lines, region);
      await grouped(state, () => replace(state, start, end, ""));
      const fresh = new BufferLines(state.buffer);
      if (region.linewise) {
        await moveTo(state, fresh, await lineStart(fresh, region.start.line));
      } else {
        await moveOnto(state, fresh, region.start);
      }
      return;
    }
    case "c":
      return change(state, lines, region, register, big);
    case ">":
    case "<":
    case "=": {
      const native = { ">": "editor.indent-lines", "<": "editor.outdent-lines", "=": "editor.reindent-selected-lines" }[
        operator
      ];
      const first = region.start.line;
      const last = region.end.line;
      // The native command takes a closed fold's lines along with its first
      // line, and a selection must not end in hidden ones.
      const shown = shownLine(await state.handle.folds(), last);
      await grouped(state, async () => {
        const anchor = await lines.offset({ line: first, col: 0 });
        const caret = await lines.offset({ line: shown, col: (await lines.text(shown)).length });
        await state.handle.setSelection({ anchor, caret });
        for (let n = 0; n < shifts; n++) await commands.invoke(native);
      });
      const fresh = new BufferLines(state.buffer);
      await moveTo(state, fresh, await lineStart(fresh, first));
      return;
    }
    case "zf": {
      await createFold(state, region.start.line, region.end.line);
      await moveOnto(state, lines, { line: region.start.line, col: from.col });
      return;
    }
    case "gu":
    case "gU":
    case "g~": {
      if (empty(region)) return moveTo(state, lines, from);
      const transform = operator === "gu" ? lower : operator === "gU" ? upper : swapCase;
      const start = await lines.offset(region.linewise ? { line: region.start.line, col: 0 } : region.start);
      const end = await lines.offset(region.linewise
        ? { line: region.end.line, col: (await lines.text(region.end.line)).length }
        : region.end);
      const { text } = await state.buffer.snapshot({ startByteOffset: start, endByteOffset: end });
      const changed = transform(text);
      if (changed !== text) await grouped(state, () => replace(state, start, end, changed));
      // Like a yank, the caret goes to the start, keeping its column on lines.
      const caret = region.linewise ? { line: region.start.line, col: from.col } : region.start;
      await moveOnto(state, new BufferLines(state.buffer), caret);
      return;
    }
  }
}

/** `c`: deletes the region and starts inserting there, as one undo step
 * with the typing. Changed lines keep the first one's indentation. */
async function change(state: State, lines: BufferLines, region: Region, register: string | null, big: boolean) {
  const group = await openGroup(state);
  if (!empty(region)) await record(state, register, await regionText(lines, region), "delete", big);
  let caret: number;
  if (region.linewise) {
    const indent = /^[ \t]*/.exec(await lines.text(region.start.line))![0];
    const start = await lines.offset({ line: region.start.line, col: 0 });
    const end = await lines.offset({ line: region.end.line, col: (await lines.text(region.end.line)).length });
    await replace(state, start, end, indent);
    caret = start + utf8Length(indent);
  } else {
    const start = await lines.offset(region.start);
    const end = await lines.offset(region.end);
    if (end > start) await replace(state, start, end, "");
    caret = start;
  }
  state.group = group;
  state.opened = region.linewise;
  state.repeat = { count: 1, open: false };
  await setMode(state, "insert");
  state.caret = caret;
  await state.handle.setSelection({ anchor: caret, caret });
}

const lower = (text: string) => text.toLowerCase();
const upper = (text: string) => text.toUpperCase();
export function swapCase(text: string): string {
  let swapped = "";
  for (const character of text) {
    const up = character.toUpperCase();
    swapped += up === character ? character.toLowerCase() : up;
  }
  return swapped;
}

/** `p` and `P`: puts a register's text count times after or before the
 * caret, or below or above its line when the text is linewise. */
async function put(
  state: State,
  register: string | null,
  count: number,
  before: boolean,
  variant?: "caret-after" | "indent",
) {
  const value = await readRegister(state, register);
  if (value === null || value.text === "") return;
  const lines = new BufferLines(state.buffer);
  const from = await lines.position((await state.handle.selection()).caret);
  state.want = null;
  if (value.linewise) {
    let one = value.text.endsWith("\n") ? value.text : `${value.text}\n`;
    if (variant === "indent") one = indented(one, /^[ \t]*/.exec(await lines.text(from.line))![0]);
    const block = one.repeat(count);
    const total = await lines.count();
    // Below a closed fold is after all of it.
    const last = before ? from.line : lastLine(await state.handle.folds(), from.line);
    const line = before ? last : last + 1;
    await grouped(state, async () => {
      if (line < total) {
        const at = await lines.offset({ line, col: 0 });
        await replace(state, at, at, block);
      } else {
        const at = await lines.offset({ line: last, col: (await lines.text(last)).length });
        await replace(state, at, at, `\n${block.slice(0, -1)}`);
      }
    });
    const fresh = new BufferLines(state.buffer);
    // `gp` goes to the line after the text, or stays on the last line.
    const after = line + block.split("\n").length - 1;
    await moveTo(state, fresh, await lineStart(fresh, variant === "caret-after" ? Math.min(after, (await fresh.count()) - 1) : line));
    return;
  }
  const block = value.text.repeat(count);
  const text = await lines.text(from.line);
  const col = before || text.length === 0 ? from.col : nextColumn(text, from.col);
  const at = await lines.offset({ line: from.line, col });
  await grouped(state, () => replace(state, at, at, block));
  const fresh = new BufferLines(state.buffer);
  if (variant === "caret-after") {
    await moveOnto(state, fresh, await fresh.position(at + utf8Length(block)));
  } else if (block.includes("\n")) {
    await moveTo(state, fresh, { line: from.line, col });
  } else {
    const end = await fresh.position(at + utf8Length(block));
    await moveOnto(state, fresh, { line: end.line, col: previousColumn(await fresh.text(end.line), end.col) });
  }
}

/** Lines of text with the first one's indentation replaced by `indent` and
 * the others shifted the same way; blank lines stay empty. */
function indented(text: string, indent: string): string {
  const lines = text.split("\n");
  const first = /^[ \t]*/.exec(lines[0]!)![0];
  return lines.map((line) => {
    if (line.trim() === "") return line === "" ? line : "";
    return indent + (line.startsWith(first) ? line.slice(first.length) : line.trimStart());
  }).join("\n");
}

/** A register's text for a put, or null when it holds none. */
export async function readRegister(state: State, register: string | null): Promise<Register | null> {
  if (register === null || register === "+" || register === "*") {
    const text = await workbench.readClipboardText();
    return text === null ? null : state.shared.registers.fromClipboard(text);
  }
  if (register === "_") return null;
  if (register === "%") {
    const uri = (state.document === null ? null : (await documents.get(state.document))?.resourceUri) ?? null;
    if (uri === null) return null;
    return { text: uri.startsWith("file://") ? decodeURIComponent(uri.slice(7)) : uri, linewise: false };
  }
  return state.shared.registers.get(register);
}

/** `r`: replaces count characters with `char`, or nothing when the line has
 * fewer, leaving the caret on the last one. */
async function replaceCharacters(state: State, char: string, count: number) {
  const lines = new BufferLines(state.buffer);
  const from = await lines.position((await state.handle.selection()).caret);
  const text = await lines.text(from.line);
  let end = from.col;
  for (let n = 0; n < count; n++) {
    if (end >= text.length) return;
    end = nextColumn(text, end);
  }
  const start = await lines.offset(from);
  if (char === "\n") {
    // One line break replaces all the characters.
    await grouped(state, async () => {
      await replace(state, start, await lines.offset({ line: from.line, col: end }), "");
      await commands.invoke("editor.insert-newline");
    });
    state.want = null;
    await clampCaret(state);
    return;
  }
  await grouped(state, async () => replace(state, start, await lines.offset({ line: from.line, col: end }), char.repeat(count)));
  const fresh = new BufferLines(state.buffer);
  const last = await fresh.position(start + utf8Length(char.repeat(count - 1)));
  await moveTo(state, fresh, last);
}

/** `~`: switches the case of count characters and moves past them. */
async function toggleCase(state: State, count: number) {
  const lines = new BufferLines(state.buffer);
  const from = await lines.position((await state.handle.selection()).caret);
  const text = await lines.text(from.line);
  if (text.length === 0) return;
  let end = from.col;
  for (let n = 0; n < count && end < text.length; n++) end = nextColumn(text, end);
  const original = text.slice(from.col, end);
  const swapped = swapCase(original);
  const start = await lines.offset(from);
  if (swapped !== original) {
    await grouped(state, () => replace(state, start, start + utf8Length(original), swapped));
  }
  state.want = null;
  const fresh = new BufferLines(state.buffer);
  await moveOnto(state, fresh, { line: from.line, col: from.col + swapped.length });
}

/** `J`: joins count lines, at least two, replacing each line break and the
 * next line's leading white space with one space, except after white space or
 * before `)` or an empty rest. `gJ` (`spaces` false) keeps the text as it is.
 * The caret goes where the last two lines met. */
export async function join(state: State, count: number, spaces = true) {
  const lines = new BufferLines(state.buffer);
  const { caret } = await state.handle.selection();
  const line = (await lines.position(caret)).line;
  const joins = Math.min(Math.max(count, 2) - 1, (await lines.count()) - 1 - line);
  if (joins <= 0) return;
  let joined = await lines.text(line);
  let col = 0;
  for (let n = 1; n <= joins; n++) {
    let next = await lines.text(line + n);
    if (spaces) next = next.replace(/^[ \t]+/, "");
    const space = spaces && joined !== "" && next !== "" && !next.startsWith(")") && !/[ \t]$/.test(joined);
    col = joined.length;
    joined += (space ? " " : "") + next;
  }
  const start = await lines.offset({ line, col: 0 });
  const end = await lines.offset({ line: line + joins, col: (await lines.text(line + joins)).length });
  await grouped(state, () => replace(state, start, end, joined));
  state.want = null;
  await moveOnto(state, new BufferLines(state.buffer), { line, col });
}

/** `ga`: shows the code of the character under the caret. */
async function showCharacter(state: State) {
  const lines = new BufferLines(state.buffer);
  const at = await lines.position((await state.handle.selection()).caret);
  const text = await lines.text(at.line);
  if (at.col >= text.length) {
    state.message = "NUL";
    return;
  }
  const code = text.codePointAt(at.col)!;
  const hex = code.toString(16).padStart(code > 0xffff ? 8 : code > 0x7f ? 4 : 2, "0");
  state.message = `<${String.fromCodePoint(code)}> ${code}, Hex ${hex}, Oct ${code.toString(8).padStart(3, "0")}`;
}

async function enterInsert(state: State, at: InsertAt, count: number) {
  // Native typing would be refused one key at a time; Vim never enters insert mode.
  const info = await windows.get(state.target.window);
  if (info !== null && windows.findView(info, state.view)?.view.readOnly === true) {
    throw Object.assign(new Error(READ_ONLY), { code: "BufferReadOnly" });
  }
  state.group = await openGroup(state);
  state.opened = at === "o" || at === "O";
  state.repeat = { count, open: state.opened };
  await setMode(state, at === "R" ? "replace" : "insert");
  const lines = new BufferLines(state.buffer);
  if (at === "gi") {
    const offset = await markOffset(state, "^");
    if (offset !== null) await moveTo(state, lines, await lines.position(offset));
  }
  const from = await lines.position((await state.handle.selection()).caret);
  const text = await lines.text(from.line);
  switch (at) {
    case "i":
    case "gi":
    case "R":
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
        await replace(state, start, start, `${indent}\n`);
        await moveTo(state, new BufferLines(state.buffer), { line: from.line, col: indent.length });
      }
      break;
  }
}

/** Ends the insert session's recording and returns its input, which becomes
 * what `.` replays and the `".` register. */
async function finishSession(state: State): Promise<readonly RecordedCommand[]> {
  const recording = state.recording;
  state.recording = null;
  const session = state.player?.session ?? null;
  if (state.player !== null) state.player.session = null;
  const change = state.change;
  state.change = null;
  if (state.replaying !== null) return state.replaying;
  if (recording === null && session === null) return [];
  const entries = recording === null ? session! : [...await recording.stop()];
  if (entries.at(-1)?.command === "vim.leave-insert") entries.pop();
  if (change !== null) change.insert = entries;
  const text = entries.map(({ command, arguments: args }) => {
    if (command === "editor.insert-newline") return "\n";
    // Replace mode's typing is `vim.type`.
    return command === "editor.type" || command === "vim.type" ? (args as { text: string }).text : "";
  }).join("");
  state.shared.registers.set(".", { text, linewise: false });
  state.shared.saveRegisters();
  return entries;
}

/** A key typed in replace mode: a character replaces the one under the
 * caret, or is appended at the line's end; Backspace steps back, restoring
 * what the session replaced there; Enter breaks the line without replacing. */
export async function replaceKey(state: State, key: string) {
  const lines = new BufferLines(state.buffer);
  const { caret } = await state.handle.selection();
  const replaced = state.replaced ??= { originals: [], caret };
  // A caret moved elsewhere, as by the mouse, leaves nothing to restore.
  if (replaced.caret !== caret) replaced.originals.length = 0;
  const at = await lines.position(caret);
  const text = await lines.text(at.line);
  let after: number;
  if (key === "<cr>") {
    await commands.invoke("editor.insert-newline");
    replaced.originals.length = 0;
    after = (await state.handle.selection()).caret;
  } else if (key === "<bs>") {
    if (at.col === 0) return;
    after = await lines.offset({ line: at.line, col: previousColumn(text, at.col) });
    const original = replaced.originals.pop();
    if (original !== undefined) await replace(state, after, caret, original);
  } else {
    // A character is at most two UTF-16 units; a named key types nothing.
    if (key.length > 2) return;
    const end = at.col < text.length ? nextColumn(text, at.col) : at.col;
    const original = text.slice(at.col, end);
    await replace(state, caret, caret + utf8Length(original), key);
    replaced.originals.push(original);
    after = caret + utf8Length(key);
  }
  replaced.caret = after;
  state.caret = after;
  await state.handle.setSelection({ anchor: after, caret: after });
}

export async function leaveInsert(state: State) {
  const entries = await finishSession(state);
  // A count runs the typed input again, on a new line each time after `o`.
  const { count, open } = state.repeat;
  state.repeat = { count: 1, open: false };
  for (let n = 1; n < count; n++) {
    if (open) await commands.invoke("editor.insert-newline");
    await replay(state, entries);
  }
  const lines = new BufferLines(state.buffer);
  const { anchor, caret } = await state.handle.selection();
  await setMarkAt(state, "^", caret);
  const from = await lines.position(caret);
  const text = await lines.text(from.line);
  if (state.opened && text.length > 0 && /^[ \t]*$/.test(text)) {
    // A line of its own left empty keeps no indentation, as in Vim.
    const start = await lines.offset({ line: from.line, col: 0 });
    const end = await lines.offset({ line: from.line, col: text.length });
    await replace(state, start, end, "");
    await moveTo(state, new BufferLines(state.buffer), { line: from.line, col: 0 });
  } else if (anchor === caret) {
    await moveTo(state, lines, { line: from.line, col: from.col > 0 ? previousColumn(text, from.col) : 0 });
  }
  const group = state.group;
  state.group = null;
  state.opened = false;
  state.want = null;
  await group?.[Symbol.asyncDispose]();
  await setMode(state, "normal");
}
