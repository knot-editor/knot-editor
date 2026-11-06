use gpui::{
    AppContext, ClipboardItem, EntityInputHandler, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, ScrollDelta, ScrollWheelEvent, TestAppContext, point, px,
};

use super::{
    BufferModel, CaretShape, ContributionSource, EditorRenderingOptions, EditorView,
    EditorViewState, ResolvedEditorContribution, Reveal, flat_utf16_to_position,
    layout::{HighlightSpan, runs_for, shape_editor_line, x_for_index_dir},
    project_contributions, to_flat_utf16,
};
use std::sync::Arc;

use crate::{
    app::{
        completion::CompletionProviderRegistry,
        languages::{self, LanguageDefinition, LanguageOwner},
        settings::{
            ApplicationSettings, EDITOR_INSERT_SPACES, EDITOR_TAB_SIZE, FONT_SIZE, SettingsScope,
        },
        syntax::{SyntaxLanguage, SyntaxState},
        theme::{Appearance, Highlight, Theme, color, native_color_definitions, theme},
    },
    host::protocol::{
        BufferHandle, ByteRange, CompletionResponse, CompletionResultItem, Decoration,
        DecorationStyle, EditorContribution, ExtensionId, ExtensionLifecycleId, LanguageInfo,
    },
};

#[gpui::test]
fn ordinary_editing_keeps_projection_clipboard_and_composition_synchronous(
    cx: &mut TestAppContext,
) {
    let model = cx.new(|_| BufferModel::from_text(""));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.replace_text_in_range(None, "a", window, cx);
            view.replace_text_in_range(None, "👩‍💻", window, cx);
            view.replace_text_in_range(None, "é", window, cx);
            assert_eq!(view.text(cx), "a👩‍💻é");
            view.execute_editing_command("editor.delete-backward", window, cx);
            assert_eq!(view.text(cx), "a👩‍💻");
            view.execute_editing_command("editor.select-all", window, cx);
            view.execute_editing_command("editor.cut", window, cx);
            assert_eq!(view.text(cx), "");
            view.execute_editing_command("editor.paste", window, cx);
            assert_eq!(view.text(cx), "a👩‍💻");
            view.replace_and_mark_text_in_range(None, "に", Some(0..1), window, cx);
            assert_eq!(view.marked_range_utf16, Some(6..7));
            view.replace_and_mark_text_in_range(None, "日本", Some(2..2), window, cx);
            assert_eq!(view.text(cx), "a👩‍💻日本");
            view.replace_text_in_range(None, "日本語", window, cx);
            assert_eq!(view.text(cx), "a👩‍💻日本語");
            assert_eq!(view.marked_range_utf16, None);
            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), "a👩‍💻");
            view.execute_editing_command("editor.redo", window, cx);
            assert_eq!(view.text(cx), "a👩‍💻日本語");
            view.execute_editing_command("editor.select-all", window, cx);
            view.replace_text_in_range(None, "one\r\ntwo\rthree", window, cx);
            assert_eq!(view.text(cx), "one\ntwo\nthree");
        })
    });
}

#[gpui::test]
fn typing_deletion_and_interaction_boundaries_form_practical_undo_steps(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text(""));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.replace_text_in_range(None, "a", window, cx);
            view.replace_text_in_range(None, "β", window, cx);
            view.replace_text_in_range(None, "c", window, cx);
            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), "");
            view.execute_editing_command("editor.redo", window, cx);
            assert_eq!(view.text(cx), "aβc");

            view.execute_editing_command("editor.move-document-end", window, cx);
            view.execute_editing_command("editor.delete-backward", window, cx);
            view.execute_editing_command("editor.delete-backward", window, cx);
            assert_eq!(view.text(cx), "a");
            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), "aβc");

            view.execute_editing_command("editor.move-document-end", window, cx);
            view.execute_editing_command("editor.move-left", window, cx);
            view.replace_text_in_range(None, "X", window, cx);
            assert_eq!(view.text(cx), "aβXc");
            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), "aβc");

            cx.write_to_clipboard(gpui::ClipboardItem::new_string(" pasted".into()));
            view.execute_editing_command("editor.paste", window, cx);
            assert_eq!(view.text(cx), "aβ pastedc");
            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), "aβc");
        });
    });
}

#[gpui::test]
fn undo_and_redo_restore_the_originating_views_grouped_selection(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("abcdef"));
    let (other, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    let origin = cx.new(|cx| EditorView::new(model.clone(), cx));

    cx.update(|window, cx| {
        origin.update(cx, |view, cx| {
            view.anchor_col = 4;
            view.cursor_col = 1;
            view.has_selection = true;
            view.sync_view_position(cx);
            view.replace_text_in_range(None, "X", window, cx);
            view.replace_text_in_range(None, "Y", window, cx);
        });
        other.update(cx, |view, cx| {
            view.cursor_col = 5;
            view.sync_view_position(cx);
            view.execute_editing_command("editor.undo", window, cx);
        });
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let origin = origin.read(cx);
        let other = other.read(cx);
        assert_eq!(origin.text(cx), "abcdef");
        assert_eq!((origin.anchor_col, origin.cursor_col), (4, 1));
        assert!(origin.has_selection);
        assert!(origin.selection_reversed);
        assert!(!other.has_selection);
        assert_ne!((other.cursor_line, other.cursor_col), (0, 1));
    });

    cx.update(|window, cx| {
        other.update(cx, |view, cx| {
            view.execute_editing_command("editor.redo", window, cx);
        });
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let origin = origin.read(cx);
        assert_eq!(origin.text(cx), "aXYef");
        assert_eq!((origin.cursor_line, origin.cursor_col), (0, 3));
        assert!(!origin.has_selection);
    });
}

#[gpui::test]
fn replay_ignores_view_state_for_an_originating_view_that_was_dropped(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("text"));
    let (survivor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    let origin = cx.new(|cx| EditorView::new(model.clone(), cx));

    cx.update(|window, cx| {
        origin.update(cx, |view, cx| {
            view.execute_editing_command("editor.move-document-end", window, cx);
            view.replace_text_in_range(None, "!", window, cx);
        });
    });
    drop(origin);
    cx.run_until_parked();

    cx.update(|window, cx| {
        survivor.update(cx, |view, cx| {
            view.execute_editing_command("editor.undo", window, cx);
        });
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let survivor = survivor.read(cx);
        assert_eq!(survivor.text(cx), "text");
        assert_eq!((survivor.cursor_line, survivor.cursor_col), (0, 0));
    });
}

#[gpui::test]
fn a_newer_edit_invalidates_an_unobserved_history_view_restore(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("abcdef"));
    let (other, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    let origin = cx.new(|cx| EditorView::new(model.clone(), cx));

    cx.update(|window, cx| {
        origin.update(cx, |view, cx| {
            view.anchor_col = 4;
            view.cursor_col = 1;
            view.has_selection = true;
            view.sync_view_position(cx);
            view.replace_text_in_range(None, "X", window, cx);
        });
        other.update(cx, |view, cx| {
            view.execute_editing_command("editor.undo", window, cx);
        });
        model.update(cx, |model, cx| {
            assert!(model.replace(0..0, "z").unwrap());
            cx.notify();
        });
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let origin = origin.read(cx);
        assert_eq!(origin.text(cx), "zabcdef");
        assert_eq!((origin.cursor_line, origin.cursor_col), (0, 5));
        assert!(!origin.has_selection);
    });
}

