//! A window's notification popups: the cards the store says this window
//! shows, their timeouts, and the stack painted over the workbench.
//!
//! The window mirrors the store into `NoticePopups` whenever the store or a
//! setting changes. A card that leaves stays, fading out, until its exit
//! animation ends. Timers are revalidated by notice ID: one that fires for a
//! notice no longer shown does nothing.

use std::{collections::HashMap, time::Duration};

use super::*;
use crate::app::convert::ThemePaint as _;
use crate::app::icons::{AppIcon, defined_icon};
use crate::app::notifications::{self, ApplicationNotifications, Notice, Position, Severity};
use crate::app::theme::{chrome, color, theme};
use crate::app::{settings, tokens, zoom};

const CARD_WIDTH: f32 = 360.;
const EDGE_GAP: f32 = 12.;
const STACK_GAP: f32 = 8.;
const SLIDE: f32 = 28.;
const ENTER: Duration = Duration::from_millis(240);
const EXIT: Duration = Duration::from_millis(160);
const DEFAULT_TIMEOUT: f32 = 8.;
const MIN_TIMEOUT: f32 = 0.5;
const DEFAULT_MAX_VISIBLE: usize = 3;

struct Card {
    notice: Notice,
    /// Whether the card is fading out after leaving the store's popups.
    leaving: bool,
}

#[derive(Default)]
pub(super) struct NoticePopups {
    /// The cards painted, newest first.
    cards: Vec<Card>,
    /// How many notices the limit collapsed.
    more: usize,
    /// Whether the pointer is on the stack, which pauses every timer.
    hovered: bool,
    timers: HashMap<u64, Task<()>>,
    exits: HashMap<u64, Task<()>>,
}

impl NoticePopups {
    #[cfg(test)]
    pub(super) fn shown(&self) -> Vec<u64> {
        self.cards
            .iter()
            .filter(|card| !card.leaving)
            .map(|card| card.notice.id)
            .collect()
    }

    #[cfg(test)]
    pub(super) fn painted(&self) -> usize {
        self.cards.len()
    }

    #[cfg(test)]
    pub(super) fn more(&self) -> usize {
        self.more
    }
}

fn severity_icon(severity: Severity) -> (&'static str, &'static str) {
    match severity {
        Severity::Info => ("info", color::NOTIFICATION_INFO),
        Severity::Warning => ("triangle-alert", color::NOTIFICATION_WARNING),
        Severity::Error => ("circle-x", color::NOTIFICATION_ERROR),
    }
}

impl KnotWindow {
    /// Mirror the store into this window's popups, start the timers of cards
    /// that need one, and start the exit of cards that left.
    pub(super) fn sync_notices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = window_id(cx.entity_id());
        let popups = if settings::boolean(settings::NOTIFICATIONS_POPUPS, cx) {
            let limit = settings::number(settings::NOTIFICATIONS_MAX_VISIBLE, cx)
                .map_or(DEFAULT_MAX_VISIBLE, |count| count.max(0.) as usize);
            cx.try_global::<ApplicationNotifications>()
                .map(|notifications| notifications.0.popups(&key, limit))
                .unwrap_or_default()
        } else {
            notifications::Popups::default()
        };
        let mut cards = popups
            .shown
            .into_iter()
            .map(|notice| Card {
                notice,
                leaving: false,
            })
            .collect::<Vec<_>>();
        let mut newly_leaving = Vec::new();
        for card in std::mem::take(&mut self.notices.cards) {
            if cards.iter().any(|shown| shown.notice.id == card.notice.id) {
                continue;
            }
            if !card.leaving {
                newly_leaving.push(card.notice.id);
            }
            cards.push(Card {
                notice: card.notice,
                leaving: true,
            });
        }
        cards.sort_by_key(|card| std::cmp::Reverse(card.notice.id));
        self.notices.cards = cards;
        self.notices.more = popups.more;

