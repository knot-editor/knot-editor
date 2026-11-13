use gpui::{
    AppContext, EntityInputHandler, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    ScrollDelta, ScrollEvent, TestAppContext, point, px,
};

use super::{
    Buffer, BufferModel, ContributionSource, Editor, EditorRenderingOptions, EditorView,
    EditorViewState, Reveal,
    layout::{HighlightSpan, runs_for, shape_editor_line, x_for_index_dir},
};
use crate::app::convert::ThemePaint as _;
use crate::app::style::{Color, FontStyle, HighlightStyle};
use std::sync::Arc;

use crate::app::{
    languages::{self, LanguageDefinition, LanguageOwner},
    settings::{
        ApplicationSettings, EDITOR_TAB_SIZE, FONT_FAMILY, FONT_FEATURES, FONT_SIZE, SettingsScope,
    },
    syntax::{SyntaxLanguage, SyntaxState},
    theme::{Appearance, Highlight, Theme, color, native_color_definitions, theme},
};
use knot_protocol::{
    ByteRange, Decoration, DecorationStyle, EditorContribution, ExtensionId, ExtensionLifecycleId,
    LanguageInfo,
};

#[gpui::test]
fn font_size_sets_line_height_visible_lines_hit_testing_and_gutter(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let text = (0..100)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(text)));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    let measure = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            editor.update(cx, |view, cx| {
                let state = view.editor.read(cx);
                let origin = view.text_bounds().origin;
                let hit = view.hit_test(
                    state,
                    point(
                        origin.x + px(1.),
                        origin.y + px(3.5 * view.metrics.line_height),
                    ),
                    window,
                    cx,
                );
                (view.metrics, state.viewport(), hit, view.gutter.width)
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
    let count = |(_, rows): (usize, usize)| rows;
    assert_eq!(count(large_lines), count(small_lines) / 2);
    assert!(large_gutter > small_gutter);
}

#[gpui::test]
fn a_font_size_change_keeps_the_top_line(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let text = (0..100)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(text)));
    let view = cx.new(|cx| EditorView::new(model, cx));
    let editor = cx.read(|cx| view.read(cx).editor().clone());
    editor.update(cx, |editor, _| {
        editor.set_viewport_rows_for_test(10.);
        editor.scroll_to_for_test(30.);
    });
    cx.update(|cx| {
        cx.global_mut::<ApplicationSettings>()
            .0
            .set(FONT_SIZE, 21.into(), SettingsScope::default())
            .unwrap();
    });
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(view.read(cx).metrics.line_height, 30.);
        assert_eq!(editor.read(cx).scroll_top(), 30.);
    });
}

#[gpui::test]
fn font_settings_reach_the_editor_font(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("a -> b")));
    let view = cx.new(|cx| EditorView::new(model, cx));
    cx.read(|cx| assert_eq!(view.read(cx).font.family.as_ref(), "Menlo"));
    cx.update(|cx| {
        let store = &mut cx.global_mut::<ApplicationSettings>().0;
        store
            .set(FONT_FAMILY, "Monaco".into(), SettingsScope::default())
            .unwrap();
        store
            .set(
                FONT_FEATURES,
                serde_json::json!(["ss01"]),
                SettingsScope {
                    view: Some("editor".into()),
                    ..Default::default()
                },
            )
            .unwrap();
    });
    cx.run_until_parked();
    cx.read(|cx| {
        let font = &view.read(cx).font;
        assert_eq!(font.family.as_ref(), "Monaco");
        assert_eq!(font.features.tag_value_list(), [("ss01".to_owned(), 1)]);
    });
}