#[gpui::test]
fn font_size_sets_line_height_visible_lines_hit_testing_and_gutter(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let text = (0..100)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    let model = cx.new(|_| BufferModel::from_text(text));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    let measure = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            editor.update(cx, |view, cx| {
                let model = view.model.read(cx);
                let origin = view.text_bounds().origin;
                let hit = view.hit_test(
                    model,
                    point(
                        origin.x + px(1.),
                        origin.y + px(3.5 * view.metrics.line_height),
                    ),
                    window,
                    cx,
                );
                (
                    view.metrics,
                    view.fully_visible_lines(),
                    hit,
                    view.gutter.width,
                )
            })
        })
    };
    let (small, small_lines, small_hit, small_gutter) = measure(cx);
    assert_eq!((small.font_size, small.line_height), (14., 20.));
    assert_eq!(small_hit.0, 3);

    // A setting for another view kind leaves documents alone.
    let set = |cx: &mut gpui::VisualTestContext, view: Option<&str>| {
        cx.update(|_, cx| {
            cx.global_mut::<ApplicationSettings>()
                .0
                .set(
                    FONT_SIZE,
                    28.into(),
                    SettingsScope {
                        view: view.map(Into::into),
                        ..Default::default()
                    },
                )
                .unwrap();
        });
        cx.run_until_parked();
    };
    set(cx, Some("terminal"));
    assert_eq!(measure(cx).0, small);

    set(cx, None);
    let (large, large_lines, large_hit, large_gutter) = measure(cx);
    assert_eq!((large.font_size, large.line_height), (28., 40.));
    assert_eq!(large_hit.0, 3);
    let count = |(first, last): (usize, usize)| last + 1 - first;
    assert_eq!(count(large_lines), count(small_lines) / 2);
    assert!(large_gutter > small_gutter);
}

#[gpui::test]
fn a_font_size_change_keeps_the_top_line(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let text = (0..100)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    let model = cx.new(|_| BufferModel::from_text(text));
    let editor = cx.new(|cx| EditorView::new(model, cx));
    editor.update(cx, |editor, _| {
        editor.viewport_h = 10. * editor.metrics.line_height;
        editor.scroll = 30. * editor.metrics.line_height;
    });
    cx.update(|cx| {
        cx.global_mut::<ApplicationSettings>()
            .0
            .set(FONT_SIZE, 21.into(), SettingsScope::default())
            .unwrap();
    });
    cx.run_until_parked();
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.metrics.line_height, 30.);
        assert_eq!(editor.scroll, 30. * 30.);
    });
}

#[gpui::test]
fn movement_uses_visual_columns_graphemes_and_pane_height(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("éééx\na\nabcde\nword next\nlast"));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.cursor_col = "ééé".len();
            view.execute_editing_command("editor.move-down", window, cx);
            assert_eq!((view.cursor_line, view.cursor_col), (1, 1));
            view.execute_editing_command("editor.move-down", window, cx);
            assert_eq!((view.cursor_line, view.cursor_col), (2, 3));
            view.execute_editing_command("editor.move-document-start", window, cx);
            view.viewport_h = 2. * view.metrics.line_height;
            view.execute_editing_command("editor.select-page-down", window, cx);
            assert_eq!(view.selected_byte_range(cx), Some(0..10));
            view.execute_editing_command("editor.move-document-end", window, cx);
            assert_eq!((view.cursor_line, view.cursor_col), (4, 4));
            view.execute_editing_command("editor.move-word-left", window, cx);
            assert_eq!((view.cursor_line, view.cursor_col), (4, 0));
            view.execute_editing_command("editor.select-word-left", window, cx);
            assert_eq!((view.cursor_line, view.cursor_col), (3, 5));
        })
    });
}

#[gpui::test]
fn tabs_hit_testing_and_horizontal_reveal_share_geometry(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text(format!("\tx{}", "z".repeat(300))));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.cursor_col = 1;
            let model = view.model.read(cx);
            let tab_x = view.caret_x(model, window, cx);
            assert!(f32::from(tab_x) > 20.);
            assert_eq!(
                view.hit_test(
                    model,
                    point(
                        view.text_bounds().origin.x + tab_x,
                        view.bounds.origin.y + px(5.)
                    ),
                    window,
                    cx
                ),
                (0, 1)
            );
            view.execute_editing_command("editor.move-line-end", window, cx);
        })
    });
    cx.refresh().unwrap();
    cx.read(|cx| assert!(editor.read(cx).scroll_x > 0.));
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.execute_editing_command("editor.move-line-start", window, cx);
        })
    });
    cx.refresh().unwrap();
    cx.read(|cx| assert_eq!(editor.read(cx).scroll_x, 0.));
}

#[gpui::test]
fn tab_stops_follow_the_settings_for_each_open_view(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(ApplicationSettings::default());
        let rust = LanguageDefinition {
            info: LanguageInfo {
                id: "rust".into(),
                name: "Rust".into(),
                grammar: "rust".into(),
                file_extensions: Vec::new(),
                file_names: Vec::new(),
                interpreters: Vec::new(),
            },
            queries: languages::LanguageQueries {
                highlights: "(identifier) @variable".into(),
                injections: String::new(),
                indents: String::new(),
            },
            editing: Default::default(),
        };
        let owner = LanguageOwner {
            extension: ExtensionId::new(1),
            lifecycle: ExtensionLifecycleId::new(1),
        };
        languages::register(owner, rust.compile().unwrap(), cx);
    });
    let text = "\tab\tc";
    let rust_model = cx.new(|_| {
        let mut model = BufferModel::from_text(text);
        model.set_language(Some("rust".into()), true);
        model
    });
    let plain_model = cx.new(|_| BufferModel::from_text(text));
    let plain = cx.new(|cx| EditorView::new(plain_model, cx));
    let (rust, cx) = cx.add_window_view(|_, cx| EditorView::new(rust_model, cx));
    cx.refresh().unwrap();

    // The caret after the leading tab, in widths of one column, and the
    // shaped width of the line.
    let geometry = |cx: &mut gpui::VisualTestContext, editor: &gpui::Entity<EditorView>| {
        cx.update(|window, cx| {
            let view = editor.read(cx);
            let model = view.model.read(cx);
            let column = |col| {
                f32::from(super::layout::x_for_index_dir(
                    &view.shape_line(model, 0, text, window, cx),
                    col,
                    text,
                    view.tab_size(model, cx),
                ))
            };
            let width = column(2) - column(1);
            let shaped = view.shape_line(model, 0, text, window, cx).width;
            (
                (column(1) / width).round() as usize,
                (f32::from(shaped) / width).round() as usize,
            )
        })
    };
    let caret_columns = |cx: &mut gpui::VisualTestContext, editor: &gpui::Entity<EditorView>| {
        cx.update(|window, cx| {
            editor.update(cx, |view, cx| {
                view.cursor_col = 1;
                let model = view.model.read(cx);
                let a = view.caret_x(model, window, cx);
                view.cursor_col = 2;
                let b = view.caret_x(model, window, cx);
                (f32::from(a) / f32::from(b - a)).round() as usize
            })
        })
    };
    assert_eq!(geometry(cx, &rust), (4, 9));
    assert_eq!(geometry(cx, &plain), (4, 9));

    let set = |cx: &mut gpui::VisualTestContext, value: i64, language: Option<&str>| {
        cx.update(|_, cx| {
            cx.global_mut::<ApplicationSettings>()
                .0
                .set(
                    EDITOR_TAB_SIZE,
                    value.into(),
                    SettingsScope {
                        language: language.map(Into::into),
                        view: None,
                        document: None,
                    },
                )
                .unwrap();
        });
        cx.run_until_parked();
    };
    let painted = cx.read(|cx| rust.read(cx).paint_count);
    set(cx, 8, None);
    set(cx, 2, Some("rust"));
    assert!(cx.read(|cx| rust.read(cx).paint_count) > painted);
    assert_eq!(geometry(cx, &rust), (2, 7));
    assert_eq!(caret_columns(cx, &rust), 2);
    assert_eq!(geometry(cx, &plain), (8, 17));
    assert_eq!(caret_columns(cx, &plain), 8);
}