        let timed = if self.notices.hovered {
            Vec::new()
        } else {
            self.notices
                .cards
                .iter()
                .filter(|card| !card.leaving && card.notice.auto_dismiss)
                .map(|card| card.notice.id)
                .collect()
        };
        self.notices.timers.retain(|id, _| timed.contains(id));
        let timeout = Duration::from_secs_f32(
            settings::number(settings::NOTIFICATIONS_TIMEOUT, cx)
                .unwrap_or(DEFAULT_TIMEOUT)
                .max(MIN_TIMEOUT),
        );
        for id in timed {
            if self.notices.timers.contains_key(&id) {
                continue;
            }
            let task = cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(timeout).await;
                this.update_in(cx, |this, _, cx| this.expire_notice(id, cx))
                    .ok();
            });
            self.notices.timers.insert(id, task);
        }
        for id in newly_leaving {
            let task = cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(EXIT).await;
                this.update_in(cx, |this, window, _| {
                    this.notices
                        .cards
                        .retain(|card| !(card.leaving && card.notice.id == id));
                    this.notices.exits.remove(&id);
                    window.refresh();
                })
                .ok();
            });
            self.notices.exits.insert(id, task);
        }
        window.refresh();
    }

    /// Hide a popup whose timeout ended, unless it already went.
    fn expire_notice(&mut self, id: u64, cx: &mut Context<Self>) {
        let key = window_id(cx.entity_id());
        notifications::update(cx, |store| store.hide_popup(id, &key));
    }

    /// Close a popup, which marks its notice read.
    fn close_notice(&mut self, id: u64, cx: &mut Context<Self>) {
        notifications::update(cx, |store| store.mark_read(&[id]));
    }

    /// Run one of a notice's commands in this window as a status item click
    /// does, then close the popup.
    fn run_notice_action(&mut self, id: u64, index: usize, cx: &mut Context<Self>) {
        let command = cx
            .try_global::<ApplicationNotifications>()
            .and_then(|notifications| notifications.0.get(id))
            .and_then(|notice| notice.actions.get(index))
            .map(|action| action.command.clone());
        if let Some(command) = command {
            let target = self.capture_command_target(cx);
            self.dispatch_command(
                knot_protocol::Command {
                    name: command.name.into(),
                    arguments: command.arguments,
                },
                target,
                cx,
            );
        }
        self.close_notice(id, cx);
    }

    /// The popup stack, in the corner `notifications.position` names.
    pub(super) fn render_notices(
        &self,
        toolbar: bool,
        status_bar: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.notices.cards.is_empty() {
            return None;
        }
        let position = Position::parse(&settings::text(settings::NOTIFICATIONS_POSITION, cx));
        let mut cards = self
            .notices
            .cards
            .iter()
            .map(|card| self.render_notice(card, position, cx))
            .collect::<Vec<_>>();
        let pill = (self.notices.more > 0).then(|| self.render_more_pill(cx));
        // Newest sits nearest the edge the stack grows from.
        if position.is_top() {
            cards.extend(pill);
        } else {
            cards.reverse();
            cards.splice(0..0, pill);
        }
        let stack = div()
            .id("notice-stack")
            .debug_selector(|| "notice-stack".to_owned())
            .flex()
            .flex_col()
            .gap(zoom::rems(STACK_GAP))
            .w(zoom::rems(CARD_WIDTH))
            // The stack hides the workbench under it from the pointer.
            .occlude()
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                this.notices.hovered = *hovered;
                this.sync_notices(window, cx);
            }))
            .children(cards);
        let edge = zoom::rems(EDGE_GAP);
        let row = div()
            .absolute()
            .left_0()
            .right_0()
            .flex()
            .flex_row()
            .px(edge)
            .map(|row| match position {
                Position::BottomLeft | Position::TopLeft => row.justify_start(),
                Position::TopCenter => row.justify_center(),
                Position::BottomRight | Position::TopRight => row.justify_end(),
            });
        Some(
            if position.is_top() {
                let toolbar = if toolbar {
                    tokens::chrome::TOOLBAR_HEIGHT
                } else {
                    0.
                };
                row.top(zoom::rems(toolbar + EDGE_GAP))
            } else {
                let bar = if status_bar {
                    tokens::chrome::STATUS_BAR_HEIGHT
                } else {
                    0.
                };
                row.bottom(zoom::rems(bar + EDGE_GAP))
            }
            .child(stack)
            .into_any_element(),
        )
    }

    fn render_more_pill(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = theme(cx);
        div()
            .id("notice-more")
            .debug_selector(|| "notice-more".to_owned())
            .flex()
            .flex_row()
            .justify_center()
            .child(
                div()
                    .px(zoom::rems(10.))
                    .py(zoom::rems(3.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme.paint_color(color::NOTIFICATION_BORDER))
                    .bg(theme.paint_color(color::NOTIFICATION_BACKGROUND))
                    .text_size(zoom::rems(tokens::chrome::STATUS_TEXT_SIZE))
                    .text_color(theme.paint_color(color::NOTIFICATION_DESCRIPTION_FOREGROUND))
                    .child(format!("+{} more", self.notices.more)),
            )
            .into_any_element()
    }

    fn render_notice(&self, card: &Card, position: Position, cx: &mut Context<Self>) -> AnyElement {
        let Card { notice, leaving } = card;
        let id = notice.id;
        let theme = theme(cx);
        let (icon_name, accent) = severity_icon(notice.severity);
        let icon = AppIcon::new(
            defined_icon(icon_name, cx)
                .unwrap_or_else(|| knot_model::icons::IconDefinition::lucide(icon_name)),
        )
        .tinted(Some(accent.to_owned()));
        let group = SharedString::from(format!("notice-{id}"));
        let foreground = theme.paint_color(color::NOTIFICATION_FOREGROUND);
        let description = theme.paint_color(color::NOTIFICATION_DESCRIPTION_FOREGROUND);
        let hover = theme.paint_color(color::BUTTON_HOVER_BACKGROUND);
        let actions = notice
            .actions
            .iter()
            .enumerate()
            .map(|(index, action)| {
                let selector = format!("notice-action-{id}-{index}");
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .px(zoom::rems(8.))
                    .py(zoom::rems(2.))
                    .rounded(zoom::rems(tokens::radius::SM))
                    .cursor_pointer()
                    .text_size(zoom::rems(tokens::chrome::STATUS_TEXT_SIZE))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.paint_color(color::TEXT_ACCENT))
                    .hover(move |style| style.bg(hover))
                    .child(action.title.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.run_notice_action(id, index, cx);
                    }))
            })
            .collect::<Vec<_>>();
        let close_selector = format!("notice-close-{id}");
        let close = div()
            .id(SharedString::from(close_selector.clone()))
            .debug_selector(move || close_selector.clone())
            .absolute()
            .top(zoom::rems(8.))
            .right(zoom::rems(8.))
            .size(zoom::rems(18.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(zoom::rems(tokens::radius::SM))
            .cursor_pointer()
            .text_color(description)
            .opacity(0.)
            .group_hover(group.clone(), |style| style.opacity(1.))
            .hover(move |style| style.bg(hover))
            .child("×")
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| this.close_notice(id, cx)));
        let body = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(zoom::rems(2.))
            .pr(zoom::rems(14.))
            .when_some(notice.title.clone(), |body, title| {
                body.child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(foreground)
                        .child(title),
                )
            })
            .child(
                div()
                    .line_clamp(4)
                    .text_size(zoom::rems(tokens::chrome::STATUS_TEXT_SIZE + 0.5))
                    .text_color(if notice.title.is_some() {
                        description
                    } else {
                        foreground
                    })
                    .child(notice.message.clone()),
            )
            .when(!actions.is_empty(), |body| {
                body.child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(zoom::rems(4.))
                        .mt(zoom::rems(6.))
                        .ml(zoom::rems(-8.))
                        .children(actions),
                )
            });
        let card = div()
            .id(SharedString::from(format!("notice-{id}")))
            .debug_selector(move || format!("notice-{id}"))
            .group(group)
            .relative()
            .flex()
            .flex_row()
            .items_start()
            .gap(zoom::rems(10.))
            .w_full()
            .p(zoom::rems(12.))
            .rounded(zoom::rems(chrome(cx).card_radius.max(tokens::radius::MD)))
            .border_1()
            .border_color(theme.paint_color(color::NOTIFICATION_BORDER))
            .bg(theme.paint_color(color::NOTIFICATION_BACKGROUND))
            .shadow_lg()
            .text_size(zoom::rems(tokens::list::TEXT_SIZE))
            .text_color(foreground)
            .child(div().pt(zoom::rems(1.)).child(icon))
            .child(body)
            .child(close);
        let from_left = matches!(position, Position::BottomLeft | Position::TopLeft);
        let from_top = position == Position::TopCenter;
        let slide = move |card: Stateful<Div>, remaining: f32| {
            let offset = px(SLIDE * remaining);
            let card = card.relative();
            if from_top {
                card.top(-offset)
            } else if from_left {
                card.left(-offset)
            } else {
                card.left(offset)
            }
        };
        if *leaving {
            card.with_animation(
                ElementId::NamedInteger("notice-exit".into(), id),
                Animation::new(EXIT).with_easing(ease_in_out),
                move |card, delta| slide(card, delta).opacity(1. - delta),
            )
            .into_any_element()
        } else {
            card.with_animation(
                ElementId::NamedInteger("notice-enter".into(), id),
                Animation::new(ENTER).with_easing(ease_out_quint()),
                move |card, delta| slide(card, 1. - delta).opacity(delta),
            )
            .into_any_element()
        }
    }
}
