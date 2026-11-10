import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { files } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

test("seeded files can be read, listed, and statted", async () => {
  host.seedFile("file:///p/a.txt", "﻿alpha");
  host.seedFile("file:///p/src/lib.rs", "fn main() {}");

  assert.equal(await files.readText("file:///p/a.txt"), "alpha");
  assert.deepEqual(
    [...(await files.list("file:///p"))].sort((a, b) => a.name.localeCompare(b.name)),
    [
      { uri: "file:///p/a.txt", name: "a.txt", kind: "file" },
      { uri: "file:///p/src", name: "src", kind: "directory" },
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
