import assert from "node:assert/strict";
import { test } from "node:test";
import { directoryOf, parseEx, resolvePath } from "../src/ex.js";

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
  assert.deepEqual(parseEx("e"), { kind: "error", message: "E32: No file name" });
  assert.deepEqual(parseEx("s/a/b/"), { kind: "error", message: "E492: Not an editor command: s/a/b/" });
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
