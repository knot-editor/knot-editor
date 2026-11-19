//! Rendering of the window's panes, split layout, dividers, and extension report.

use super::*;
use crate::app::convert::ThemePaint as _;
use crate::app::documents::{DocumentDirty, ExternalChange};
use crate::app::status_bar::{BarItem, StatusItem};
use crate::app::theme::{Chrome, Theme, chrome, color, theme};
use crate::app::tokens;
use crate::app::tree_view::TreeDrag;
use crate::app::workbench::{BottomDockLayout, ToolKind};
use crate::app::zoom;

/// The widest a terminal tab's label gets, in rems, before it ends in an ellipsis.
const TERMINAL_TAB_MAX_WIDTH: f32 = 200.;

/// What dragging a divider resizes.
#[derive(Clone, Copy)]
pub(super) enum Divider {
    /// A side's absolute size.
    Dock(DockLocation),
    /// How a side divides between its two tool windows.
    Slots(DockLocation),
    /// The ratio of the split identified by its boundary pane.
    Split(PaneId),
}

impl KnotWindow {
    /// The workbench's areas, arranged as `workbench.bottomDockLayout`
    /// places the bottom dock; a maximized pane fills it alone.
    pub(super) fn render_workbench(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let card_background = theme(cx).paint_color(color::CARD_BACKGROUND);
        let border = theme(cx).paint_color(color::BORDER);
        let chrome = chrome(cx);
        // Cards clip their content to a rectangle, so a child's own background
        // matches the card's wherever it reaches a rounded corner.
        let card = |element: Div| {
            element
                .overflow_hidden()
                .rounded(zoom::rems(chrome.card_radius))
                .border(zoom::rems(chrome.card_border))
                .border_color(border)
                .bg(card_background)
        };
        if let Some(pane) = self.maximized_pane(cx) {
            return card(div().size_full())
                .child(self.render_pane(pane, cx))
                .into_any_element();
        }
        let workbench = self.workbench.read(cx);
        let sides = DockLocation::ALL.map(|side| {
            let size = workbench.side_extent(side)?;
            let windows = workbench
                .side_tools(side)
                .into_iter()
                .flatten()
                .filter_map(|id| {
                    let layout = workbench.area_layout(Area::Tool(id))?.clone();
                    Some((id, layout, workbench.tool(id)?.share()))
                })
                .collect::<Vec<_>>();
            Some((size, windows))
        });
        let center = workbench.center().cloned();
        let center = match center {
            Some(layout) => self.render_layout(&layout, cx),
            None => div()
                .id("empty-center")
                .debug_selector(|| "empty-center".to_owned())
                .track_focus(&self.empty_focus)
                .relative()
                .size_full()
                .child(self.empty_center_drag_geometry())
                .into_any_element(),
        };
        let mut docks = [None, None, None];
        for (dock, shown) in DockLocation::ALL.into_iter().zip(sides) {
            let Some((size, windows)) = shown else {
                continue;
            };
            let both = windows.len() == 2;
            let mut element = div()
                .flex_none()
                .min_w_0()
                .min_h_0()
                .flex()
                .debug_selector(move || format!("dock-{}", dock.index()))
                .map(|element| match dock {
                    DockLocation::Bottom => element.flex_row().h(px(size)).w_full(),
                    DockLocation::Left | DockLocation::Right => {
                        element.flex_col().w(px(size)).h_full()
                    }
                });
            for (index, (id, layout, share)) in windows.into_iter().enumerate() {
                if index == 1 {
                    element = element.child(self.render_divider(
                        if dock == DockLocation::Bottom {
                            SplitDirection::Horizontal
                        } else {
                            SplitDirection::Vertical
                        },
                        Divider::Slots(dock),
                        chrome,
                        cx,
                    ));
                }
                let grow = if both { share } else { 1. };
                let mut window = card(div())
                    .min_w_0()
                    .min_h_0()
                    .flex_basis(px(0.))
                    .flex_shrink(1.)
                    .debug_selector(move || format!("tool-window-{}", id.value()))
                    .child(self.render_layout(&layout, cx));
                window.style().flex_grow = Some(grow);
                element = element.child(window);
            }
            let divider = self.render_divider(dock.axis(), Divider::Dock(dock), chrome, cx);
            docks[dock.index()] = Some((element, divider));
        }
        let [left, right, bottom] = docks;
        let center = card(div()).flex_1().min_w_0().min_h_0().child(center);
        let column = |element: Div| element.flex().flex_col().min_w_0().min_h_0();
        let row = |element: Div| element.flex().flex_row().min_w_0().min_h_0();
        let sides = |element: Div, middle: Div| {
            element
                .when_some(left, |element, (dock, divider)| {
                    element.child(dock).child(divider)
                })
                .child(middle)
                .when_some(right, |element, (dock, divider)| {
                    element.child(divider).child(dock)
                })
        };
        let bottom_under = |element: Div| {
            element.when_some(bottom, |element, (dock, divider)| {
                element.child(divider).child(dock)
            })
        };
        match crate::app::settings::bottom_dock_layout(cx) {
            BottomDockLayout::Contained => {
                let middle = bottom_under(column(div()).flex_1().child(center));
                sides(row(div()).size_full(), middle).into_any_element()
            }
            BottomDockLayout::Full => {
                let upper = sides(row(div()).flex_1(), center);
                bottom_under(column(div()).size_full().child(upper)).into_any_element()
            }
        }
    }

