import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { buffers, workbench } from "knot";
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
  assert.deepEqual(composite.tab, { title: "Example", icon: "file", resource: null });
  await instance.update({ title: "Renamed", resource: "file:///a.txt" });
  await instance.update({ icon: null });
  assert.deepEqual(composite.tab, { title: "Renamed", icon: null, resource: "file:///a.txt" });
  assert.deepEqual(await instance.close(), { kind: "closed" });
  await assert.rejects(() => instance.update({ title: "Late" }), { code: "InvalidTarget" });
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
