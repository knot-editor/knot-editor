//! Status items registered through the public extension API, the bar that
//! shows them, and the settings and command that control it.
use super::*;
use crate::app::{
    convert::ThemePaint as _,
    settings::{ApplicationSettings, STATUS_BAR_HIDDEN_ITEMS, SettingsScope},
    status_bar::ApplicationStatusBar,
    theme::{color, theme},
};
use knot_protocol::ExtensionKey;

const SOURCE: &str = r##"
import { commands, windows, workbench } from "knot";
await commands.register("status.click", async ({ arguments: [n] }) => {
  await workbench.writeClipboardText(`clicked ${n}`);
});
const position = await workbench.registerStatusItem("position", {
  text: "Ln 1", tooltip: "Go to line", side: "right", priority: 2,
  command: { name: "status.click", arguments: [7] },
});
await workbench.registerStatusItem("language", { text: "Rust", side: "right" });
await workbench.registerStatusItem("branch", { text: "main", icon: "folder", color: "warning" });
await commands.register("status.override", async () => {
  const [window] = await windows.list();
  await position.update({ text: "Ln 9", side: "right", priority: 2 }, { window: window.target });
});
await commands.register("status.update", async () => {
  await position.update({ text: "Ln 2", side: "right", priority: 2 });
});
await commands.register("status.clearOverride", async () => {
  const [window] = await windows.list();
  await position.update(null, { window: window.target });
});
await commands.register("status.drop", async () => { await position.dispose(); });
"##;

fn bounds(
    window: gpui::AnyWindowHandle,
    selector: &'static str,
    cx: &mut TestAppContext,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    VisualTestContext::from_window(window, cx).debug_bounds(selector)
}

fn texts(window: gpui::AnyWindowHandle, cx: &mut TestAppContext) -> (Vec<String>, Vec<String>) {
    let key = crate::app::product::test_session_id(window);
    cx.read(|cx| {
        let hidden = crate::app::settings::strings(STATUS_BAR_HIDDEN_ITEMS, cx);
        let items = cx.global::<ApplicationStatusBar>().0.items(&key, &hidden);
        let texts = |items: Vec<crate::app::status_bar::BarItem>| {
            items.into_iter().map(|bar| bar.item.text).collect()
        };
        (texts(items.left), texts(items.right))
    })
}

fn run(
    shell: &Entity<KnotWindow>,
    window: gpui::AnyWindowHandle,
    command: &'static str,
    cx: &mut TestAppContext,
) {
    let execution = dispatch_product_command(shell, window, command, cx);
    assert_eq!(wait_for_command(execution, cx), CommandOutcome::Completed);
    cx.run_until_parked();
}

