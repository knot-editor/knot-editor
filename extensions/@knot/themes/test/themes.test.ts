import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { themes } from "knot";
import * as host from "knot/testing";
import { start } from "../src/extension.js";
import { THEMES } from "../src/themes.js";

beforeEach(() => host.reset());

test("registers every theme before start resolves", async () => {
  await start();
  assert.deepEqual(
    host.themeDefinitions().map((definition) => definition.name),
    THEMES.map((theme) => theme.name),
  );
  assert.deepEqual(
    (await themes.list()).map((theme) => `${theme.name}:${theme.appearance}`),
    [
      "Catppuccin Latte:light", "Catppuccin Mocha:dark", "Knot Islands Dark:dark",
      "Knot Islands Light:light", "One Dark:dark", "One Light:light",
      "Solarized Dark:dark", "Solarized Light:light", "Tokyo Night:dark", "Tokyo Night Light:light",
    ],
  );
});

test("every theme sets exactly the native color keys", () => {
  for (const theme of THEMES) {
    assert.deepEqual(Object.keys(theme.colors ?? {}).sort(), [...host.NATIVE_COLOR_KEYS], theme.name);
  }
});

test("every theme styles only known captures and valid colors", () => {
  const color = /^#(?:[0-9a-f]{6}|[0-9a-f]{8})$/;
  for (const theme of THEMES) {
    for (const [name, style] of Object.entries(theme.syntax ?? {})) {
      assert.ok(host.CAPTURE_NAMES.includes(name), `${theme.name}: ${name}`);
      if (style.color !== undefined) assert.match(style.color, color, `${theme.name}: ${name}`);
    }
    for (const [key, value] of Object.entries(theme.colors ?? {})) {
      assert.match(value ?? "", color, `${theme.name}: ${key}`);
    }
  }
});

test("the Islands themes are the native default pair and keep the window darker than the card in dark", () => {
  const dark = THEMES.find((theme) => theme.name === "Knot Islands Dark");
  const light = THEMES.find((theme) => theme.name === "Knot Islands Light");
  assert.equal(dark?.appearance, "dark");
  assert.equal(light?.appearance, "light");
  assert.equal(dark?.colors?.["window.background"], "#26282c");
  assert.equal(dark?.colors?.["card.background"], "#191a1c");
  assert.equal(dark?.colors?.["tab.accent"], "#3871e1");
  assert.equal(light?.colors?.["tab.accent"], "#3574f0");
});

test("every theme defines a complete chrome of known values", () => {
  const keys = ["accentWidth", "cardBorder", "cardGap", "cardInset", "cardRadius", "headerWeight", "stripeWidth", "tabBarHeight", "tabInset", "tabRadius"];
  for (const theme of THEMES) {
    assert.deepEqual(Object.keys(theme.chrome ?? {}).sort(), keys, theme.name);
  }
});

test("Islands themes float rounded cards and flat themes meet at borders", () => {
  const byName = new Map(THEMES.map((theme) => [theme.name, theme]));
  assert.equal(byName.get("Knot Islands Dark")?.chrome?.cardRadius, 8);
  assert.equal(byName.get("Knot Islands Dark")?.chrome?.cardBorder, 1);
  assert.equal(byName.get("One Dark")?.chrome?.cardGap, 0);
  assert.equal(byName.get("One Dark")?.chrome?.cardBorder, 1);
});