#[gpui::test]
fn mouse_selection_keeps_press_anchor_across_reversal_and_ignores_foreign_drags(
    cx: &mut TestAppContext,
) {
    let model = cx.new(|_| BufferModel::from_text("alpha beta\nnext"));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            let position = |view: &mut EditorView, col, cx: &gpui::App| {
                view.cursor_col = col;
                point(
                    view.text_bounds().origin.x + view.caret_x(view.model.read(cx), window, cx),
                    view.bounds.origin.y + px(5.),
                )
            };
            let start = position(view, 3, cx);
            let end = position(view, 8, cx);
            let before = position(view, 1, cx);
            view.on_mouse_move(
                &MouseMoveEvent {
                    position: end,
                    modifiers: Modifiers::default(),
                    pressed_button: Some(MouseButton::Left),
                },
                window,
                cx,
            );
            assert!(!view.has_selection);
            view.on_mouse_down(
                &MouseDownEvent {
                    position: start,
                    modifiers: Modifiers::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                    first_mouse: false,
                },
                window,
                cx,
            );
            for p in [end, start, before] {
                view.on_mouse_move(
                    &MouseMoveEvent {
                        position: p,
                        modifiers: Modifiers::default(),
                        pressed_button: Some(MouseButton::Left),
                    },
                    window,
                    cx,
                );
            }
            assert_eq!(view.selected_byte_range(cx), Some(1..3));
            view.on_mouse_down(
                &MouseDownEvent {
                    position: start,
                    modifiers: Modifiers::default(),
                    button: MouseButton::Left,
                    click_count: 2,
                    first_mouse: false,
                },
                window,
                cx,
            );
            assert_eq!(view.selected_byte_range(cx), Some(0..5));
            view.on_mouse_down(
                &MouseDownEvent {
                    position: start,
                    modifiers: Modifiers::default(),
                    button: MouseButton::Left,
                    click_count: 3,
                    first_mouse: false,
                },
                window,
                cx,
            );
            assert_eq!(view.selected_byte_range(cx), Some(0..11));
        })
    });
}

#[gpui::test]
fn crlf_is_one_navigation_and_deletion_boundary(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("a\r\nb"));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.execute_editing_command("editor.move-line-end", window, cx);
            assert_eq!((view.cursor_line, view.cursor_col), (0, 1));
            view.execute_editing_command("editor.move-right", window, cx);
            assert_eq!((view.cursor_line, view.cursor_col), (1, 0));
            view.execute_editing_command("editor.delete-backward", window, cx);
            assert_eq!(view.text(cx), "ab");
        })
    });
}

#[gpui::test]
fn delete_to_line_end_removes_the_rest_of_the_line_then_its_break(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("hello world\r\nnext"));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.execute_editing_command("editor.move-word-right", window, cx);
            view.execute_editing_command("editor.delete-to-line-end", window, cx);
            assert_eq!(view.text(cx), "hello\r\nnext");
            view.execute_editing_command("editor.delete-to-line-end", window, cx);
            assert_eq!(view.text(cx), "hellonext");
            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), "hello\r\nnext");

            view.execute_editing_command("editor.select-left", window, cx);
            view.execute_editing_command("editor.delete-to-line-end", window, cx);
            assert_eq!(view.text(cx), "hell\r\nnext");

            view.execute_editing_command("editor.move-document-end", window, cx);
            assert!(view.execute_editing_command("editor.delete-to-line-end", window, cx));
            assert_eq!(view.text(cx), "hell\r\nnext");
        })
    });
}

#[gpui::test]
fn inserted_newlines_follow_a_crlf_buffer(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("a\r\nb"));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.execute_editing_command("editor.move-line-end", window, cx);
            view.execute_editing_command("editor.insert-newline", window, cx);
            assert_eq!(view.text(cx), "a\r\n\r\nb");
            assert_eq!((view.cursor_line, view.cursor_col), (1, 0));

            cx.write_to_clipboard(ClipboardItem::new_string("1\n2\r\n3".into()));
            view.execute_editing_command("editor.paste", window, cx);
            assert_eq!(view.text(cx), "a\r\n1\r\n2\r\n3\r\nb");
            assert_eq!((view.cursor_line, view.cursor_col), (3, 1));

            view.replace_text_in_range(None, "\n", window, cx);
            assert_eq!(view.text(cx), "a\r\n1\r\n2\r\n3\r\n\r\nb");

            view.execute_editing_command("editor.delete-backward", window, cx);
            view.execute_editing_command("editor.delete-forward", window, cx);
            assert_eq!(view.text(cx), "a\r\n1\r\n2\r\n3b");
        })
    });
}

#[gpui::test]
fn composition_is_cleared_by_navigation_and_external_edits(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("abc"));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.replace_and_mark_text_in_range(None, "に", Some(1..1), window, cx);
            view.execute_editing_command("editor.move-right", window, cx);
            assert_eq!(view.marked_range_utf16, None);
            view.replace_and_mark_text_in_range(None, "本", Some(1..1), window, cx);
            view.execute_editing_command("editor.select-all", window, cx);
            assert_eq!(view.marked_range_utf16, None);
            view.replace_and_mark_text_in_range(None, "日", Some(1..1), window, cx);
            view.select_reveal_and_focus(
                ByteRange {
                    start_byte_offset: 0,
                    end_byte_offset: "日".len(),
                },
                window,
                cx,
            );
            assert_eq!(view.marked_range_utf16, None);
            view.replace_and_mark_text_in_range(None, "語", Some(1..1), window, cx);
            assert!(view.marked_range_utf16.is_some());
        })
    });
    model.update(cx, |model, cx| {
        model.replace(0..0, "prefix").unwrap();
        cx.notify();
    });
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(editor.read(cx).marked_range_utf16, None));
}

#[gpui::test]
fn editing_a_large_buffer_never_reads_the_whole_text(cx: &mut TestAppContext) {
    let text = (0..100_000)
        .map(|line| format!("line {line} with some words"))
        .collect::<Vec<_>>()
        .join("\n");
    let model = cx.new(|_| BufferModel::from_text(text));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    let (other, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.focus.focus(window);
            view.restore_view_state(
                EditorViewState {
                    anchor: 50_000,
                    caret: 50_000,
                    top: 49_000,
                    scroll_x: 0.,
                },
                cx,
            );
            for command in [
                "editor.move-word-right",
                "editor.move-word-left",
                "editor.move-down",
                "editor.move-page-down",
                "editor.move-line-end",
                "editor.select-left",
                "editor.select-up",
                "editor.delete-backward",
                "editor.delete-forward",
                "editor.insert-newline",
                "editor.undo",
                "editor.redo",
            ] {
                assert!(
                    view.execute_editing_command(command, window, cx),
                    "{command}"
                );
            }
            view.replace_text_in_range(None, "typed", window, cx);
            view.replace_and_mark_text_in_range(None, "に", Some(1..1), window, cx);
            let selection = view.selected_text_range(false, window, cx).unwrap();
            view.text_for_range(selection.range, &mut None, window, cx);
            view.replace_text_in_range(None, "日", window, cx);
        })
    });
    cx.simulate_event(ScrollWheelEvent {
        position: cx.read(|cx| editor.read(cx).interaction_bounds().center()),
        delta: ScrollDelta::Pixels(point(px(-30.), px(-200.))),
        ..Default::default()
    });
    cx.refresh().unwrap();
    cx.run_until_parked();

    cx.read(|cx| {
        assert!(other.read(cx).paint_count > 0);
        assert_eq!(model.read(cx).whole_text_reads(), 0);
    });
}

