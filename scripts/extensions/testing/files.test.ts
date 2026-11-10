import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { files, settings } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

test("seeded files can be read, listed, and statted", async () => {
  host.seedFile("file:///p/a.txt", "﻿alpha");
  host.seedFile("file:///p/src/lib.rs", "fn main() {}");

  assert.equal(await files.readText("file:///p/a.txt"), "alpha");
  assert.deepEqual(
    [...(await files.list("file:///p"))].sort((a, b) => a.name.localeCompare(b.name)),
    [
      { uri: "file:///p/a.txt", name: "a.txt", kind: "file", ignored: false },
      { uri: "file:///p/src", name: "src", kind: "directory", ignored: false },
    ],
  );
  assert.deepEqual(await files.stat("file:///p/src"), { kind: "directory" });
  assert.equal(await files.stat("file:///p/missing"), null);
});

test("wrong kinds and missing resources reject with host error codes", async () => {
  host.seedFile("file:///p/a.txt", "alpha");
  await assert.rejects(files.readText("file:///p/missing"), { code: "ResourceNotFound" });
  await assert.rejects(files.readText("file:///p"), { code: "ResourceWrongKind" });
  await assert.rejects(files.list("file:///p/a.txt"), { code: "ResourceWrongKind" });
  await assert.rejects(files.stat("nonsense"), { code: "InvalidUri" });
});

test("listing omits hidden entries and marks ignored ones", async () => {
  host.seedFile("file:///p/.git/HEAD", "ref");
  host.seedFile("file:///p/a.txt", "alpha");
  host.seedFile("file:///p/dist/out.js", "");
  await settings.set("files.ignored", ["dist"]);

  assert.deepEqual(
    [...(await files.list("file:///p"))].map((entry) => [entry.name, entry.ignored]).sort(),
    [["a.txt", false], ["dist", true]],
  );
  assert.deepEqual(
    [...(await files.list("file:///p/dist"))].map((entry) => [entry.name, entry.ignored]),
    [["out.js", true]],
  );
  assert.equal(await files.isIgnored("file:///p/dist/out.js"), true);
  assert.equal(await files.isIgnored("file:///p/a.txt"), false);
  assert.equal(await files.isIgnored("file:///p/.git"), true);
});

test("walking resolves visible files at any depth and skips ignored directories", async () => {
  host.seedFile("file:///p/.git/HEAD", "ref");
  host.seedFile("file:///p/a.txt", "alpha");
  host.seedFile("file:///p/src/deep/lib.rs", "");
  host.seedFile("file:///p/dist/out.js", "");
  await settings.set("files.ignored", ["dist"]);

  assert.deepEqual(
    (await files.walk("file:///p")).map((entry) => entry.uri).sort(),
    ["file:///p/a.txt", "file:///p/src/deep/lib.rs"],
  );
  assert.deepEqual(await files.walk("file:///p/dist"), []);
  await assert.rejects(files.walk("file:///p/a.txt"), { code: "ResourceWrongKind" });
});
