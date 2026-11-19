import assert from "node:assert/strict";
import { test } from "node:test";
import { settings } from "knot";
import { testLanguagePackage } from "knot/testing/languages";
import { start } from "../src/extension.js";
import { linkOver } from "../src/paste.js";
import { FILE_ICONS, LANGUAGES } from "../src/languages.js";

testLanguagePackage(start, LANGUAGES, FILE_ICONS, ["markdown"]);

test("Markdown documents wrap their long lines", async () => {
  await start();
  assert.equal(await settings.get("editor.softWrap", { language: "markdown" }), true);
  assert.equal(await settings.get("editor.softWrap", { language: "rust" }), false);
});

const request = (text: string, selectedText: string) => ({ text, selectedText });

test("a URL pasted over selected text becomes its link", () => {
  assert.equal(linkOver(request("https://example.com/a?b=1", "word")), "[word](https://example.com/a?b=1)");
  assert.equal(linkOver(request("  mailto:a@b.c\n", "mail me")), "[mail me](mailto:a@b.c)");
});

test("anything else pastes as is", () => {
  assert.equal(linkOver(request("https://example.com", "")), null, "no selection");
  assert.equal(linkOver(request("https://example.com", "http://old.example")), null, "selected URL");
  assert.equal(linkOver(request("not a url", "word")), null);
  assert.equal(linkOver(request("https://a.example https://b.example", "word")), null, "two tokens");
  assert.equal(linkOver(request("example.com", "word")), null, "no scheme");
});