#[gpui::test]
fn tabs_hit_testing_and_horizontal_reveal_share_geometry(cx: &mut TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(format!("\tx{}", "z".repeat(300)))));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    let state = cx.read(|cx| editor.read(cx).editor().clone());
    state.update(cx, |state, cx| state.place_for_test((0, 1), (0, 1), cx));
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            let state = view.editor.read(cx);
            let tab_x = view.caret_x(state, window, cx);
            assert!(f32::from(tab_x) > 20.);
            assert_eq!(
                view.hit_test(
                    state,
                    point(
                        view.text_bounds().origin.x + tab_x,
                        view.bounds.origin.y + px(5.)
                    ),
                    window,
                    cx
                ),
                (0, 1)
            );
        })
    });
    state.update(cx, |state, cx| {
        state.execute_editing_command("editor.move-line-end", cx)
    });
    cx.refresh().unwrap();
    cx.read(|cx| assert!(state.read(cx).scroll_x() > 0.));
    state.update(cx, |state, cx| {
        state.execute_editing_command("editor.move-line-start", cx)
    });
    cx.refresh().unwrap();
    cx.read(|cx| assert_eq!(state.read(cx).scroll_x(), 0.));
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
                folds: String::new(),
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
        Buffer::new(model)
    });
    let plain_model = cx.new(|_| Buffer::new(BufferModel::from_text(text)));
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
                    &view.shape_line(0, text, window, cx),
                    col,
                    text,
                    view.editor.read(cx).tab_size(model, cx),
                ))
            };
            let width = column(2) - column(1);
            let shaped = view.shape_line(0, text, window, cx).width;
            (
                (column(1) / width).round() as usize,
                (f32::from(shaped) / width).round() as usize,
            )
        })
    };
    let caret_columns = |cx: &mut gpui::VisualTestContext, editor: &gpui::Entity<EditorView>| {
        cx.update(|window, cx| {
            editor.update(cx, |view, cx| {
                let a = view.caret_x_at(1, window, cx);
                let b = view.caret_x_at(2, window, cx);
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
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("alpha beta\nnext")));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            let position = |view: &mut EditorView, col, cx: &mut gpui::App| {
                point(
                    view.text_bounds().origin.x + view.caret_x_at(col, window, cx),
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
            assert!(!view.editor.read(cx).has_selection());
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
            assert_eq!(view.editor.read(cx).selected_byte_range(cx), Some(1..3));
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
            assert_eq!(view.editor.read(cx).selected_byte_range(cx), Some(0..5));
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
            assert_eq!(view.editor.read(cx).selected_byte_range(cx), Some(0..11));
        })
    });
}

#[gpui::test]
fn editing_a_large_buffer_never_reads_the_whole_text(cx: &mut TestAppContext) {
    let text = (0..100_000)
        .map(|line| format!("line {line} with some words"))
        .collect::<Vec<_>>()
        .join("\n");
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(text)));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    let (other, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            view.focus.focus(window, cx);
            let state = view.editor.clone();
            state.update(cx, |state, cx| {
                state.restore_view_state(
                    EditorViewState {
                        anchor: 50_000,
                        caret: 50_000,
                        top: 49_000,
                        scroll_x: 0.,
                        folds: Vec::new(),
                    },
                    cx,
                )
            });
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
                    state.update(cx, |state, cx| state.execute_editing_command(command, cx)),
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
    cx.simulate_event(ScrollEvent {
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
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(text)));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    cx.refresh().unwrap();
    cx.simulate_event(ScrollEvent {
        position: cx.read(|cx| editor.read(cx).interaction_bounds().center()),
        delta: ScrollDelta::Pixels(point(px(-100_000.), px(0.))),
        ..Default::default()
    });
    let long_width = cx.read(|cx| {
        let view = editor.read(cx);
        let limit = view.max_line_width + 2. - f32::from(view.text_bounds().size.width);
        assert!(limit > 0.);
        assert_eq!(view.editor.read(cx).scroll_x(), limit);
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
        Buffer::new(model)
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
            assert_eq!(editor.editor.read(cx).selected_byte_range(cx), None);
        });
    });
}

#[gpui::test]
fn read_only_text_keeps_normal_selection_copy_and_scroll_behavior(cx: &mut TestAppContext) {
    let text = (0..100)
        .map(|line| format!("result {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let model = cx.new(|_| Buffer::new(BufferModel::from_read_only_text(text.clone())));
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
    cx.simulate_event(ScrollEvent {
        position: cx.read(|cx| editor.read(cx).interaction_bounds().center()),
        delta: ScrollDelta::Lines(point(0., -5.)),
        ..Default::default()
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.model().read(cx).text(), text);
        assert!(editor.editor.read(cx).scroll_top() > 0.);
    });
}

#[gpui::test]
fn views_share_model_state_and_keep_presentation_state_independent(cx: &mut TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("one\ntwo\nthree")));
    let first = cx.new(|cx| EditorView::new(model.clone(), cx));
    let second = cx.new(|cx| {
        EditorView::new_with_options(
            model.clone(),
            1,
            EditorRenderingOptions { show_gutter: false },
            cx,
        )
    });

    let views = (first.clone(), second.clone());
    let first = cx.read(|cx| first.read(cx).editor().clone());
    let second = cx.read(|cx| second.read(cx).editor().clone());
    first.update(cx, |view, cx| {
        view.scroll_to_for_test(1.);
        view.place_for_test((0, 1), (1, 2), cx);
    });
    second.update(cx, |view, cx| {
        view.scroll_to_for_test(2.);
        view.place_for_test((2, 3), (2, 3), cx);
    });

    model.update(cx, |model, cx| {
        assert!(model.replace(0..0, "shared\n").unwrap());
        let revision = model.revision();
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
                revision,
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
        assert_eq!(first.decorations().len(), 1);
        assert_eq!(first.decorations().len(), second.decorations().len());
        assert_eq!(first.gutter_markers().len(), second.gutter_markers().len());

        assert_eq!((first.caret().0, first.caret().1), (2, 2));
        assert_eq!((second.caret().0, second.caret().1), (3, 3));
        assert!(first.has_selection());
        assert!(!second.has_selection());
        assert_eq!(first.scroll_top(), 1.);
        assert_eq!(second.scroll_top(), 2.);
        let (first, second) = (views.0.read(cx), views.1.read(cx));
        assert!(first.rendering.show_gutter);
        assert!(!second.rendering.show_gutter);
        assert_ne!(first.focus, second.focus);
    });
}

