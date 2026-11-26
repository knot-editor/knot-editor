//! `@knot/diff` in the launched product, called through its contract by a
//! probe package.
use gpui::TestAppContext;
use knot::test_support::{CommandOutcome, TestProduct, TestWindow};

const PROBE_MANIFEST: &str = r#"{ "name": "@knot/test-probe", "apiVersion": 1, "version": "0.1.0", "main": "dist/main.js", "requires": ["@knot/diff"] }"#;
const PROBE: &str = r#"
import { diffs } from "@knot/diff";
import { commands, documents } from "knot";
let open = null;
const sides = {
  original: { label: "HEAD", text: "a\nb\nc\nd\ne\nf\ng\nh\n" },
  modified: { label: "Now", text: "a\nB\nc\nd\nf\nG\nh\n" },
};
await commands.register("probe.snapshots", async () => {
  open = await (await diffs.client()).open({ ...sides, line: 1 });
});
const numbered = (from, to) => Array.from({ length: to - from }, (_, line) => `line ${from + line}\n`).join("");
await commands.register("probe.long", async () => {
  open = await (await diffs.client()).open({
    original: { label: "HEAD", text: numbered(0, 100) },
    modified: { label: "Now", text: numbered(0, 10) + numbered(200, 210) + numbered(10, 100) },
  });
});
await commands.register("probe.document", async () => {
  const [document] = await documents.list();
  open = await (await diffs.client()).open({
    original: { label: "HEAD", text: "one\ntwo\n" },
    modified: { label: "Working copy", document: document.id },
  });
});
await commands.register("probe.replace", async () => {
  await (await diffs.client()).setText({ id: open, side: "original", text: "a\nb\nc\nd\ne\nf\ng\nh\ni\n" });
});
await commands.register("probe.close", async () => {
  await (await diffs.client()).close(open);
});
"#;

fn packages() -> &'static [knot_bundled::BundledSource] {
    let mut packages = knot_ext_vcs::PACKAGES.to_vec();
    packages.push(knot_bundled::BundledSource {
        manifest: PROBE_MANIFEST,
        sources: &[("dist/main.js", PROBE)],
    });
    Box::leak(packages.into_boxed_slice())
}

/// Waits for `ready`, moving the clock on so the debounce of an edit elapses.
#[track_caller]
fn wait_until(cx: &mut TestAppContext, mut ready: impl FnMut(&mut TestAppContext) -> bool) {
    knot::test_support::wait_until(cx, |cx| {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        ready(cx)
    });
}

fn run(window: &TestWindow, command: &str, cx: &mut TestAppContext) {
    assert_eq!(window.run_command(command, cx), CommandOutcome::Completed);
}

type Marks = (Vec<(usize, String)>, Vec<(usize, String)>);

/// The line bands and the wedges of each editor.
fn bands(window: &TestWindow, cx: &mut TestAppContext) -> Vec<Marks> {
    window
        .composite_editors(cx)
        .into_iter()
        .map(|editor| (editor.bands, editor.wedges))
        .collect()
}

fn key(key: &str) -> String {
    key.to_owned()
}

#[gpui::test]
fn a_diff_of_snapshots_shows_bands_wedges_and_a_strip_in_a_composite_tab(cx: &mut TestAppContext) {
    let product = TestProduct::launch(packages(), cx);
    let window = product.active_window(cx);
    run(&window, "probe.snapshots", cx);
    wait_until(cx, |cx| !window.composite_links(cx).is_empty());

    assert_eq!(window.composite_titles(cx), ["HEAD ↔ Now"]);
    let editors = window.composite_editors(cx);
    assert_eq!(editors[0].text, "a\nb\nc\nd\ne\nf\ng\nh\n");
    assert_eq!(editors[1].text, "a\nB\nc\nd\nf\nG\nh\n");
    // `b` became `B`, `e` was removed, `f`/`g` became `G`.
    assert_eq!(
        window.composite_links(cx),
        [
            (1..2, 1..2, key("diff.modified.background")),
            (4..5, 4..4, key("diff.removed.background")),
            (6..7, 5..6, key("diff.modified.background")),
        ]
    );
    wait_until(cx, |cx| {
        bands(&window, cx)
            .iter()
            .all(|(bands, _)| !bands.is_empty())
    });
    assert_eq!(
        bands(&window, cx),
        [
            (
                vec![
                    (1, key("diff.modified.background")),
                    (4, key("diff.removed.background")),
                    (6, key("diff.modified.background")),
                ],
                vec![],
            ),
            (
                vec![
                    (1, key("diff.modified.background")),
                    (5, key("diff.modified.background")),
                ],
                vec![(4, key("diff.removed"))],
            ),
        ]
    );
    // The caret starts at the requested line of the modified side.
    assert_eq!(window.composite_editors(cx)[1].caret_line, 1);
    assert_eq!(
        window.composite_toolbar(cx),
        ["diff.previousHunk", "diff.nextHunk", "diff.toggleLayout"]
    );
    for editor in window.composite_editors(cx) {
        assert!(editor.key_contexts.contains(&key("diff")));
    }
}