    pub(super) fn render_layout(
        &self,
        layout: &WorkbenchLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match layout {
            WorkbenchLayout::Pane(pane) => self.render_pane(*pane, cx),
            WorkbenchLayout::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let direction = *direction;
                let boundary = layout.boundary().expect("splits have a boundary");
                let child = |grow: f32| {
                    let mut element = div().min_w_0().min_h_0().flex_basis(px(0.)).flex_shrink(1.);
                    element.style().flex_grow = Some(grow);
                    element
                };
                div()
                    .flex()
                    .when(direction == SplitDirection::Horizontal, |element| {
                        element.flex_row()
                    })
                    .when(direction == SplitDirection::Vertical, |element| {
                        element.flex_col()
                    })
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .relative()
                    .child(self.split_drag_geometry(boundary))
                    .child(child(*ratio).child(self.render_layout(first, cx)))
                    .child(self.render_divider(direction, Divider::Split(boundary), chrome(cx), cx))
                    .child(child(1. - *ratio).child(self.render_layout(second, cx)))
                    .into_any_element()
            }
        }
    }

    /// Divider between two areas or split children, with a wider hit area
    /// that drags to resize: a dock's divider changes its size, a split's
    /// divider its ratio.
    pub(super) fn render_divider(
        &self,
        direction: SplitDirection,
        divider: Divider,
        chrome: Chrome,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let horizontal = direction == SplitDirection::Horizontal;
        // The handle is at least `MIN_RESIZE_HANDLE` wide; what exceeds the
        // gap overlaps the area before it, which the handle paints over.
        let handle = chrome.card_gap.max(tokens::chrome::MIN_RESIZE_HANDLE);
        let overlap = handle - chrome.card_gap;
        let element = match divider {
            Divider::Dock(dock) => div()
                .id(("dock-divider", dock.index()))
                .debug_selector(move || format!("dock-divider-{}", dock.index())),
            Divider::Slots(side) => div()
                .id(("slot-divider", side.index()))
                .debug_selector(move || format!("slot-divider-{}", side.index())),
            Divider::Split(boundary) => div()
                .id(("split-divider", boundary.value()))
                .debug_selector(move || format!("split-divider-{}", boundary.value())),
        };
        element
            .flex()
            .flex_none()
            .when(horizontal, |element| {
                element
                    .flex_row()
                    .w(zoom::rems(handle))
                    .ml(zoom::rems(-overlap))
                    .pl(zoom::rems(overlap))
                    .h_full()
                    .justify_center()
                    .cursor(CursorStyle::ResizeLeftRight)
            })
            .when(!horizontal, |element| {
                element
                    .flex_col()
                    .h(zoom::rems(handle))
                    .mt(zoom::rems(-overlap))
                    .pt(zoom::rems(overlap))
                    .w_full()
                    .items_center()
                    .cursor(CursorStyle::ResizeUpDown)
            })
            .when(
                matches!(divider, Divider::Split(_) | Divider::Slots(_)),
                |element| {
                    element.child(
                        div()
                            .when(horizontal, |element| element.w(px(1.)).h_full())
                            .when(!horizontal, |element| element.h(px(1.)).w_full())
                            .bg(theme(cx).paint_color(color::BORDER))
                            .hover({
                                let hover = theme(cx).paint_color(color::BORDER_HOVER);
                                move |style| style.bg(hover)
                            }),
                    )
                },
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    match divider {
                        Divider::Dock(dock) => {
                            let size = this.workbench.read(cx).side_extent(dock);
                            if let Some(size) = size {
                                this.dock_resize = Some(DockResizeDrag {
                                    dock,
                                    start_pointer: event.position,
                                    start_size: size,
                                });
                            }
                        }
                        Divider::Slots(side) => {
                            let workbench = this.workbench.read(cx);
                            let share = workbench.side_share(side);
                            let extent = this.tab_drag.borrow().side_extent(workbench, side);
                            if let (Some(share), Some(extent)) = (share, extent) {
                                this.slot_resize = Some(SlotResizeDrag {
                                    side,
                                    start_pointer: event.position,
                                    start_share: share,
                                    extent,
                                });
                            }
                        }
                        Divider::Split(boundary) => {
                            let ratio = this.workbench.read(cx).split_ratio(boundary);
                            let bounds = this.tab_drag.borrow().split_bounds(boundary);
                            if let (Some(ratio), Some(bounds)) = (ratio, bounds) {
                                this.split_resize = Some(SplitResizeDrag {
                                    boundary,
                                    horizontal,
                                    start_pointer: event.position,
                                    start_ratio: ratio,
                                    extent: if horizontal {
                                        f32::from(bounds.size.width)
                                    } else {
                                        f32::from(bounds.size.height)
                                    },
                                });
                            }
                        }
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    /// The focused pane's outline, inside the pane's edge so the card's
    /// clipping keeps it whole. It takes no layout space and no input.
    fn focus_ring(&self, cx: &App) -> Div {
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .debug_selector(|| "focus-ring".to_owned())
            .rounded(zoom::rems(chrome(cx).card_radius))
            .border(zoom::rems(chrome(cx).card_border))
            .border_color(theme(cx).paint_color(color::CARD_FOCUSED_BORDER))
    }

    pub(super) fn render_pane(&self, pane_id: PaneId, cx: &mut Context<Self>) -> AnyElement {
        let workbench = self.workbench.read(cx);
        let pane = workbench.pane(pane_id).unwrap();
        let area = pane.area();
        let focused = workbench.focused_pane_id() == Some(pane_id);
        let active_tab = pane.active_tab_id();
        let active_view: Option<AnyElement> = workbench
            .editor(pane.active_tab())
            .cloned()
            .map(|editor| editor.into_any_element())
            .or_else(|| {
                workbench
                    .terminal_view(pane.active_tab())
                    .cloned()
                    .map(|view| view.into_any_element())
            });
        let active_view = active_view.or_else(|| {
            let TabSurfaceId::Tool(tool) = pane.active_tab().surface_id() else {
                return None;
            };
            workbench.tool_content(tool).map(|content| match content {
                ToolContent::Extension(crate::app::workbench::ExtensionView::Tree(view)) => {
                    view.clone().into_any_element()
                }
                ToolContent::Extension(crate::app::workbench::ExtensionView::Text(view)) => {
                    view.clone().into_any_element()
                }
            })
        });
        let active_tool_view = match pane.active_tab().surface_id() {
            TabSurfaceId::Tool(tool) => workbench.tool_content(tool).map(|content| match content {
                ToolContent::Extension(view) => view.clone(),
            }),
            _ => None,
        };
        let documents = Self::documents(cx);
        let window_tool = match area {
            Area::Tool(id) => workbench.tool(id),
            Area::Center => None,
        };
        let in_dock = window_tool.is_some();
        let container = window_tool.is_some_and(|tool| tool.kind() == ToolKind::Container);
        let tool_id = window_tool.map(|tool| tool.id);
        // The window's header is its first pane's strip.
        let header_title = window_tool
            .filter(|_| workbench.area_panes(area).first() == Some(&pane_id))
            .map(|tool| tool.title.clone());
        let content_menu = !matches!(pane.active_tab().surface_id(), TabSurfaceId::Tool(_));
        // A fixed tool window shows just its title; a container's panes show
        // their tabs even when there is one.
        let titled_alone = in_dock && !container;
        let chrome = chrome(cx);
        let accent = theme(cx).paint_color(color::TAB_ACCENT);
        let (active_background, active_border) = if focused {
            (color::TAB_ACTIVE_BACKGROUND, color::TAB_ACTIVE_BORDER)
        } else {
            (
                color::TAB_ACTIVE_UNFOCUSED_BACKGROUND,
                color::TAB_ACTIVE_UNFOCUSED_BORDER,
            )
        };
        let active_background_color = theme(cx).paint_color(active_background);
        let active_border = theme(cx).paint_color(active_border);
        let transparent = gpui::transparent_black();
        let pill = chrome.tab_radius > 0.;
        let inactive_background = theme(cx).paint_color(color::TAB_INACTIVE_BACKGROUND);
        let tabs = pane
            .tabs()
            .iter()
            .filter(|_| !titled_alone)
            .map(|tab| {
                let tab_id = tab.id();
                let documents = documents.read(cx);
                let title = match tab.surface_id() {
                    TabSurfaceId::Document(document_id) => {
                        let document = documents.get(document_id).unwrap();
                        format!(
                            "{}{}",
                            document.title(),
                            match document.external_change() {
                                Some(ExternalChange::Deleted) => " (deleted)",
                                Some(ExternalChange::Changed { .. }) => " (changed on disk)",
                                None => "",
                            }
                        )
                    }
                    TabSurfaceId::Terminal(session) => Self::terminal_session_label(session, cx)
                        .unwrap_or_else(|| "Terminal".to_owned()),
                    TabSurfaceId::Tool(id) => workbench.tool(id).unwrap().title.to_owned(),
                };
                let dirty = match tab.surface_id() {
                    TabSurfaceId::Document(document_id) => {
                        documents.get(document_id).unwrap().is_dirty(cx)
                    }
                    _ => false,
                };
                let bell_marked = matches!(
                    tab.surface_id(),
                    TabSurfaceId::Terminal(session) if workbench.bell_marked(session)
                );
                let capture = workbench.capture_tab_move(pane_id, tab_id).unwrap();
                let source = self.capture_tab_relocation(pane_id, tab_id, cx).unwrap();
                let icon = match tab.surface_id() {
                    TabSurfaceId::Document(document_id) => {
                        let document = documents.get(document_id).unwrap();
                        let language = crate::app::languages::effective_language(
                            document.model().read(cx),
                            cx,
                        );
                        Some(crate::app::icons::file_definition(
                            Some(document.title()),
                            Some(&language),
                            false,
                            cx,
                        ))
                    }
                    _ => None,
                };
                let drag_label = self.tab_drag_label(title.clone().into(), icon.clone(), cx);
                div()
                    .id(("tab", tab_id.value()))
                    .debug_selector(move || format!("tab-{}", tab_id.value()))
                    .relative()
                    .group(format!("tab-{}", tab_id.value()))
                    .on_drag(source, drag_label)
                    .child(self.tab_drag_geometry(pane_id, capture))
                    .h(zoom::rems(chrome.tab_bar_height - 2. * chrome.tab_inset))
                    .pl(zoom::rems(12.))
                    .pr(zoom::rems(10.))
                    .cursor_pointer()
                    .text_size(zoom::rems(13.))
                    .rounded(zoom::rems(chrome.tab_radius))
                    .text_color(theme(cx).paint_color(if tab_id == active_tab {
                        color::TAB_ACTIVE_FOREGROUND
                    } else {
                        color::TAB_INACTIVE_FOREGROUND
                    }))
                    // Every tab sits on the tab bar's background; the selected
                    // one is filled, or underlined with the accent in a
                    // theme without pills. A lone tool shows only its title.
                    .map(|element| {
                        let selected = tab_id == active_tab && !titled_alone;
                        let background = if selected {
                            active_background_color
                        } else {
                            inactive_background
                        };
                        if pill {
                            element.bg(background).border_1().border_color(if selected {
                                active_border
                            } else {
                                transparent
                            })
                        } else {
                            element
                                .bg(background)
                                .border_b(px(chrome.accent_width))
                                .border_color(if selected { accent } else { background })
                        }
                    })
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(zoom::rems(8.))
                    .when(tab.is_preview(), |element| element.italic())
                    .children(icon.map(|icon| {
                        div()
                            .debug_selector(move || format!("tab-icon-{}", tab_id.value()))
                            .child(crate::app::icons::AppIcon::new(icon))
                    }))
                    .when(bell_marked, |element| {
                        element.child(
                            div()
                                .debug_selector(move || format!("bell-{}", tab_id.value()))
                                .child(
                                    crate::app::icons::AppIcon::new(
                                        knot_model::icons::IconDefinition::lucide("bell-ring"),
                                    )
                                    .tinted(Some(color::NOTIFICATION_INFO.to_owned())),
                                ),
                        )
                    })
                    .map(|element| {
                        if matches!(tab.surface_id(), TabSurfaceId::Terminal(_)) {
                            // A program's title can be arbitrarily long.
                            let full = SharedString::from(title.clone());
                            element
                                .child(
                                    div()
                                        .max_w(zoom::rems(TERMINAL_TAB_MAX_WIDTH))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(title.clone()),
                                )
                                .tooltip(move |window, cx| {
                                    gpui_component::tooltip::Tooltip::new(full.clone())
                                        .text_xs()
                                        .build(window, cx)
                                })
                        } else {
                            element.child(title.clone())
                        }
                    })
                    .when(tab.is_pinned(), |element| {
                        element.child(
                            div()
                                .debug_selector(move || format!("pin-{}", tab_id.value()))
                                .child(crate::app::icons::AppIcon::new(
                                    knot_model::icons::IconDefinition::lucide("pin"),
                                )),
                        )
                    })
                    .when(
                        !matches!(tab.surface_id(), TabSurfaceId::Tool(_)) || !in_dock,
                        |element| {
                            element.child(
                                div()
                                    .id(("close-tab", tab_id.value()))
                                    .debug_selector(move || format!("close-tab-{}", tab_id.value()))
                                    .group(format!("close-{}", tab_id.value()))
                                    .size(zoom::rems(16.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_sm()
                                    .hover(|style| {
                                        style
                                            .bg(theme(cx)
                                                .paint_color(color::BUTTON_HOVER_BACKGROUND))
                                    })
                                    .relative()
                                    .child(if in_dock {
                                        div().child("×")
                                    } else {
                                        // The × shows on the active or hovered
                                        // tab; a dirty tab's dot takes its place
                                        // until the pointer is on the button.
                                        let group = if dirty {
                                            format!("close-{}", tab_id.value())
                                        } else {
                                            format!("tab-{}", tab_id.value())
                                        };
                                        div()
                                            .opacity(if tab_id == active_tab && !dirty {
                                                1.
                                            } else {
                                                0.
                                            })
                                            .group_hover(group, |style| style.opacity(1.))
                                            .child("×")
                                    })
                                    .when(dirty && !in_dock, |close| {
                                        let group = format!("close-{}", tab_id.value());
                                        close.child(
                                            div()
                                                .debug_selector(move || {
                                                    format!("dirty-{}", tab_id.value())
                                                })
                                                .absolute()
                                                .inset_0()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .group_hover(group, |style| style.opacity(0.))
                                                .child(
                                                    div()
                                                        .size(zoom::rems(7.))
                                                        .rounded_full()
                                                        .bg(accent),
                                                ),
                                        )
                                    })
                                    // Keep the press from starting a tab drag or
                                    // activating the tab it closes.
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.close_tab_from_pointer(pane_id, tab_id, cx);
                                    })),
                            )
                        },
                    )
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        if event.click_count() > 1 {
                            this.workbench.update(cx, |workbench, _| {
                                workbench.promote_preview(pane_id, tab_id)
                            });
                        }
                        this.activate_tab(pane_id, tab_id, cx);
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.open_tab_menu(pane_id, tab_id, event.position, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Middle,
                        cx.listener(move |this, _, _, cx| {
                            this.close_tab_from_pointer(pane_id, tab_id, cx);
                        }),
                    )
            })
            .collect::<Vec<_>>();

        let header_title = header_title.map(|title| {
            div()
                .debug_selector(move || format!("tool-title-{}", pane_id.value()))
                .px(zoom::rems(10.))
                .font_weight(FontWeight::BOLD)
                .text_size(zoom::rems(13.))
                .text_color(theme(cx).paint_color(color::TAB_ACTIVE_FOREGROUND))
                .child(title)
        });
        let header_end = header_title.is_some();
        let frame_actions = match active_tool_view {
            Some(crate::app::workbench::ExtensionView::Tree(tree)) if in_dock => {
                crate::app::tree_view::TreeView::frame_actions_bar(&tree, cx)
            }
            _ => None,
        };
        div()
            .id(("pane", pane_id.value()))
            .debug_selector(move || format!("pane-{}", pane_id.value()))
            .relative()
            .child(self.pane_drag_geometry(pane_id, area, area == Area::Center || container))
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(theme(cx).paint_color(if in_dock {
                color::CARD_BACKGROUND
            } else {
                color::BACKGROUND
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_none()
                    .h(zoom::rems(chrome.tab_bar_height))
                    .items_center()
                    .when(pill, |bar| bar.pl(zoom::rems(6.)))
                    .relative()
                    .id(("tab-bar", pane_id.value()))
                    .debug_selector(move || format!("tab-bar-{}", pane_id.value()))
                    .when(header_end, |bar| {
                        // A double click on a tool window's header maximizes it.
                        bar.on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                            if event.click_count() == 2 {
                                this.toggle_maximize(pane_id, cx);
                            }
                        }))
                    })
                    .child(self.tab_bar_drag_geometry(pane_id))
                    .overflow_hidden()
                    .bg(theme(cx).paint_color(color::TAB_BAR_BACKGROUND))
                    .children(header_title)
                    .children(tabs)
                    // The line between the tab bar and its content.
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .right_0()
                            .h(px(1.))
                            .bg(theme(cx).paint_color(color::BORDER)),
                    )
                    .when(in_dock, |bar| {
                        bar.child(div().flex_1())
                            .children(frame_actions)
                            .when(container, |bar| {
                                bar.child(self.render_new_tab_button(pane_id, tool_id, cx))
                            })
                            .when(header_end, |bar| {
                                bar.children(tool_id.map(|id| self.render_options_button(id, cx)))
                                    .child(self.render_hide_button(pane_id, cx))
                            })
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .when(content_menu, |element| {
                        // The editor or terminal below leaves the click
                        // unconsumed when it has a menu to show.
                        element.on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                this.open_content_menu(pane_id, event.position, window, cx);
                                cx.stop_propagation();
                            }),
                        )
                    })
                    .when_some(active_view, |element, view| element.child(view)),
            )
            // Last, so it paints over the tab bar and the content.
            .when(focused, |pane| pane.child(self.focus_ring(cx)))
            .into_any_element()
    }
}