#[gpui::test]
fn selection_tracks_a_deletion_from_another_view(cx: &mut TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("0123456789")));
    let first = cx.new(|cx| Editor::new(model.clone(), crate::app::EDITOR_KEY_CONTEXT, cx));
    let second = cx.new(|cx| Editor::new(model.clone(), crate::app::EDITOR_KEY_CONTEXT, cx));

    second.update(cx, |view, cx| view.place_for_test((0, 4), (0, 7), cx));
    model.update(cx, |model, cx| {
        assert!(model.replace(1..2, "").unwrap());
        cx.notify();
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let first = first.read(cx);
        let second = second.read(cx);
        assert_eq!(first.text(cx), "023456789");
        assert_eq!((second.anchor().0, second.anchor().1), (0, 3));
        assert_eq!((second.caret().0, second.caret().1), (0, 6));
        assert!(second.has_selection());
    });
}

/// `range` styled as `test_theme` styles capture `name`.
/// The text runs `view` paints `text`, line `line`'s content, with.
fn line_runs(
    view: &gpui::Entity<EditorView>,
    line: usize,
    text: &str,
    cx: &gpui::App,
) -> Vec<gpui::TextRun> {
    let view = view.read(cx);
    view.line_runs(view.editor.read(cx), line, text, cx)
}

fn span(range: std::ops::Range<usize>, name: &str) -> HighlightSpan {
    HighlightSpan {
        range,
        style: test_theme().highlight_style(Highlight::named(name)),
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
    let style = |hex: u32| HighlightStyle::color(Color::rgb(hex));
    Theme::new(
        Appearance::Dark,
        &std::collections::HashMap::new(),
        &native_color_definitions(),
        &[
            (
                "comment",
                HighlightStyle {
                    font_style: Some(FontStyle::Italic),
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
    let color = |name| theme.highlight_style(Highlight::named(name)).color.unwrap();
    let error = Some(theme.paint_color(color::ERROR));
    let fg = theme.paint_color(color::EDITOR_FOREGROUND);
    let text = "let x = \"é\";";
    let syntax = [
        span(0..3, "keyword"),
        span(4..5, "variable"),
        span(8..12, "string"),
    ];
    let semantic = [span(0..3, "type")];
    let decorations = [HighlightSpan {
        range: 6..11,
        style: theme.decoration_style(color::ERROR, DecorationStyle::Wavy),
    }];

    let runs = runs_for(
        text,
        &[&syntax, &semantic, &decorations],
        &theme,
        &test_font(),
    );
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
    let keyword = theme
        .highlight_style(Highlight::named("keyword"))
        .color
        .unwrap();
    let layer = [
        span(0..2, "keyword"),
        span(2..4, "keyword"),
        span(5..99, "keyword"),
    ];
    assert_eq!(
        run_styles(&runs_for("abcd e", &[&layer], &theme, &test_font())),
        [
            (4, keyword, None),
            (1, theme.paint_color(color::EDITOR_FOREGROUND), None),
            (1, keyword, None)
        ]
    );
    assert!(runs_for("", &[&layer], &theme, &test_font()).is_empty());
    assert_eq!(
        run_styles(&runs_for("plain", &[], &theme, &test_font())),
        [(5, theme.paint_color(color::EDITOR_FOREGROUND), None)]
    );

    let comment = [span(0..5, "comment")];
    let runs = runs_for("plain", &[&comment], &theme, &test_font());
    assert_eq!(runs[0].font.style, gpui::FontStyle::Italic);
}

#[gpui::test]
fn layered_runs_keep_tab_and_rtl_geometry(cx: &mut TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("")));
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
                &runs_for(text, &[], &theme, &test_font()),
                4,
            );
            let layered = shape_editor_line(
                window,
                text.into(),
                px(14.),
                &runs_for(text, &[&layer], &theme, &test_font()),
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
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("abcdefgh")));
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
        let revision = model.revision();
        model
            .replace_contributions(
                ContributionSource::fixture(),
                &[
                    decoration(0..4, "warning"),
                    decoration(1..3, "warning"),
                    decoration(2..6, "error"),
                ],
                revision,
            )
            .unwrap();
        cx.notify();
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let theme = theme(cx);
        let underline = |key| Some(theme.paint_color(key));
        let fg = theme.paint_color(color::EDITOR_FOREGROUND);
        assert_eq!(
            run_styles(&line_runs(&editor, 0, "abcdefgh", cx)),
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
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("abcdefgh")));
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
        let revision = model.revision();
        model
            .replace_contributions(
                ContributionSource::fixture(),
                &[
                    decoration(0..4, "diff.add", DecorationStyle::Underline),
                    decoration(2..6, "diff.add", DecorationStyle::Wavy),
                    decoration(6..8, "diff.add", DecorationStyle::Background),
                    decoration(0..8, "diff.add", DecorationStyle::Line),
                ],
                revision,
            )
            .unwrap();
        cx.notify();
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let color = Some(theme(cx).paint_color("diff.add"));
        let runs = line_runs(&editor, 0, "abcdefgh", cx);
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
        let model = cx.new(|_| Buffer::new(BufferModel::from_text("abc")));
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
                let x = view.caret_x_at(1, window, cx);
                let y = bounds.origin.y + px(5.);
                let state = view.editor.read(cx);
                assert_eq!(
                    view.hit_test(state, point(text.origin.x + x, y), window, cx),
                    (0, 1)
                );
                assert_eq!(
                    view.hit_test(state, point(bounds.origin.x, y), window, cx),
                    (0, 0)
                );
            })
        });
    }
}

