// Vim's search patterns and substitution replacements, translated to what
// the buffer's native regex search takes. Pure text work, without host calls.

/** The syntax `vim.regex` selects for patterns and replacements. */
export type Syntax = "vim" | "rust";

/** A pattern the engine can search for, and the case it forces, if any. */
export type Translated =
  | { readonly ok: true; readonly source: string; readonly forcedCase: "ignore" | "match" | null }
  | { readonly ok: false; readonly message: string };

type Magic = "magic" | "very" | "nomagic" | "verynomagic";

/** Classes after a backslash: the characters each matches, and whether the
 * class is negated. A negated class never matches a line break, as in Vim. */
const CLASSES: Readonly<Record<string, readonly [string, boolean]>> = {
  s: [" \\t", false], S: [" \\t", true],
  d: ["0-9", false], D: ["0-9", true],
  w: ["0-9A-Za-z_", false], W: ["0-9A-Za-z_", true],
  a: ["A-Za-z", false], A: ["A-Za-z", true],
  l: ["a-z", false], L: ["a-z", true],
  u: ["A-Z", false], U: ["A-Z", true],
  x: ["0-9A-Fa-f", false], X: ["0-9A-Fa-f", true],
  o: ["0-7", false], O: ["0-7", true],
  h: ["A-Za-z_", false], H: ["A-Za-z_", true],
};

