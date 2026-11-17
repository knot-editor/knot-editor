import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import * as host from "knot/testing";
import { completeEx, directoryOf, fileCompletionTarget, parseEx, pathOfUri, resolvePath } from "../src/ex.js";

test("noh hides the search highlights", () => {
  for (const line of ["noh", "nohl", "nohlsearch", ":noh"]) assert.deepEqual(parseEx(line), { kind: "nohlsearch" });
  assert.equal(parseEx("no").kind, "error");
});

const flags = { global: false, ignoreCase: null, count: false, quiet: false };
const line = (n: number, offset = 0) => ({ base: { kind: "line", line: n }, offset }) as const;
const current = (offset = 0) => ({ base: { kind: "current" }, offset }) as const;
const last = (offset = 0) => ({ base: { kind: "last" }, offset }) as const;

test(":s takes a pattern, a replacement and flags", () => {
  assert.deepEqual(parseEx("s/a/b/"), { kind: "substitute", range: null, pattern: "a", replacement: "b", flags });
  for (const name of ["s", "su", "sub", "substitute"]) {
    assert.deepEqual(parseEx(`${name}/a/b/`), { kind: "substitute", range: null, pattern: "a", replacement: "b", flags });
  }
  assert.deepEqual(parseEx("s/a/b/gI"), {
    kind: "substitute", range: null, pattern: "a", replacement: "b", flags: { ...flags, global: true, ignoreCase: false },
  });
  assert.deepEqual(parseEx("s/a/b/ine"), {
    kind: "substitute", range: null, pattern: "a", replacement: "b",
    flags: { ...flags, ignoreCase: true, count: true, quiet: true },
  });
  assert.deepEqual(parseEx("s/a"), { kind: "substitute", range: null, pattern: "a", replacement: "", flags });
  assert.deepEqual(parseEx("s//x/"), { kind: "substitute", range: null, pattern: "", replacement: "x", flags });
});

test(":s takes any delimiter and a backslash before it is literal", () => {
  assert.deepEqual(parseEx("s#a/b#c/d#"), { kind: "substitute", range: null, pattern: "a/b", replacement: "c/d", flags });
  assert.deepEqual(parseEx("s/a\\/b/c\\/d/"), { kind: "substitute", range: null, pattern: "a/b", replacement: "c/d", flags });
  assert.deepEqual(parseEx("s/\\(a\\)/\\1\\n/"), { kind: "substitute", range: null, pattern: "\\(a\\)", replacement: "\\1\\n", flags });
  for (const bad of ["sxaxbx", "s1a1b1", "s\\a\\b\\", "s|a|b|", "s a b "]) assert.equal(parseEx(bad).kind, "error", bad);
  assert.deepEqual(parseEx("s/a/b/c"), { kind: "error", message: "E488: Trailing characters: c" });
  assert.deepEqual(parseEx("s/a/b/ 3"), { kind: "error", message: "E488: Trailing characters: 3" });
});

test("a range takes numbers, marks, offsets and %", () => {
  const range = (text: string) => (parseEx(`${text}s/a/b/`) as { range: unknown }).range;
  assert.deepEqual(range("%"), { first: line(1), last: last() });
  assert.deepEqual(range("2,5"), { first: line(2), last: line(5) });
  assert.deepEqual(range("3"), { first: line(3), last: line(3) });
  assert.deepEqual(range(".,$"), { first: current(), last: last() });
  assert.deepEqual(range(".+1,$-2"), { first: current(1), last: last(-2) });
  assert.deepEqual(range("+,-"), { first: current(1), last: current(-1) });
  assert.deepEqual(range("2+3"), { first: line(2, 3), last: line(2, 3) });
  assert.deepEqual(range("'<,'>"), {
    first: { base: { kind: "mark", name: "<" }, offset: 0 }, last: { base: { kind: "mark", name: ">" }, offset: 0 },
  });
  assert.deepEqual(range("'a,'b+1"), {
    first: { base: { kind: "mark", name: "a" }, offset: 0 }, last: { base: { kind: "mark", name: "b" }, offset: 1 },
  });
  assert.deepEqual(range(" 1 , 2 "), { first: line(1), last: line(2) });
});

test("commands take Vim's abbreviations and a bang", () => {
  for (const line of ["w", "wr", "write", ":w", "  w  "]) assert.deepEqual(parseEx(line), { kind: "write" });
  assert.deepEqual(parseEx("q"), { kind: "quit", force: false });
  assert.deepEqual(parseEx("quit!"), { kind: "quit", force: true });
  assert.deepEqual(parseEx("wq"), { kind: "write-quit", ifChanged: false });
  for (const line of ["x", "xit", "exi", "exit"]) {
    assert.deepEqual(parseEx(line), { kind: "write-quit", ifChanged: true });
  }
  assert.deepEqual(parseEx("e src/main.ts"), { kind: "edit", path: "src/main.ts" });
  assert.deepEqual(parseEx("edit! a b"), { kind: "edit", path: "a b" });
  assert.deepEqual(parseEx(""), { kind: "none" });
  assert.deepEqual(parseEx(":"), { kind: "none" });
});

test("lines go to a number or the last line", () => {
  assert.deepEqual(parseEx("12"), { kind: "line", line: 12 });
  assert.deepEqual(parseEx("0"), { kind: "line", line: 1 });
  assert.deepEqual(parseEx("$"), { kind: "line", line: null });
});