#[gpui::test]
fn syntax_highlights_paint_beneath_decorations(cx: &mut TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("fn ab() {}\nlet c;")));
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
        let revision = model.revision();
        model
            .replace_contributions(ContributionSource::fixture(), &[warning], revision)
            .unwrap();
        cx.notify();
    });
    cx.run_until_parked();

    let runs = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            let view = editor.read(cx);
            let model = model.read(cx);
            view.cache_syntax(model, 0..model.line_count());
            run_styles(&view.line_runs(
                view.editor.read(cx),
                0,
                &model.read(model.line_range(0)),
                cx,
            ))
        })
    };
    let (keyword, function, warning, fg) = cx.read(|cx| {
        let theme = theme(cx);
        let color = |name| theme.highlight_style(Highlight::named(name)).color.unwrap();
        (
            color("keyword"),
            color("function"),
            Some(theme.paint_color(color::WARNING)),
            theme.paint_color(color::EDITOR_FOREGROUND),
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
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("fn f() {}\n".repeat(200))));
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
        let view = editor.read(cx);
        view.caret_x(view.editor.read(cx), window, cx);
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    assert_eq!(queries(cx), before + 1);

    // Scrolling by a line stays within the overscan.
    let state = cx.read(|cx| editor.read(cx).editor().clone());
    state.update(cx, |state, cx| {
        let top = state.scroll_top();
        state.scroll_to_for_test(top + 1.);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(queries(cx), before + 1);
}

#[gpui::test]
fn the_caret_blinks_while_focused_and_shows_again_after_typing(cx: &mut TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("")));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model.clone(), cx));
    let visible =
        |cx: &mut gpui::VisualTestContext| cx.read(|cx| editor.read(cx).caret_blink.visible);
    let blinking =
        |cx: &mut gpui::VisualTestContext| cx.read(|cx| editor.read(cx).caret_blink.key.is_some());
    cx.update(|window, cx| {
        let focus = editor.read(cx).focus.clone();
        focus.focus(window, cx);
    });
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

    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    assert!(!blinking(cx));
}

type Position = (usize, usize);

mod gutter;
mod indentation;
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

fn selection(view: &EditorView, cx: &gpui::App) -> (Position, Position) {
    let editor = view.editor.read(cx);
    (
        (editor.anchor().0, editor.anchor().1),
        (editor.caret().0, editor.caret().1),
    )
}

