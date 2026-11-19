//! Dropping a row onto another row of an extension tree reaches the view's
//! `drop` command with both rows and the copy flag.
use super::*;
use knot_protocol::ExtensionKey;

const SOURCE: &str = r#"
import { commands, workbench } from "knot";
await workbench.registerView({
  id: "example.drag",
  title: "Drag",
  drop: "test.drop",
  content: { kind: "tree", provider: {
    getChildren(parentId) {
      return parentId === null ? [
        { id: "folder", label: "Folder", collapsibleState: "collapsed",
          dropTarget: true, arguments: { path: "folder" } },
        { id: "file", label: "File", collapsibleState: "none",
          draggable: true, arguments: { path: "file" } },
      ] : [];
    },
  } },
});
await commands.register("test.drop", async (drop) => {
  await workbench.writeClipboardText(JSON.stringify(drop.arguments));
});
"#;

fn load_source(
    host: &Entity<crate::app::extensions::host::ProductExtensionHost>,
    cx: &mut TestAppContext,
) {
    let key = ExtensionKey::new(ExtensionId::new(991), ExtensionLifecycleId::new(1));
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
}

#[gpui::test]
fn a_drop_reaches_the_view_command_with_both_rows_and_the_copy_flag(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    cx.update(super::super::bind_product_keys);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    load_source(&host, cx);
    let tree = wait_for_contributed_view(&window, "example.drag", cx);
    wait_for_api_condition(cx, |cx| tree.read(cx).root_labels().len() == 2);
    cx.update_window(native_window, |_, native_window, cx| {
        let handle = tree.read(cx).focus_handle(cx);
        handle.focus(native_window, cx);
        focus_view(&handle, native_window, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();

    let center = |index: usize, cx: &mut TestAppContext| {
        VisualTestContext::from_window(native_window, cx)
            .debug_bounds(Box::leak(
                format!("extension-tree-row-{index}").into_boxed_str(),
            ))
            .expect("the row is laid out")
            .center()
    };
    let from = center(1, cx);
    let to = center(0, cx);
    let option = Modifiers {
        alt: true,
        ..Default::default()
    };
    let mut visual = VisualTestContext::from_window(native_window, cx);
    visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_move(
        from + gpui::point(px(12.), px(0.)),
        MouseButton::Left,
        option,
    );
    visual.refresh().unwrap();
    visual.simulate_mouse_move(to, MouseButton::Left, option);
    visual.refresh().unwrap();
    visual.simulate_mouse_up(to, MouseButton::Left, option);
    drop(visual);

    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard().is_some_and(|item| {
            item.text().as_deref()
                == Some(
                    r#"{"copy":true,"source":{"arguments":{"path":"file"},"id":"file"},"target":{"arguments":{"path":"folder"},"id":"folder"}}"#,
                )
        })
    });
}