test("unknown commands and arguments are errors", () => {
  assert.deepEqual(parseEx("frob"), { kind: "error", message: "E492: Not an editor command: frob" });
  assert.deepEqual(parseEx("ex"), { kind: "error", message: "E492: Not an editor command: ex" });
  assert.deepEqual(parseEx("w other.txt"), { kind: "error", message: "E488: Trailing characters: other.txt" });
  assert.deepEqual(parseEx("e"), { kind: "edit", path: null });
  assert.deepEqual(parseEx("2,3w"), { kind: "error", message: "E481: No range allowed" });
  assert.deepEqual(parseEx("s"), { kind: "error", message: "E492: Not an editor command: s" });
});

test(":set turns options on, off, and over, and sets and shows values", () => {
  assert.deepEqual(parseEx("set nu nornu invwrap et! ts=2 sw:4 tabstop? number?"), {
    kind: "set",
    arguments: [
      { option: "number", action: "on" },
      { option: "relativenumber", action: "off" },
      { option: "wrap", action: "toggle" },
      { option: "expandtab", action: "toggle" },
      { option: "tabstop", action: "set", value: 2 },
      { option: "shiftwidth", action: "set", value: 4 },
      { option: "tabstop", action: "show" },
      { option: "number", action: "show" },
    ],
  });
  assert.deepEqual(parseEx("se ts"), { kind: "set", arguments: [{ option: "tabstop", action: "show" }] });
  assert.deepEqual(parseEx("set"), { kind: "set", arguments: [] });
});

test(":set takes the case options", () => {
  assert.deepEqual(parseEx("set ic noscs smartcase? invic"), {
    kind: "set",
    arguments: [
      { option: "ignorecase", action: "on" },
      { option: "smartcase", action: "off" },
      { option: "smartcase", action: "show" },
      { option: "ignorecase", action: "toggle" },
    ],
  });
});

test(":set rejects unknown options and invalid values", () => {
  assert.deepEqual(parseEx("set hlsearch"), { kind: "error", message: "E518: Unknown option: hlsearch" });
  assert.deepEqual(parseEx("set ts=0"), { kind: "error", message: "E474: Invalid argument: ts=0" });
  assert.deepEqual(parseEx("set ts=x"), { kind: "error", message: "E474: Invalid argument: ts=x" });
  assert.deepEqual(parseEx("set ts=33"), { kind: "error", message: "E474: Invalid argument: ts=33" });
  assert.deepEqual(parseEx("set nu=1"), { kind: "error", message: "E474: Invalid argument: nu=1" });
  assert.deepEqual(parseEx("set nonu!"), { kind: "error", message: "E474: Invalid argument: nonu!" });
});

test("paths resolve against a directory, as file URIs, or as given", () => {
  assert.equal(resolvePath("b.txt", "file:///home/me/src"), "file:///home/me/src/b.txt");
  assert.equal(resolvePath("../lib/./c.ts", "file:///home/me/src"), "file:///home/me/lib/c.ts");
  assert.equal(resolvePath("a b.txt", "file:///home"), "file:///home/a%20b.txt");
  assert.equal(resolvePath("x.txt", "file:///"), "file:///x.txt");
  assert.equal(resolvePath("notes.txt", "mem://product/dir"), "mem://product/dir/notes.txt");
  assert.equal(resolvePath("/tmp/x", null), "file:///tmp/x");
  assert.equal(resolvePath("mem://other/y", null), "mem://other/y");
  assert.equal(resolvePath("x", null), null);
  assert.equal(directoryOf("file:///home/me/a.txt"), "file:///home/me");
});

beforeEach(() => host.reset());

const values = async (line: string) => (await completeEx(line)).map((item) => item.value);

test("command names complete, in the order of the matcher", async () => {
  assert.deepEqual(await values("w"), ["write", "wq"]);
  assert.deepEqual(await values("se"), ["set", "nohlsearch", "substitute"]);
  assert.deepEqual(await values(":q"), [":quit", ":wq"]);
  assert.deepEqual(await values("zz"), []);
  assert.equal((await values("")).length, 9);
});

test("line numbers and other arguments get no items", async () => {
  assert.deepEqual(await values("12"), []);
  assert.deepEqual(await values("$"), []);
  assert.deepEqual(await values("w "), []);
});

test(":set completes option names with their short forms", async () => {
  const items = await completeEx("set nu");
  assert.deepEqual(items.map((item) => [item.value, item.description]), [
    ["set number", "nu"],
    ["set relativenumber", "rnu"],
  ]);
  assert.deepEqual(await values("set wr"), ["set wrap"]);
  assert.deepEqual(await values("set zz"), []);
});

test(":set keeps no and inv prefixes and earlier arguments", async () => {
  assert.deepEqual(await values("set noet"), ["set noexpandtab", "set norelativenumber"]);
  assert.deepEqual(await values("set invwr"), ["set invwrap"]);
  assert.deepEqual(await values("set ts=2 sw"), ["set ts=2 shiftwidth"]);
  assert.ok((await values("set no")).every((value) => value.startsWith("set no")));
});

test(":e lists files for the path typed after it, and nothing else does", () => {
  assert.deepEqual(fileCompletionTarget("e"), { head: "e", query: "" });
  assert.deepEqual(fileCompletionTarget(":e "), { head: ":e", query: "" });
  assert.deepEqual(fileCompletionTarget(" : edit!  src/ma"), { head: " : edit!", query: "src/ma" });
  for (const line of ["", "ex", "exit", "eds x", "set e", "w e", "2"]) {
    assert.equal(fileCompletionTarget(line), null, line);
  }
});

test("a file row's URI becomes the path the line takes", () => {
  assert.equal(pathOfUri("file:///work/my%20dir/a.ts"), "/work/my dir/a.ts");
  assert.equal(pathOfUri("memory://notes"), "memory://notes");
});