#[gpui::test]
fn dragging_after_a_double_or_triple_click_extends_by_words_or_lines(cx: &mut TestAppContext) {
    let model = cx.new(|_| {
        Buffer::new(BufferModel::from_text(
            "alpha beta gamma\nsecond line\nthird",
        ))
    });
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            let [beta, gamma, alpha, inside, second, third, first] =
                [(0, 7), (0, 13), (0, 2), (0, 8), (1, 3), (2, 1), (0, 4)].map(|(line, col)| {
                    view.editor.update(cx, |editor, cx| {
                        editor.place_for_test((line, col), (line, col), cx)
                    });
                    point(
                        view.text_bounds().origin.x
                            + view.caret_x(view.editor.read(cx), window, cx),
                        view.bounds.origin.y + px(5. + view.metrics.line_height * line as f32),
                    )
                });

            // Words: forward to the end of the hovered word, back to the start
            // of one before, keeping "beta" selected.
            press(view, beta, 2, window, cx);
            assert_eq!(selection(view, cx), ((0, 6), (0, 10)));
            drag(view, gamma, window, cx);
            assert_eq!(selection(view, cx), ((0, 6), (0, 16)));
            drag(view, alpha, window, cx);
            assert_eq!(selection(view, cx), ((0, 10), (0, 0)));
            drag(view, inside, window, cx);
            assert_eq!(selection(view, cx), ((0, 6), (0, 10)));

            // Lines: whole lines in both directions from the clicked one.
            press(view, second, 3, window, cx);
            assert_eq!(selection(view, cx), ((1, 0), (2, 0)));
            drag(view, third, window, cx);
            assert_eq!(selection(view, cx), ((1, 0), (2, 5)));
            drag(view, first, window, cx);
            assert_eq!(selection(view, cx), ((2, 0), (0, 0)));

            // A single click drags by characters again.
            press(view, beta, 1, window, cx);
            drag(view, gamma, window, cx);
            assert_eq!(selection(view, cx), ((0, 7), (0, 13)));
        })
    });
}

#[gpui::test]
fn double_click_and_word_movement_stop_at_separators(cx: &mut TestAppContext) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text("a.some_thing(x)  end")));
    let (editor, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |view, cx| {
            let [inside, paren] = [5, 12].map(|col| {
                point(
                    view.text_bounds().origin.x + view.caret_x_at(col, window, cx),
                    view.bounds.origin.y + px(5.),
                )
            });
            press(view, inside, 2, window, cx);
            assert_eq!(selection(view, cx), ((0, 2), (0, 12)));
            press(view, paren, 2, window, cx);
            assert_eq!(selection(view, cx), ((0, 12), (0, 13)));

            let editor = view.editor.clone();
            let mut run = |command| {
                editor.update(cx, |editor, cx| {
                    editor.execute_editing_command(command, cx);
                    editor.caret().1
                })
            };
            run("editor.move-document-start");
            let stops = (0..7)
                .map(|_| run("editor.move-word-right"))
                .collect::<Vec<_>>();
            assert_eq!(stops, [1, 2, 12, 13, 14, 15, 20]);
            let stops = (0..7)
                .map(|_| run("editor.move-word-left"))
                .collect::<Vec<_>>();
            assert_eq!(stops, [17, 14, 13, 12, 2, 1, 0]);
        })
    });
}

#[gpui::test]
fn reset_state_starts_over_on_the_same_buffer(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(ApplicationSettings::default()));
    let text = (0..20)
        .map(|line| format!("line {line}\n"))
        .collect::<String>()
        .trim_end()
        .to_owned();
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(text)));
    let editor = cx.new(|cx| Editor::new(model.clone(), crate::app::EDITOR_KEY_CONTEXT, cx));
    let owner = knot_protocol::ExtensionKey::new(ExtensionId::new(1), ExtensionLifecycleId::new(1));
    editor.update(cx, |editor, cx| {
        editor.set_key_context(owner, "vim.normal", true);
        editor.lay_out_for_test(5., 100, cx);
        assert!(editor.fold(2..5, cx));
        editor.set_selection(10, 15, Reveal::Nearest, cx);
        editor.scroll_by_rows(5, cx);
        assert_eq!(editor.folds().len(), 1);
        assert_ne!(editor.viewport().0, 0);
    });

    editor.update(cx, |editor, cx| editor.reset_state(cx));
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert!(editor.folds().is_empty(), "folds do not survive a reset");
        assert_eq!(editor.selection(cx), (0, 0), "selection starts over");
        assert_eq!(editor.viewport().0, 0, "scroll starts over");
        assert_eq!(
            editor.key_contexts(),
            ["vim.normal"],
            "lifecycle presentation stays with the extension"
        );
        assert_eq!(editor.model(), &model, "the buffer entity is kept");
    });
}

fn test_font() -> gpui::Font {
    crate::app::typography::font(&Default::default())
}
