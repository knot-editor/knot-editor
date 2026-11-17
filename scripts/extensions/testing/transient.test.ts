import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";
import { workbench } from "knot";
import * as host from "knot/testing";

beforeEach(() => host.reset());

test("an input reports its initial and typed values and shows the returned messages", async () => {
  const window = host.openWindow();
  host.type("12", "x");
  host.answer("showInput", { kind: "accepted", value: "x", highlighted: null });
  const seen: string[] = [];
  const result = await workbench.showInput({
    title: "Line",
    value: "1",
    target: window,
    onChange: (value) => {
      seen.push(value);
      return value === "x" ? "not a number" : undefined;
    },
  });
  assert.deepEqual(result, { kind: "accepted", value: "x", highlighted: null });
  assert.deepEqual(seen, ["1", "12", "x"]);
  assert.deepEqual(host.inputMessages(), [null, null, "not a number"]);
});

test("an input records the completions an object answer carries", async () => {
  const window = host.openWindow();
  host.type("b");
  host.answer("showInput", { kind: "accepted", value: "b", highlighted: null });
  await workbench.showInput({
    title: "Word",
    target: window,
    onChange: (value) => ({
      message: value === "" ? null : "matches",
      items: ["alpha", "beta"]
        .filter((word) => word.startsWith(value))
        .map((word) => ({ label: word, value: word, description: "word" })),
    }),
  });
  assert.deepEqual(host.inputMessages(), [null, "matches"]);
  assert.deepEqual(host.inputFeedback(), [
    [
      { label: "alpha", value: "alpha", description: "word" },
      { label: "beta", value: "beta", description: "word" },
    ],
    [{ label: "beta", value: "beta", description: "word" }],
  ]);
});

test("an input reports the completion highlighted when it was accepted", async () => {
  const window = host.openWindow();
  host.answer("showInput", { kind: "accepted", value: "b", highlighted: 1 });
  const result = await workbench.showInput({
    title: "Word",
    target: window,
    onChange: () => ({
      items: [
        { label: "alpha", value: "alpha", description: "first", hint: "#0", descriptionMatches: [[0, 1]] },
        { label: "beta", value: "beta", description: "second", hint: "#1" },
      ],
    }),
  });
  assert.deepEqual(result, { kind: "accepted", value: "b", highlighted: 1 });
  assert.equal(host.inputFeedback()[0]![0]!.hint, "#0");
});

test("a picker with tabs asks again with the active tab when the tab or query changes", async () => {
  const window = host.openWindow();
  host.query("a");
  host.selectTab("files");
  host.answer("showPicker", { kind: "dismissed" });
  const asked: [string, string | null][] = [];
  await workbench.showPicker({
    title: "Everywhere",
    target: window,
    tabs: [{ id: "all", label: "All" }, { id: "files", label: "Files" }],
    onQuery: (query, tab) => {
      asked.push([query, tab]);
      return [];
    },
  });
  assert.deepEqual(asked, [["", "all"], ["a", "all"], ["a", "files"]]);
});

test("a picker with tabs starts on its initial tab", async () => {
  const window = host.openWindow();
  host.selectTab("all");
  host.answer("showPicker", { kind: "dismissed" });
  const asked: (string | null)[] = [];
  await workbench.showPicker({
    title: "Everywhere",
    target: window,
    tabs: [{ id: "all", label: "All" }, { id: "files", label: "Files" }],
    initialTab: "files",
    onQuery: (_, tab) => {
      asked.push(tab);
      return [];
    },
  });
  assert.deepEqual(asked, ["files", "all"]);
});