impl KnotWindow {
    /// The button at a dock header's right that hides the pane's active tool,
    /// or the dock's terminals when a terminal is active.
    fn render_hide_button(&self, pane_id: PaneId, cx: &mut Context<Self>) -> Stateful<Div> {
        let hover = theme(cx).paint_color(color::BUTTON_HOVER_BACKGROUND);
        div()
            .id(("hide-pane", pane_id.value()))
            .debug_selector(move || format!("hide-pane-{}", pane_id.value()))
            .px_1p5()
            .mx_1()
            .rounded(zoom::rems(tokens::radius::SM))
            .cursor_pointer()
            .text_color(theme(cx).paint_color(color::TAB_INACTIVE_FOREGROUND))
            .hover(move |style| style.bg(hover))
            .child("–")
            .on_click(cx.listener(move |this, _, _, cx| {
                this.hide_pane_from_header(pane_id, cx);
            }))
    }

    /// The header button that opens a tab in the container tool window,
    /// beside the pane's tabs.
    fn render_new_tab_button(
        &self,
        pane_id: PaneId,
        tool: Option<ToolFrameId>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let hover = theme(cx).paint_color(color::BUTTON_HOVER_BACKGROUND);
        div()
            .id(("new-terminal", pane_id.value()))
            .debug_selector(move || format!("new-terminal-{}", pane_id.value()))
            .px_1p5()
            .rounded(zoom::rems(tokens::radius::SM))
            .cursor_pointer()
            .text_color(theme(cx).paint_color(color::TAB_INACTIVE_FOREGROUND))
            .hover(move |style| style.bg(hover))
            .child("+")
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(tool) = tool {
                    this.workbench
                        .update(cx, |workbench, _| workbench.focus_pane(pane_id));
                    this.focus_active_surface(cx);
                    this.run_new_tab(tool, cx);
                }
            }))
    }

    /// The header's options button (⋮), which opens the window's menu.
    fn render_options_button(&self, tool: ToolFrameId, cx: &mut Context<Self>) -> Stateful<Div> {
        let hover = theme(cx).paint_color(color::BUTTON_HOVER_BACKGROUND);
        div()
            .id(("tool-options", tool.value()))
            .debug_selector(move || format!("tool-options-{}", tool.value()))
            .px_1p5()
            .rounded(zoom::rems(tokens::radius::SM))
            .cursor_pointer()
            .text_color(theme(cx).paint_color(color::TAB_INACTIVE_FOREGROUND))
            .hover(move |style| style.bg(hover))
            .child("⋮")
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.open_tool_menu(tool, false, event.position(), window, cx);
            }))
    }

    /// Hide the tool window a pane belongs to.
    fn hide_pane_from_header(&mut self, pane_id: PaneId, cx: &mut Context<Self>) {
        if let Some(tool) = self.workbench.read(cx).pane_tool(pane_id) {
            self.hide_tool_from_pointer(tool, cx);
        }
    }
}

