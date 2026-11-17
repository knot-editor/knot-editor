// A buffer's lines, fetched once per key: a key's motion reads a few lines,
// and an edit makes a fresh instance. The lines of a selection read seed it,
// so a motion within them reads nothing more.
import type { Buffer, EditorSelection, EditorSelectionWithLines } from "knot";
import type { Lines, Position } from "./motions.js";
import { columnAtByte, utf8Length } from "./text.js";

type Line = Readonly<{ text: string; start: number }>;

export class BufferLines implements Lines {
  private readonly cache = new Map<number, Line>();
  private total: number | null = null;

  /** `selection`'s lines, when it carries any, are already loaded. */
  constructor(readonly buffer: Buffer, selection?: EditorSelection | EditorSelectionWithLines) {
    if (selection !== undefined && "lines" in selection) {
      for (const { line, text, startByteOffset } of selection.lines) this.cache.set(line, { text, start: startByteOffset });
    }
  }

  async count(): Promise<number> {
    this.total ??= await this.buffer.lineCount();
    return this.total;
  }

  async text(line: number): Promise<string> {
    return (await this.load(line)).text;
  }

  /** The byte offset of a position. */
  async offset({ line, col }: Position): Promise<number> {
    const { text, start } = await this.load(line);
    return start + utf8Length(text.slice(0, col));
  }

  /** The position of a byte offset. */
  async position(offset: number): Promise<Position> {
    for (const [line, { text, start }] of this.cache) {
      if (offset >= start && offset <= start + utf8Length(text)) return { line, col: columnAtByte(text, offset - start) };
    }
    const line = await this.buffer.lineAt(offset);
    const { text, start } = await this.load(line);
    return { line, col: columnAtByte(text, offset - start) };
  }

  private async load(line: number): Promise<Line> {
    let cached = this.cache.get(line);
    if (cached === undefined) {
      const loaded = (await this.buffer.lines(line, line))[0]!;
      cached = { text: loaded.text, start: loaded.startByteOffset };
      this.cache.set(line, cached);
    }
    return cached;
  }
}