#[gpui::test]
fn registered_items_paint_in_order_follow_settings_and_run_their_command(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, window) = product_window(document, model, cx);
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

    // The bar sits below the workbench, the left group first and the right
    // group by descending priority.
    let bar = bounds(window, "status-bar", cx).expect("the bar is laid out");
    let branch = bounds(window, "status-item-branch", cx).unwrap();
    let position = bounds(window, "status-item-position", cx).unwrap();
    let language = bounds(window, "status-item-language", cx).unwrap();
    let pane = bounds(window, "pane-1", cx).unwrap();
    assert!(bar.top() >= pane.bottom());
    assert!(branch.right() <= position.left());
    assert!(position.right() <= language.left());
    assert!(language.right() <= bar.right());

    // An item paints its icon before the text; one without an icon paints none.
    let icon = bounds(window, "status-icon-branch", cx).expect("the icon is painted");
    assert!(
        icon.size.width > px(0.) && icon.left() >= branch.left() && icon.right() < branch.right()
    );
    assert!(bounds(window, "status-icon-language", cx).is_none());

    // Clicking an item runs its command with its arguments, in the window's
    // focused view.
    cx.update_window(window, |_, _, cx| {
        shell.update(cx, |shell, cx| shell.focus_active_editor(cx))
    })
    .unwrap();
    cx.update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("untouched".into())));
    {
        let mut window_cx = VisualTestContext::from_window(window, cx);
        let center = position.center();
        window_cx.simulate_click(center, Modifiers::default());
    }
    wait_for_api_condition(cx, |cx| {
        cx.read_from_clipboard()
            .is_some_and(|item| item.text().as_deref() == Some("clicked 7"))
    });

    // A window's own value shadows the shared one, which still changes beneath.
    run(&shell, window, "status.override", cx);
    assert_eq!(texts(window, cx).1, ["Ln 9", "Rust"]);
    run(&shell, window, "status.update", cx);
    assert_eq!(texts(window, cx).1, ["Ln 9", "Rust"]);
    run(&shell, window, "status.clearOverride", cx);
    assert_eq!(texts(window, cx).1, ["Ln 2", "Rust"]);

    // Hidden IDs leave the bar while their registrations live on: the item
    // beside the hidden one takes its place at the edge. (Debug bounds of a
    // selector outlive its element, so absence shows in the layout.)
    let edge =
        |cx: &mut TestAppContext| bounds(window, "status-item-position", cx).unwrap().right();
    let before = edge(cx);
    cx.update(|cx| {
        cx.default_global::<ApplicationSettings>()
            .0
            .set(
                STATUS_BAR_HIDDEN_ITEMS,
                serde_json::json!(["language"]),
                SettingsScope::default(),
            )
            .unwrap();
    });
    assert!(edge(cx) > before);
    assert_eq!(texts(window, cx).1, ["Ln 2"]);
    cx.update(|cx| {
        cx.default_global::<ApplicationSettings>()
            .0
            .reset(STATUS_BAR_HIDDEN_ITEMS, &SettingsScope::default());
    });
    assert_eq!(edge(cx), before);

    // Toggle Status Bar flips the visibility setting and the workbench takes
    // the bar's height.
    let height = |cx: &mut TestAppContext| bounds(window, "pane-1", cx).unwrap().size.height;
    let with_bar = height(cx);
    run(&shell, window, "workbench.toggle-status-bar", cx);
    assert!(height(cx) > with_bar);
    run(&shell, window, "workbench.toggle-status-bar", cx);
    assert_eq!(height(cx), with_bar);

    // Disposing an item removes it; unloading removes the rest.
    run(&shell, window, "status.drop", cx);
    assert_eq!(
        texts(window, cx),
        (vec!["main".into()], vec!["Rust".into()])
    );
    host.update(cx, |host, cx| host.unload(key, cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(texts(window, cx), (vec![], vec![]));
}

#[gpui::test]
fn hovering_an_item_with_a_tooltip_shows_it_on_a_popover(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (_, window) = product_window(document, model, cx);
    let key = ExtensionKey::new(ExtensionId::new(972), ExtensionLifecycleId::new(1));
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

    // The tooltip is the only surface on the theme's popover background.
    let popover: gpui::Background = cx
        .read(|cx| theme(cx).paint_color(color::POPOVER_BACKGROUND))
        .into();
    let popovers = |cx: &mut TestAppContext| {
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            window.draw(cx).clear(cx);
            window
                .painted_quads()
                .iter()
                .filter(|quad| quad.background == popover)
                .count()
        })
        .unwrap()
    };
    let hover = |selector: &'static str, cx: &mut TestAppContext| {
        let item = bounds(window, selector, cx).unwrap();
        VisualTestContext::from_window(window, cx).simulate_mouse_move(
            item.center(),
            None,
            Modifiers::default(),
        );
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
    };
    assert_eq!(popovers(cx), 0);
    hover("status-item-position", cx);
    assert_eq!(popovers(cx), 1);
    // An item without a tooltip shows none.
    hover("status-item-language", cx);
    assert_eq!(popovers(cx), 0);
}

#[gpui::test]
fn an_unchanged_update_redraws_nothing(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let host = cx.update(super::super::super::extensions::host::install);
    let document = cx.update(|cx| create_untitled_document(&documents, cx));
    let model = cx.read(|cx| documents.read(cx).get(document).unwrap().model().clone());
    let (shell, window) = product_window(document, model, cx);
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

    for command in ["status.update", "status.override"] {
        run(&shell, window, command, cx);
        crate::app::work_counts::take();
        run(&shell, window, command, cx);
        let counts = crate::app::work_counts::take();
        assert!(
            counts.requests.contains(&"updateStatusItem"),
            "{command}: {counts:?}"
        );
        assert_eq!(counts.draws, 0, "{command}: {counts:?}");
    }
}