/// Whether the window's title bar is transparent, with the toolbar row in its
/// place; other platforms keep their native frame.
pub(super) const NATIVE_TOOLBAR_ROW: bool = cfg!(target_os = "macos");

impl KnotWindow {
    /// The row at the window's top: on macOS under the transparent title bar
    /// and beside the window buttons, where it drags the window and zooms it
    /// on a double click; elsewhere only while an extension contributes toolbar
    /// items. It shows those items, left and right.
    fn render_toolbar_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::app::{settings, status_bar::ApplicationStatusBar};
        let window_key = window_id(cx.entity_id());
        let hidden = settings::strings(settings::STATUS_BAR_HIDDEN_ITEMS, cx);
        let items = cx
            .try_global::<ApplicationStatusBar>()
            .map(|bar| bar.0.toolbar_items(&window_key, &hidden))
            .unwrap_or_default();
        if !NATIVE_TOOLBAR_ROW && items.left.is_empty() && items.right.is_empty() {
            return None;
        }
        let left = items
            .left
            .into_iter()
            .map(|item| self.render_status_item(item, cx))
            .collect::<Vec<_>>();
        let right = items
            .right
            .into_iter()
            .map(|item| self.render_status_item(item, cx))
            .collect::<Vec<_>>();
        let group = || div().flex().flex_row().items_center().h_full().gap_1();
        Some(
            div()
                .id("toolbar")
                .debug_selector(|| "toolbar".to_owned())
                .flex()
                .flex_none()
                .flex_row()
                .items_center()
                .w_full()
                .when(NATIVE_TOOLBAR_ROW, |row| {
                    // The window buttons are native, so the row's height and
                    // their reserved width are too.
                    row.h(px(tokens::chrome::TOOLBAR_HEIGHT))
                        .pl(px(tokens::chrome::WINDOW_BUTTONS_WIDTH))
                })
                .when(!NATIVE_TOOLBAR_ROW, |row| {
                    row.h(zoom::rems(tokens::chrome::TOOLBAR_HEIGHT))
                })
                .pr_2()
                .text_sm()
                .bg(theme(cx).paint_color(color::TOOLBAR_BACKGROUND))
                .text_color(theme(cx).paint_color(color::STATUS_BAR_FOREGROUND))
                .on_mouse_down(MouseButton::Left, |event, window, _| {
                    if !NATIVE_TOOLBAR_ROW {
                        return;
                    }
                    if event.click_count == 2 {
                        window.titlebar_double_click();
                    } else {
                        window.start_window_move();
                    }
                })
                .child(group().children(left))
                .child(div().flex_1())
                .child(group().children(right))
                .into_any_element(),
        )
    }

    /// The status bar below the workbench, unless the setting hides it.
    fn render_status_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::app::{settings, status_bar::ApplicationStatusBar};
        if !settings::boolean(settings::STATUS_BAR_VISIBLE, cx) {
            return None;
        }
        let window_key = window_id(cx.entity_id());
        let hidden = settings::strings(settings::STATUS_BAR_HIDDEN_ITEMS, cx);
        let items = cx
            .try_global::<ApplicationStatusBar>()
            .map(|bar| bar.0.items(&window_key, &hidden))
            .unwrap_or_default();
        let left = items
            .left
            .into_iter()
            .map(|item| self.render_status_item(item, cx))
            .collect::<Vec<_>>();
        let right = items
            .right
            .into_iter()
            .map(|item| self.render_status_item(item, cx))
            .collect::<Vec<_>>();
        let group = || div().flex().flex_row().items_center().h_full();
        Some(
            div()
                .id("status-bar")
                .debug_selector(|| "status-bar".to_owned())
                .flex()
                .flex_none()
                .flex_row()
                .items_center()
                .h(zoom::rems(tokens::chrome::STATUS_BAR_HEIGHT))
                .px(zoom::rems(tokens::list::ROW_INSET))
                .text_size(zoom::rems(tokens::chrome::STATUS_TEXT_SIZE))
                .bg(theme(cx).paint_color(color::STATUS_BAR_BACKGROUND))
                .text_color(theme(cx).paint_color(color::STATUS_BAR_FOREGROUND))
                .child(group().children(left))
                .child(div().flex_1())
                .child(group().children(right))
                .into_any_element(),
        )
    }

    fn render_status_item(&self, bar_item: BarItem, cx: &mut Context<Self>) -> AnyElement {
        let BarItem { id, item } = bar_item;
        let StatusItem {
            text,
            icon,
            tooltip,
            command,
            color: color_key,
            ..
        } = item;
        let hover = theme(cx).paint_color(color::STATUS_BAR_ITEM_HOVER_BACKGROUND);
        let item_selector = format!("status-item-{id}");
        div()
            .id(SharedString::from(item_selector.clone()))
            .debug_selector(move || item_selector.clone())
            .flex()
            .items_center()
            .h(zoom::rems(tokens::chrome::STATUS_ITEM_HEIGHT))
            .px(zoom::rems(8.))
            .rounded(zoom::rems(tokens::radius::SM))
            // A press on an item never starts a window drag from a toolbar.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when_some(color_key.as_ref(), |element, key| {
                element.text_color(theme(cx).paint_color(key))
            })
            .when_some(command, |element, command| {
                element
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let target = this.capture_command_target(cx);
                        let name = command.name.clone();
                        let arguments = command.arguments.clone();
                        this.dispatch_command(
                            knot_protocol::Command {
                                name: name.into(),
                                arguments,
                            },
                            target,
                            cx,
                        );
                    }))
            })
            .when_some(tooltip, |element, tooltip| {
                let tooltip = SharedString::from(tooltip);
                element.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone())
                        .text_xs()
                        .build(window, cx)
                })
            })
            .when_some(icon, |element, icon| {
                let icon = crate::app::icons::AppIcon::new(crate::app::icons::ref_definition(
                    &icon, false, cx,
                ))
                .tinted(color_key);
                let selector = format!("status-icon-{id}");
                element
                    .gap_1()
                    .child(div().debug_selector(move || selector.clone()).child(icon))
            })
            .child(text)
            .into_any_element()
    }
}

