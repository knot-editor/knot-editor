// Visual and visual-line modes in one followed editor. The package keeps the
// selection's ends as the characters where it began and where its caret is,
// and shows them as an editor selection that includes both. Motions move the
// caret; operators apply to the selected text through normal mode's code.
import type { EditorSelection } from "knot";
import type { Command, TextObject } from "./keys.js";
import { BufferLines } from "./lines.js";
import { lineStart, type Position } from "./motions.js";
import {
  applyOperator, clampCaret, grouped, isVisual, join, moveOnto, moveTo, readRegister, record, recordJump, regionText,
  replace, resolve, setMode, type State, tabSizeOf,
} from "./normal.js";
import { type Region, textObject } from "./objects.js";
import { lastColumn, nextColumn, previousColumn, screenColumn, utf8Length } from "./text.js";

/** Runs a command in a visual mode, or `v`, `V`, or `gv` from normal mode. */
export async function runVisual(state: State, command: Command) {
  const lines = new BufferLines(state.buffer);
  switch (command.kind) {
    case "visual":
      return toggle(state, lines, command.linewise);
    case "reselect":
      return reselect(state, lines);
  }
  const visual = state.visual;
  if (!isVisual(state.mode) || visual === null) return;
  switch (command.kind) {
    case "leave-visual":
      await leave(state, lines);
      return;
    case "swap-ends":
      state.visual = { anchor: visual.caret, caret: visual.anchor };
      state.caret = visual.anchor;
      state.want = null;
      await show(state, lines);
      return;
    case "motion": {
      const from = await lines.position(visual.caret);
      const moved = await resolve(state, lines, from, command.motion, command.count, null);
      if (moved === null) {
        // A failed `%` may have moved the editor's caret.
        await show(state, lines);
        return;
      }
      if (moved.jump === true) await recordJump(state, visual.caret);
      const text = await lines.text(moved.to.line);
      // After `$` the caret sits on the line break, which the selection
      // then includes, as Vim's does.
      const col = moved.want === Infinity ? text.length : Math.min(moved.to.col, lastColumn(text));
      const to = { line: moved.to.line, col };
      visual.caret = await lines.offset(to);
      state.caret = visual.caret;
      state.want = moved.want ?? screenColumn(text, col, await tabSizeOf(state));
      await show(state, lines, moved.reveal);
      return;
    }
    case "select-object":
      return selectObject(state, lines, command.object, command.around, command.count);
    case "operator": {
      if (command.target.kind !== "selection") return;
      const linewise = command.target.linewise || state.mode === "visual-line";
      const { region, from } = await selectedRegion(state, lines, linewise);
      await exit(state);
      // A count on `>` and `<` is how many times to shift.
      await applyOperator(state, lines, command.operator, command.register, region, from, false, command.count ?? 1);
      return;
    }
    case "put":
      return put(state, lines, command.register, command.count, command.before);
    case "replace":
      return replaceSelection(state, lines, command.char);
    case "join": {
      const { region } = await selectedRegion(state, lines, true);
      await exit(state);
      await moveTo(state, lines, { line: region.start.line, col: 0 });
      await join(state, Math.max(2, region.end.line - region.start.line + 1));
      return;
    }
  }
}

/** Follows the editor's selection after it changed: a selection made
 * outside Vim, as by a mouse drag or Select All, enters visual mode, and
 * clearing it leaves visual mode. In normal mode a bare caret is kept off
 * the line's end. Insert mode is left alone. */
export async function followSelection(state: State) {
  if (state.mode === "insert") return;
  const selection = await state.handle.selection();
  const bare = selection.anchor === selection.caret;
  if (isVisual(state.mode)) {
    const own = state.selected;
    if (own !== null && own.anchor === selection.anchor && own.caret === selection.caret) return;
    if (bare) {
      await exit(state);
      await clampCaret(state);
      return;
    }
  } else if (bare) {
    await clampCaret(state);
    return;
  }
  // The editor's selection ends after its last character, where Vim's
  // includes the character under its caret.
  const lines = new BufferLines(state.buffer);
  const { anchor, caret } = selection;
  const ends = caret > anchor
    ? { anchor, caret: await characterBefore(lines, caret) }
    : { anchor: await characterBefore(lines, anchor), caret };
  await setMode(state, "visual");
  state.visual = ends;
  state.caret = ends.caret;
  state.want = null;
  await show(state, lines);
}

/** `v` and `V`: enters that mode from normal mode, switches to it from the
 * other visual mode, or leaves it when it is the current one. */