#[gpui::test]
fn horizontal_scroll_is_bounded_by_the_widest_shaped_line(cx: &mut TestAppContext) {
    let text = format!("{}\nshort", "x".repeat(400));
    let model = cx.new(|_| BufferModel::from_text(text));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.refresh().unwrap();
    cx.simulate_event(ScrollWheelEvent {
        position: cx.read(|cx| editor.read(cx).interaction_bounds().center()),
        delta: ScrollDelta::Pixels(point(px(-100_000.), px(0.))),
        ..Default::default()
    });
    let long_width = cx.read(|cx| {
        let view = editor.read(cx);
        let limit = view.max_line_width + 2. - f32::from(view.text_bounds().size.width);
        assert!(limit > 0.);
        assert_eq!(view.scroll_x, limit);
        view.max_line_width
    });

    model.update(cx, |model, cx| {
        model.replace(0..400, "x").unwrap();
        cx.notify();
    });
    cx.run_until_parked();
    // Text changes forget widths; the repaint measures the remaining lines.
    cx.read(|cx| assert!(editor.read(cx).max_line_width < long_width / 10.));
}

#[gpui::test]
fn constructs_from_arbitrary_buffer_text(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("generated\ntext"));
    let editor = cx.new(|cx| EditorView::new(model.clone(), cx));

    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.model(), &model);
        assert_eq!(editor.line_count, 2);
        assert!(editor.rendering.show_gutter);
    });
}

#[gpui::test]
fn completion_acceptance_rechecks_the_captured_revision(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("fo"));
    let editor = cx.new(|cx| EditorView::new(model.clone(), cx));
    let mut registry = CompletionProviderRegistry::new();
    registry.register(
        ExtensionId::new(1),
        ExtensionLifecycleId::new(1),
        "fixture".into(),
    );

    let request = editor.update(cx, |editor, cx| {
        editor.cursor_col = 2;
        editor
            .start_completion(BufferHandle::new(1), registry.snapshot(), cx)
            .remove(0)
            .1
    });
    editor.update(cx, |editor, cx| {
        assert!(editor.apply_completion_response(
            request.buffer,
            CompletionResponse {
                registration: request.registration,
                revision: request.revision,
                generation: request.generation,
                result: Ok(vec![CompletionResultItem {
                    label: "foo".into(),
                    insert_text: "foo".into(),
                }]),
            },
            cx,
        ));
    });
    model.update(cx, |model, _| {
        assert!(model.replace(0..0, "x").unwrap());
    });
    editor.update(cx, |editor, cx| editor.accept_selected_completion(cx));

    cx.read(|cx| assert_eq!(model.read(cx).text(), "xfo"));

    let model = cx.new(|_| BufferModel::from_text("fo"));
    let editor = cx.new(|cx| EditorView::new(model.clone(), cx));
    let request = editor.update(cx, |editor, cx| {
        editor.cursor_col = 2;
        editor
            .start_completion(BufferHandle::new(2), registry.snapshot(), cx)
            .remove(0)
            .1
    });
    editor.update(cx, |editor, cx| {
        assert!(editor.apply_completion_response(
            request.buffer,
            CompletionResponse {
                registration: request.registration,
                revision: request.revision,
                generation: request.generation,
                result: Ok(vec![CompletionResultItem {
                    label: "foo".into(),
                    insert_text: "foo".into(),
                }]),
            },
            cx,
        ));
        editor.accept_selected_completion(cx);
    });
    cx.read(|cx| assert_eq!(model.read(cx).text(), "foo"));
}

#[gpui::test]
fn action_click_pointer_jitter_does_not_start_text_selection(cx: &mut TestAppContext) {
    let model = cx.new(|_| {
        let mut model = BufferModel::from_read_only_text("result");
        model
            .replace_contributions(
                ContributionSource::fixture(),
                &[EditorContribution {
                    range: ByteRange {
                        start_byte_offset: 0,
                        end_byte_offset: 6,
                    },
                    decoration: None,
                    gutter: None,
                    command: Some("fixture.action".into()),
                }],
                0,
            )
            .unwrap();
        model
    });
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    let click = cx.read(|cx| {
        let editor = editor.read(cx);
        let text = editor.text_bounds();
        point(text.origin.x + px(4.), text.origin.y + px(10.))
    });

    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.on_mouse_down(
                &MouseDownEvent {
                    position: click,
                    modifiers: Modifiers::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                    first_mouse: false,
                },
                window,
                cx,
            );
            editor.on_mouse_move(
                &MouseMoveEvent {
                    position: point(click.x + px(3.), click.y),
                    modifiers: Modifiers::default(),
                    pressed_button: Some(MouseButton::Left),
                },
                window,
                cx,
            );
            assert_eq!(editor.selected_byte_range(cx), None);
        });
    });
}

#[gpui::test]
fn read_only_text_keeps_normal_selection_copy_and_scroll_behavior(cx: &mut TestAppContext) {
    let text = (0..100)
        .map(|line| format!("result {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let model = cx.new(|_| BufferModel::from_read_only_text(text.clone()));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.refresh().unwrap();

    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.select_reveal_and_focus(
                ByteRange {
                    start_byte_offset: 0,
                    end_byte_offset: 8,
                },
                window,
                cx,
            );
            let selection = editor.selected_text_range(false, window, cx).unwrap();
            assert_eq!(selection.range, 0..8);
            assert_eq!(
                editor.text_for_range(selection.range, &mut None, window, cx),
                Some("result 0".into())
            );

            editor.replace_text_in_range(None, "changed", window, cx);
        });
    });
    cx.simulate_event(ScrollWheelEvent {
        position: cx.read(|cx| editor.read(cx).interaction_bounds().center()),
        delta: ScrollDelta::Lines(point(0., -5.)),
        ..Default::default()
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.model().read(cx).text(), text);
        assert!(editor.scroll > 0.);
    });
}

#[test]
fn contribution_projection_indexes_multiline_ranges() {
    let model = BufferModel::from_text("aa\nbbbb\ncc");
    let (decorations, gutters, actions) = project_contributions(
        &model,
        vec![ResolvedEditorContribution {
            range: ByteRange {
                start_byte_offset: 1,
                end_byte_offset: 9,
            },
            source: ContributionSource::fixture(),
            decoration: Some(Decoration::new("warning", DecorationStyle::Wavy)),
            gutter: Some("warning".to_owned()),
            command: Some("fixture.action".into()),
        }],
    );

    assert_eq!(
        decorations
            .iter()
            .map(|decoration| (decoration.line, decoration.start, decoration.end))
            .collect::<Vec<_>>(),
        [(0, 1, 2), (1, 0, 4), (2, 0, 1)]
    );
    assert_eq!(gutters.len(), 1);
    assert_eq!(gutters[0].line, 0);
    assert_eq!(gutters[0].command.as_deref(), Some("fixture.action"));
    assert!(actions.iter().all(|action| {
        action.range
            == ByteRange {
                start_byte_offset: 1,
                end_byte_offset: 9,
            }
    }));
    assert_eq!(
        actions
            .iter()
            .map(|action| (action.line, action.start, action.end))
            .collect::<Vec<_>>(),
        [(0, 1, 2), (1, 0, 4), (2, 0, 1)]
    );
}

#[test]
fn each_line_keeps_its_most_severe_marker() {
    let model = BufferModel::from_text("aa\nbb");
    let marker = |start, key: &str, command: &str| ResolvedEditorContribution {
        range: ByteRange {
            start_byte_offset: start,
            end_byte_offset: start + 1,
        },
        source: ContributionSource::fixture(),
        decoration: None,
        gutter: Some(key.to_owned()),
        command: Some(command.into()),
    };
    let (_, gutters, _) = project_contributions(
        &model,
        vec![
            marker(0, "error", "first.error"),
            marker(1, "info", "info"),
            marker(0, "error", "second.error"),
            marker(3, "diff.add", "diff"),
            marker(4, "warning", "warning"),
            marker(3, "made.up", "later"),
        ],
    );
    assert_eq!(
        gutters
            .iter()
            .map(|marker| (marker.line, marker.key.as_str(), marker.command.as_deref()))
            .collect::<Vec<_>>(),
        [
            (0, "error", Some("second.error")),
            (1, "warning", Some("warning"))
        ]
    );
}

