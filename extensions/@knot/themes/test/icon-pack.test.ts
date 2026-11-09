import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { icons, settings } from "knot";
import * as host from "knot/testing";
import { start } from "../src/extension.js";
import { RECORD_KEY } from "../src/icon-pack.js";

beforeEach(async () => {
  host.reset();
  host.openWindow();
  await icons.registerPack({ name: "Mono", icons: { file: { lucide: "file-text" } } });
  await icons.registerPack({ name: "Color", icons: { file: { svg: "<svg></svg>", color: "original" } } });
});

test("selecting a pack sets the icons.pack setting and keeps the choice", async () => {
  await start();
  host.answer("showPicker", { kind: "selected", id: "Color" });
  await host.run("themes.selectIconPack");

  assert.equal(await settings.get("icons.pack"), "Color");
  assert.equal(host.record(RECORD_KEY), "Color");
  const [prompt] = host.prompts();
  const items = (prompt?.options as { items: { id: string; label: string }[] }).items;
  assert.deepEqual(items.map((item) => item.id), ["", "Color", "Mono"]);
});

test("the choice survives a restart of the package", async () => {
  await start();
  host.answer("showPicker", { kind: "selected", id: "Mono" });
  await host.run("themes.selectIconPack");
  const records = host.records();
  host.reset();
  host.openWindow();
  for (const [key, value] of Object.entries(records)) host.setRecord(key, value);
  await start();

  assert.equal(await settings.get("icons.pack"), "Mono");
});

test("choosing the default icons clears the pack", async () => {
  await start();
  await settings.set("icons.pack", "Mono");
  host.answer("showPicker", { kind: "selected", id: "" });
  await host.run("themes.selectIconPack");

  assert.equal(await settings.get("icons.pack"), "");
  assert.equal(host.record(RECORD_KEY), "");
});

test("highlighting previews a pack and dismissing restores the setting", async () => {
  await start();
  await settings.set("icons.pack", "Mono");
  const seen: unknown[] = [];
  const observer = await settings.observe("icons.pack", async () => {
    seen.push(await settings.get("icons.pack"));
  });
  host.highlight("Color", null);
  host.answer("showPicker", { kind: "dismissed" });
  await host.run("themes.selectIconPack");
  await observer.dispose();

  assert.ok(seen.includes("Color"));
  assert.equal(await settings.get("icons.pack"), "Mono");
  assert.equal(host.record(RECORD_KEY), undefined);
});

test("the fake host layers overrides over packs over package defaults", async () => {
  await icons.define("x.icon", { lucide: "box" });
  await assert.rejects(icons.define("x.icon", { lucide: "box" }), { code: "DuplicateIcon" });
  await assert.rejects(icons.define("file", { lucide: "box" }), { code: "DuplicateIcon" });
  await icons.registerPack({ name: "P", icons: { "x.icon": { lucide: "circle" } } });
  await settings.set("icons.pack", "P");
  assert.deepEqual(host.resolveIcon("x.icon"), { lucide: "circle" });
  const override = await icons.override("x.icon", { lucide: "star" });
  assert.deepEqual(host.resolveIcon("x.icon"), { lucide: "star" });
  await override.dispose();
  assert.deepEqual(host.resolveIcon("x.icon"), { lucide: "circle" });
  await assert.rejects(icons.define("y", { lucide: "box", svg: "<svg></svg>" } as never), TypeError);
});
