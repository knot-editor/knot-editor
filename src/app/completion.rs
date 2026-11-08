//! The native completion list. Sessions and aggregation live in
//! [`knot_model::completion`].

use gpui::{prelude::*, *};

use crate::app::convert::ThemePaint as _;

use super::theme::{Theme, color};

pub(crate) use knot_model::completion::*;

/// The completion list of `surface`, its top-left corner at `anchor`.
pub(crate) fn render_completion(
    surface: &CompletionSurface,
    anchor: Point<Pixels>,
    theme: &Theme,
) -> AnyElement {
    let selected = surface.selected_item();
    let status = format!(
        "{} pending · {} failed",
        surface.snapshot().pending_provider_count,
        surface.snapshot().failures.len()
    );
    div()
        .absolute()
        .left(anchor.x)
        .top(anchor.y)
        .w(crate::app::zoom::rems(360.))
        .max_h(crate::app::zoom::rems(220.))
        .overflow_hidden()
        .rounded_md()
        .border_1()
        .border_color(theme.paint_color(color::POPOVER_BORDER))
        .bg(theme.paint_color(color::POPOVER_BACKGROUND))
        .text_sm()
        .children(surface.snapshot().items.iter().take(8).map(|item| {
            div()
                .flex()
                .justify_between()
                .px_2()
                .py_1()
                .when(Some(item.id) == selected, |row| {
                    row.bg(theme.paint_color(color::LIST_SELECTED_BACKGROUND))
                })
                .child(item.label.clone())
                .child(
                    div()
                        .text_color(theme.paint_color(color::TEXT_ACCENT))
                        .child(item.provider_label.clone()),
                )
        }))
        .children(surface.snapshot().failures.iter().map(|failure| {
            div()
                .px_2()
                .py_1()
                .text_color(theme.paint_color(color::ERROR))
                .child(format!("{}: {}", failure.provider_label, failure.message))
        }))
        .child(
            div()
                .border_t_1()
                .border_color(theme.paint_color(color::POPOVER_BORDER))
                .px_2()
                .py_1()
                .text_color(theme.paint_color(color::TEXT_MUTED))
                .child(status),
        )
        .into_any_element()
}
