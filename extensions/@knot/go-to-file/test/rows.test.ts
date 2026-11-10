import assert from "node:assert/strict";
import { test } from "node:test";
import { rootName, rowFor } from "../src/rows.js";

const hit = {
  uri: "file:///work/app/src/ui/button.ts",
  path: "src/ui/button.ts",
  name: "button.ts",
  ranges: [[3, 6], [7, 12]] as [number, number][],
};

test("the label is the name and the description the directory", () => {
  const row = rowFor({ ...hit, ranges: [] }, null);
  assert.equal(row.id, hit.uri);
  assert.equal(row.label, "button.ts");
  assert.equal(row.description, "src/ui");
});

test("path ranges split between the directory and the name", () => {
  const row = rowFor({ ...hit, ranges: [[0, 3], [4, 6], [7, 9], [11, 16]] }, null);
  assert.deepEqual(row.descriptionMatches, [[0, 3], [4, 6]]);
  // The range crossing the slash is clipped to each side.
  assert.deepEqual(row.labelMatches, [[0, 2], [4, 9]]);
});

test("a file at the root has no description", () => {
  const row = rowFor({ uri: "file:///work/app/Cargo.toml", path: "Cargo.toml", name: "Cargo.toml", ranges: [[0, 5]] }, null);
  assert.equal(row.description, null);
  assert.deepEqual(row.labelMatches, [[0, 5]]);
});

test("a root name starts the description and shifts its ranges", () => {
  const row = rowFor({ ...hit, ranges: [[0, 3]] }, "app");
  assert.equal(row.description, "app/src/ui");
  assert.deepEqual(row.descriptionMatches, [[4, 7]]);
  const top = rowFor({ uri: "file:///work/app/a.ts", path: "a.ts", name: "a.ts", ranges: [] }, "app");
  assert.equal(top.description, "app");
});

test("root names are the last segment", () => {
  assert.equal(rootName("file:///work/app"), "app");
  assert.equal(rootName("file:///work/app/"), "app");
});
