// Runs normal-mode commands in one followed editor: motions, operators over
// motions, text objects, and lines, and the commands that edit directly.
// Text math is done here over line snapshots; indentation, joining, bracket
// matching, and visual-row movement are the native editor commands.
import { commands, documents, languages, settings, workbench } from "knot";
import type { Buffer, Disposable, DocumentId, EditorHandle, ViewTarget } from "knot";
import type { Command, FindKind, InsertAt, Motion, Operator, Target } from "./keys.js";
import { BufferLines } from "./lines.js";
import {
  bracketFrom, findInLine, left, lineStart, matchBracketText, paragraph, type Position, right,
  wordBackward, wordEnd, wordEndBackward, wordForward,
} from "./motions.js";
import { type Region, textObject } from "./objects.js";
import type { Register, Registers } from "./registers.js";
import {
  columnAtScreen, firstNonBlank, lastColumn, nextColumn, previousColumn, screenColumn, utf8Length,
} from "./text.js";

export const NORMAL = "vim.normal";
export const INSERT = "vim.insert";

export type Mode = "normal" | "insert";

/** State every followed editor shares. */
export type Shared = {
  readonly registers: Registers;
  /** The last `f`, `F`, `t`, or `T`, which `;` and `,` repeat. */
  lastFind: { readonly kind: FindKind; readonly char: string } | null;
  /** Persists the registers after they change. */
  saveRegisters(): void;
};

/** One followed editor. */
export type State = {
  readonly target: ViewTarget;
  readonly handle: EditorHandle;
  readonly buffer: Buffer;
  readonly document: DocumentId;
  readonly shared: Shared;
  language: string;
  mode: Mode;
  /** The screen column `j` and `k` aim for, valid while the caret is still
   * where this package last put it; Infinity after `$`. */
  want: number | null;
  caret: number | null;
  /** The undo group of the current insert session. */
  group: Disposable | null;
  /** Whether insert mode began on a line of its own, from `o`, `O`, or a
   * linewise change, whose indentation alone is removed again on Escape. */
  opened: boolean;
};

export async function setMode(state: State, mode: Mode) {
  state.mode = mode;
  await state.handle.setKeyContext(NORMAL, mode === "normal");
  await state.handle.setKeyContext(INSERT, mode === "insert");
  await state.handle.setCursorStyle(mode === "normal" ? "block" : null);
}

/** Puts a caret that sits past its line's last character back on it, as
 * normal mode never rests on a line's end. A selection is left alone. */
