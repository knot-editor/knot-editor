// Vim's registers, without host calls. The unnamed register, `"+`, and `"*`
// are the system clipboard, which the caller reads and writes; this model
// keeps the rest and remembers what it last gave the clipboard, so a paste
// of that same text is linewise when the yank was.
import type { JsonValue } from "knot";

/** Text in a register. Linewise text ends with a line break. */
export type Register = Readonly<{ text: string; linewise: boolean }>;

/** Registers a yank or delete can name. */
const WRITABLE = /^[a-zA-Z+*_]$/;

/** Registers the model stores and persists. */
const STORED = /^[a-z0-9:-]$/;

export class Registers {
  private readonly stored = new Map<string, Register>();
  /** What the package last put on the clipboard. */
  private clipboard: Register | null = null;

  /**
   * Records a yank or a delete into `name`, or the unnamed register for null,
   * following Vim: a yank into the unnamed register also goes to `"0`; a
   * delete of a line or more, or `big` for one by `%`, `{`, or `}`, shifts
   * `"1`–`"9` whatever register it names; a smaller one into the unnamed
   * register goes to `"-`; and an uppercase name appends to its letter.
   * Returns the text the clipboard should now hold, or null to leave it.
   */
  record(name: string | null, value: Register, kind: "yank" | "delete", big: boolean): string | null {
    if (name !== null && !WRITABLE.test(name)) return null;
    if (name === "_") return null;
    if (kind === "delete" && (big || value.linewise || value.text.includes("\n"))) {
      for (let index = 9; index > 1; index--) {
        const previous = this.stored.get(String(index - 1));
        if (previous === undefined) this.stored.delete(String(index));
        else this.stored.set(String(index), previous);
      }
      this.stored.set("1", value);
    } else if (kind === "delete" && name === null) {
      this.stored.set("-", value);
    }
    if (kind === "yank" && name === null) this.stored.set("0", value);
    if (name === null || name === "+" || name === "*") {
      this.clipboard = value;
      return value.text;
    }
    const lower = name.toLowerCase();
    const previous = this.stored.get(lower);
    this.stored.set(lower, name !== lower && previous !== undefined ? appended(previous, value) : value);
    return null;
  }

  /** Sets a register no yank or delete writes, such as `":`. */
  set(name: string, value: Register) {
    if (STORED.test(name)) this.stored.set(name, value);
  }

  /** A stored register's text, for any name but the clipboard's and `"%`. */
  get(name: string): Register | null {
    return this.stored.get(name.toLowerCase()) ?? null;
  }

  /** What the clipboard's `text` pastes as: linewise when it is still
   * exactly what the package last put there as linewise text. */
  fromClipboard(text: string): Register {
    return { text, linewise: this.clipboard?.text === text && this.clipboard.linewise };
  }

  /** Empties every register and forgets what the package gave the clipboard. */
  clear() {
    this.stored.clear();
    this.clipboard = null;
  }

  toJson(): JsonValue {
    const registers: Record<string, JsonValue> = {};
    for (const [name, { text, linewise }] of this.stored) registers[name] = { text, linewise };
    return {
      version: 1,
      registers,
      clipboard: this.clipboard === null ? null : { ...this.clipboard },
    };
  }

  /** Restores registers saved by `toJson`, ignoring anything else. */
  static fromJson(value: JsonValue | undefined): Registers {
    const registers = new Registers();
    if (!isObject(value) || value.version !== 1) return registers;
    if (isObject(value.registers)) {
      for (const [name, entry] of Object.entries(value.registers)) {
        const register = registerOf(entry);
        if (register !== null && STORED.test(name)) registers.stored.set(name, register);
      }
    }
    registers.clipboard = registerOf(value.clipboard);
    return registers;
  }
}

/** `previous` with `value` appended, as `"A` appends to `"a`: linewise when
 * either is, with a line break between characterwise and linewise text. */
function appended(previous: Register, value: Register): Register {
  if (previous.linewise && !value.linewise) return { text: `${previous.text}${value.text}\n`, linewise: true };
  if (!previous.linewise && value.linewise) return { text: `${previous.text}\n${value.text}`, linewise: true };
  return { text: previous.text + value.text, linewise: previous.linewise };
}

function isObject(value: JsonValue | undefined): value is { readonly [key: string]: JsonValue } {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function registerOf(value: JsonValue | undefined): Register | null {
  if (!isObject(value) || typeof value.text !== "string" || typeof value.linewise !== "boolean") return null;
  return { text: value.text, linewise: value.linewise };
}
