//! Application zoom across windows, editors, and its commands.
use super::*;
use crate::app::commands::dispatch::{RESET_ZOOM_COMMAND, ZOOM_IN_COMMAND, ZOOM_OUT_COMMAND};
use crate::app::workbench::CommandView;
use crate::app::zoom;

fn active_editor(
    shell: &Entity<KnotWindow>,
    cx: &gpui::App,
) -> Entity<crate::app::editor::EditorView> {
    let workbench = shell.read(cx).workbench.read(cx);
    match workbench.focused_command_view() {
        Some(CommandView::Editor(editor)) => editor,
        _ => panic!("the active tab is not an editor"),
    }
}

#[gpui::test]
async fn zoom_commands_step_clamp_and_scale_every_window(cx: &mut TestAppContext) {
    let documents = install_globals(cx);
    let first_document = cx.update(|cx| create_untitled_document(&documents, cx));
    let first_model = cx.read(|cx| {
        documents
            .read(cx)
            .get(first_document)
            .unwrap()
            .model()
            .clone()
    });
    let (first, first_window) = product_window(first_document, first_model, cx);
    let second_document = cx.update(|cx| create_untitled_document(&documents, cx));
    let second_model = cx.read(|cx| {
        documents
            .read(cx)
            .get(second_document)
            .unwrap()
            .model()
            .clone()
    });
    let (second, second_window) = product_window(second_document, second_model, cx);

    let run = |name, cx: &mut TestAppContext| {
        let command = dispatch_product_command(&first, first_window, name, cx);
        cx.run_until_parked();
        command
    };
    let observed = |cx: &mut TestAppContext| {
        cx.refresh().unwrap();
        let rem = |window: gpui::AnyWindowHandle, cx: &mut TestAppContext| {
            cx.update_window(window, |_, window, _| window.rem_size())
                .unwrap()
        };
        let rems = (rem(first_window, cx), rem(second_window, cx));
        let font_sizes = cx.read(|cx| {
            (
                active_editor(&first, cx).read(cx).metrics().font_size,
                active_editor(&second, cx).read(cx).metrics().font_size,
            )
        });
        (cx.read(zoom::level), rems, font_sizes)
    };
    assert_eq!(observed(cx), (0, (px(16.), px(16.)), (14., 14.)));

    for _ in 0..2 {
        assert_eq!(
            run(ZOOM_IN_COMMAND, cx).completion.await.unwrap(),
            CommandOutcome::Completed
        );
    }
    let (level, (first_rem, second_rem), (first_size, second_size)) = observed(cx);
    assert_eq!(level, 2);
    assert_eq!(first_rem, second_rem);
    assert!((f32::from(first_rem) - 16. * 1.44).abs() < 1e-3);
    assert_eq!(first_size, second_size);
    assert!((first_size - 14. * 1.44).abs() < 1e-3);

    run(ZOOM_OUT_COMMAND, cx).completion.await.unwrap();
    assert_eq!(cx.read(zoom::level), 1);
    run(RESET_ZOOM_COMMAND, cx).completion.await.unwrap();
    assert_eq!(observed(cx), (0, (px(16.), px(16.)), (14., 14.)));

    for _ in 0..zoom::MAX_LEVEL + 2 {
        run(ZOOM_IN_COMMAND, cx).completion.await.unwrap();
    }
    assert_eq!(cx.read(zoom::level), zoom::MAX_LEVEL);
    run(RESET_ZOOM_COMMAND, cx).completion.await.unwrap();
    for _ in 0..-zoom::MIN_LEVEL + 2 {
        run(ZOOM_OUT_COMMAND, cx).completion.await.unwrap();
    }
    assert_eq!(cx.read(zoom::level), zoom::MIN_LEVEL);
}
