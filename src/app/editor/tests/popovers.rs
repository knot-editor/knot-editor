//! Popover sizing and placement against an editor's scrolling, folding, and
//! the application zoom.

use gpui::{AppContext, TestAppContext, px, size};

use knot_model::editor::popovers::{PopoverSpec, PopoverWidth};

use super::super::popovers::PopoverFrame;
use super::super::{Buffer, BufferModel, Editor, EditorView, PopoverContentSpec};
use super::blocks::owner;
use crate::app::view_layout::{
    Arrangement, ContentSpec, GroupSpec, LayoutSize, MemberSpec, TOOLBAR_HEIGHT, ToolbarButton,
};

const LINE_HEIGHT: f32 = 22.;

/// An editor of `lines` lines in a 400 by 300 window, with a popover on the
/// lines `anchor` that holds an editor of `content` lines limited to
/// `limit`.
fn open(
    lines: usize,
    anchor: std::ops::Range<u32>,
    width: PopoverWidth,
    content: usize,
    limit: Option<u32>,
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<EditorView>,
    gpui::Entity<Editor>,
    &mut gpui::VisualTestContext,
) {
    let text = (0..lines)
        .map(|n| format!("line {n}\n"))
        .collect::<String>();
    let model = cx.new(|_| Buffer::new(BufferModel::from_text(text.as_str())));
    let (view, cx) = cx.add_window_view(|_, cx| EditorView::new(model, cx));
    cx.simulate_resize(size(px(400.), px(300.)));
    let shown = cx.new(|_| {
        let text = (0..content)
            .map(|n| format!("c{n}"))
            .collect::<Vec<_>>()
            .join("\n");
        Buffer::new(BufferModel::from_text(text.as_str()))
    });
    let state = cx.read(|cx| view.read(cx).editor().clone());
    state.update(cx, |state, cx| {
        state.open_popover(
            owner(),
            "p",
            "window",
            PopoverContentSpec {
                spec: PopoverSpec {
                    anchor,
                    width,
                    focus: false,
                },
                toolbar: Vec::new(),
                layout: GroupSpec {
                    arrangement: Arrangement::Column,
                    members: vec![MemberSpec {
                        key: None,
                        size: LayoutSize::default(),
                        focused: false,
                        title: None,
                        limit,
                        content: ContentSpec::Editor {
                            buffer: shown,
                            document: None,
                            placeholder: None,
                            gutter: true,
                        },
                    }],
                },
            },
            cx,
        );
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    (view, state, cx)
}

fn frames(view: &gpui::Entity<EditorView>, cx: &gpui::VisualTestContext) -> Vec<(f32, f32, f32)> {
    cx.read(|cx| {
        view.read(cx)
            .popover_frames(cx)
            .iter()
            .map(|frame: &PopoverFrame| (frame.top, frame.width, frame.height))
            .collect()
    })
}

fn line_height(view: &gpui::Entity<EditorView>, cx: &gpui::VisualTestContext) -> f32 {
    cx.read(|cx| view.read(cx).line_height())
}

#[gpui::test]
fn the_height_is_the_contents_lines_up_to_the_limit(cx: &mut TestAppContext) {
    let (view, _, cx) = open(30, 1..2, PopoverWidth::Editor, 3, None, cx);
    let lh = line_height(&view, cx);
    assert_eq!(lh, LINE_HEIGHT);
    // Below the anchor's last line, as tall as three lines and the border.
    let frame = frames(&view, cx);
    assert_eq!(frame.len(), 1);
    assert_eq!(frame[0].0, 2. * lh);
    assert_eq!(frame[0].2, 3. * lh + 2.);

    let (view, _, cx) = open(30, 1..2, PopoverWidth::Editor, 40, None, cx);
    // Forty lines are limited to twelve, and twelve do not fit below line 1
    // of the 300 pixel window, so the popover takes the room there is.
    assert_eq!(frames(&view, cx)[0].2, 300. - 2. * lh - 8.);
    let (view, _, cx) = open(30, 1..2, PopoverWidth::Editor, 40, Some(5), cx);
    assert_eq!(frames(&view, cx)[0].2, 5. * lh + 2.);
}

#[gpui::test]
fn the_width_is_pixels_or_the_text_area_and_never_wider(cx: &mut TestAppContext) {
    let text_width = |view: &gpui::Entity<EditorView>, cx: &gpui::VisualTestContext| {
        cx.read(|cx| f32::from(view.read(cx).text_bounds().size.width)) - 16.
    };
    let (view, _, cx) = open(10, 0..1, PopoverWidth::Editor, 1, None, cx);
    assert_eq!(frames(&view, cx)[0].1, text_width(&view, cx));
    let (view, _, cx) = open(10, 0..1, PopoverWidth::Pixels(120.), 1, None, cx);
    assert_eq!(frames(&view, cx)[0].1, 120.);
    let (view, _, cx) = open(10, 0..1, PopoverWidth::Pixels(5000.), 1, None, cx);
    assert_eq!(frames(&view, cx)[0].1, text_width(&view, cx));
}

#[gpui::test]
fn the_popover_goes_above_when_it_does_not_fit_below(cx: &mut TestAppContext) {
    // Eight lines tall, anchored near the bottom of the 300 pixel window.
    let (view, _, cx) = open(30, 11..12, PopoverWidth::Editor, 8, None, cx);
    let lh = line_height(&view, cx);
    let anchor_top = 11. * lh;
    let [(top, _, height)] = frames(&view, cx)[..] else {
        panic!("one popover");
    };
    assert_eq!(height, 8. * lh + 2.);
    assert_eq!(top, anchor_top - height);
}

#[gpui::test]
fn the_popover_follows_scrolling_and_hides_while_its_anchor_is_out_of_view(
    cx: &mut TestAppContext,
) {
    let (view, state, cx) = open(60, 2..3, PopoverWidth::Editor, 2, None, cx);
    let lh = line_height(&view, cx);
    assert_eq!(frames(&view, cx)[0].0, 3. * lh);
    state.update(cx, |state, _| state.scroll_to_for_test(1.));
    assert_eq!(frames(&view, cx)[0].0, 2. * lh);
    // The anchor scrolled off the top.
    state.update(cx, |state, _| state.scroll_to_for_test(10.));
    assert!(frames(&view, cx).is_empty());
    // Scrolling back shows it again, still open.
    state.update(cx, |state, _| state.scroll_to_for_test(0.));
    assert_eq!(frames(&view, cx).len(), 1);
    // The anchor below the viewport.
    let (view, _, cx) = open(60, 50..51, PopoverWidth::Editor, 2, None, cx);
    assert!(frames(&view, cx).is_empty());
}

#[gpui::test]
fn a_popover_is_hidden_while_its_anchor_is_folded_away(cx: &mut TestAppContext) {
    let (view, state, cx) = open(20, 4..5, PopoverWidth::Editor, 1, None, cx);
    assert_eq!(frames(&view, cx).len(), 1);
    state.update(cx, |state, cx| {
        assert!(state.fold(2..8, cx));
    });
    assert!(frames(&view, cx).is_empty());
    state.update(cx, |state, cx| state.unfold(&(2..8), cx));
    assert_eq!(frames(&view, cx).len(), 1);
}

#[gpui::test]
fn zoom_scales_the_width_and_the_lines_and_relays_out_an_open_popover(cx: &mut TestAppContext) {
    let (view, _, cx) = open(30, 1..2, PopoverWidth::Pixels(100.), 2, None, cx);
    let before = frames(&view, cx)[0];
    let lh = line_height(&view, cx);
    assert_eq!(before.1, 100.);
    cx.update(|_, cx| crate::app::zoom::set_level(2, cx));
    cx.run_until_parked();
    let factor = cx.read(crate::app::zoom::factor);
    let after = frames(&view, cx)[0];
    assert_eq!(after.1, 100. * factor);
    assert!(line_height(&view, cx) > lh);
    assert_eq!(after.2, 2. * line_height(&view, cx) + 2.);
    cx.update(|_, cx| crate::app::zoom::set_level(0, cx));
}

#[gpui::test]
fn a_toolbar_adds_its_height(cx: &mut TestAppContext) {
    let (view, state, cx) = open(30, 1..2, PopoverWidth::Editor, 2, None, cx);
    let lh = line_height(&view, cx);
    let button = ToolbarButton {
        label: "x".into(),
        icon: serde_json::from_str("\"file\"").unwrap(),
        command: "x".into(),
        arguments: knot_protocol::CommandArgumentValue::Null,
        toggled: false,
        disabled: false,
    };
    let shown = cx.new(|_| Buffer::new(BufferModel::from_text("a\nb")));
    state.update(cx, |state, cx| {
        state.open_popover(
            owner(),
            "p",
            "window",
            PopoverContentSpec {
                spec: PopoverSpec {
                    anchor: 1..2,
                    width: PopoverWidth::Editor,
                    focus: false,
                },
                toolbar: vec![button],
                layout: GroupSpec {
                    arrangement: Arrangement::Column,
                    members: vec![MemberSpec {
                        key: None,
                        size: LayoutSize::default(),
                        focused: false,
                        title: None,
                        limit: None,
                        content: ContentSpec::Editor {
                            buffer: shown,
                            document: None,
                            placeholder: None,
                            gutter: true,
                        },
                    }],
                },
            },
            cx,
        );
    });
    assert_eq!(frames(&view, cx)[0].2, TOOLBAR_HEIGHT + 2. * lh + 2.);
}
