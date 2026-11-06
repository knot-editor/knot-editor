//! The bundled status bar package, following the editor a window describes.
use super::editor_api::active_editor;
use super::language_editing::start_bundled_packages;
use super::*;
use crate::app::status_bar::ApplicationStatusBar;

fn right_texts(window: gpui::AnyWindowHandle, cx: &gpui::App) -> Vec<String> {
    let key = window.window_id().as_u64().to_string();
    cx.global::<ApplicationStatusBar>()
        .0
        .items(&key, &[])
        .right
        .into_iter()
        .map(|bar| bar.item.text)
        .collect()
}

#[gpui::test]
fn the_items_follow_the_selection_and_the_document_of_the_status_target(cx: &mut TestAppContext) {
    start_bundled_packages(cx);
    let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
    let model = cx.new(|_| BufferModel::from_text("one\ntwo\nthree é\n"));
    let document = documents.update(cx, |documents, cx| {
        documents.create_untitled("Lines", model.clone(), cx)
    });
    let (shell, window) = product_window(document, model, cx);
    let editor = active_editor(&shell, cx);
    // Nothing describes an editor before one is focused.
    cx.run_until_parked();
    assert!(cx.read(|cx| right_texts(window, cx)).is_empty());

    cx.update_window(window, |_, _, cx| {
        shell.update(cx, |shell, cx| shell.focus_active_editor(cx))
    })
    .unwrap();
    wait_for_api_condition(cx, |cx| right_texts(window, cx).len() == 3);
    assert_eq!(
        cx.read(|cx| right_texts(window, cx)),
        ["Ln 1, Col 1", "Spaces: 4", "Plain Text"]
    );

    let select = |anchor: usize, caret: usize, cx: &mut TestAppContext| {
        editor.update(cx, |editor, cx| {
            editor.set_selection(anchor, caret, crate::app::editor::Reveal::Nearest, cx)
        })
    };
    select(6, 6, cx);
    wait_for_api_condition(cx, |cx| right_texts(window, cx)[0] == "Ln 2, Col 3");
    select(0, 3, cx);
    wait_for_api_condition(cx, |cx| {
        right_texts(window, cx)[0] == "Ln 1, Col 4 (3 selected)"
    });

    // A document-scoped indentation write reaches the bar without a listener.
    let uuid = cx.read(|cx| documents.read(cx).get(document).unwrap().identity());
    cx.update(|cx| {
        crate::app::settings::set_document_insert_spaces(uuid, false, cx);
        crate::app::application_observation::changed(cx);
    });
    wait_for_api_condition(cx, |cx| right_texts(window, cx)[1] == "Tab Size: 4");
}
