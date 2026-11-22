//! Clicks on contribution marks and text run the contribution's command
//! against the clicked view, with its current range.
use super::*;
use crate::app::editor::EditorView;
use gpui::{Modifiers, Pixels, Point, VisualTestContext};
use knot_protocol::ExtensionKey;

const SOURCE: &str = r#"
import { commands, workbench } from "knot";
await commands.register("marks.click", async ({ editor, buffer, arguments: args }) => {
  await workbench.writeClipboardText(JSON.stringify({ args, editor: editor !== null, buffer: buffer !== null }));
});
await commands.register("marks.set", async ({ buffer }) => {
  const { revision } = await buffer.snapshot();
  const range = (startByteOffset, endByteOffset) => ({ startByteOffset, endByteOffset });
  await buffer.contributions.replace([
    { range: range(4, 7), gutter: { color: "error", style: "dot" }, command: "marks.click" },
    { range: range(8, 13), gutter: { color: "diff.added", style: "bar" }, command: { name: "marks.click", arguments: { id: "bar" } } },
    { range: range(14, 14), gutter: { color: "diff.removed", style: "deletion" }, command: { name: "marks.click", arguments: { id: "wedge" } } },
    { range: range(19, 23), decoration: { color: "info", style: "wavy" }, command: { name: "marks.click", arguments: { id: "text" } } },
    { range: range(0, 3), gutter: { color: "info", style: "dot" }, command: "marks.missing" },
  ], { ifRevision: revision });
});
"#;

const TEXT: &str = "one\ntwo\nthree\nfour\nfive\n";

fn load(cx: &mut TestAppContext) -> ExtensionKey {
    let host = cx.read(|cx| crate::app::extensions::host::entity(cx).unwrap());
    let key = ExtensionKey::new(ExtensionId::new(975), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| execution.await.unwrap())
            .detach();
    });
    wait_for_api_condition(cx, |cx| {
        cx.global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .catalog()
            .borrow()
            .resolve("marks.set")
            .is_ok()
    });
    key
}

struct Clicks {
    window: Entity<KnotWindow>,
    native_window: gpui::AnyWindowHandle,
    model: Entity<Buffer>,
    view: Entity<EditorView>,
}

impl Clicks {
    fn new(cx: &mut TestAppContext) -> Self {
        let documents = install_globals(cx);
        cx.update(crate::app::extensions::host::install);
        let document = cx.update(|cx| create_untitled_document(&documents, cx));
        let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
        model.update(cx, |model, _| model.replace(0..0, TEXT).unwrap());
        let (window, native_window) = product_window(document, model.clone(), cx);
        cx.refresh().unwrap();
        cx.run_until_parked();
        let view = cx.read(|cx| {
            let workbench = window.read(cx).workbench.read(cx);
            let pane = workbench.focused_pane().unwrap();
            workbench.editor(pane.active_tab()).unwrap().clone()
        });
        Self {
            window,
            native_window,
            model,
            view,
        }
    }

    fn set_marks(&self, cx: &mut TestAppContext) {
        load(cx);
        let execution = dispatch_product_command(&self.window, self.native_window, "marks.set", cx);
        assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
        cx.run_until_parked();
    }

    /// Click at `point(view)` and report what the command saw.
    fn click(
        &self,
        lane: Option<bool>,
        line: usize,
        cx: &mut TestAppContext,
    ) -> Option<serde_json::Value> {
        cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
        let at: Point<Pixels> = cx.read(|cx| self.view.read(cx).click_point_for_test(lane, line));
        let mut visual = VisualTestContext::from_window(self.native_window, cx);
        visual.simulate_click(at, Modifiers::default());
        visual.run_until_parked();
        let mut seen = None;
        crate::test_support::wait_until(cx, |cx| {
            seen = cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .filter(|text| !text.is_empty());
            seen.is_some()
        });
        seen.map(|text| serde_json::from_str(&text).unwrap())
    }

    fn selection(&self, cx: &mut TestAppContext) -> Option<std::ops::Range<usize>> {
        cx.read(|cx| self.view.read(cx).editor().read(cx).selected_byte_range(cx))
    }
}

