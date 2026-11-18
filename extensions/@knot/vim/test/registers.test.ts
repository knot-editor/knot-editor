import assert from "node:assert/strict";
import { test } from "node:test";
import { Registers } from "../src/registers.js";

const lines = (text: string) => ({ text, linewise: true });
const chars = (text: string) => ({ text, linewise: false });

test("deletes of a line or more shift the numbered registers", () => {
  const registers = new Registers();
  for (let n = 1; n <= 10; n++) registers.record(null, lines(`${n}\n`), "delete", false);
  assert.deepEqual(registers.get("1"), lines("10\n"));
  assert.deepEqual(registers.get("2"), lines("9\n"));
  assert.deepEqual(registers.get("9"), lines("2\n"));
});

test("a characterwise delete across lines shifts them too, and so does a big one", () => {
  const registers = new Registers();
  registers.record(null, chars("a\nb"), "delete", false);
  registers.record(null, chars("x"), "delete", true);
  assert.deepEqual(registers.get("1"), chars("x"));
  assert.deepEqual(registers.get("2"), chars("a\nb"));
});

test("small deletes go to the small delete register only", () => {
  const registers = new Registers();
  registers.record(null, lines("a\n"), "delete", false);
  registers.record(null, chars("b"), "delete", false);
  assert.deepEqual(registers.get("-"), chars("b"));
  assert.deepEqual(registers.get("1"), lines("a\n"));
  assert.equal(registers.get("2"), null);
});

test("a named delete shifts the numbered registers but skips the small one", () => {
  const registers = new Registers();
  assert.equal(registers.record("a", lines("x\n"), "delete", false), null, "the clipboard stays");
  registers.record("a", chars("y"), "delete", false);
  assert.deepEqual(registers.get("1"), lines("x\n"));
  assert.equal(registers.get("-"), null);
  assert.deepEqual(registers.get("a"), chars("y"));
});

test("yanks go to the clipboard and register 0 unless named", () => {
  const registers = new Registers();
  assert.equal(registers.record(null, chars("a"), "yank", false), "a");
  assert.equal(registers.record("b", chars("b"), "yank", false), null);
  assert.equal(registers.record("+", chars("c"), "yank", false), "c");
  assert.deepEqual(registers.get("0"), chars("a"));
  assert.deepEqual(registers.get("b"), chars("b"));
  assert.equal(registers.get("1"), null);
});

test("the black hole register records nothing", () => {
  const registers = new Registers();
  assert.equal(registers.record("_", lines("a\n"), "delete", false), null);
  assert.equal(registers.get("1"), null);
});

test("uppercase names append, joining lines and characters", () => {
  const registers = new Registers();
  registers.record("a", chars("x"), "yank", false);
  registers.record("A", chars("y"), "yank", false);
  assert.deepEqual(registers.get("a"), chars("xy"));
  registers.record("A", lines("z\n"), "yank", false);
  assert.deepEqual(registers.get("A"), lines("xy\nz\n"));
  registers.record("A", chars("w"), "yank", false);
  assert.deepEqual(registers.get("a"), lines("xy\nz\nw\n"));
  registers.record("B", chars("new"), "yank", false);
  assert.deepEqual(registers.get("b"), chars("new"), "appending to an empty register sets it");
});

test("the clipboard pastes linewise only while it holds the package's linewise text", () => {
  const registers = new Registers();
  registers.record(null, lines("a\n"), "yank", false);
  assert.deepEqual(registers.fromClipboard("a\n"), lines("a\n"));
  assert.deepEqual(registers.fromClipboard("other\n"), chars("other\n"));
  registers.record(null, chars("b"), "delete", false);
  assert.deepEqual(registers.fromClipboard("a\n"), chars("a\n"));
});