export async function clampCaret(state: State) {
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

/** Moves to `position`, or the line's last character when it is past it. */
async function moveOnto(state: State, lines: BufferLines, position: Position) {
  const text = await lines.text(position.line);
  await moveTo(state, lines, { line: position.line, col: Math.min(position.col, lastColumn(text)) });
}

export async function run(state: State, command: Command) {
  switch (command.kind) {
    case "motion":
      await motion(state, command.motion, command.count);
      break;
    case "operator":
      await operate(state, command.operator, command.register, command.count, command.target);
      break;
    case "put":
      await put(state, command.register, command.count, command.before);
      break;
    case "replace":
      await replaceCharacters(state, command.char, command.count);
      break;
    case "toggle-case":
      await toggleCase(state, command.count);
      break;
    case "join":
      await join(state, command.count);
      break;
    case "insert":
      await enterInsert(state, command.at);
      break;
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
type Moved = {
  readonly to: Position;
  readonly type: "exclusive" | "inclusive" | "linewise";
  /** The screen column `j` and `k` keep aiming for afterwards, when the
   * motion sets one other than where it lands. */
  readonly want?: number;
  readonly reveal?: "nearest" | "center";
};

/** Resolves a motion from `from`, or null when it fails, as `j` on the last
 * line or `fx` without an x does. With an operator, motions follow Vim's
 * operator-pending forms: `l` may reach the line's end and `w` stops there. */
async function resolve(
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
      const line = Math.max(0, Math.min(total - 1, from.line + (name === "up" ? -n : n)));
      if (operator !== null) return line === from.line ? null : { to: { line, col: 0 }, type: "linewise" };
      const tabSize = await tabSizeOf(state);
      const caret = await lines.offset(from);
      const want = state.caret === caret && state.want !== null
        ? state.want
        : screenColumn(text, from.col, tabSize);
      const target = await lines.text(line);
      const col = want === Infinity ? lastColumn(target) : columnAtScreen(target, want, tabSize);
      return { to: { line, col }, type: "linewise", want };
    }
    case "row-up":
    case "row-down": {
      const native = name === "row-up" ? "editor.move-up" : "editor.move-down";
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
      return { to: await lineStart(lines, count === null ? 0 : count - 1), type: "linewise", reveal: "center" };
    case "last-line":
      return {
        to: await lineStart(lines, count === null ? total - 1 : count - 1), type: "linewise", reveal: "center",
      };
    case "match-pair": {
      if (count !== null) {
        if (count > 100) return null;
        const line = Math.ceil((count * total) / 100) - 1;
        return { to: await lineStart(lines, line), type: "linewise", reveal: "center" };
      }
      const to = await matchingBracket(state, lines, from);
      return to === null ? null : { to, type: "inclusive", reveal: "center" };
    }
    case "paragraph-forward":
    case "paragraph-back": {
      const found = await paragraph(lines, from, n, name === "paragraph-forward");
      return found === null ? null : { to: found.position, type: found.inclusive ? "inclusive" : "exclusive" };
    }
    case "repeat-find":
    case "repeat-find-reverse": {
      const last = state.shared.lastFind;
      if (last === null) return null;
      const kind = name === "repeat-find" ? last.kind : REVERSED[last.kind];
      return find(text, from, kind, last.char, n, true);
    }
  }
}

const REVERSED: Readonly<Record<FindKind, FindKind>> = { f: "F", F: "f", t: "T", T: "t" };

function find(text: string, from: Position, kind: FindKind, char: string, count: number, repeat: boolean): Moved | null {
  const col = findInLine(text, from.col, kind, char, count, repeat);
  if (col === null) return null;
  return { to: { line: from.line, col }, type: kind === "f" || kind === "t" ? "inclusive" : "exclusive" };
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
  const lines = new BufferLines(state.buffer);
  const from = await lines.position((await state.handle.selection()).caret);
  const moved = await resolve(state, lines, from, name, count, null);
  if (moved === null) {
    // A failed `%` may have moved the native caret.
    await moveTo(state, lines, from);
    return;
  }
  const text = await lines.text(moved.to.line);
  const to = { line: moved.to.line, col: Math.min(moved.to.col, lastColumn(text)) };
  await moveTo(state, lines, to, moved.reveal);
  state.want = moved.want ?? screenColumn(text, to.col, await tabSizeOf(state));
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
async function regionText(lines: BufferLines, region: Region): Promise<Register> {
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
 * break with them, the one after, or before the last line. */
async function deletedRange(lines: BufferLines, region: Region): Promise<[number, number]> {
  if (!region.linewise) return [await lines.offset(region.start), await lines.offset(region.end)];
  const { start: { line: first }, end: { line: last } } = region;
  if (last + 1 < (await lines.count())) {
    return [await lines.offset({ line: first, col: 0 }), await lines.offset({ line: last + 1, col: 0 })];
  }
  const end = await lines.offset({ line: last, col: (await lines.text(last)).length });
  if (first === 0) return [0, end];
  return [await lines.offset({ line: first - 1, col: (await lines.text(first - 1)).length }), end];
}

async function replace(state: State, start: number, end: number, text: string) {
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

async function grouped(state: State, body: () => Promise<void>) {
  const group = await openGroup(state);
  try {
    await body();
  } finally {
    await group?.dispose();
  }
}

/** Records a yank or delete in the registers and the clipboard. */
async function record(
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
    case "line": {
      const last = Math.min((await lines.count()) - 1, from.line + (count ?? 1) - 1);
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
        || target.motion === "paragraph-back";
      break;
    }
  }
  if (region === null) {
    await moveTo(state, lines, from);
    return;
  }
  region = await adjust(lines, region, operator);
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
      await grouped(state, async () => {
        const anchor = await lines.offset({ line: first, col: 0 });
        const caret = await lines.offset({ line: last, col: (await lines.text(last)).length });
        await state.handle.setSelection({ anchor, caret });
        await commands.invoke(native);
      });
      const fresh = new BufferLines(state.buffer);
      await moveTo(state, fresh, await lineStart(fresh, first));
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
  await setMode(state, "insert");
  state.caret = caret;
  await state.handle.setSelection({ anchor: caret, caret });
}

const lower = (text: string) => text.toLowerCase();
const upper = (text: string) => text.toUpperCase();
function swapCase(text: string): string {
  let swapped = "";
  for (const character of text) {
    const up = character.toUpperCase();
    swapped += up === character ? character.toLowerCase() : up;
  }
  return swapped;
}

/** `p` and `P`: puts a register's text count times after or before the
 * caret, or below or above its line when the text is linewise. */
async function put(state: State, register: string | null, count: number, before: boolean) {
  const value = await readRegister(state, register);
  if (value === null || value.text === "") return;
  const lines = new BufferLines(state.buffer);
  const from = await lines.position((await state.handle.selection()).caret);
  state.want = null;
  if (value.linewise) {
    const block = (value.text.endsWith("\n") ? value.text : `${value.text}\n`).repeat(count);
    const total = await lines.count();
    const line = before ? from.line : from.line + 1;
    await grouped(state, async () => {
      if (line < total) {
        const at = await lines.offset({ line, col: 0 });
        await replace(state, at, at, block);
      } else {
        const at = await lines.offset({ line: from.line, col: (await lines.text(from.line)).length });
        await replace(state, at, at, `\n${block.slice(0, -1)}`);
      }
    });
    const fresh = new BufferLines(state.buffer);
    await moveTo(state, fresh, await lineStart(fresh, line));
    return;
  }
  const block = value.text.repeat(count);
  const text = await lines.text(from.line);
  const col = before || text.length === 0 ? from.col : nextColumn(text, from.col);
  const at = await lines.offset({ line: from.line, col });
  await grouped(state, () => replace(state, at, at, block));
  const fresh = new BufferLines(state.buffer);
  if (block.includes("\n")) {
    await moveTo(state, fresh, { line: from.line, col });
  } else {
    const end = await fresh.position(at + utf8Length(block));
    await moveOnto(state, fresh, { line: end.line, col: previousColumn(await fresh.text(end.line), end.col) });
  }
}

/** A register's text for a put, or null when it holds none. */
async function readRegister(state: State, register: string | null): Promise<Register | null> {
  if (register === null || register === "+" || register === "*") {
    const text = await workbench.readClipboardText();
    return text === null ? null : state.shared.registers.fromClipboard(text);
  }
  if (register === "_") return null;
  if (register === "%") {
    const uri = (await documents.get(state.document))?.resourceUri ?? null;
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

/** `J`: joins count lines, at least two, with the native Join Lines, which
 * leaves the caret where the last two lines met. */
async function join(state: State, count: number) {
  const lines = new BufferLines(state.buffer);
  const { caret } = await state.handle.selection();
  const line = (await lines.position(caret)).line;
  const joins = Math.min(Math.max(count, 2) - 1, (await lines.count()) - 1 - line);
  if (joins <= 0) return;
  await grouped(state, async () => {
    for (let n = 0; n < joins; n++) await commands.invoke("editor.join-lines");
  });
  state.want = null;
  await clampCaret(state);
}

export async function tabSizeOf(state: State): Promise<number> {
  const size = await settings.get("editor.tabSize", {
    document: state.document, language: state.language, view: "editor",
  });
  return typeof size === "number" && size >= 1 ? size : 4;
}

async function enterInsert(state: State, at: InsertAt) {
  state.group = await openGroup(state);
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
        await replace(state, start, start, `${indent}\n`);
        await moveTo(state, new BufferLines(state.buffer), { line: from.line, col: indent.length });
      }
      break;
  }
}

export async function leaveInsert(state: State) {
  const lines = new BufferLines(state.buffer);
  const { anchor, caret } = await state.handle.selection();
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
  await group?.dispose();
  await setMode(state, "normal");
}