#[gpui::test]
fn clicks_run_the_command_in_the_clicked_editor_with_the_current_range(cx: &mut TestAppContext) {
    let clicks = Clicks::new(cx);
    clicks.set_marks(cx);
    // Edits move the ranges before the click reads them.
    clicks.model.update(cx, |model, cx| {
        model.replace(0..0, "zero\n").unwrap();
        cx.notify();
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    let selection = clicks.selection(cx);

    let range = |start, end| serde_json::json!({ "startByteOffset": start, "endByteOffset": end });
    let expected = |args: serde_json::Value| serde_json::json!({ "args": args, "editor": true, "buffer": true });
    // A dot in the marker lane, a bar and a deletion wedge in the bar lane,
    // and decorated text.
    assert_eq!(
        clicks.click(Some(false), 2, cx),
        Some(expected(serde_json::json!({ "range": range(9, 12) })))
    );
    assert_eq!(
        clicks.click(Some(true), 3, cx),
        Some(expected(
            serde_json::json!({ "id": "bar", "range": range(13, 18) })
        ))
    );
    assert_eq!(
        clicks.click(Some(true), 4, cx),
        Some(expected(
            serde_json::json!({ "id": "wedge", "range": range(19, 19) })
        ))
    );
    assert_eq!(
        clicks.click(None, 5, cx),
        Some(expected(
            serde_json::json!({ "id": "text", "range": range(24, 28) })
        ))
    );
    // None of them moved the caret or selected anything.
    assert_eq!(clicks.selection(cx), selection);
}

#[gpui::test]
fn a_click_on_an_unregistered_command_reports_like_other_unknown_commands(cx: &mut TestAppContext) {
    let clicks = Clicks::new(cx);
    clicks.set_marks(cx);
    let selection = clicks.selection(cx);
    cx.read(|cx| {
        let dispatcher = cx.global::<ApplicationProductCommands>().0.read(cx);
        assert_eq!(dispatcher.last_outcome(), None);
    });
    let at = cx.read(|cx| clicks.view.read(cx).click_point_for_test(Some(false), 0));
    let mut visual = VisualTestContext::from_window(clicks.native_window, cx);
    visual.simulate_click(at, Modifiers::default());
    visual.run_until_parked();
    assert_eq!(clicks.selection(cx), selection);
    crate::test_support::wait_until(cx, |cx| {
        cx.read(|cx| {
            let dispatcher = cx.global::<ApplicationProductCommands>().0.read(cx);
            dispatcher.last_outcome() == Some(&CommandOutcome::Unavailable)
        })
    });
}

#[gpui::test]
fn a_text_frame_click_runs_the_command_without_an_editor(cx: &mut TestAppContext) {
    use crate::app::model::ContributionSource;
    use crate::app::workbench::ExtensionView;
    use knot_protocol::{ByteRange, EditorContribution};
    const FRAME: &str = r#"
import { commands, workbench } from "knot";
const frame = await workbench.registerView({
  id: "frame.marks", title: "Marks", content: { kind: "text" }, gutter: true,
  placement: { location: "bottomRight" },
});
await frame.setText("one\ntwo\n");
await commands.register("frame.click", async ({ editor, buffer, arguments: args }) => {
  await workbench.writeClipboardText(JSON.stringify({ args, editor: editor !== null, buffer: buffer !== null }));
});
"#;
    let clicks = Clicks::new(cx);
    let host = cx.read(|cx| crate::app::extensions::host::entity(cx).unwrap());
    let key = ExtensionKey::new(ExtensionId::new(976), ExtensionLifecycleId::new(1));
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, FRAME).unwrap();
        cx.spawn(async move |_, _| execution.await.unwrap())
            .detach();
    });
    wait_for_api_condition(cx, |cx| {
        cx.global::<ApplicationProductCommands>()
            .0
            .read(cx)
            .catalog()
            .borrow()
            .resolve("frame.click")
            .is_ok()
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    let view =
        cx.read(
            |cx| match &clicks.window.read(cx).contributed_views["frame.marks"].content {
                ExtensionView::Text(view) => view.clone(),
                ExtensionView::Tree(_) => panic!("frame.marks is a text frame"),
            },
        );
    let (model, editor) = cx.read(|cx| {
        (
            view.read(cx).model().clone(),
            view.read(cx).editor().clone(),
        )
    });
    let marks = [
        (0, 3, Some(("info", "dot")), Some("one")),
        (4, 7, None, Some("two")),
    ]
    .map(|(start, end, gutter, id)| EditorContribution {
        range: ByteRange {
            start_byte_offset: start,
            end_byte_offset: end,
        },
        decoration: None,
        gutter: gutter.map(|(color, style)| knot_protocol::GutterMark {
            color: color.into(),
            style: match style {
                "dot" => knot_protocol::GutterStyle::Dot,
                _ => unreachable!(),
            },
        }),
        command: Some(knot_protocol::ContributionCommand {
            name: "frame.click".into(),
            arguments: [(
                "id".to_owned(),
                knot_protocol::CommandArgumentValue::String(id.unwrap().into()),
            )]
            .into(),
        }),
    });
    model.update(cx, |model, cx| {
        let revision = model.revision();
        model
            .replace_contributions(ContributionSource::fixture(), &marks, revision)
            .unwrap();
        cx.notify();
    });
    cx.refresh().unwrap();
    cx.run_until_parked();

    for (lane, line, id, range) in [(Some(false), 0, "one", (0, 3)), (None, 1, "two", (4, 7))] {
        cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new())));
        let at = cx.read(|cx| editor.read(cx).click_point_for_test(lane, line));
        let mut visual = VisualTestContext::from_window(clicks.native_window, cx);
        visual.simulate_click(at, Modifiers::default());
        visual.run_until_parked();
        let mut seen = None;
        crate::test_support::wait_until(cx, |cx| {
            seen = cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .filter(|text| !text.is_empty());
            seen.is_some()
        });
        let seen: serde_json::Value = serde_json::from_str(&seen.unwrap()).unwrap();
        assert_eq!(
            seen,
            serde_json::json!({
                "args": { "id": id, "range": { "startByteOffset": range.0, "endByteOffset": range.1 } },
                "editor": false,
                "buffer": false,
            })
        );
    }
}