#[gpui::test]
async fn flat_utf16_offsets_round_trip_across_emoji_lines(cx: &mut TestAppContext) {
    let family = "👨‍👩‍👧‍👦";
    let first_line = "a😀b";
    let second_line = format!("{family}z");
    let model = cx.new(|_| BufferModel::from_text(format!("{first_line}\n{second_line}")));
    let editor = cx.new(|cx| EditorView::new(model, cx));

    cx.read(|cx| {
        let model = editor.read(cx).model.read(cx);
        let second_line_start = 5;
        let family_utf16_len = family.chars().map(char::len_utf16).sum::<usize>();

        assert_eq!(flat_utf16_to_position(model, 1), (0, 1));
        assert_eq!(flat_utf16_to_position(model, 3), (0, "a😀".len()));
        assert_eq!(flat_utf16_to_position(model, second_line_start), (1, 0));
        assert_eq!(
            flat_utf16_to_position(model, second_line_start + family_utf16_len),
            (1, family.len())
        );

        for (line, byte_col) in [
            (0, 0),
            (0, 1),
            (0, "a😀".len()),
            (0, first_line.len()),
            (1, 0),
            (1, family.len()),
            (1, second_line.len()),
        ] {
            let utf16 = to_flat_utf16(model, line, byte_col);
            assert_eq!(flat_utf16_to_position(model, utf16), (line, byte_col));
        }
    });
}

#[gpui::test]
fn set_selection_reveals_nearest_or_centers_an_offscreen_caret(cx: &mut TestAppContext) {
    let text = (0..100)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    let model = cx.new(|_| BufferModel::from_text(text));
    let editor = cx.new(|cx| EditorView::new(model.clone(), cx));
    let line_start = |line: usize| cx.read(|cx| model.read(cx).line_range(line).start);
    let (line_10, line_15, line_50, line_90) = (
        line_start(10),
        line_start(15),
        line_start(50),
        line_start(90),
    );
    editor.update(cx, |editor, cx| {
        editor.viewport_h = 10. * editor.metrics.line_height;

        // Nearest scrolls just enough to show the caret's line at the bottom.
        editor.set_selection(line_50, line_50 + 2, Reveal::Nearest, cx);
        assert_eq!(editor.scroll, 41. * editor.metrics.line_height);
        assert_eq!(editor.selection(cx), (line_50, line_50 + 2));
        assert_eq!(editor.selected_byte_range(cx), Some(line_50..line_50 + 2));

        // Center keeps the scroll while the caret's line is visible...
        editor.set_selection(line_10, line_10, Reveal::Nearest, cx);
        assert_eq!(editor.scroll, 10. * editor.metrics.line_height);
        editor.set_selection(line_15, line_15, Reveal::Center, cx);
        assert_eq!(editor.scroll, 10. * editor.metrics.line_height);

        // ...and centers it when the line is outside the viewport.
        editor.set_selection(line_90, line_90, Reveal::Center, cx);
        assert_eq!(editor.scroll, 85.5 * editor.metrics.line_height);
        assert_eq!(editor.selected_byte_range(cx), None);
    });
}

#[gpui::test]
fn top_and_bottom_reveals_scroll_and_scrolling_leaves_the_caret(cx: &mut TestAppContext) {
    let text = (0..100)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    let model = cx.new(|_| BufferModel::from_text(text));
    let editor = cx.new(|cx| EditorView::new(model.clone(), cx));
    let line_start = |line: usize| cx.read(|cx| model.read(cx).line_range(line).start);
    let (line_5, line_50, line_100) = (line_start(5), line_start(50), line_start(100));
    editor.update(cx, |editor, cx| {
        // Before layout there is a single visible line.
        assert_eq!(editor.fully_visible_lines(), (0, 0));
        editor.viewport_h = 10. * editor.metrics.line_height;
        assert_eq!(editor.fully_visible_lines(), (0, 9));

        editor.set_selection(line_50, line_50, Reveal::Top, cx);
        assert_eq!(editor.scroll, 50. * editor.metrics.line_height);
        assert_eq!(editor.fully_visible_lines(), (50, 59));
        editor.set_selection(line_50, line_50, Reveal::Bottom, cx);
        assert_eq!(editor.scroll, 41. * editor.metrics.line_height);
        assert_eq!(editor.fully_visible_lines(), (41, 50));

        // Both stop at the ends of the text.
        editor.set_selection(line_5, line_5, Reveal::Bottom, cx);
        assert_eq!(editor.scroll, 0.);
        editor.set_selection(line_100, line_100, Reveal::Top, cx);
        assert_eq!(editor.scroll, 91. * editor.metrics.line_height);
        assert_eq!(editor.fully_visible_lines(), (91, 100));

        // Scrolling moves neither the caret nor the reveal-on-paint flag.
        editor.scroll_by_lines(-30., cx);
        assert_eq!(editor.scroll, 61. * editor.metrics.line_height);
        assert_eq!(editor.selection(cx), (line_100, line_100));
        assert!(!editor.reveal_caret);
        editor.scroll_by_lines(-1000., cx);
        assert_eq!(editor.scroll, 0.);
        editor.scroll_by_lines(1000., cx);
        assert_eq!(editor.scroll, 91. * editor.metrics.line_height);
    });
}

#[gpui::test]
fn view_state_round_trips_and_clamps_to_shorter_multibyte_text(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("one\ntwo\nthree\nfour"));
    let editor = cx.new(|cx| EditorView::new(model.clone(), cx));
    let state = EditorViewState {
        anchor: 4,
        caret: 9,
        top: 8,
        scroll_x: 14.,
    };
    editor.update(cx, |editor, cx| editor.restore_view_state(state, cx));
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.view_state(cx), state);
        assert_eq!(editor.selected_byte_range(cx), Some(4..9));
        assert_eq!(editor.scroll, 2. * editor.metrics.line_height);
    });

    // Offsets past the end clamp, and offsets inside a scalar snap back.
    let shorter = cx.new(|_| BufferModel::from_text("é😀\nb"));
    let editor = cx.new(|cx| EditorView::new(shorter, cx));
    editor.update(cx, |editor, cx| {
        editor.restore_view_state(
            EditorViewState {
                anchor: 3,
                caret: 400,
                top: 400,
                scroll_x: -5.,
            },
            cx,
        )
    });
    cx.read(|cx| {
        assert_eq!(
            editor.read(cx).view_state(cx),
            EditorViewState {
                anchor: 2,
                caret: "é😀\nb".len(),
                top: "é😀\n".len(),
                scroll_x: 0.,
            }
        );
    });
}

