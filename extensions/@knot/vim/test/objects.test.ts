import assert from "node:assert/strict";
import { test } from "node:test";
import type { TextObject } from "../src/keys.js";
import { linesOf } from "../src/motions.js";
import { textObject } from "../src/objects.js";

/** The text a text object covers in `text`, where "‸" marks the caret, or
 * the covered lines joined with "\n" plus "(lines)" when linewise. */
async function covered(text: string, object: TextObject, around: boolean, count = 1): Promise<string | null> {
  const before = text.split("\n");
  const line = before.findIndex((candidate) => candidate.includes("‸"));
  const col = before[line]!.indexOf("‸");
  const plain = text.replace("‸", "");
  const lines = linesOf(plain);
  const region = await textObject(lines, { line, col }, object, around, count);
  if (region === null) return null;
  const rows = plain.split("\n");
  if (region.linewise) return `${rows.slice(region.start.line, region.end.line + 1).join("\n")} (lines)`;
  const offset = ({ line: row, col: column }: { line: number; col: number }) =>
    rows.slice(0, row).reduce((sum, candidate) => sum + candidate.length + 1, 0) + column;
  return plain.slice(offset(region.start), offset(region.end));
}

test("iw and aw take words, white space, and the white space around them", async () => {
  assert.equal(await covered("foo b‸ar baz", "word", false), "bar");
  assert.equal(await covered("foo b‸ar baz", "word", true), "bar ");
  assert.equal(await covered("foo b‸ar", "word", true), " bar", "no white space after takes it before");
  assert.equal(await covered("  b‸ar", "word", true), "bar", "indentation stays");
  assert.equal(await covered("a‸  b c", "word", false), "  ");
  assert.equal(await covered("a‸  b c", "word", true), "  b");
  assert.equal(await covered("‸a b c", "word", false, 3), "a b");
  assert.equal(await covered("‸a b c", "word", true, 2), "a b ");
  assert.equal(await covered("‸foo.bar", "word", false), "foo");
  assert.equal(await covered("‸foo.bar x", "WORD", false), "foo.bar");
  assert.equal(await covered("‸", "word", false), null);
});

test("bracket objects find the enclosing pair by counting", async () => {
  assert.equal(await covered("f(a, ‸b)", "paren", false), "a, b");
  assert.equal(await covered("f(a, ‸b)", "paren", true), "(a, b)");
  assert.equal(await covered("f‸(a)", "paren", false), "a", "on the opener");
  assert.equal(await covered("f(a‸)", "paren", false), "a", "on the closer");
  assert.equal(await covered("(a (‸b) c)", "paren", false, 2), "a (b) c");
  assert.equal(await covered("(a (b) ‸c)", "paren", false), "a (b) c", "a nested pair before is skipped");
  assert.equal(await covered("‸a (b)", "paren", false), null);
  assert.equal(await covered("f(‸)", "paren", false), "");
  assert.equal(await covered("[<‸a>]", "angle", false), "a");
  assert.equal(await covered("[<‸a>]", "bracket", true), "[<a>]");
});

test("i{ over a block leaves out the opener's line end and the closer's indentation", async () => {
  // The region ends at the closer line's start; operators then take whole
  // lines by Vim's rule for characterwise text ending at a line's start.
  assert.equal(await covered("f {\n    a‸;\n    b;\n}", "brace", false), "    a;\n    b;\n");
  assert.equal(await covered("f {\n    a‸;\n  }", "brace", false), "    a;\n");
  assert.equal(await covered("f {\n  a‸, b }", "brace", false), "  a, b ");
});

test("quote objects pair quotes from the line's start when on one", async () => {
  assert.equal(await covered('x = "a‸b";', "dquote", false), "ab");
  assert.equal(await covered('x = ‸"ab";', "dquote", false), "ab");
  assert.equal(await covered('x = "ab‸";', "dquote", false), "ab");
  assert.equal(await covered('x "a‸b" y', "dquote", true), '"ab" ');
  assert.equal(await covered('x "a‸b"', "dquote", true), ' "ab"');
  assert.equal(await covered('‸x "ab"', "dquote", false), "ab", "the next quoted text");
  assert.equal(await covered('"a\\"‸b"', "dquote", false), 'a\\"b', "escaped quotes are skipped");
  assert.equal(await covered("'‸a'", "squote", false), "a");
  assert.equal(await covered("`‸a`", "backtick", false), "a");
  assert.equal(await covered('‸x "ab', "dquote", false), null);
});

test("ip and ap take runs of lines", async () => {
  const text = "a\n‸b\n\n\nc\nd";
  assert.equal(await covered(text, "paragraph", false), "a\nb (lines)");
  assert.equal(await covered(text, "paragraph", true), "a\nb\n\n (lines)");
  assert.equal(await covered(text, "paragraph", false, 2), "a\nb\n\n (lines)");
  assert.equal(await covered("a\n\n‸ \nb", "paragraph", false), "\n  (lines)", "white space counts as blank");
  assert.equal(await covered("a\n\n‸\nb", "paragraph", true), "\n\nb (lines)", "blank lines take the paragraph after");
  assert.equal(await covered("a\n\n‸b", "paragraph", true), "\nb (lines)", "the last paragraph takes the blank lines before");
});