impl Render for KnotWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(any(test, feature = "test-support"))]
        crate::app::work_counts::draw();
        self.attach_window(window, cx);
        self.observe_application_focus(window, cx);
        window.set_rem_size(px(zoom::BASE_REM * zoom::factor(cx)));
        tab_drag::coordinator(cx);
        self.tab_drag.borrow_mut().begin_frame();
        let workbench = self.render_workbench(cx);
        let (left_stripe, right_stripe) = self.render_stripes(cx);
        let stripe_menu = self.stripe_menu.as_ref().map(|open| open.element());
        let surface_menu = self.surface_menu.as_ref().map(|open| open.element());
        let entity = cx.entity();
        let command_palette = self
            .command_palette
            .as_ref()
            .map(|palette| palette.picker().clone());
        let transient = self.render_transient();
        let toolbar = self.render_toolbar_row(cx);
        let status_bar = self.render_status_bar(cx);
        let notices = self.render_notices(toolbar.is_some(), status_bar.is_some(), cx);
        let dismiss_transient =
            cx.listener(|this: &mut Self, _: &MouseDownEvent, _: &mut Window, cx| {
                this.finish_transient(TransientOutcome::Dismissed, cx);
            });
        let scrim = theme(cx).paint_color(color::SCRIM);
        let startup_report = crate::app::extensions::host::startup_report(cx);
        let extension_report = self.extension_report_open.then(|| {
            (
                startup_report,
                crate::app::extensions::host::setting_warnings(cx),
            )
        });
        window.set_window_title("Knot");

        div()
            .relative()
            .key_context("product")
            .capture_key_down(cx.listener(Self::cancel_tab_drag))
            .on_modifiers_changed(cx.listener(Self::modifiers_changed))
            .on_action(cx.listener(|this, source: &ProductCommandSource, _, cx| {
                this.dispatch_source(source.command(), cx)
            }))
            .on_action(cx.listener(|this, source: &ContributedMenuCommand, _, cx| {
                this.dispatch_source(source.command(), cx)
            }))
            .on_action(
                cx.listener(|this, source: &ApplicationCommandSource, _, cx| {
                    this.dispatch_source(source.command(), cx)
                }),
            )
            .flex()
            .flex_col()
            .size_full()
            .bg(theme(cx).paint_color(color::WINDOW_BACKGROUND))
            .text_color(theme(cx).paint_color(color::TEXT))
            .children(toolbar)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .min_h_0()
                    .min_w_0()
                    .children(left_stripe)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .p(zoom::rems(chrome(cx).card_inset))
                            .child(
                                div()
                                    .size_full()
                                    .relative()
                                    .on_drag_move::<TreeDrag>(cx.listener(
                                        |this, event: &DragMoveEvent<TreeDrag>, _, cx| {
                                            let drag = event.drag(cx).clone();
                                            this.resource_drag_moved(
                                                &drag,
                                                event.event.position,
                                                cx,
                                            );
                                        },
                                    ))
                                    .on_drop::<TreeDrag>(cx.listener(
                                        |this, drag: &TreeDrag, window, cx| {
                                            this.drop_resource(drag, window.mouse_position(), cx);
                                        },
                                    ))
                                    .child(workbench)
                                    .child(self.workbench_drag_geometry()),
                            ),
                    )
                    .children(right_stripe),
            )
            .children(status_bar)
            .child(self.drag_listener(cx))
            .when_some(stripe_menu, |root, menu| root.child(menu))
            .when_some(surface_menu, |root, menu| root.child(menu))
            .when_some(transient, |root, transient| {
                root.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_start()
                        .justify_center()
                        .pt(zoom::rems(tokens::list::PANEL_TOP))
                        .bg(scrim)
                        .on_mouse_down(MouseButton::Left, dismiss_transient)
                        .child(transient),
                )
            })
            .when_some(command_palette, |root, palette| {
                root.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_start()
                        .justify_center()
                        .pt(zoom::rems(tokens::list::PANEL_TOP))
                        .bg(scrim)
                        .child(palette),
                )
            })
            .children(notices)
            .when_some(extension_report, |root, (report, setting_warnings)| {
                root.child(render_extension_report(
                    report,
                    setting_warnings,
                    &entity,
                    theme(cx),
                ))
            })
            .into_any_element()
    }
}