#[gpui::test]
fn views_share_model_state_and_keep_presentation_state_independent(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("one\ntwo\nthree"));
    let first = cx.new(|cx| EditorView::new(model.clone(), cx));
    let second = cx.new(|cx| {
        EditorView::new_with_options(
            model.clone(),
            1,
            EditorRenderingOptions { show_gutter: false },
            cx,
        )
    });

    first.update(cx, |view, cx| {
        view.cursor_line = 1;
        view.cursor_col = 2;
        view.anchor_line = 0;
        view.anchor_col = 1;
        view.has_selection = true;
        view.scroll = 20.;
        view.sync_view_position(cx);
    });
    second.update(cx, |view, cx| {
        view.cursor_line = 2;
        view.cursor_col = 3;
        view.scroll = 40.;
        view.sync_view_position(cx);
    });

    model.update(cx, |model, cx| {
        assert!(model.replace(0..0, "shared\n").unwrap());
        model
            .replace_contributions(
                ContributionSource::fixture(),
                &[EditorContribution {
                    range: ByteRange {
                        start_byte_offset: 0,
                        end_byte_offset: 6,
                    },
                    decoration: Some(Decoration::new("warning", DecorationStyle::Wavy)),
                    gutter: Some("warning".to_owned()),
                    command: None,
                }],
                model.revision(),
            )
            .unwrap();
        cx.notify();
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let first = first.read(cx);
        let second = second.read(cx);

        assert_eq!(first.model, second.model);
        assert_eq!(first.text(cx), "shared\none\ntwo\nthree");
        assert_eq!(first.decorations.len(), 1);
        assert_eq!(first.decorations.len(), second.decorations.len());
        assert_eq!(first.gutter_markers.len(), second.gutter_markers.len());

        assert_eq!((first.cursor_line, first.cursor_col), (2, 2));
        assert_eq!((second.cursor_line, second.cursor_col), (3, 3));
        assert!(first.has_selection);
        assert!(!second.has_selection);
        assert_eq!(first.scroll, 20.);
        assert_eq!(second.scroll, 40.);
        assert!(first.rendering.show_gutter);
        assert!(!second.rendering.show_gutter);
        assert_ne!(first.focus, second.focus);
    });
}

#[gpui::test]
fn selection_tracks_a_deletion_from_another_view(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("0123456789"));
    let first = cx.new(|cx| EditorView::new(model.clone(), cx));
    let second = cx.new(|cx| EditorView::new(model.clone(), cx));

    second.update(cx, |view, cx| {
        view.anchor_col = 4;
        view.cursor_col = 7;
        view.has_selection = true;
        view.sync_view_position(cx);
    });
    model.update(cx, |model, cx| {
        assert!(model.replace(1..2, "").unwrap());
        cx.notify();
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let first = first.read(cx);
        let second = second.read(cx);
        assert_eq!(first.text(cx), "023456789");
        assert_eq!((second.anchor_line, second.anchor_col), (0, 3));
        assert_eq!((second.cursor_line, second.cursor_col), (0, 6));
        assert!(second.has_selection);
    });
}

/// `range` styled as `test_theme` styles capture `name`.
fn span(range: std::ops::Range<usize>, name: &str) -> HighlightSpan {
    HighlightSpan {
        range,
        style: test_theme().style(Highlight::named(name)),
    }
}

/// Each run as `(len, color, underline color)`.
fn run_styles(runs: &[gpui::TextRun]) -> Vec<(usize, gpui::Hsla, Option<gpui::Hsla>)> {
    runs.iter()
        .map(|run| {
            let underline = run.underline.and_then(|underline| underline.color);
            (run.len, run.color, underline)
        })
        .collect()
}

/// A theme styling the captures these tests paint.
fn test_theme() -> Theme {
    let style = |hex: u32| gpui::HighlightStyle::color(gpui::rgb(hex).into());
    Theme::new(
        Appearance::Dark,
        &std::collections::HashMap::new(),
        &native_color_definitions(),
        &[
            (
                "comment",
                gpui::HighlightStyle {
                    font_style: Some(gpui::FontStyle::Italic),
                    ..style(0x111111)
                },
            ),
            ("keyword", style(0x222222)),
            ("string", style(0x333333)),
            ("type", style(0x444444)),
            ("variable", style(0x555555)),
        ],
    )
}

#[test]
fn layered_runs_apply_layers_in_order_and_fill_gaps() {
    let theme = test_theme();
    let color = |name| theme.style(Highlight::named(name)).color.unwrap();
    let error = Some(theme.color(color::ERROR));
    let fg = theme.color(color::EDITOR_FOREGROUND);
    let text = "let x = \"é\";";
    let syntax = [
        span(0..3, "keyword"),
        span(4..5, "variable"),
        span(8..12, "string"),
    ];
    let semantic = [span(0..3, "type")];
    let decorations = [HighlightSpan {
        range: 6..11,
        style: theme.decoration(color::ERROR, DecorationStyle::Wavy),
    }];

    let runs = runs_for(text, &[&syntax, &semantic, &decorations], &theme);
    assert_eq!(
        run_styles(&runs),
        [
            (3, color("type"), None),
            (1, fg, None),
            (1, color("variable"), None),
            (1, fg, None),
            (2, fg, error),
            (3, color("string"), error),
            (1, color("string"), None),
            (1, fg, None),
        ]
    );
    assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), text.len());
}

#[test]
fn layered_runs_merge_equal_styles_and_clip_to_the_line() {
    let theme = test_theme();
    let keyword = theme.style(Highlight::named("keyword")).color.unwrap();
    let layer = [
        span(0..2, "keyword"),
        span(2..4, "keyword"),
        span(5..99, "keyword"),
    ];
    assert_eq!(
        run_styles(&runs_for("abcd e", &[&layer], &theme)),
        [
            (4, keyword, None),
            (1, theme.color(color::EDITOR_FOREGROUND), None),
            (1, keyword, None)
        ]
    );
    assert!(runs_for("", &[&layer], &theme).is_empty());
    assert_eq!(
        run_styles(&runs_for("plain", &[], &theme)),
        [(5, theme.color(color::EDITOR_FOREGROUND), None)]
    );

    let comment = [span(0..5, "comment")];
    let runs = runs_for("plain", &[&comment], &theme);
    assert_eq!(runs[0].font.style, gpui::FontStyle::Italic);
}

#[gpui::test]
fn layered_runs_keep_tab_and_rtl_geometry(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text(""));
    let (_, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    let theme = test_theme();
    cx.update(|window, _| {
        for text in ["\tfoo\tbar(baz)\t", "مرحبا بالعالم", "\tשלום world"] {
            let layer: Vec<HighlightSpan> = text
                .char_indices()
                .map(|(start, ch)| span(start..start + ch.len_utf8(), "string"))
                .collect();
            let plain = shape_editor_line(
                window,
                text.into(),
                px(14.),
                &runs_for(text, &[], &theme),
                4,
            );
            let layered = shape_editor_line(
                window,
                text.into(),
                px(14.),
                &runs_for(text, &[&layer], &theme),
                4,
            );
            assert_eq!(layered.width, plain.width, "{text:?}");
            for (index, _) in text.char_indices() {
                assert_eq!(
                    x_for_index_dir(&layered, index, text, 4),
                    x_for_index_dir(&plain, index, text, 4),
                    "{text:?} at {index}"
                );
            }
        }
    });
}

#[gpui::test]
fn overlapping_decorations_underline_with_the_most_severe(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("abcdefgh"));
    let editor = cx.new(|cx| EditorView::new(model.clone(), cx));
    let decoration = |range: std::ops::Range<usize>, token| EditorContribution {
        range: ByteRange {
            start_byte_offset: range.start,
            end_byte_offset: range.end,
        },
        decoration: Some(Decoration::new(token, DecorationStyle::Wavy)),
        gutter: None,
        command: None,
    };
    model.update(cx, |model, cx| {
        model
            .replace_contributions(
                ContributionSource::fixture(),
                &[
                    decoration(0..4, "warning"),
                    decoration(1..3, "warning"),
                    decoration(2..6, "error"),
                ],
                model.revision(),
            )
            .unwrap();
        cx.notify();
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let theme = theme(cx);
        let underline = |key| Some(theme.color(key));
        let fg = theme.color(color::EDITOR_FOREGROUND);
        assert_eq!(
            run_styles(&editor.read(cx).line_runs(model.read(cx), 0, "abcdefgh", cx)),
            [
                (2, fg, underline(color::WARNING)),
                (4, fg, underline(color::ERROR)),
                (2, fg, None),
            ]
        );
    });
}