/// The original's lines above each hunk of the snapshots probe's diff, inline.
fn inline_blocks() -> Vec<(usize, Vec<String>)> {
    [(1, "b"), (4, "e"), (5, "g")]
        .map(|(line, text)| (line, vec![text.to_owned()]))
        .to_vec()
}

#[gpui::test]
fn the_layout_command_switches_between_side_by_side_and_inline(cx: &mut TestAppContext) {
    let product = TestProduct::launch(packages(), cx);
    let window = product.active_window(cx);
    run(&window, "probe.snapshots", cx);
    wait_until(cx, |cx| !window.composite_links(cx).is_empty());
    assert_eq!(window.composite_editors(cx).len(), 2);

    run(&window, "diff.toggleLayout", cx);
    wait_until(cx, |cx| window.composite_editors(cx).len() == 1);
    wait_until(cx, |cx| !window.composite_editors(cx)[0].blocks.is_empty());
    let editors = window.composite_editors(cx);
    assert_eq!(editors[0].text, "a\nB\nc\nd\nf\nG\nh\n");
    assert_eq!(editors[0].blocks, inline_blocks());
    assert_eq!(window.composite_links(cx), []);
    wait_until(cx, |cx| !bands(&window, cx)[0].0.is_empty());
    assert_eq!(
        bands(&window, cx)[0],
        (
            vec![
                (1, key("diff.modified.background")),
                (5, key("diff.modified.background")),
            ],
            vec![(4, key("diff.removed"))],
        )
    );

    run(&window, "diff.toggleLayout", cx);
    wait_until(cx, |cx| window.composite_editors(cx).len() == 2);
    assert_eq!(window.composite_editors(cx)[1].blocks, []);
    wait_until(cx, |cx| window.composite_links(cx).len() == 3);
    wait_until(cx, |cx| {
        bands(&window, cx)
            .iter()
            .all(|(bands, _)| !bands.is_empty())
    });
}

#[gpui::test]
fn the_diff_layout_setting_opens_a_diff_inline_and_hunk_commands_follow_the_modified_text(
    cx: &mut TestAppContext,
) {
    let product = TestProduct::launch(packages(), cx);
    let window = product.active_window(cx);
    product.set_setting("diff.layout", "inline".into(), cx);
    run(&window, "probe.snapshots", cx);
    wait_until(cx, |cx| {
        window
            .composite_editors(cx)
            .first()
            .is_some_and(|editor| !editor.blocks.is_empty())
    });
    assert_eq!(window.composite_editors(cx).len(), 1);
    assert_eq!(window.composite_editors(cx)[0].blocks, inline_blocks());
    assert_eq!(
        window.composite_toolbar(cx),
        ["diff.previousHunk", "diff.nextHunk", "diff.toggleLayout"]
    );

    window.focus_composite_editor(0, cx);
    let mut visited = Vec::new();
    for _ in 0..3 {
        run(&window, "diff.nextHunk", cx);
        visited.push(window.composite_editors(cx)[0].caret_line);
    }
    assert_eq!(visited, [4, 5, 1]);
}

#[gpui::test]
fn a_snapshot_side_refuses_typing(cx: &mut TestAppContext) {
    let product = TestProduct::launch(packages(), cx);
    let window = product.active_window(cx);
    run(&window, "probe.snapshots", cx);
    wait_until(cx, |cx| !window.composite_links(cx).is_empty());
    for index in [0, 1] {
        window.focus_composite_editor(index, cx);
        window.type_input("x", cx);
    }
    let editors = window.composite_editors(cx);
    assert_eq!(editors[0].text, "a\nb\nc\nd\ne\nf\ng\nh\n");
    assert_eq!(editors[1].text, "a\nB\nc\nd\nf\nG\nh\n");
}

