//! Zone sizing and placement against an editor's scrolling, folding, and
//! width.

use gpui::{AppContext, TestAppContext, px, size};

use knot_model::editor::zones::ZoneSpec;

use super::super::zones::{ZoneContentSpec, ZoneFrame};
use super::super::{Buffer, BufferModel, Editor, EditorView};
use super::blocks::owner;
use crate::app::view_layout::{Arrangement, ContentSpec, GroupSpec, LayoutSize, MemberSpec};

const LINE_HEIGHT: f32 = 22.;

fn content(shown: gpui::Entity<Buffer>, line: usize) -> ZoneContentSpec {
    ZoneContentSpec {
        spec: ZoneSpec { line, focus: false },
        toolbar: Vec::new(),
        layout: GroupSpec {
            arrangement: Arrangement::Column,
            members: vec![MemberSpec {
                key: None,
                size: LayoutSize::default(),
                focused: false,
                title: None,
                limit: Some(100),
                resize: None,
                content: ContentSpec::Editor {
                    buffer: shown,
                    document: None,
                    placeholder: None,
                    gutter: false,
                    rulers: None,
                },
            }],
        },
    }
}

fn lines(count: usize, prefix: &str) -> String {
    (0..count)
        .map(|n| format!("{prefix}{n}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// An editor of `lines` lines in a 400 by 300 window, with a zone above
/// `line` that holds an editor of `shown` lines.
fn open(
    count: usize,
    line: usize,
    shown: usize,
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<EditorView>,
    gpui::Entity<Editor>,
    gpui::Entity<Buffer>,
    &mut gpui::VisualTestContext,
) {
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(lines(count, "line "))));
    let (view, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.simulate_resize(size(px(400.), px(300.)));
    cx.run_until_parked();
    let shown = cx.new(|_| Buffer::new(BufferModel::from_text(lines(shown, "z"))));
    let state = cx.read(|cx| view.read(cx).editor().clone());
    state.update(cx, |state, cx| {
        state.open_zone(owner(), "z", "window", content(shown.clone(), line), cx);
    });
    settle(cx);
    (view, state, shown, cx)
}

/// Draw twice: a frame measures the zones against the viewport the frame
/// before laid out, and repaints once it changed.
fn settle(cx: &mut gpui::VisualTestContext) {
    for _ in 0..2 {
        cx.refresh().unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
}

/// The top, left, width, and height of each zone frame.
fn frames(
    view: &gpui::Entity<EditorView>,
    cx: &gpui::VisualTestContext,
) -> Vec<(f32, f32, f32, f32)> {
    cx.read(|cx| {
        view.read(cx)
            .zone_frames(cx)
            .iter()
            .map(|frame: &ZoneFrame| (frame.top, frame.left, frame.width, frame.height))
            .collect()
    })
}

fn row_count(state: &gpui::Entity<Editor>, cx: &gpui::VisualTestContext) -> usize {
    cx.read(|cx| state.read(cx).wrap().row_count())
}

#[gpui::test]
fn a_zone_takes_the_whole_rows_its_content_needs(cx: &mut TestAppContext) {
    let (view, state, _, cx) = open(10, 2, 3, cx);
    // Three lines and the border take just over three rows.
    assert_eq!(row_count(&state, cx), 10 + 4);
    let [(top, left, width, height)] = frames(&view, cx)[..] else {
        panic!("one zone");
    };
    assert_eq!(top, 2. * LINE_HEIGHT);
    assert_eq!(height, 4. * LINE_HEIGHT);
    cx.read(|cx| {
        let view = view.read(cx);
        assert_eq!(left, view.gutter.width);
        assert_eq!(width, f32::from(view.text_bounds().size.width));
    });
}

#[gpui::test]
fn a_zone_below_the_last_line_takes_the_rows_after_the_text(cx: &mut TestAppContext) {
    let (view, _, _, cx) = open(4, 4, 1, cx);
    let [(top, _, _, height)] = frames(&view, cx)[..] else {
        panic!("one zone");
    };
    assert_eq!(top, 4. * LINE_HEIGHT);
    assert_eq!(height, 2. * LINE_HEIGHT);
}

#[gpui::test]
fn measuring_again_follows_a_new_layout_and_a_change_of_content(cx: &mut TestAppContext) {
    let (view, state, shown, cx) = open(10, 2, 3, cx);
    assert_eq!(row_count(&state, cx), 14);
    // The same keyed content, replaced with a taller layout.
    let taller = cx.new(|_| Buffer::new(BufferModel::from_text(lines(6, "t"))));
    state.update(cx, |state, cx| {
        state.open_zone(owner(), "z", "window", content(taller, 2), cx);
    });
    settle(cx);
    assert_eq!(row_count(&state, cx), 10 + 7);
    assert_eq!(frames(&view, cx)[0].3, 7. * LINE_HEIGHT);
    // Back to the first buffer, then its text grows.
    state.update(cx, |state, cx| {
        state.open_zone(owner(), "z", "window", content(shown.clone(), 2), cx);
    });
    settle(cx);
    assert_eq!(row_count(&state, cx), 14);
    shown.update(cx, |shown, cx| {
        shown.replace(0..0, "a\nb\n").unwrap();
        cx.notify();
    });
    settle(cx);
    assert_eq!(row_count(&state, cx), 10 + 6);
    assert_eq!(frames(&view, cx)[0].3, 6. * LINE_HEIGHT);
}

#[gpui::test]
fn a_zone_is_never_taller_than_the_viewport(cx: &mut TestAppContext) {
    let (view, state, _, cx) = open(30, 2, 60, cx);
    let viewport_rows = cx.read(|cx| state.read(cx).viewport_rows().floor() as usize);
    assert_eq!(viewport_rows, 13);
    assert_eq!(row_count(&state, cx), 30 + viewport_rows);
    assert_eq!(frames(&view, cx)[0].3, viewport_rows as f32 * LINE_HEIGHT);
}

#[gpui::test]
fn a_resize_of_the_viewport_clamps_and_releases_the_rows(cx: &mut TestAppContext) {
    let (view, state, _, cx) = open(30, 2, 12, cx);
    assert_eq!(row_count(&state, cx), 30 + 13);
    cx.simulate_resize(size(px(400.), px(110.)));
    settle(cx);
    assert_eq!(row_count(&state, cx), 30 + 5);
    cx.simulate_resize(size(px(400.), px(300.)));
    settle(cx);
    assert_eq!(row_count(&state, cx), 30 + 13);
    assert_eq!(frames(&view, cx)[0].3, 13. * LINE_HEIGHT);
}

#[gpui::test]
fn a_zone_scrolls_with_the_text_and_stays_pinned_horizontally(cx: &mut TestAppContext) {
    let (view, state, _, cx) = open(60, 4, 2, cx);
    let lh = LINE_HEIGHT;
    let [(top, left, width, _)] = frames(&view, cx)[..] else {
        panic!("one zone");
    };
    assert_eq!(top, 4. * lh);
    state.update(cx, |state, _| state.scroll_to_for_test(2.));
    assert_eq!(frames(&view, cx)[0].0, 2. * lh);
    // Half way past the top row, the frame is still partly shown.
    state.update(cx, |state, _| state.scroll_to_for_test(5.5));
    assert_eq!(frames(&view, cx)[0].0, -1.5 * lh);
    state.update(cx, |state, _| state.scroll_to_for_test(8.));
    assert!(frames(&view, cx).is_empty());
    // Horizontal scroll moves the text, not the zone.
    state.update(cx, |state, cx| {
        state.scroll_to_for_test(0.);
        state.scroll_by(0., 50., 100., cx);
    });
    let [(_, scrolled_left, scrolled_width, _)] = frames(&view, cx)[..] else {
        panic!("one zone");
    };
    assert_eq!((scrolled_left, scrolled_width), (left, width));
}

#[gpui::test]
fn a_zone_is_hidden_while_its_line_is_folded_away(cx: &mut TestAppContext) {
    let (view, state, _, cx) = open(20, 4, 1, cx);
    assert_eq!(frames(&view, cx).len(), 1);
    let rows = row_count(&state, cx);
    state.update(cx, |state, cx| assert!(state.fold(2..8, cx)));
    settle(cx);
    assert!(frames(&view, cx).is_empty());
    assert!(row_count(&state, cx) < rows);
    state.update(cx, |state, cx| state.unfold(&(2..8), cx));
    settle(cx);
    assert_eq!(frames(&view, cx).len(), 1);
    assert_eq!(row_count(&state, cx), rows);
}

#[gpui::test]
fn closing_a_zone_frees_its_rows_and_drops_its_content(cx: &mut TestAppContext) {
    let (view, state, _, cx) = open(10, 2, 3, cx);
    assert_eq!(cx.read(|cx| state.read(cx).zone_contents().len()), 1);
    assert!(state.update(cx, |state, cx| state.close_zone(owner(), "z", cx)));
    settle(cx);
    assert_eq!(row_count(&state, cx), 10);
    assert!(frames(&view, cx).is_empty());
    assert!(cx.read(|cx| state.read(cx).zone_contents().is_empty()));
}

#[gpui::test]
fn measuring_settles_after_one_change(cx: &mut TestAppContext) {
    let (_, state, _, cx) = open(10, 2, 3, cx);
    let rows = row_count(&state, cx);
    for _ in 0..3 {
        settle(cx);
        assert_eq!(row_count(&state, cx), rows);
    }
}
