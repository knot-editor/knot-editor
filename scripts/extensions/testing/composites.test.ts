import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { buffers, workbench } from "knot";
import type * as Knot from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

test("a kind is unique and an instance shows its tab until closed", async () => {
  const registration = await workbench.registerComposite({ id: "example.view" });
  await assert.rejects(() => workbench.registerComposite({ id: "example.view" }), { code: "ViewIdInUse" });
  await assert.rejects(() => workbench.registerComposite({ id: "knot.view" }), { code: "InvalidViewId" });
  host.openWindow();
  const buffer = await buffers.create({ text: "hello" });
  const instance = await registration.open({ title: "Example", icon: "file", layout: { type: "row", children: [{ type: "editor", buffer }] } });
  const composite = host.composites()[0]!;
  assert.deepEqual(composite.tab, { title: "Example", icon: "file", resource: null, toolbar: [] });
  await instance.update({ title: "Renamed", resource: "file:///a.txt" });
  await instance.update({ icon: null });
  assert.deepEqual(composite.tab, { title: "Renamed", icon: null, resource: "file:///a.txt", toolbar: [] });
  assert.deepEqual(await instance.close(), { kind: "closed" });
  await assert.rejects(() => instance.update({ title: "Late" }), { code: "InvalidTarget" });
});

test("a toolbar of buttons is replaced by an update and kept otherwise", async () => {
  const registration = await workbench.registerComposite({ id: "example.view" });
  host.openWindow();
  const buffer = await buffers.create();
  const next = { label: "Next", icon: "arrow-down", command: "example.next" };
  const instance = await registration.open({
    title: "Example",
    layout: { type: "row", children: [{ type: "editor", buffer }] },
    toolbar: [next],
  });
  const composite = host.composites()[0]!;
  assert.deepEqual(composite.tab.toolbar, [next]);
  await instance.update({ title: "Renamed" });
  assert.deepEqual(composite.tab.toolbar, [next]);
  await instance.update({ toolbar: [{ ...next, toggled: true }] });
  assert.deepEqual(composite.tab.toolbar, [{ ...next, toggled: true }]);
  await instance.update({ toolbar: [] });
  assert.deepEqual(composite.tab.toolbar, []);
});

test("a scroll link is replaced or removed by an update and kept otherwise", async () => {
  const registration = await workbench.registerComposite({ id: "example.view" });
  host.openWindow();
  const [a, b] = await Promise.all([buffers.create(), buffers.create()]);
  const link = { first: "a", second: "b", map: [{ first: { start: 1, end: 1 }, second: { start: 1, end: 3 } }] };
  const instance = await registration.open({
    title: "Linked",
    layout: { type: "row", children: [{ type: "editor", key: "a", buffer: a }, { type: "editor", key: "b", buffer: b }] },
    scrollLink: link,
  });
  const composite = host.composites()[0]!;
  assert.deepEqual(composite.scrollLink, link);
  await instance.update({ title: "Renamed" });
  assert.deepEqual(composite.scrollLink, link);
  await instance.update({ scrollLink: { ...link, map: [] } });
  assert.deepEqual(composite.scrollLink, { ...link, map: [] });
  await instance.update({ scrollLink: null });
  assert.equal(composite.scrollLink, null);
});

test("a connector joins two editors of the layout by key and is replaced with the layout", async () => {
  const registration = await workbench.registerComposite({ id: "example.view" });
  host.openWindow();
  const [a, b] = await Promise.all([buffers.create(), buffers.create()]);
  const link = { first: { start: 1, end: 2 }, second: { start: 1, end: 4 }, color: "diff.modified.background" };
  const layout = (links: readonly (typeof link)[]): Knot.CompositeGroup => ({
    type: "row",
    children: [
      { type: "editor", key: "a", buffer: a },
      { type: "connector", first: "a", second: "b", links },
      { type: "editor", key: "b", buffer: b },
    ],
  });
  const instance = await registration.open({ title: "Diff", layout: layout([link]) });
  const composite = host.composites()[0]!;
  assert.deepEqual(composite.layout, layout([link]));
  await instance.update({ layout: layout([]) });
  assert.deepEqual(composite.layout, layout([]));
  const bad = (children: readonly Knot.CompositeLayout[]) =>
    registration.open({ title: "Bad", layout: { type: "row", children } });
  const editors: Knot.CompositeLayout[] = [{ type: "editor", key: "a", buffer: a }, { type: "editor", key: "b", buffer: b }];
  await assert.rejects(() => bad([...editors, { type: "connector", first: "a", second: "nope", links: [] }]), TypeError);
  await assert.rejects(() => bad([...editors, { type: "connector", first: "a", second: "a", links: [] }]), TypeError);
  await assert.rejects(() => instance.update({ layout: { type: "row", children: [editors[0]!, { type: "connector", first: "a", second: "b", links: [] }] } }), TypeError);
  assert.deepEqual(composite.layout, layout([]));
});

test("disposing a registration closes its instances and frees the kind", async () => {
  const registration = await workbench.registerComposite({ id: "example.view" });
  host.openWindow();
  const buffer = await buffers.create();
  await registration.open({ title: "One", layout: { type: "column", children: [{ type: "editor", buffer }] } });
  await registration[Symbol.asyncDispose]();
  assert.equal(host.composites()[0]!.closed, true);
  await assert.rejects(() => registration.open({ title: "Two", layout: { type: "column", children: [{ type: "editor", buffer }] } }), { code: "OwnershipDisposed" });
  await workbench.registerComposite({ id: "example.view" });
});

test("a layout nests rows and columns around editors and needs one editor", async () => {
  const registration = await workbench.registerComposite({ id: "example.view" });
  host.openWindow();
  const buffer = await buffers.create();
  const layout = {
    type: "column",
    children: [
      { type: "editor", buffer, size: { weight: 2 } },
      { type: "row", size: { pixels: 80 }, children: [{ type: "editor", buffer, focused: true }] },
    ],
  } as const;
  await registration.open({ title: "Nested", layout });
  assert.deepEqual(host.composites()[0]!.options.layout, layout);
  await assert.rejects(
    () => registration.open({ title: "Empty", layout: { type: "row", children: [] } }),
    TypeError,
  );
});

test("a layout holds embedded trees and text frames, and tabs of members", async () => {
  const registration = await workbench.registerComposite({ id: "example.view" });
  host.openWindow();
  const buffer = await buffers.create();
  const tree = await workbench.registerView({
    id: "example.files",
    title: "Files",
    embedded: true,
    content: { kind: "tree", provider: { getChildren: () => [] } },
  });
  const log = await workbench.registerView({ id: "example.log", title: "Log", embedded: true, content: { kind: "text" } });
  const layout = {
    type: "row",
    children: [
      { type: "tree", view: tree },
      {
        type: "tabs",
        children: [
          { type: "editor", title: "Code", buffer },
          { type: "text", title: "Log", view: log, focused: true },
        ],
      },
    ],
  } as const;
  await registration.open({ title: "Views", layout });
  assert.deepEqual(host.composites()[0]!.options.layout, layout);
  const tool = await workbench.registerView({ id: "example.tool", title: "Tool", content: { kind: "text" } });
  await assert.rejects(
    () => registration.open({ title: "Tool", layout: { type: "row", children: [{ type: "text", view: tool }] } }),
    TypeError,
  );
});