#[gpui::test]
fn a_document_side_is_editable_and_the_diff_follows_its_edits(cx: &mut TestAppContext) {
    let product = TestProduct::launch(packages(), cx);
    let window = product.active_window(cx);
    window.new_document(cx);
    window.append("one\ntwo\n", cx);
    run(&window, "probe.document", cx);
    wait_until(cx, |cx| !window.composite_editors(cx).is_empty());
    assert_eq!(window.composite_links(cx), []);

    window.focus_composite_editor(1, cx);
    window.type_input("X", cx);
    wait_until(cx, |cx| !window.composite_links(cx).is_empty());
    let editors = window.composite_editors(cx);
    assert!(editors[1].text.contains('X'), "{:?}", editors[1].text);
    assert_eq!(window.document_texts(cx).last().unwrap(), &editors[1].text);
    wait_until(cx, |cx| !bands(&window, cx)[1].0.is_empty());
}

#[gpui::test]
fn scrolling_one_side_scrolls_the_other_through_the_hunks(cx: &mut TestAppContext) {
    let product = TestProduct::launch(packages(), cx);
    let window = product.active_window(cx);
    run(&window, "probe.long", cx);
    wait_until(cx, |cx| !window.composite_links(cx).is_empty());
    let scrolls = |cx: &mut TestAppContext| {
        let editors = window.composite_editors(cx);
        [editors[0].scroll_line, editors[1].scroll_line]
    };
    // Ten lines were inserted at line 10 of the modified side.
    window.scroll_composite_editor(0, 50., cx);
    assert_eq!(scrolls(cx), [50., 60.]);
    window.scroll_composite_editor(1, 30., cx);
    assert_eq!(scrolls(cx), [20., 30.]);
    window.scroll_composite_editor(1, 4., cx);
    assert_eq!(scrolls(cx), [4., 4.]);
}

#[gpui::test]
fn the_hunk_commands_move_the_caret_between_hunks_and_wrap(cx: &mut TestAppContext) {
    let product = TestProduct::launch(packages(), cx);
    let window = product.active_window(cx);
    run(&window, "probe.snapshots", cx);
    wait_until(cx, |cx| !window.composite_links(cx).is_empty());
    window.focus_composite_editor(1, cx);

    // The modified side's hunks start at lines 1, 4 (a removal), and 5.
    let caret = |cx: &mut TestAppContext| window.composite_editors(cx)[1].caret_line;
    let mut visited = Vec::new();
    for _ in 0..4 {
        run(&window, "diff.nextHunk", cx);
        visited.push(caret(cx));
    }
    assert_eq!(visited, [4, 5, 1, 4]);
    run(&window, "diff.previousHunk", cx);
    assert_eq!(caret(cx), 1);
    run(&window, "diff.previousHunk", cx);
    assert_eq!(caret(cx), 5);
}

#[gpui::test]
fn alt_f5_moves_between_hunks_in_a_diff(cx: &mut TestAppContext) {
    let product = TestProduct::launch(packages(), cx);
    let window = product.active_window(cx);
    run(&window, "probe.snapshots", cx);
    wait_until(cx, |cx| !window.composite_links(cx).is_empty());
    window.focus_composite_editor(1, cx);
    for (key, line) in [("alt-f5", 4), ("alt-f5", 5), ("alt-shift-f5", 4)] {
        window.press(key, cx);
        wait_until(cx, |cx| window.composite_editors(cx)[1].caret_line == line);
    }
}

#[gpui::test]
fn the_hunk_commands_are_unavailable_outside_a_diff(cx: &mut TestAppContext) {
    let product = TestProduct::launch(packages(), cx);
    let window = product.active_window(cx);
    window.new_document(cx);
    assert_eq!(
        window.run_command("diff.nextHunk", cx),
        CommandOutcome::Unavailable
    );
}

#[gpui::test]
fn setting_a_snapshot_replaces_its_text_and_closing_ends_the_tab(cx: &mut TestAppContext) {
    let product = TestProduct::launch(packages(), cx);
    let window = product.active_window(cx);
    run(&window, "probe.snapshots", cx);
    wait_until(cx, |cx| !window.composite_links(cx).is_empty());
    // The command runs in the modified editor, which the replacement leaves alone.
    window.focus_composite_editor(1, cx);
    run(&window, "probe.replace", cx);
    wait_until(cx, |cx| {
        window.composite_editors(cx)[0].text.ends_with("i\n")
    });
    wait_until(cx, |cx| window.composite_links(cx).len() == 4);

    // The command ends the tab it ran in.
    assert_eq!(
        window.run_command("probe.close", cx),
        CommandOutcome::InvalidTarget
    );
    wait_until(cx, |cx| window.composite_titles(cx).is_empty());
}
