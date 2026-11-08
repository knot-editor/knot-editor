//! Contributed text frames: the editor gutter they opt into.

use super::*;
use crate::app::workbench::ExtensionView;
use knot_host::lifecycle::ExtensionKey;

const SOURCE: &str = r#"
import { workbench } from "knot";
const bottom = { location: "bottom" };
const plain = await workbench.registerView({
  id: "frame.plain", title: "Plain", content: { kind: "text" }, placement: bottom,
});
const numbered = await workbench.registerView({
  id: "frame.numbered", title: "Numbered", content: { kind: "text" }, gutter: true,
  placement: bottom,
});
await plain.setText("one\ntwo\n");
await numbered.setText("one\ntwo\n");
for (const [contribution, message] of [
  [{ id: "frame.tree", title: "Tree", content: { kind: "tree", provider: { getChildren: async () => [] } }, gutter: true },
   "view gutter applies only to text content"],
  [{ id: "frame.bad", title: "Bad", content: { kind: "text" }, gutter: "yes" },
   "view gutter must be a boolean"],
]) {
  try {
    await workbench.registerView(contribution);
    throw new Error(`accepted ${contribution.id}`);
  } catch (error) {
    if (!(error instanceof TypeError) || error.message !== message) throw error;
  }
}
"#;

#[gpui::test]
fn text_frames_show_the_gutter_only_when_registered_with_one(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, _) = product_window(document, model, cx);
    let key = ExtensionKey::new(ExtensionId::new(970), ExtensionLifecycleId::new(1));
    let loaded = Arc::new(Mutex::new(None));
    let result = loaded.clone();
    host.update(cx, |host, cx| {
        let execution = host.load_test_source(key, SOURCE).unwrap();
        cx.spawn(async move |_, _| {
            *result.lock().unwrap() = Some(execution.await.map(|_| ()));
        })
        .detach();
    });
    wait_for_api_condition(cx, |_| loaded.lock().unwrap().is_some());
    loaded.lock().unwrap().take().unwrap().unwrap();
    cx.refresh().unwrap();
    cx.run_until_parked();

    let text_offset = |id: &str, cx: &mut TestAppContext| {
        cx.read(|cx| match &shell.read(cx).contributed_views[id].content {
            ExtensionView::Text(view) => {
                let editor = view.read(cx).editor().read(cx);
                let bounds = editor.interaction_bounds();
                assert!(bounds.size.width > px(0.), "{id} was painted");
                editor.text_bounds().origin.x - bounds.origin.x
            }
            ExtensionView::Tree(_) => panic!("{id} is a text frame"),
        })
    };
    // Pushed text is what the frame's editor shows.
    cx.read(
        |cx| match &shell.read(cx).contributed_views["frame.plain"].content {
            ExtensionView::Text(view) => assert_eq!(view.read(cx).text(cx), "one\ntwo\n"),
            ExtensionView::Tree(_) => panic!("frame.plain is a text frame"),
        },
    );
    assert_eq!(text_offset("frame.plain", cx), px(0.));
    assert!(text_offset("frame.numbered", cx) > px(16.));
}
