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
  const instance = await registration.open({ title: "Example", icon: "file", buffer });
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
  await registration.open({ title: "One", buffer });
  await registration[Symbol.asyncDispose]();
  assert.equal(host.composites()[0]!.closed, true);
  await assert.rejects(() => registration.open({ title: "Two", buffer }), { code: "OwnershipDisposed" });
  await workbench.registerComposite({ id: "example.view" });
});