pub(super) fn render_extension_report(
    snapshot: Option<crate::app::extensions::host::StartupReportSnapshot>,
    setting_warnings: Vec<String>,
    window: &Entity<KnotWindow>,
    theme: &Theme,
) -> AnyElement {
    use crate::app::extensions::host::StartupState;
    use crate::app::extensions::load::LoadResult;

    let (root, state, entries) = match snapshot {
        Some(snapshot) => (
            snapshot.root.display().to_string(),
            snapshot.state,
            snapshot.report.entries,
        ),
        None => (
            "No installed extension scan".to_owned(),
            StartupState::Complete,
            Vec::new(),
        ),
    };
    let status = match state {
        StartupState::Scanning => "Scanning installed extensions",
        StartupState::Loading => "Loading installed extensions",
        StartupState::Complete => "Extension startup report",
    };
    let rows = entries
        .into_iter()
        .map(|entry| {
            let name = entry.name.unwrap_or_else(|| "<invalid package>".into());
            let (label, status_color, cause) = match entry.result {
                LoadResult::Loaded => ("Loaded", color::SUCCESS, None),
                LoadResult::Failed(cause) => ("Failed", color::ERROR, Some(cause)),
            };
            div()
                .flex()
                .flex_col()
                .gap_1()
                .py_2()
                .border_b_1()
                .border_color(theme.paint_color(color::BORDER))
                .child(
                    div()
                        .text_color(theme.paint_color(status_color))
                        .child(format!("{label}: {name}")),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.paint_color(color::TEXT_MUTED))
                        .child(entry.directory.display().to_string()),
                )
                .when_some(cause, |row, cause| {
                    row.child(
                        div()
                            .text_color(theme.paint_color(color::ERROR))
                            .child(cause),
                    )
                })
        })
        .chain(setting_warnings.into_iter().map(|warning| {
            div()
                .py_2()
                .border_b_1()
                .border_color(theme.paint_color(color::BORDER))
                .text_color(theme.paint_color(color::WARNING))
                .child(warning)
        }))
        .collect::<Vec<_>>();
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_start()
        .justify_center()
        .pt(px(60.))
        .bg(theme.paint_color(color::SCRIM))
        .child(
            div()
                .w(px(760.))
                .h(px(520.))
                .flex()
                .flex_col()
                .bg(theme.paint_color(color::POPOVER_BACKGROUND))
                .border_1()
                .border_color(theme.paint_color(color::POPOVER_BORDER))
                .p_4()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .child(div().flex_1().text_lg().child(status))
                        .child(command_button(
                            "close",
                            "close-extension-report",
                            window,
                            SHOW_EXTENSION_REPORT_COMMAND,
                            theme.paint_color(color::TEXT_ACCENT),
                        )),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.paint_color(color::TEXT_MUTED))
                        .child(root),
                )
                .child(
                    div()
                        .id("extension-report-rows")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .when(rows.is_empty() && state == StartupState::Complete, |body| {
                            body.child("No installed extensions")
                        })
                        .children(rows),
                ),
        )
        .into_any_element()
}

pub(super) fn command_button(
    label: impl Into<SharedString>,
    id: &'static str,
    window: &Entity<KnotWindow>,
    command: &'static str,
    accent: Hsla,
) -> Stateful<Div> {
    let window = window.clone();
    let label = label.into();
    div()
        .id(id)
        .cursor_pointer()
        .text_color(accent)
        .child(label)
        .on_click(move |_, _, cx| {
            window.update(cx, |window, cx| {
                window.dispatch_source(ProductCommandSource::new(command).command(), cx);
            })
        })
}