#[gpui::test]
fn underline_styles_paint_as_separate_layers(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("abcdefgh"));
    let editor = cx.new(|cx| EditorView::new(model.clone(), cx));
    let decoration = |range: std::ops::Range<usize>, key, style| EditorContribution {
        range: ByteRange {
            start_byte_offset: range.start,
            end_byte_offset: range.end,
        },
        decoration: Some(Decoration::new(key, style)),
        gutter: None,
        command: None,
    };
    model.update(cx, |model, cx| {
        model
            .replace_contributions(
                ContributionSource::fixture(),
                &[
                    decoration(0..4, "diff.add", DecorationStyle::Underline),
                    decoration(2..6, "diff.add", DecorationStyle::Wavy),
                    decoration(6..8, "diff.add", DecorationStyle::Background),
                    decoration(0..8, "diff.add", DecorationStyle::Line),
                ],
                model.revision(),
            )
            .unwrap();
        cx.notify();
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let color = Some(theme(cx).color("diff.add"));
        let runs = editor.read(cx).line_runs(model.read(cx), 0, "abcdefgh", cx);
        let underlines = runs
            .iter()
            .map(|run| {
                let underline = run.underline.map(|u| (u.color, u.wavy, u.thickness));
                (run.len, underline)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            underlines,
            [
                (2, Some((color, false, px(1.)))),
                (4, Some((color, true, px(1.5)))),
                (2, None),
            ]
        );
    });
}

#[gpui::test]
fn the_gutter_reserves_its_lanes_beside_the_text(cx: &mut TestAppContext) {
    for show_gutter in [true, false] {
        let model = cx.new(|_| BufferModel::from_text("abc"));
        let (editor, cx) = cx.add_window_view(|_, cx| {
            EditorView::new_with_options(model, 0, EditorRenderingOptions { show_gutter }, cx)
        });
        cx.refresh().unwrap();
        cx.update(|window, cx| {
            editor.update(cx, |view, cx| {
                let bounds = view.interaction_bounds();
                let text = view.text_bounds();
                let gutter = px(view.gutter.width);
                assert_eq!(gutter > px(16.), show_gutter);
                assert_eq!(gutter == px(0.), !show_gutter);
                assert_eq!(text.origin.x - bounds.origin.x, gutter);
                assert_eq!(text.size.width, bounds.size.width - gutter);
                view.cursor_col = 1;
                let x = view.caret_x(view.model.read(cx), window, cx);
                let y = bounds.origin.y + px(5.);
                let model = view.model.read(cx);
                assert_eq!(
                    view.hit_test(model, point(text.origin.x + x, y), window, cx),
                    (0, 1)
                );
                assert_eq!(
                    view.hit_test(model, point(bounds.origin.x, y), window, cx),
                    (0, 0)
                );
            })
        });
    }
}

#[gpui::test]
fn syntax_highlights_paint_beneath_decorations(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("fn ab() {}\nlet c;"));
    let editor = cx.new(|cx| EditorView::new(model.clone(), cx));
    model.update(cx, |model, cx| {
        let highlights = "\"fn\" @keyword\n(function_item name: (identifier) @function)\n";
        let language = SyntaxLanguage::new("rust", highlights, "").unwrap();
        model.set_syntax(
            Some(SyntaxState::new(Arc::new(language), Arc::new(|_| None))),
            cx,
        );
        let warning = EditorContribution {
            range: ByteRange {
                start_byte_offset: 0,
                end_byte_offset: 5,
            },
            decoration: Some(Decoration::new("warning", DecorationStyle::Wavy)),
            gutter: None,
            command: None,
        };
        model
            .replace_contributions(ContributionSource::fixture(), &[warning], model.revision())
            .unwrap();
        cx.notify();
    });
    cx.run_until_parked();

    let runs = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            let view = editor.read(cx);
            let model = model.read(cx);
            view.cache_syntax(model, 0..model.line_count());
            run_styles(&view.line_runs(model, 0, &model.read(model.line_range(0)), cx))
        })
    };
    let (keyword, function, warning, fg) = cx.read(|cx| {
        let theme = theme(cx);
        let color = |name| theme.style(Highlight::named(name)).color.unwrap();
        (
            color("keyword"),
            color("function"),
            Some(theme.color(color::WARNING)),
            theme.color(color::EDITOR_FOREGROUND),
        )
    });
    assert_eq!(
        runs(cx),
        [
            (2, keyword, warning),
            (1, fg, warning),
            (2, function, warning),
            (5, fg, None),
        ]
    );

    // An edit changes the syntax generation, so the cached visible range is
    // not reused for the new text. The anchored warning grows with it.
    model.update(cx, |model, cx| {
        model.replace(3..5, "abcd").unwrap();
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        runs(cx),
        [
            (2, keyword, warning),
            (1, fg, warning),
            (4, function, warning),
            (5, fg, None),
        ]
    );
}

#[gpui::test]
fn caret_geometry_and_painting_share_one_syntax_query(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("fn f() {}\n".repeat(200)));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    model.update(cx, |model, cx| {
        // A reparse that overruns the foreground budget on a slow machine
        // finishes in the background and queries again.
        model.set_syntax_budget(std::time::Duration::from_secs(60));
        let language = SyntaxLanguage::new("rust", "\"fn\" @keyword\n", "").unwrap();
        model.set_syntax(
            Some(SyntaxState::new(Arc::new(language), Arc::new(|_| None))),
            cx,
        );
    });
    cx.run_until_parked();
    let queries = |cx: &mut gpui::VisualTestContext| cx.read(|cx| model.read(cx).syntax_queries());

    // An edit invalidates the cache; caret geometry before the next paint
    // queries the visible lines, and painting reuses that query.
    let before = queries(cx);
    cx.update(|window, cx| {
        model.update(cx, |model, cx| {
            model.replace(0..0, "// ").unwrap();
            cx.notify();
        });
        editor.update(cx, |view, cx| {
            view.caret_x(view.model.read(cx), window, cx);
        })
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    assert_eq!(queries(cx), before + 1);

    // Scrolling by a line stays within the overscan.
    editor.update(cx, |view, cx| {
        view.scroll += view.metrics.line_height;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(queries(cx), before + 1);
}

#[gpui::test]
fn the_caret_blinks_while_focused_and_shows_again_after_typing(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text(""));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    let visible =
        |cx: &mut gpui::VisualTestContext| cx.read(|cx| editor.read(cx).caret_blink.visible);
    let blinking =
        |cx: &mut gpui::VisualTestContext| cx.read(|cx| editor.read(cx).caret_blink.key.is_some());
    cx.update(|window, cx| editor.read(cx).focus.focus(window));
    cx.run_until_parked();
    assert!(visible(cx));
    cx.executor().advance_clock(super::CARET_BLINK_INTERVAL);
    cx.run_until_parked();
    assert!(!visible(cx));

    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.replace_text_in_range(None, "a", window, cx)
        });
    });
    cx.run_until_parked();
    assert!(visible(cx));
    cx.executor().advance_clock(super::CARET_BLINK_INTERVAL);
    cx.run_until_parked();
    assert!(!visible(cx));
    cx.executor().advance_clock(super::CARET_BLINK_INTERVAL);
    cx.run_until_parked();
    assert!(visible(cx));

    cx.update(|window, _| window.blur());
    cx.run_until_parked();
    assert!(!blinking(cx));
}

type Position = (usize, usize);