test("registers round-trip through JSON and ignore what they do not know", () => {
  const registers = new Registers();
  registers.record(null, lines("a\n"), "yank", false);
  registers.record("q", chars("q"), "yank", false);
  registers.record(null, lines("d\n"), "delete", false);
  const restored = Registers.fromJson(JSON.parse(JSON.stringify(registers.toJson())));
  assert.deepEqual(restored.get("0"), lines("a\n"));
  assert.deepEqual(restored.get("q"), chars("q"));
  assert.deepEqual(restored.get("1"), lines("d\n"));
  assert.deepEqual(restored.fromClipboard("d\n"), lines("d\n"));
  assert.equal(Registers.fromJson({ version: 1 }).get("q"), null);
  assert.equal(Registers.fromJson({ version: 1, registers: { q: { text: "x", linewise: false } } }).get("q"), null);
  assert.equal(Registers.fromJson({ version: 2, registers: { "!": { text: "x", linewise: false } } }).get("!"), null);
});

test("history keeps entries newest first, moves repeats forward, and is capped", () => {
  const registers = new Registers();
  for (const entry of ["a", "b", "a", ""]) registers.remember("search", entry);
  assert.deepEqual(registers.entries("search"), ["a", "b"]);
  assert.deepEqual(registers.entries("command"), []);
  for (let n = 0; n < 150; n++) registers.remember("command", `c${n}`);
  assert.equal(registers.entries("command").length, 100);
  assert.equal(registers.entries("command")[0], "c149");
});

test("history and the search register survive a save, and clear empties them", () => {
  const registers = new Registers();
  registers.remember("search", "foo");
  registers.remember("command", "set ic");
  registers.set("/", chars("foo"));
  const restored = Registers.fromJson(registers.toJson());
  assert.deepEqual(restored.entries("search"), ["foo"]);
  assert.deepEqual(restored.entries("command"), ["set ic"]);
  assert.deepEqual(restored.get("/"), chars("foo"));
  restored.clear();
  assert.deepEqual(restored.entries("search"), []);
  assert.equal(restored.get("/"), null);
});

test("the last inserted text is stored for reading but no yank writes it", () => {
  const registers = new Registers();
  registers.set(".", chars("typed"));
  registers.record(".", chars("yanked"), "yank", false);
  assert.deepEqual(registers.get("."), chars("typed"));
  assert.deepEqual(Registers.fromJson(registers.toJson()).get("."), chars("typed"));
});

const type = (text: string) => ({ command: "vim.type", arguments: { text } });
const leave = { command: "vim.leave-insert" };

test("a macro replaces its register's text, and an uppercase name appends", () => {
  const registers = new Registers();
  registers.recordMacro("a", [type("i"), { command: "editor.type", arguments: { text: "hi" } }, leave]);
  assert.deepEqual(registers.macro("a"), [type("i"), { command: "editor.type", arguments: { text: "hi" } }, leave]);
  assert.deepEqual(registers.get("a"), chars("ihi<esc>"));
  registers.recordMacro("A", [type("x")]);
  assert.equal(registers.macro("a")?.length, 4);
  assert.deepEqual(registers.get("a"), chars("ihi<esc>x"));
  registers.recordMacro("a", [type("j")]);
  assert.deepEqual(registers.macro("a"), [type("j")]);
});

test("writing a register drops the macro it held", () => {
  const registers = new Registers();
  registers.recordMacro("a", [type("x")]);
  registers.record("a", chars("text"), "yank", false);
  assert.equal(registers.macro("a"), null);
  assert.deepEqual(registers.get("a"), chars("text"));
});

test("macros persist with the registers", () => {
  const registers = new Registers();
  registers.recordMacro("a", [type("x"), { command: "vim.answer", arguments: { text: "foo" } }, leave]);
  const restored = Registers.fromJson(JSON.parse(JSON.stringify(registers.toJson())));
  assert.deepEqual(restored.macro("a"), registers.macro("a"));
  assert.deepEqual(restored.get("a"), registers.get("a"));
  registers.clear();
  assert.equal(registers.macro("a"), null);
});
