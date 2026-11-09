import assert from "node:assert/strict";
import { test } from "node:test";
import { settings } from "knot";
import { testLanguagePackage } from "knot/testing/languages";
import { start } from "../src/extension.js";
import { FILE_ICONS, LANGUAGES } from "../src/languages.js";

testLanguagePackage(start, LANGUAGES, FILE_ICONS, ["markdown"]);

test("Markdown documents wrap their long lines", async () => {
  await start();
  assert.equal(await settings.get("editor.softWrap", { language: "markdown" }), true);
  assert.equal(await settings.get("editor.softWrap", { language: "rust" }), false);
});