async function toggle(state: State, lines: BufferLines, linewise: boolean) {
  const mode = linewise ? "visual-line" : "visual";
  if (state.mode === mode) return leave(state, lines);
  if (state.visual === null) {
    const { caret } = await state.handle.selection();
    state.visual = { anchor: caret, caret };
  }
  await setMode(state, mode);
  await show(state, lines);
}

/** `gv`: selects the previous visual area in its mode. In a visual mode the
 * current area becomes the previous one, so `gv` again goes back. */
async function reselect(state: State, lines: BufferLines) {
  const previous = state.previous;
  if (previous === null) return;
  const anchor = await previous.anchor.position();
  const caret = await previous.caret.position();
  // Text that held an end was deleted.
  if (anchor === null || caret === null) return;
  if (isVisual(state.mode)) await remember(state);
  await setMode(state, previous.linewise ? "visual-line" : "visual");
  state.visual = { anchor, caret };
  state.caret = caret;
  state.want = null;
  await show(state, lines);
}

/** Leaves visual mode for normal mode with the caret where the selection's
 * caret was. */
async function leave(state: State, lines: BufferLines) {
  const caret = await lines.position(state.visual!.caret);
  await exit(state);
  await moveOnto(state, lines, caret);
}

/** Remembers the area for `gv` and returns to normal mode. */
async function exit(state: State) {
  await remember(state);
  await setMode(state, "normal");
}

async function remember(state: State) {
  const visual = state.visual;
  if (visual === null) return;
  const old = state.previous;
  state.previous = {
    anchor: await state.buffer.createMark(visual.anchor),
    caret: await state.buffer.createMark(visual.caret),
    linewise: state.mode === "visual-line",
  };
  old?.anchor.dispose();
  old?.caret.dispose();
}

/** Shows the visual area as the editor's selection: from the first
 * character through the last, or through whole lines with their line
 * breaks, with the editor's caret at the end that Vim's caret is on. */
async function show(state: State, lines: BufferLines, reveal: "nearest" | "center" = "nearest") {
  const { anchor, caret } = state.visual!;
  let selection: EditorSelection;
  if (state.mode === "visual-line") {
    const anchorLine = (await lines.position(anchor)).line;
    const caretLine = (await lines.position(caret)).line;
    const start = await lines.offset({ line: Math.min(anchorLine, caretLine), col: 0 });
    const end = await lineEnd(lines, Math.max(anchorLine, caretLine));
    selection = caretLine >= anchorLine ? { anchor: start, caret: end } : { anchor: end, caret: start };
  } else {
    const start = Math.min(anchor, caret);
    const end = await characterEnd(lines, Math.max(anchor, caret));
    selection = caret >= anchor ? { anchor: start, caret: end } : { anchor: end, caret: start };
  }
  state.selected = selection;
  await state.handle.setSelection(selection, { reveal });
}

/** The offset after the line break that ends `line`, or the text's end. */
async function lineEnd(lines: BufferLines, line: number): Promise<number> {
  if (line + 1 < (await lines.count())) return lines.offset({ line: line + 1, col: 0 });
  return lines.offset({ line, col: (await lines.text(line)).length });
}

/** The offset after the character at `offset`, a line break included. */
async function characterEnd(lines: BufferLines, offset: number): Promise<number> {
  const { line, col } = await lines.position(offset);
  const text = await lines.text(line);
  if (col < text.length) return lines.offset({ line, col: nextColumn(text, col) });
  return lineEnd(lines, line);
}

/** The offset of the character before `offset`, a line break included. */
async function characterBefore(lines: BufferLines, offset: number): Promise<number> {
  const { line, col } = await lines.position(offset);
  if (col > 0) return lines.offset({ line, col: previousColumn(await lines.text(line), col) });
  if (line === 0) return offset;
  return lines.offset({ line: line - 1, col: (await lines.text(line - 1)).length });
}

/** The selected text as an operator's region, and where its first end is,
 * which a yank leaves the caret at. */
async function selectedRegion(
  state: State,
  lines: BufferLines,
  linewise: boolean,
): Promise<{ region: Region; from: Position }> {
  const { anchor, caret } = state.visual!;
  const from = await lines.position(Math.min(anchor, caret));
  if (linewise) {
    const last = Math.max((await lines.position(anchor)).line, (await lines.position(caret)).line);
    return { region: { start: { line: from.line, col: 0 }, end: { line: last, col: 0 }, linewise: true }, from };
  }
  const end = await lines.position(await characterEnd(lines, Math.max(anchor, caret)));
  return { region: { start: from, end, linewise: false }, from };
}

/** A text object typed in visual mode selects it, or adds it to a
 * selection of more than one character. A linewise object, as `ip` is,
 * switches to visual-line mode. */