/// Run `command` over `text` with the selection from `anchor` to `caret`,
/// check the text and selection it leaves, then check that undo restores the
/// original and redo the result.
pub(super) fn check(
    cx: &mut TestAppContext,
    insert_spaces: bool,
    (text, anchor, caret): (&str, Position, Position),
    command: &str,
    (expected, expected_anchor, expected_caret): (&str, Position, Position),
) {
    cx.update(|cx| {
        let mut settings = ApplicationSettings::default();
        settings
            .0
            .set(
                EDITOR_INSERT_SPACES,
                insert_spaces.into(),
                SettingsScope::default(),
            )
            .unwrap();
        cx.set_global(settings);
    });
    let model = cx.new(|_| BufferModel::from_text(text));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    let selection = |view: &EditorView| {
        let caret = (view.cursor_line, view.cursor_col);
        let anchor = if view.has_selection {
            (view.anchor_line, view.anchor_col)
        } else {
            caret
        };
        (anchor, caret)
    };
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            (view.anchor_line, view.anchor_col) = anchor;
            (view.cursor_line, view.cursor_col) = caret;
            view.has_selection = anchor != caret;
            view.sync_view_position(cx);
            let context = format!("{command} on {text:?} from {anchor:?} to {caret:?}");

            assert!(view.execute_editing_command(command, window, cx));
            assert_eq!(view.text(cx), expected, "{context}");
            assert_eq!(
                selection(view),
                (expected_anchor, expected_caret),
                "{context}"
            );

            view.execute_editing_command("editor.undo", window, cx);
            assert_eq!(view.text(cx), text, "undo of {context}");
            if expected != text {
                assert_eq!(selection(view), (anchor, caret), "undo of {context}");
                view.execute_editing_command("editor.redo", window, cx);
                assert_eq!(view.text(cx), expected, "redo of {context}");
                assert_eq!(
                    selection(view),
                    (expected_anchor, expected_caret),
                    "redo of {context}"
                );
            }
        })
    });
}

mod brackets;
mod comments;
mod decorations;
mod gutter;
mod indentation;
mod lines;
mod wrap;

fn press(
    view: &mut EditorView,
    at: gpui::Point<gpui::Pixels>,
    clicks: usize,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<EditorView>,
) {
    view.on_mouse_down(
        &MouseDownEvent {
            position: at,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: clicks,
            first_mouse: false,
        },
        window,
        cx,
    );
}

fn drag(
    view: &mut EditorView,
    to: gpui::Point<gpui::Pixels>,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<EditorView>,
) {
    view.on_mouse_move(
        &MouseMoveEvent {
            position: to,
            modifiers: Modifiers::default(),
            pressed_button: Some(MouseButton::Left),
        },
        window,
        cx,
    );
}

fn selection(view: &EditorView) -> (Position, Position) {
    (
        (view.anchor_line, view.anchor_col),
        (view.cursor_line, view.cursor_col),
    )
}

#[gpui::test]
fn dragging_after_a_double_or_triple_click_extends_by_words_or_lines(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("alpha beta gamma\nsecond line\nthird"));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            let [beta, gamma, alpha, inside, second, third, first] =
                [(0, 7), (0, 13), (0, 2), (0, 8), (1, 3), (2, 1), (0, 4)].map(|(line, col)| {
                    view.cursor_line = line;
                    view.cursor_col = col;
                    point(
                        view.text_bounds().origin.x + view.caret_x(view.model.read(cx), window, cx),
                        view.bounds.origin.y + px(5. + view.metrics.line_height * line as f32),
                    )
                });

            // Words: forward to the end of the hovered word, back to the start
            // of one before, keeping "beta" selected.
            press(view, beta, 2, window, cx);
            assert_eq!(selection(view), ((0, 6), (0, 10)));
            drag(view, gamma, window, cx);
            assert_eq!(selection(view), ((0, 6), (0, 16)));
            drag(view, alpha, window, cx);
            assert_eq!(selection(view), ((0, 10), (0, 0)));
            drag(view, inside, window, cx);
            assert_eq!(selection(view), ((0, 6), (0, 10)));

            // Lines: whole lines in both directions from the clicked one.
            press(view, second, 3, window, cx);
            assert_eq!(selection(view), ((1, 0), (2, 0)));
            drag(view, third, window, cx);
            assert_eq!(selection(view), ((1, 0), (2, 5)));
            drag(view, first, window, cx);
            assert_eq!(selection(view), ((2, 0), (0, 0)));

            // A single click drags by characters again.
            press(view, beta, 1, window, cx);
            drag(view, gamma, window, cx);
            assert_eq!(selection(view), ((0, 7), (0, 13)));
        })
    });
}

#[test]
fn words_split_at_vs_code_separators_and_keep_unicode_segments() {
    fn segments(text: &str) -> Vec<&str> {
        super::word_segments(text)
            .into_iter()
            .map(|(start, end, _)| &text[start..end])
            .collect()
    }
    assert_eq!(segments("some.thing"), ["some", ".", "thing"]);
    assert_eq!(segments("foo::bar()"), ["foo", "::", "bar", "()"]);
    assert_eq!(
        segments("don't 3.14"),
        ["don", "'", "t", " ", "3", ".", "14"]
    );
    assert_eq!(
        segments("snake_case  x-y"),
        ["snake_case", "  ", "x", "-", "y"]
    );
    assert_eq!(segments("日本語 👩‍💻é"), ["日", "本", "語", " ", "👩‍💻", "é"]);
    assert!(segments("").is_empty());
}

#[gpui::test]
fn double_click_and_word_movement_stop_at_separators(cx: &mut TestAppContext) {
    let model = cx.new(|_| BufferModel::from_text("a.some_thing(x)  end"));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            let [inside, paren] = [5, 12].map(|col| {
                view.cursor_col = col;
                point(
                    view.text_bounds().origin.x + view.caret_x(view.model.read(cx), window, cx),
                    view.bounds.origin.y + px(5.),
                )
            });
            press(view, inside, 2, window, cx);
            assert_eq!(selection(view), ((0, 2), (0, 12)));
            press(view, paren, 2, window, cx);
            assert_eq!(selection(view), ((0, 12), (0, 13)));

            view.execute_editing_command("editor.move-document-start", window, cx);
            let mut stops = Vec::new();
            for _ in 0..7 {
                view.execute_editing_command("editor.move-word-right", window, cx);
                stops.push(view.cursor_col);
            }
            assert_eq!(stops, [1, 2, 12, 13, 14, 15, 20]);
            let mut stops = Vec::new();
            for _ in 0..7 {
                view.execute_editing_command("editor.move-word-left", window, cx);
                stops.push(view.cursor_col);
            }
            assert_eq!(stops, [17, 14, 13, 12, 2, 1, 0]);
        })
    });
}

#[gpui::test]
fn caret_shape_is_the_latest_lifecycles_and_reverts_when_cleared(cx: &mut TestAppContext) {
    use crate::host::lifecycle::ExtensionKey;
    let first = ExtensionKey::new(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    let second = ExtensionKey::new(ExtensionId::new(2), ExtensionLifecycleId::new(1));
    let model = cx.new(|_| BufferModel::from_text("abc".to_string()));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    editor.update(cx, |view, cx| {
        assert_eq!(view.caret_shape(), CaretShape::Bar);
        view.set_caret_shape(first, Some(CaretShape::Block), cx);
        view.set_caret_shape(second, Some(CaretShape::Underline), cx);
        assert_eq!(view.caret_shape(), CaretShape::Underline);
        view.set_caret_shape(second, None, cx);
        assert_eq!(view.caret_shape(), CaretShape::Block);
        view.set_caret_shape(second, Some(CaretShape::Bar), cx);
        view.remove_owner_state(second, cx);
        assert_eq!(view.caret_shape(), CaretShape::Block);
        view.remove_owner_state(first, cx);
        assert_eq!(view.caret_shape(), CaretShape::Bar);
    });
}
