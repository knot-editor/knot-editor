import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { documents, files, settings } from "knot";
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

test("searching ranks name matches first, limits results, and skips ignored directories", async () => {
  host.seedFile("file:///p/.git/HEAD", "ref");
  host.seedFile("file:///p/docs/maintenance/notes.md", "");
  host.seedFile("file:///p/src/main.rs", "");
  host.seedFile("file:///p/src/lib.rs", "");
  host.seedFile("file:///p/dist/main.js", "");
  await settings.set("files.ignored", ["dist"]);

  const found = await files.search("file:///p", "main");
  assert.deepEqual(found.results.map((hit) => hit.path), ["src/main.rs", "docs/maintenance/notes.md"]);
  assert.equal(found.more, false);
  assert.equal(found.results[0]!.uri, "file:///p/src/main.rs");
  assert.equal(found.results[0]!.name, "main.rs");
  assert.deepEqual(found.results[0]!.ranges, [[4, 8]]);

  const limited = await files.search("file:///p", "", { limit: 2 });
  assert.deepEqual(limited.results.map((hit) => hit.path), ["docs/maintenance/notes.md", "src/lib.rs"]);
  assert.equal(limited.more, true);
  assert.deepEqual((await files.search("file:///p/dist", "main")).results, []);
  await assert.rejects(files.search("file:///p/src/main.rs", "x"), { code: "ResourceWrongKind" });
  await assert.rejects(files.search("file:///p", "x", { limit: 0 }), TypeError);
});

test("searching several directories ranks them as one and reports each hit's directory", async () => {
  host.seedFile("file:///a/main/readme.md", "");
  host.seedFile("file:///a/x/main.rs", "");
  host.seedFile("file:///b/main.rs", "");

  const found = await files.search(["file:///a", "file:///b"], "main");
  assert.deepEqual(
    found.results.map((hit) => `${hit.directory}:${hit.path}`),
    ["file:///a:x/main.rs", "file:///b:main.rs", "file:///a:main/readme.md"],
  );
  assert.deepEqual((await files.search([], "main")).results, []);
});

test("watching reports changes under the directory until disposed", async () => {
  host.seedFile("file:///p/src/a.txt", "a");
  const direct: string[][] = [];
  const deep: string[][] = [];
  const directWatch = await files.watch("file:///p/src", (uris) => void direct.push([...uris]));
  await files.watch("file:///p", (uris) => void deep.push([...uris]), { recursive: true });

  host.seedFile("file:///p/src/b.txt", "b");
  host.seedFile("file:///p/src/inner/c.txt", "c");
  host.seedFile("file:///p/.git/HEAD", "ref");
  await Promise.resolve();
  assert.deepEqual(direct, [["file:///p/src/b.txt"]]);
  assert.deepEqual(deep, [["file:///p/src/b.txt"], ["file:///p/src/inner/c.txt"]]);

  await directWatch.dispose();
  host.removeFile("file:///p/src/b.txt");
  await Promise.resolve();
  assert.equal(direct.length, 1);
  await assert.rejects(files.watch("file:///p/missing", () => {}), { code: "ResourceNotFound" });
});

test("creating makes missing parents and refuses to overwrite", async () => {
  assert.equal(await files.createFile("file:///p/a/b/c.txt"), "file:///p/a/b/c.txt");
  assert.equal(await files.readText("file:///p/a/b/c.txt"), "");
  assert.deepEqual(await files.stat("file:///p/a/b"), { kind: "directory" });
  assert.equal(await files.createDirectory("file:///p/empty/inner"), "file:///p/empty/inner");
  assert.deepEqual(await files.stat("file:///p/empty"), { kind: "directory" });

  await assert.rejects(files.createFile("file:///p/a/b/c.txt"), { code: "ResourceAlreadyExists" });
  await assert.rejects(files.createFile("file:///p/a"), { code: "ResourceAlreadyExists" });
  await assert.rejects(files.createDirectory("file:///p/empty"), { code: "ResourceAlreadyExists" });
  await assert.rejects(files.createFile("file:///p/a/b/c.txt/d"), { code: "ResourceWrongKind" });
  await assert.rejects(files.createFile("nonsense"), { code: "InvalidUri" });
});

test("renaming moves a tree, creates parents, and refuses to overwrite or nest", async () => {
  host.seedFile("file:///p/src/a.txt", "a");
  host.seedFile("file:///p/src/sub/b.txt", "b");
  host.seedFile("file:///p/taken.txt", "t");

  assert.equal(await files.rename("file:///p/src", "file:///p/out/dst"), "file:///p/out/dst");
  assert.equal(await files.readText("file:///p/out/dst/sub/b.txt"), "b");
  assert.equal(await files.stat("file:///p/src"), null);

  await assert.rejects(files.rename("file:///p/missing", "file:///p/x"), { code: "ResourceNotFound" });
  await assert.rejects(files.rename("file:///p/out", "file:///p/taken.txt"), { code: "ResourceAlreadyExists" });
  await assert.rejects(files.rename("file:///p/out", "file:///p/out/dst/in"), { code: "InvalidUri" });
});

test("open documents follow a rename and become deleted with their file", async () => {
  host.seedFile("file:///p/src/a.txt", "a");
  host.seedFile("file:///p/src/b.txt", "b");
  host.openWindow();
  const moved = await documents.open("file:///p/src/a.txt");
  const removed = await documents.open("file:///p/src/b.txt");

  await files.rename("file:///p/src", "file:///p/lib");
  const followed = (await documents.get(moved.document.id))!;
  assert.equal(followed.resourceUri, "file:///p/lib/a.txt");
  assert.equal(followed.title, "a.txt");

  await files.delete("file:///p/lib/b.txt");
  const deleted = (await documents.get(removed.document.id))!;
  assert.equal(deleted.kind, "destination");
  assert.equal(deleted.dirty, true);
  assert.equal(deleted.resourceUri, "file:///p/lib/b.txt");
});

test("deleting removes a tree and notifies watchers", async () => {
  host.seedFile("file:///p/dir/a.txt", "a");
  const seen: string[][] = [];
  await files.watch("file:///p", (uris) => void seen.push([...uris]), { recursive: true });

  await files.delete("file:///p/dir", { permanent: true });
  assert.equal(await files.stat("file:///p/dir"), null);
  assert.deepEqual(seen, [["file:///p/dir/a.txt"]]);
  await assert.rejects(files.delete("file:///p/dir"), { code: "ResourceNotFound" });
});