async function selectObject(state: State, lines: BufferLines, object: TextObject, around: boolean, count: number) {
  const visual = state.visual!;
  // Extending a selection starts from the character after it, so `vawaw`
  // takes the next word.
  const extending = visual.anchor !== visual.caret;
  const at = extending && visual.caret > visual.anchor ? await characterEnd(lines, visual.caret) : visual.caret;
  const region = await textObject(lines, await lines.position(at), object, around, count);
  if (region === null) return;
  let start: Position;
  let last: Position;
  if (region.linewise) {
    start = { line: region.start.line, col: 0 };
    last = { line: region.end.line, col: 0 };
    await setMode(state, "visual-line");
  } else {
    ({ start, last } = await inclusive(lines, region));
    if (!before(start, last)) return;
  }
  let anchor = await lines.offset(start);
  let caret = await lines.offset(last);
  if (extending) {
    anchor = Math.min(anchor, visual.anchor, visual.caret);
    caret = Math.max(caret, visual.anchor, visual.caret);
  }
  state.visual = { anchor, caret };
  state.caret = caret;
  state.want = null;
  await show(state, lines);
}

/** The first and last characters of a characterwise region. A region that
 * starts after its line's last character starts on the next line, and one
 * that ends at a line's start ends on the line break before it, so `i{`
 * over a block selects its lines. */
async function inclusive(lines: BufferLines, region: Region): Promise<{ start: Position; last: Position }> {
  let start = region.start;
  if (start.line < region.end.line && start.col >= (await lines.text(start.line)).length) {
    start = { line: start.line + 1, col: 0 };
  }
  const { end } = region;
  if (end.col === 0) {
    if (end.line === 0) return { start, last: { line: 0, col: -1 } };
    return { start, last: { line: end.line - 1, col: (await lines.text(end.line - 1)).length } };
  }
  return { start, last: { line: end.line, col: previousColumn(await lines.text(end.line), end.col) } };
}

function before(a: Position, b: Position): boolean {
  return a.line < b.line || (a.line === b.line && a.col <= b.col);
}

/** The byte range of a region's text, without the last line break of whole
 * lines. */
async function range(lines: BufferLines, region: Region): Promise<[number, number]> {
  if (!region.linewise) return [await lines.offset(region.start), await lines.offset(region.end)];
  const { start: { line: first }, end: { line: last } } = region;
  return [await lines.offset({ line: first, col: 0 }), await lines.offset({ line: last, col: (await lines.text(last)).length })];
}

/** `p` and `P`: replace the selection with a register's text, count times.
 * `p` then holds the replaced text in the unnamed register; `P` leaves the
 * registers alone. Lines put over characters go on lines of their own. */
async function put(state: State, lines: BufferLines, register: string | null, count: number, before: boolean) {
  const value = await readRegister(state, register);
  const { region } = await selectedRegion(state, lines, state.mode === "visual-line");
  await exit(state);
  if (value === null || value.text === "") return moveOnto(state, lines, region.start);
  const replaced = await regionText(lines, region);
  const [start, end] = await range(lines, region);
  const block = value.linewise ? (value.text.endsWith("\n") ? value.text : `${value.text}\n`).repeat(count) : "";
  let text: string;
  if (region.linewise) {
    text = value.linewise ? block.slice(0, -1) : value.text.repeat(count);
  } else {
    text = value.linewise ? `\n${block}` : value.text.repeat(count);
  }
  await grouped(state, () => replace(state, start, end, text));
  if (!before) await record(state, null, replaced, "delete");
  const fresh = new BufferLines(state.buffer);
  if (value.linewise) {
    const line = region.linewise ? region.start.line : region.start.line + 1;
    await moveTo(state, fresh, await lineStart(fresh, line));
  } else if (region.linewise || text.includes("\n")) {
    await moveTo(state, fresh, region.start);
  } else {
    const after = await fresh.position(start + utf8Length(text));
    await moveOnto(state, fresh, { line: after.line, col: previousColumn(await fresh.text(after.line), after.col) });
  }
}

/** `r` in visual mode: replaces every selected character but line breaks. */
async function replaceSelection(state: State, lines: BufferLines, char: string) {
  const { region } = await selectedRegion(state, lines, state.mode === "visual-line");
  await exit(state);
  const [start, end] = await range(lines, region);
  const { text } = await state.buffer.snapshot({ startByteOffset: start, endByteOffset: end });
  const replaced = text.replace(/[^\n]/gu, char);
  if (replaced !== text) await grouped(state, () => replace(state, start, end, replaced));
  await moveOnto(state, new BufferLines(state.buffer), region.start);
}
