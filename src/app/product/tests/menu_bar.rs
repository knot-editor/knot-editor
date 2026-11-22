//! Items packages add to the application menu bar, which join the native
//! groups, follow them, or open the Go menu.
use super::*;
use crate::app::commands::dispatch::ContributedMenuCommand;
use gpui::MenuItem;
use knot_protocol::ExtensionKey;

const SOURCE: &str = r##"
import { commands, menus, workbench } from "knot";
await commands.register("bar.run", async ({ arguments: [n] }) => {
  await workbench.writeClipboardText(`ran ${n}`);
}, { title: "Run Bar" });
await menus.add("menubar.file", { command: "bar.run", title: "Bar Save", arguments: [7], group: "3_save", order: 5 });
await menus.add("menubar.file", { command: "bar.run", group: "5_extra" });
await menus.add("menubar.go", { command: "bar.run", title: "Go Bar" });
const dropped = await menus.add("menubar.view", { command: "bar.run", title: "Dropped", group: "3_view" });
await commands.register("bar.drop", async () => { await dropped[Symbol.asyncDispose](); });
"##;

/// A menu's item titles, `-` for a separator.
fn titles(name: &str, cx: &TestAppContext) -> Option<Vec<String>> {
    cx.read(|cx| {
        super::super::launch::product_menus(cx)
            .into_iter()
            .find(|menu| menu.name == name)
            .map(|menu| {
                menu.items
                    .iter()
                    .map(|item| match item {
                        MenuItem::Action { name, .. } => name.to_string(),
                        _ => "-".to_owned(),
                    })
                    .collect()
            })
    })
}

#[gpui::test]
fn contributed_items_join_native_groups_follow_them_and_run_with_their_arguments(
    cx: &mut TestAppContext,
) {
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (window, native_window) = product_window(document, model, cx);
    assert_eq!(titles("Go", cx), None, "Go shows only when filled");

    let key = ExtensionKey::new(ExtensionId::new(971), ExtensionLifecycleId::new(1));
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

    let file = titles("File", cx).unwrap();
    let save = file
        .iter()
        .position(|title| title == "Recover Unsaved Documents…")
        .unwrap();
    assert_eq!(file[save + 1], "Bar Save", "joins the native Save group");
    assert_eq!(file[save + 2], "-");
    assert_eq!(
        file[save + 3],
        "Close Tab",
        "the extra group sorts after Close"
    );
    assert_eq!(file.last().unwrap(), "Run Bar");
    assert_eq!(titles("Go", cx).unwrap(), ["Go Bar"]);
    let view = titles("View", cx).unwrap();
    assert_eq!(view.last().unwrap(), "Dropped");

    // The item runs its command with the arguments it was added with.
    cx.update_window(native_window, |_, _, cx| {
        window.update(cx, |window, cx| window.focus_active_editor(cx))
    })
    .unwrap();
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("untouched".into())));
    cx.dispatch_action(
        native_window,
        ContributedMenuCommand {
            name: "bar.run".into(),
            arguments: knot_protocol::CommandArgumentValue::Array(vec![
                knot_protocol::CommandArgumentValue::Number(7.0),
            ]),
        },
    );
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .is_some_and(|item| item.text().as_deref() == Some("ran 7"))
    });

    // Disposing an item or its lifecycle takes it out of the bar.
    let execution = dispatch_product_command(&window, native_window, "bar.drop", cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    assert!(!titles("View", cx).unwrap().contains(&"Dropped".to_owned()));
    cx.update(|cx| crate::app::menus::remove_lifecycle(key.extension, key.lifecycle, cx));
    assert_eq!(titles("Go", cx), None);
    assert!(!titles("File", cx).unwrap().contains(&"Bar Save".to_owned()));
}
