import assert from "node:assert/strict";
import { test } from "node:test";
import { caseSensitive, translate, translateReplacement } from "../src/pattern.js";

const source = (pattern: string) => {
  const result = translate(pattern, "vim");
  assert.ok(result.ok, pattern);
  return result.source;
};
const message = (pattern: string) => {
  const result = translate(pattern, "vim");
  assert.ok(!result.ok, pattern);
  return result.message;
};

test("plain text and Rust metacharacters stay literal", () => {
  assert.equal(source("foo bar"), "foo bar");
  assert.equal(source("a(b)|c+d?e{2}"), "a\\(b\\)\\|c\\+d\\?e\\{2\\}");
  assert.equal(source("a.b*c"), "a.b*c");
  assert.equal(source("a/b#c"), "a/b\\#c");
});

test("magic operators are the escaped forms", () => {
  assert.equal(source("\\(a\\|b\\)\\+"), "(a|b)+");
  assert.equal(source("ab\\=c\\?"), "ab?c?");
  assert.equal(source("a\\{2,3}b\\{2}c\\{,4}d\\{2,}"), "a{2,3}b{2}c{0,4}d{2,}");
  assert.equal(source("a\\{-}b\\{-1,2}c\\{-2,}"), "a*?b{1,2}?c{2,}?");
  assert.equal(source("a\\{1\\}"), "a{1}");
  assert.equal(source("\\<word\\>"), "\\b{start}word\\b{end}");
  assert.equal(source("\\%(a\\)"), "(?:a)");
});

test("a star with nothing before it is a literal", () => {
  assert.equal(source("*a"), "\\*a");
  assert.equal(source("\\(*a\\)"), "(\\*a)");
  assert.equal(source("^*a"), "^\\*a");
});

test("anchors work at the ends of a branch and are literal elsewhere", () => {
  assert.equal(source("^a$"), "^a$");
  assert.equal(source("a^b$c"), "a\\^b\\$c");
  assert.equal(source("\\(^a$\\)"), "(^a$)");
  assert.equal(source("a\\|^b$\\|c"), "a|^b$|c");
  assert.equal(source("a\\_^b\\_$"), "a^b$");
  assert.equal(source("\\%^a\\%$"), "\\Aa\\z");
});

test("character classes never match a line break unless they say so", () => {
  assert.equal(source("\\s\\S\\d\\w"), "[ \\t][^ \\t\\n][0-9][0-9A-Za-z_]");
  assert.equal(source("\\_s\\_S"), "[ \\t\\n][^ \\t]");
  assert.equal(source("\\_."), "(?s:.)");
  assert.equal(source("\\a\\u\\l"), "[A-Za-z][A-Z][a-z]");
});

test("collections", () => {
  assert.equal(source("[abc]"), "[abc]");
  assert.equal(source("[^abc]"), "[^abc\\n]");
  assert.equal(source("\\_[abc]"), "[abc\\n]");
  assert.equal(source("\\_[^abc]"), "[^abc]");
  assert.equal(source("[a-z[:digit:]]"), "[a-z[:digit:]]");
  assert.equal(source("[]a]"), "[\\]a]");
  assert.equal(source("[a\\]\\n]"), "[a\\]\\n]");
  assert.equal(source("[a[b]"), "[a\\[b]");
  assert.equal(source("a[b"), "a\\[b");
});

test("escapes for characters", () => {
  assert.equal(source("a\\nb\\tc\\re"), "a\\nb\\tc\\re");
  assert.equal(source("\\.\\*\\[\\/\\\\"), "\\.\\*\\[/\\\\");
  assert.equal(source("\\e"), "\\x1b");
});

test("very magic and very nomagic change what is bare", () => {
  assert.equal(source("\\v(a|b)+<c>"), "(a|b)+\\b{start}c\\b{end}");
  assert.equal(source("\\va{2}b=c"), "a{2}b?c");
  assert.equal(source("\\v\\(a\\)"), "\\(a\\)");
  assert.equal(source("\\V.*[a]"), "\\.\\*\\[a\\]");
  assert.equal(source("\\V\\(a\\|b\\)"), "(a|b)");
  assert.equal(source("\\M.*\\."), "\\.\\*.");
  assert.equal(source("\\Va\\.b\\*"), "a.b*");
});

test("case flags are removed and reported", () => {
  const ignore = translate("a\\cb", "vim");
  assert.ok(ignore.ok);
  assert.deepEqual([ignore.source, ignore.forcedCase], ["ab", "ignore"]);
  const match = translate("\\Ca", "vim");
  assert.ok(match.ok);
  assert.equal(match.forcedCase, "match");
});

test("the Rust syntax passes as typed", () => {
  const result = translate("(a|b)\\d+\\c", "rust");
  assert.deepEqual(result, { ok: true, source: "(a|b)\\d+\\c", forcedCase: null });
});

test("atoms the engine cannot express fail with a Vim-style message", () => {
  assert.match(message("a\\zsb"), /^E383: .*\\z/);
  assert.match(message("\\(a\\)\\1"), /^E383: Backreferences/);
  assert.match(message("a\\(b\\)\\@=c"), /^E383: Lookaround/);
  assert.match(message("a\\&b"), /^E383/);
  assert.match(message("a\\%Vb"), /^E383: \\%V/);
  assert.match(message("a~"), /^E383: ~/);
  assert.match(message("\\+a"), /^E64/);
  assert.match(message("a\\{x}"), /^E554/);
});

test("smartcase applies to an uppercase letter outside escapes", () => {
  const both = { ignoreCase: true, smartCase: true };
  assert.equal(caseSensitive("foo", null, both, true), false);
  assert.equal(caseSensitive("Foo", null, both, true), true);
  assert.equal(caseSensitive("\\Sfoo", null, both, true), false);
  assert.equal(caseSensitive("\\_Sfoo", null, both, true), false);
  assert.equal(caseSensitive("Foo", null, both, false), false);
  assert.equal(caseSensitive("Foo", null, { ignoreCase: true, smartCase: false }, true), false);
  assert.equal(caseSensitive("foo", null, { ignoreCase: false, smartCase: true }, true), true);
});

test("\\c and \\C override ignorecase and smartcase", () => {
  const both = { ignoreCase: true, smartCase: true };
  assert.equal(caseSensitive("Foo", "ignore", both, true), false);
  assert.equal(caseSensitive("foo", "match", both, true), true);
});

test("replacements translate groups, line breaks and dollars", () => {
  assert.equal(translateReplacement("<\\1-&-\\0>", "vim"), "<${1}-${0}-${0}>");
  assert.equal(translateReplacement("\\&", "vim"), "&");
  assert.equal(translateReplacement("a\\rb\\nc\\td", "vim"), "a\nb\nc\\td");
  assert.equal(translateReplacement("$1 $", "vim"), "$$1 $$");
  assert.equal(translateReplacement("\\\\ \\/", "vim"), "\\\\ /");
  assert.equal(translateReplacement("\\u\\1\\L&\\E", "vim"), "\\u${1}\\L${0}\\E");
  assert.equal(translateReplacement("a&b", "rust"), "a&b");
  assert.equal(translateReplacement("$1\\u", "rust"), "$1\\u");
});