/** Characters the Rust syntax treats specially, escaped when literal. */
const RUST_SPECIAL = /[\\.+*?()|[\]{}^$#&~-]/;

/** Characters that are operators bare in very magic and escaped in the
 * other modes. */
const VERY_MAGIC_BARE = new Set("()|+?={<>@%");

/** Translates `pattern` from the Vim syntax; the Rust syntax is the engine's
 * own and passes as typed. */
export function translate(pattern: string, syntax: Syntax): Translated {
  if (syntax === "rust") return { ok: true, source: pattern, forcedCase: null };
  return new Translator(pattern).run();
}

class Translator {
  private at = 0;
  private magic: Magic = "magic";
  private out = "";
  private forcedCase: "ignore" | "match" | null = null;
  /** Whether nothing that a quantifier applies to precedes, so `*` is a
   * literal, as at the start of a pattern or group. */
  private atStart = true;

  constructor(private readonly pattern: string) {}

  run(): Translated {
    const text = this.pattern;
    while (this.at < text.length) {
      const failure = this.step();
      if (failure !== null) return { ok: false, message: failure };
    }
    return { ok: true, source: this.out, forcedCase: this.forcedCase };
  }

  /** Translates the next token; returns an error message, if it has one. */
  private step(): string | null {
    const text = this.pattern;
    const char = text[this.at]!;
    if (char === "\\") {
      const next = text[this.at + 1];
      if (next === undefined) {
        this.at++;
        this.emit("\\\\");
        return null;
      }
      this.at += 2;
      return this.escaped(next);
    }
    this.at += char.length;
    return this.bare(char);
  }

  private emit(source: string, quantifiable = true) {
    this.out += source;
    this.atStart = !quantifiable;
  }

  /** A character typed without a backslash. */
  private bare(char: string): string | null {
    const very = this.magic === "very";
    if (very && VERY_MAGIC_BARE.has(char)) return this.operator(char);
    if ((very || this.magic === "magic") && (char === "." || char === "*" || char === "[")) {
      return this.operator(char);
    }
    if (char === "~" && (this.magic === "very" || this.magic === "magic")) {
      return "E383: ~ is not supported in patterns";
    }
    if (char === "^") return this.caret();
    if (char === "$") return this.dollar();
    this.emit(literal(char));
    return null;
  }

  /** What follows a backslash: an operator in the less magic modes, or a
   * class, escape, or a literal. */
  private escaped(char: string): string | null {
    const very = this.magic === "very";
    if (VERY_MAGIC_BARE.has(char)) return very ? this.literalChar(char) : this.operator(char);
    if (char === "." || char === "*" || char === "[") {
      const bareMagic = this.magic === "very" || this.magic === "magic";
      return bareMagic ? this.literalChar(char) : this.operator(char);
    }
    if (char === "^" || char === "$") return this.literalChar(char);
    if (char === "~") return this.literalChar(char);
    switch (char) {
      case "v": this.magic = "very"; return null;
      case "m": this.magic = "magic"; return null;
      case "M": this.magic = "nomagic"; return null;
      case "V": this.magic = "verynomagic"; return null;
      case "c": this.forcedCase = "ignore"; return null;
      case "C": this.forcedCase = "match"; return null;
      case "n": this.emit("\\n"); return null;
      case "t": this.emit("\\t"); return null;
      case "r": this.emit("\\r"); return null;
      case "e": this.emit("\\x1b"); return null;
      case "b": this.emit("\\x08"); return null;
      case "_": return this.underscore();
      case "z": return `E383: \\z is not supported in patterns`;
      case "&": return "E383: \\& is not supported in patterns";
      case "i": case "I": case "k": case "K": case "f": case "F": case "p": case "P":
        return `E383: \\${char} is not supported in patterns`;
    }
    if (/[1-9]/.test(char)) return "E383: Backreferences are not supported in patterns";
    const entry = CLASSES[char];
    if (entry !== undefined) {
      this.emit(classSource(entry, false));
      return null;
    }
    this.emit(literal(char));
    return null;
  }

  private literalChar(char: string): null {
    this.emit(literal(char));
    return null;
  }

  /** An operator character, as the current mode spells it. */
  private operator(char: string): string | null {
    switch (char) {
      case ".": this.emit("."); return null;
      case "*":
        if (this.atStart) this.emit("\\*");
        else this.emit("*", false);
        return null;
      case "[": return this.collection(false);
      case "(": this.emit("(", false); return null;
      case ")": this.emit(")"); return null;
      case "|": this.emit("|", false); return null;
      case "+":
      case "?":
      case "=": {
        if (this.atStart) return `E64: ${char} follows nothing`;
        this.emit(char === "+" ? "+" : "?", false);
        return null;
      }
      case "{": return this.interval();
      case "<": this.emit("\\b{start}"); return null;
      case ">": this.emit("\\b{end}"); return null;
      case "@": return "E383: Lookaround is not supported in patterns";
      case "%": return this.percent();
    }
    return null;
  }

  private caret(): string | null {
    const startOfBranch = this.atStart || this.magic === "very";
    this.emit(startOfBranch ? "^" : "\\^", false);
    // `^` leaves the pattern at its start, so a quantifier after it is literal.
    if (startOfBranch) this.atStart = true;
    return null;
  }

  private dollar(): string | null {
    const rest = this.pattern.slice(this.at);
    const closing = this.magic === "very" ? /^[|)]/ : /^\\[|)]/;
    this.emit(rest === "" || closing.test(rest) || this.magic === "very" ? "$" : "\\$");
    return null;
  }

  /** `\_x`: a class or `.` that also matches a line break, or an anchor that
   * works anywhere. */
  private underscore(): string | null {
    const next = this.pattern[this.at];
    if (next === undefined) return "E383: Invalid use of \\_";
    this.at++;
    if (next === "^") { this.emit("^", false); return null; }
    if (next === "$") { this.emit("$"); return null; }
    if (next === ".") { this.emit("(?s:.)"); return null; }
    if (next === "[") return this.collection(true);
    const entry = CLASSES[next];
    if (entry === undefined) return `E383: \\_${next} is not supported in patterns`;
    this.emit(classSource(entry, true));
    return null;
  }

  private percent(): string | null {
    const next = this.pattern[this.at];
    this.at++;
    if (next === "(") { this.emit("(?:", false); return null; }
    if (next === "^") { this.emit("\\A", false); return null; }
    if (next === "$") { this.emit("\\z"); return null; }
    return `E383: \\%${next ?? ""} is not supported in patterns`;
  }

  /** `\{n,m}`, `\{-n,m}`, and their closing `}` or `\}`. */
  private interval(): string | null {
    const match = /^(-?)(\d*)(,?)(\d*)\\?\}/.exec(this.pattern.slice(this.at));
    if (match === null) return "E554: Syntax error in \\{...}";
    if (this.atStart) return "E64: \\{ follows nothing";
    this.at += match[0].length;
    const [, lazy, low, comma, high] = match as unknown as [string, string, string, string, string];
    let bounds: string;
    if (comma === "") bounds = low === "" ? "*" : `{${low}}`;
    else if (low === "" && high === "") bounds = "*";
    else bounds = `{${low === "" ? "0" : low},${high}}`;
    this.emit(lazy === "-" ? `${bounds}?` : bounds, false);
    return null;
  }

  /** `[...]` after its opening bracket, or a literal bracket when unclosed. */
  private collection(withBreak: boolean): string | null {
    const text = this.pattern;
    let index = this.at;
    let negated = false;
    if (text[index] === "^") {
      negated = true;
      index++;
    }
    let body = "";
    let first = true;
    for (; index < text.length; index++) {
      const char = text[index]!;
      if (char === "]" && !first) {
        this.at = index + 1;
        const breaks = negated ? (withBreak ? "" : "\\n") : (withBreak ? "\\n" : "");
        this.emit(`[${negated ? "^" : ""}${body}${breaks}]`);
        return null;
      }
      first = false;
      if (char === "\\") {
        const next = text[index + 1];
        if (next === undefined) break;
        index++;
        body += collectionEscape(next);
      } else if (char === "[" && text[index + 1] === ":") {
        const close = text.indexOf(":]", index + 2);
        if (close === -1) {
          body += "\\[";
        } else {
          body += text.slice(index, close + 2);
          index = close + 1;
        }
      } else if (char === "]") {
        body += "\\]";
      } else if (char === "[" || char === "&" || char === "~") {
        body += `\\${char}`;
      } else {
        body += char;
      }
    }
    // Without a closing bracket the bracket is itself.
    this.emit("\\[");
    return null;
  }
}

