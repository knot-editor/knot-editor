// A buffer's lines, fetched once per key: a key's motion reads a few lines,
// and an edit makes a fresh instance.
import type { Buffer } from "knot";
import type { Lines, Position } from "./motions.js";
import { columnAtByte, utf8Length } from "./text.js";

type Line = Readonly<{ text: string; start: number }>;

export class BufferLines implements Lines {
  private readonly cache = new Map<number, Line>();
  private total: number | null = null;

  constructor(readonly buffer: Buffer) {}

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
    const line = await this.buffer.lineAt(offset);
    const { text, start } = await this.load(line);
    return { line, col: columnAtByte(text, offset - start) };
  }

  private async load(line: number): Promise<Line> {
    let cached = this.cache.get(line);
    if (cached === undefined) {
      const range = await this.buffer.lineRange(line);
      cached = { text: (await this.buffer.snapshot(range)).text, start: range.startByteOffset };
      this.cache.set(line, cached);
    }
    return cached;
  }
}