function collectionEscape(char: string): string {
  switch (char) {
    case "n": return "\\n";
    case "t": return "\\t";
    case "r": return "\\r";
    case "e": return "\\x1b";
    case "b": return "\\x08";
    case "\\": case "]": case "^": case "-": return `\\${char}`;
    default: return RUST_SPECIAL.test(char) ? `\\${char}` : char;
  }
}

function classSource([body, negated]: readonly [string, boolean], withBreak: boolean): string {
  if (negated) return `[^${body}${withBreak ? "" : "\\n"}]`;
  return `[${body}${withBreak ? "\\n" : ""}]`;
}

/** `char` as a literal in the Rust syntax. */
function literal(char: string): string {
  return RUST_SPECIAL.test(char) ? `\\${char}` : char;
}

/** Whether a search for `pattern` is case-sensitive: `forced` by `\c` or
 * `\C` wins; then `ignoreCase` off; then `smartCase` makes an uppercase
 * letter outside an escape count. `smart` is false for searches that skip
 * smartcase, as `*` does. */
export function caseSensitive(
  pattern: string,
  forced: "ignore" | "match" | null,
  options: { readonly ignoreCase: boolean; readonly smartCase: boolean },
  smart: boolean,
): boolean {
  if (forced !== null) return forced === "match";
  if (!options.ignoreCase) return true;
  return smart && options.smartCase && hasUppercase(pattern);
}

/** Whether `pattern` has an uppercase letter that is not an escape's name,
 * as the `S` of `\S` is. */
function hasUppercase(pattern: string): boolean {
  for (let index = 0; index < pattern.length; index++) {
    const char = pattern[index]!;
    if (char === "\\") {
      index += pattern[index + 1] === "_" ? 2 : 1;
    } else if (char !== char.toLowerCase()) {
      return true;
    }
  }
  return false;
}

/** The replacement `text` of `:s` in the native expansion syntax. In the Vim
 * syntax `\0`–`\9` and `&` are the match and its groups, `\&` an ampersand,
 * `\r` and `\n` a line break, and `$` a dollar; the case escapes pass through.
 * The Rust syntax is native as typed. */
export function translateReplacement(text: string, syntax: Syntax): string {
  if (syntax === "rust") return text;
  let out = "";
  for (let index = 0; index < text.length; index++) {
    const char = text[index]!;
    if (char === "&") {
      out += "${0}";
    } else if (char === "$") {
      out += "$$";
    } else if (char !== "\\") {
      out += char;
    } else {
      const next = text[++index];
      if (next === undefined) out += "\\\\";
      else if (/[0-9]/.test(next)) out += `\${${next}}`;
      else if (next === "r" || next === "n") out += "\n";
      else if (next === "t") out += "\\t";
      else if (next === "\\") out += "\\\\";
      else if ("uUlLeE".includes(next)) out += `\\${next}`;
      else if (next === "$") out += "$$";
      else out += next;
    }
  }
  return out;
}

/** The pattern that finds `word`, as a whole word when `whole` and the word
 * begins or ends with a keyword character, in the syntax `syntax`. */
export function wordPattern(word: string, whole: boolean, syntax: Syntax): string {
  const keyword = (char: string | undefined) => char !== undefined && /[\p{L}\p{N}_]/u.test(char);
  const body = word.replace(syntax === "vim" ? /[\\.*$^~[\]]/g : /[\\.+*?()|[\]{}^$#&~-]/g, "\\$&");
  const start = whole && keyword(word[0]) ? (syntax === "vim" ? "\\<" : "\\b{start}") : "";
  const end = whole && keyword([...word].at(-1)) ? (syntax === "vim" ? "\\>" : "\\b{end}") : "";
  return `${start}${body}${end}`;
}
