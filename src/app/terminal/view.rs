use crate::app::convert::ThemePaint as _;
use gpui::prelude::FluentBuilder;
use gpui::*;
use knot_terminal::{
    CellWidth, CursorStyle as TerminalCursor, Key, KeyEvent, Mode, MouseAction,
    MouseButton as ReportButton, MouseEvent as ReportEvent, Point as TerminalPoint, RenderState,
    Rgb, Scroll, TerminalSize, Underline,
};
use std::ops::Range;
use std::rc::Rc;

use crate::app::keys::Keystroke;

use crate::app::{
    TERMINAL_KEY_CONTEXT,
    settings::{self, ApplicationSettings, SettingsScope},
    terminal::colors::{TerminalPalette, dim},
    terminal::input::{InputModes, MouseTracking, terminal_key_bytes},
    terminal::layout::TerminalLayout,
    terminal::links::{LinkGrid, LinkGridBuilder, LinkTarget},
    terminal::mouse::{
        PointerOwner, PointerState, WheelRoute, report_button, report_modifiers, wheel_route,
    },
    terminal::session::{TerminalAttachment, TerminalSession, TerminalStatus, lock},
    theme::{color, theme},
    typography::{self, bold, italic},
};

/// The settings scope of terminals.
fn terminal_scope() -> SettingsScope {
    SettingsScope {
        view: Some(TERMINAL_KEY_CONTEXT.into()),
        ..Default::default()
    }
}

/// The terminal's text size, from `font.size` in the `terminal` view scope
/// and the application zoom.
fn font_size(cx: &App) -> f32 {
    settings::font_size(&terminal_scope(), cx) * crate::app::zoom::factor(cx)
}

/// The terminal's font settings, from `font.*` in the `terminal` view scope.
fn font_settings(cx: &App) -> knot_model::typography::FontSettings {
    settings::font_settings(&terminal_scope(), cx)
}

type LinkOpener = Rc<dyn Fn(&str, &mut App)>;

/// Native terminal presentation attached to one stable session.
pub(crate) struct TerminalView {
    session: Entity<TerminalSession>,
    attachment: Option<TerminalAttachment>,
    _session_subscription: Subscription,
    _settings_subscription: Subscription,
    focus: FocusHandle,
    pointer: PointerState,
    /// The generation and modes the pointer's gestures began under.
    pointer_epoch: Option<(u64, InputModes)>,
    pointer_position: Option<Point<Pixels>>,
    pointer_modifiers: Modifiers,
    /// The grid position of the last report sent, in the terminal's cell
    /// pixels.
    reported_position: Option<(f32, f32)>,
    interaction_bounds: Bounds<Pixels>,
    layout: Option<TerminalLayout>,
    focus_subscriptions: Vec<Subscription>,
    render_state: RenderState,
    /// Where a local left press began, while it is held.
    selection_anchor: Option<TerminalPoint>,
    local_dragged: bool,
    /// The links of the last painted frame.
    links: Option<LinkGrid>,
    hovered_link: Option<LinkTarget>,
    armed_link: Option<ArmedLink>,
    open_link: LinkOpener,
}

/// A link pressed with the link modifier, opened if the release finds it
/// unchanged.
struct ArmedLink {
    grid: LinkGrid,
    target: LinkTarget,
    generation: u64,
    bounds: Bounds<Pixels>,
    layout: TerminalLayout,
    position: Point<Pixels>,
}

/// Command-click on macOS and Control-click elsewhere open links; Shift
/// selects instead.
fn link_modifier(modifiers: Modifiers) -> bool {
    !modifiers.shift
        && if cfg!(target_os = "macos") {
            modifiers.platform
        } else {
            modifiers.control
        }
}

/// The armed link moves at most this far before the gesture is a drag.
const LINK_SLOP: Pixels = px(4.);

fn within_slop(a: Point<Pixels>, b: Point<Pixels>) -> bool {
    (a.x - b.x).abs() <= LINK_SLOP && (a.y - b.y).abs() <= LINK_SLOP
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum TerminalCommand {
    Copy,
    Paste,
    ToggleMouseReporting,
    Restart,
}

impl TerminalView {
    pub(crate) fn execute_terminal_command(
        &mut self,
        command: TerminalCommand,
        cx: &mut Context<Self>,
    ) -> crate::app::commands::dispatch::CommandClaim {
        use crate::app::commands::dispatch::CommandClaim;
        if self.attachment.is_none() {
            return CommandClaim::Declined;
        }
        if command == TerminalCommand::ToggleMouseReporting {
            self.cancel_pointer(cx);
            self.session
                .update(cx, |session, cx| session.toggle_mouse_reporting(cx));
            cx.notify();
            return CommandClaim::Finished(knot_protocol::CommandOutcome::Completed);
        }
        if command == TerminalCommand::Restart {
            self.restart(cx);
            return CommandClaim::Finished(knot_protocol::CommandOutcome::Completed);
        }
        if command == TerminalCommand::Paste {
            if self.session.read(cx).status() != TerminalStatus::Running {
                return CommandClaim::Declined;
            }
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                let bracketed = self
                    .session
                    .read(cx)
                    .with_terminal(|terminal| terminal.mode(Mode::BracketedPaste))
                    .unwrap_or(false);
                let text = if bracketed {
                    format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
                } else {
                    text.replace("\r\n", "\r").replace('\n', "\r")
                };
                self.send_user_input(text.into_bytes(), cx);
            }
            return CommandClaim::Finished(knot_protocol::CommandOutcome::Completed);
        }
        let selected = self
            .session
            .read(cx)
            .with_terminal(|terminal| terminal.selection_text())
            .flatten()
            .filter(|text| !text.is_empty());
        if let Some(text) = selected {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            CommandClaim::Finished(knot_protocol::CommandOutcome::Completed)
        } else {
            CommandClaim::Declined
        }
    }

    pub(crate) fn new(session: Entity<TerminalSession>, cx: &mut Context<Self>) -> Self {
        let attachment = session
            .update(cx, |session, _| session.attach())
            .expect("terminal session already has a view");
        let subscription = cx.observe(&session, |this, _, cx| {
            this.clear_links();
            this.sync_pointer(cx);
            cx.notify();
        });
        // Cells are measured during layout, so a settings change only needs
        // a repaint, which resizes the grid when the cells change.
        let settings_subscription = cx.observe_global::<ApplicationSettings>(|_, cx| cx.notify());
        cx.on_release(|view, cx| {
            view.cancel_pointer(cx);
            view.release_focus(cx);
        })
        .detach();
        Self {
            session,
            attachment: Some(attachment),
            _session_subscription: subscription,
            _settings_subscription: settings_subscription,
            focus: cx.focus_handle(),
            pointer: PointerState::default(),
            pointer_epoch: None,
            pointer_position: None,
            pointer_modifiers: Modifiers::default(),
            reported_position: None,
            interaction_bounds: Bounds::default(),
            layout: None,
            focus_subscriptions: Vec::new(),
            render_state: RenderState::new(),
            selection_anchor: None,
            local_dragged: false,
            links: None,
            hovered_link: None,
            armed_link: None,
            open_link: Rc::new(|uri, cx| cx.open_url(uri)),
        }
    }

    /// Release the sole presentation before replacing or moving this view.
    pub(crate) fn detach(&mut self, cx: &mut Context<Self>) {
        self.cancel_pointer(cx);
        self.release_focus(cx);
        self.attachment = None;
        self.layout = None;
        self.reset_pointer();
        cx.notify();
    }

    pub(crate) fn is_attached_to(&self, session: &Entity<TerminalSession>) -> bool {
        self.attachment.is_some() && &self.session == session
    }

    pub(crate) fn restart(&mut self, cx: &mut Context<Self>) {
        if self.attachment.is_none() {
            return;
        }
        self.cancel_pointer(cx);
        self.session.update(cx, |session, cx| session.restart(cx));
        self.reset_pointer();
    }

    pub(crate) fn resize(&mut self, size: TerminalSize, cx: &mut Context<Self>) {
        if self.attachment.is_none() {
            return;
        }
        if self.session.read(cx).size() != size {
            self.cancel_pointer(cx);
        }
        self.session.update(cx, |session, _| session.resize(size));
    }

    /// Send a key the keymap left unclaimed to the program; false when it
    /// has no encoding or the session is not running.
    pub(crate) fn key_down(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        if self.attachment.is_none() || self.session.read(cx).status() != TerminalStatus::Running {
            return false;
        }
        let bytes = self
            .session
            .read(cx)
            .with_terminal(|terminal| terminal_key_bytes(keystroke, terminal))
            .unwrap_or_default();
        if bytes.is_empty() {
            return false;
        }
        self.send_user_input(bytes, cx);
        true
    }

    /// Send typed `text` to the program.
    pub(crate) fn insert_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.send_user_input(text.as_bytes().to_vec(), cx);
    }

    fn release_focus(&mut self, cx: &mut App) {
        if let Some(attachment) = &self.attachment {
            self.session
                .update(cx, |session, _| session.set_focus(attachment, false));
        }
    }

    fn sync_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.attachment.is_some() {
            let focused = self.focus.is_focused(window) && window.is_window_active();
            if !focused {
                self.cancel_pointer(cx);
            }
            let attachment = self.attachment.as_ref().unwrap();
            self.session
                .update(cx, |session, _| session.set_focus(attachment, focused));
            cx.notify();
        }
    }

    fn send_user_input(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        if self.attachment.is_none() {
            return;
        }
        if bytes.is_empty() || self.session.read(cx).status() != TerminalStatus::Running {
            return;
        }
        self.cancel_pointer(cx);
        let attachment = self.attachment.as_ref().unwrap();
        let session = self.session.read(cx);
        session.with_terminal(|terminal| terminal.scroll_viewport(Scroll::Bottom));
        session.send_input(attachment, session.generation(), bytes);
        self.reset_pointer();
        cx.notify();
    }

    fn reset_pointer(&mut self) {
        self.clear_gestures();
        self.pointer_epoch = None;
        self.pointer_position = None;
        self.pointer_modifiers = Modifiers::default();
    }

    fn clear_gestures(&mut self) {
        self.clear_links();
        self.pointer.clear();
        self.reported_position = None;
        self.selection_anchor = None;
        self.local_dragged = false;
    }

    fn clear_links(&mut self) {
        self.hovered_link = None;
        self.armed_link = None;
    }

    /// The painted link under `position`, if the pointer is over the grid.
    fn link_at(&self, position: Point<Pixels>, cx: &App) -> Option<&LinkTarget> {
        self.attachment.as_ref()?;
        if !self.interaction_bounds.contains(&position) {
            return None;
        }
        let (column, row) = self.viewport_cell(position, cx)?;
        self.links.as_ref()?.at(column, row)
    }

    fn hover_link(&mut self, position: Point<Pixels>, hovered: bool, cx: &mut Context<Self>) {
        let target = hovered
            .then(|| self.link_at(position, cx).cloned())
            .flatten();
        if target != self.hovered_link {
            self.hovered_link = target;
            cx.notify();
        }
    }

    /// The links the terminal displays now, rather than at the last paint.
    fn current_links(&mut self, cx: &App) -> Option<LinkGrid> {
        self.attachment.as_ref()?;
        let terminal = self.session.read(cx).terminal()?;
        Some(LinkGrid::capture(&mut self.render_state, terminal))
    }

    /// Opens the armed link if the release finds it where it was pressed,
    /// in the same frame, process, and geometry.
    fn activate_link(&mut self, armed: ArmedLink, event: &MouseUpEvent, cx: &mut Context<Self>) {
        if !link_modifier(event.modifiers)
            || !self.interaction_bounds.contains(&event.position)
            || !within_slop(event.position, armed.position)
            || armed.generation != self.session.read(cx).generation()
            || armed.bounds != self.interaction_bounds
            || Some(armed.layout) != self.layout
        {
            return;
        }
        let current = self.current_links(cx);
        let cell = self.viewport_cell(event.position, cx);
        if current.as_ref() == Some(&armed.grid)
            && cell.is_some_and(|(column, row)| {
                current.as_ref().and_then(|grid| grid.at(column, row)) == Some(&armed.target)
            })
        {
            (self.open_link)(&armed.target.uri, cx);
        }
    }

    /// Starts a local selection at `position`, clearing the previous one.
    fn start_selection(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((column, row)) = self.viewport_cell(position, cx) else {
            return;
        };
        self.selection_anchor = Some(TerminalPoint::new(column as u16, row as u16));
        self.local_dragged = false;
        self.session
            .read(cx)
            .with_terminal(|terminal| terminal.clear_selection());
        cx.notify();
    }

    /// Selects from the anchor to the cell under `position`. The terminal
    /// keeps the anchor on its text as the viewport scrolls.
    fn extend_selection(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let (Some(anchor), Some((column, row))) =
            (self.selection_anchor, self.viewport_cell(position, cx))
        else {
            return;
        };
        let to = TerminalPoint::new(column as u16, row as u16);
        let first = !self.local_dragged;
        self.local_dragged = true;
        self.session.read(cx).with_terminal(|terminal| {
            if first {
                terminal.select(anchor, to);
            } else {
                terminal.extend_selection(to);
            }
        });
        cx.notify();
    }

    fn cancel_pointer(&mut self, cx: &App) {
        if let Some(position) = self.pointer_position {
            let session = self.session.read(cx);
            let mode = session
                .with_terminal(|terminal| InputModes::read(terminal))
                .unwrap_or_default();
            if self.pointer_epoch == Some((session.generation(), mode))
                && mode.mouse_reporting()
                && let Some(at) = self.report_position(position, cx)
            {
                for button in self.pointer.application_buttons() {
                    let release = self.release_bytes(button, self.pointer_modifiers, at, cx);
                    self.send_pointer(release, cx);
                }
            }
        }
        self.reset_pointer();
    }

    fn sync_pointer(&mut self, cx: &App) -> Option<InputModes> {
        if self.attachment.is_none() {
            self.reset_pointer();
            return None;
        }
        let session = self.session.read(cx);
        let mut mode = if session.status() == TerminalStatus::Running {
            session
                .with_terminal(|terminal| InputModes::read(terminal))
                .unwrap_or_default()
        } else {
            InputModes::default()
        };
        if !session.mouse_reporting_enabled() {
            mode.mouse = MouseTracking::Off;
        }
        let epoch = (session.generation(), mode);
        if self.pointer_epoch.is_some_and(|previous| previous != epoch) {
            self.clear_gestures();
        }
        self.pointer_epoch = Some(epoch);
        Some(mode)
    }

    fn viewport_cell(&self, position: Point<Pixels>, cx: &App) -> Option<(usize, usize)> {
        let layout = self.layout?;
        let size = self.session.read(cx).size();
        Some(layout.cell_at(
            position,
            self.interaction_bounds,
            size.columns.into(),
            size.rows.into(),
        ))
    }

    /// Where `position` falls on the grid in the terminal's cell pixels,
    /// clamped to the grid, so reports name the cell `viewport_cell` finds.
    fn report_position(&self, position: Point<Pixels>, cx: &App) -> Option<(f32, f32)> {
        let layout = self.layout?;
        let size = self.session.read(cx).size();
        let (column, row) = self.viewport_cell(position, cx)?;
        let origin = layout.origin(self.interaction_bounds, column, row);
        let within = |offset: Pixels, cell: Pixels| (offset / cell).clamp(0., 0.99);
        Some((
            (column as f32 + within(position.x - origin.x, layout.cell_width))
                * size.cell_width as f32,
            (row as f32 + within(position.y - origin.y, layout.cell_height))
                * size.cell_height as f32,
        ))
    }

    fn report(
        &self,
        action: MouseAction,
        button: Option<ReportButton>,
        modifiers: Modifiers,
        (x, y): (f32, f32),
        cx: &App,
    ) -> Vec<u8> {
        let event = ReportEvent {
            action,
            button,
            modifiers: report_modifiers(modifiers),
            x,
            y,
        };
        self.session
            .read(cx)
            .with_terminal(|terminal| {
                terminal.encode_mouse(&event, action == MouseAction::Motion && button.is_some())
            })
            .unwrap_or_default()
    }

    fn release_bytes(
        &self,
        button: MouseButton,
        modifiers: Modifiers,
        at: (f32, f32),
        cx: &App,
    ) -> Vec<u8> {
        let release = |at| {
            self.report(
                MouseAction::Release,
                Some(report_button(button)),
                modifiers,
                at,
                cx,
            )
        };
        let bytes = release(at);
        if bytes.is_empty() {
            // Balance the press where the last report was when the pointer
            // is past what the format can represent.
            self.reported_position.map(release).unwrap_or_default()
        } else {
            bytes
        }
    }

    /// Sends a motion report unless it repeats the last one.
    fn send_motion(&mut self, bytes: Vec<u8>, at: (f32, f32), cx: &App) {
        if bytes.is_empty() || !self.pointer.motion_changed(&bytes) {
            return;
        }
        self.reported_position = Some(at);
        self.send_pointer(bytes, cx);
    }

    fn send_pointer(&self, bytes: Vec<u8>, cx: &App) {
        if let Some(attachment) = &self.attachment {
            let session = self.session.read(cx);
            session.send_input(attachment, session.generation(), bytes);
        }
    }

    fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mode) = self.sync_pointer(cx) else {
            return false;
        };
        let Some((column, row)) = self.viewport_cell(event.position, cx) else {
            return false;
        };
        self.pointer_position = Some(event.position);
        self.pointer_modifiers = event.modifiers;
        self.focus.focus(window, cx);
        if event.button == MouseButton::Left
            && link_modifier(event.modifiers)
            && let (Some(grid), Some(layout)) = (self.links.clone(), self.layout)
            && let Some(target) = grid.at(column, row).cloned()
        {
            self.armed_link = Some(ArmedLink {
                grid,
                target,
                generation: self.session.read(cx).generation(),
                bounds: self.interaction_bounds,
                layout,
                position: event.position,
            });
            self.pointer.press(event.button, PointerOwner::Link);
            return true;
        }
        if mode.mouse_reporting() && !event.modifiers.shift {
            let at = self.report_position(event.position, cx).unwrap_or_default();
            let button = Some(report_button(event.button));
            let bytes = self.report(MouseAction::Press, button, event.modifiers, at, cx);
            if !bytes.is_empty() {
                self.reported_position = Some(at);
                self.pointer.press(event.button, PointerOwner::Application);
                // A drag that stays in the pressed cell reports nothing.
                let drag = self.report(MouseAction::Motion, button, event.modifiers, at, cx);
                self.pointer.motion_changed(&drag);
                self.send_pointer(bytes, cx);
            }
            return true;
        }
        if event.button == MouseButton::Left {
            self.pointer.press(event.button, PointerOwner::Local);
            self.start_selection(event.position, cx);
            return true;
        }
        false
    }

    fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        hovered: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mode) = self.sync_pointer(cx) else {
            return false;
        };
        self.pointer_position = Some(event.position);
        self.pointer_modifiers = event.modifiers;
        self.hover_link(event.position, hovered, cx);
        if let Some(button) = event.pressed_button {
            match self.pointer.owner(button) {
                Some(PointerOwner::Link) => {
                    if !link_modifier(event.modifiers)
                        || self
                            .armed_link
                            .as_ref()
                            .is_some_and(|armed| !within_slop(event.position, armed.position))
                    {
                        self.armed_link = None;
                    }
                    return true;
                }
                Some(PointerOwner::Local) => {
                    if button == MouseButton::Left {
                        self.extend_selection(event.position, cx);
                    }
                    return true;
                }
                Some(PointerOwner::Application) => {
                    if matches!(mode.mouse, MouseTracking::Drag | MouseTracking::Motion)
                        && let Some(at) = self.report_position(event.position, cx)
                    {
                        let bytes = self.report(
                            MouseAction::Motion,
                            Some(report_button(button)),
                            event.modifiers,
                            at,
                            cx,
                        );
                        self.send_motion(bytes, at, cx);
                    }
                    return true;
                }
                None => return false,
            }
        }
        if !hovered {
            self.pointer.leave();
            return false;
        }
        if mode.mouse == MouseTracking::Motion && !event.modifiers.shift {
            if let Some(at) = self.report_position(event.position, cx) {
                let bytes = self.report(MouseAction::Motion, None, event.modifiers, at, cx);
                self.send_motion(bytes, at, cx);
            }
            return true;
        }
        false
    }

    fn mouse_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) -> bool {
        if self.sync_pointer(cx).is_none() {
            return false;
        }
        self.pointer_position = Some(event.position);
        self.pointer_modifiers = event.modifiers;
        match self.pointer.release(event.button) {
            Some(PointerOwner::Application) => {
                if let Some(at) = self.report_position(event.position, cx) {
                    let release = self.release_bytes(event.button, event.modifiers, at, cx);
                    self.send_pointer(release, cx);
                }
                true
            }
            Some(PointerOwner::Link) => {
                if let Some(armed) = self.armed_link.take() {
                    self.activate_link(armed, event, cx);
                }
                true
            }
            Some(PointerOwner::Local) => {
                if self.local_dragged {
                    self.extend_selection(event.position, cx);
                }
                self.selection_anchor = None;
                self.local_dragged = false;
                true
            }
            None => false,
        }
    }

    fn scroll(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) -> bool {
        let Some(mode) = self.sync_pointer(cx) else {
            return false;
        };
        let Some(layout) = self.layout else {
            return false;
        };
        self.clear_links();
        let route = wheel_route(mode, event.modifiers.shift);
        let (horizontal, vertical) =
            self.pointer
                .wheel_steps(event.delta, layout.cell_width, layout.cell_height, route);
        match route {
            WheelRoute::Local => {
                if vertical != 0 {
                    // The wheel scrolls up with positive steps, the viewport
                    // with negative deltas.
                    self.session.read(cx).with_terminal(|terminal| {
                        terminal.scroll_viewport(Scroll::Delta(-(vertical as isize)))
                    });
                    cx.notify();
                }
            }
            WheelRoute::Application => {
                if let Some(at) = self.report_position(event.position, cx) {
                    let mut bytes = Vec::new();
                    for (steps, positive, negative) in [
                        (vertical, ReportButton::WheelUp, ReportButton::WheelDown),
                        (
                            horizontal,
                            ReportButton::WheelLeft,
                            ReportButton::WheelRight,
                        ),
                    ] {
                        let button = if steps > 0 { positive } else { negative };
                        bytes.extend(
                            self.report(MouseAction::Press, Some(button), event.modifiers, at, cx)
                                .repeat(steps.unsigned_abs() as usize),
                        );
                    }
                    self.send_pointer(bytes, cx);
                }
            }
            WheelRoute::Alternate => {
                let mut bytes = Vec::new();
                for (steps, positive, negative) in [
                    (vertical, Key::Up, Key::Down),
                    (horizontal, Key::Left, Key::Right),
                ] {
                    let event = KeyEvent {
                        key: Some(if steps > 0 { positive } else { negative }),
                        ..KeyEvent::default()
                    };
                    let key = self
                        .session
                        .read(cx)
                        .with_terminal(|terminal| terminal.encode_key(&event))
                        .unwrap_or_default();
                    bytes.extend(key.repeat(steps.unsigned_abs() as usize));
                }
                self.send_pointer(bytes, cx);
            }
        }
        true
    }

    #[cfg(all(test, unix))]
    pub(crate) fn interaction_bounds(&self) -> Bounds<Pixels> {
        self.interaction_bounds
    }
}

#[cfg(test)]
mod tests;

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_subscriptions.is_empty() {
            self.focus_subscriptions = vec![
                cx.on_focus(&self.focus.clone(), window, Self::sync_focus),
                cx.on_blur(&self.focus.clone(), window, Self::sync_focus),
                cx.observe_window_activation(window, Self::sync_focus),
            ];
            self.sync_focus(window, cx);
        }
        let entity = cx.entity();
        let status = self.session.read(cx).status();
        div()
            .size_full()
            .relative()
            .bg(theme(cx).paint_color(color::TERMINAL_BACKGROUND))
            .occlude()
            .child(
                div()
                    .size_full()
                    .track_focus(&self.focus)
                    .key_context(TERMINAL_KEY_CONTEXT)
                    .child(TerminalElement { entity }),
            )
            .when(terminal_overlay_label(&status).is_some(), |this| {
                let label = terminal_overlay_label(&status).unwrap_or_default();
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap_1()
                                .px_4()
                                .py_2()
                                .rounded_md()
                                .border_1()
                                .border_color(theme(cx).paint_color(color::BORDER))
                                .bg(theme(cx).paint_color(color::TAB_BAR_BACKGROUND))
                                .text_xs()
                                .text_color(theme(cx).paint_color(color::TEXT_MUTED))
                                .child(label)
                                .child(
                                    div()
                                        .id("restart-terminal")
                                        .cursor_pointer()
                                        .text_color(theme(cx).paint_color(color::TEXT_ACCENT))
                                        .child("restart")
                                        .on_click(cx.listener(|this, _, _, cx| this.restart(cx))),
                                ),
                        ),
                )
            })
    }
}

/// The overlay caption for a non-running session; `None` while running,
/// where the grid paints alone with no header chrome. The exit payload is
/// already a plain description (`exit code 0`, a signal line), so it is
/// shown verbatim instead of gaining a redundant prefix.
fn terminal_overlay_label(status: &TerminalStatus) -> Option<String> {
    match status {
        TerminalStatus::Running => None,
        TerminalStatus::Exited(status) => Some(status.clone()),
        TerminalStatus::Failed(error) => Some(format!("failed: {error}")),
        TerminalStatus::Closed => Some("closed".to_owned()),
    }
}

struct TerminalElement {
    entity: Entity<TerminalView>,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct RenderCell {
    line: usize,
    column: usize,
    text: String,
    wide: bool,
    foreground: Rgb,
    background: Rgb,
    style: knot_terminal::Style,
    selected: bool,
}

/// The index ranges of `cells` to shape together. With `ligatures`,
/// consecutive single-width ASCII punctuation cells on one line that draw
/// identically form one run, so ligatures such as `->` or `!==` can form;
/// every other cell is its own run.
fn shaping_runs(
    cells: &[RenderCell],
    ligatures: bool,
    underlined: impl Fn(&RenderCell) -> bool,
) -> Vec<Range<usize>> {
    let joinable = |cell: &RenderCell| {
        ligatures
            && !cell.wide
            && !cell.style.invisible
            && cell.text.len() == 1
            && cell.text.as_bytes()[0].is_ascii_punctuation()
    };
    let looks = |cell: &RenderCell| {
        (
            cell.style.bold,
            cell.style.italic,
            cell.style.strikethrough,
            cell.foreground,
            underlined(cell),
        )
    };
    let mut runs: Vec<Range<usize>> = Vec::new();
    for (index, cell) in cells.iter().enumerate() {
        if let Some(run) = runs.last_mut()
            && run.end == index
            && joinable(cell)
            && joinable(&cells[run.start])
            && cells[index - 1].line == cell.line
            && cells[index - 1].column + 1 == cell.column
            && looks(&cells[run.start]) == looks(cell)
        {
            run.end += 1;
        } else {
            runs.push(index..index + 1);
        }
    }
    runs
}

struct RenderCursor {
    line: usize,
    column: usize,
    style: TerminalCursor,
    color: Option<Rgb>,
}

/// One frame of the viewport, read from the session's terminal.
#[derive(Default)]
struct TerminalSnapshot {
    cells: Vec<RenderCell>,
    cursor: Option<RenderCursor>,
    links: Option<LinkGrid>,
}

impl TerminalSnapshot {
    fn capture(view: &mut TerminalView, cx: &App) -> Self {
        if view.attachment.is_none() {
            return Self::default();
        }
        let session = view.session.read(cx);
        let Some(terminal) = session.terminal() else {
            return Self::default();
        };
        let state = &mut view.render_state;
        state.update(terminal);
        let colors = state.colors();
        let cursor = state.cursor();
        let cursor = cursor
            .position
            .filter(|_| cursor.visible)
            .map(|(column, line)| RenderCursor {
                line: line.into(),
                // A cursor on the tail of a wide character marks the whole glyph.
                column: usize::from(column).saturating_sub(usize::from(cursor.wide_tail)),
                style: if session.focused() {
                    cursor.style
                } else {
                    TerminalCursor::BlockHollow
                },
                color: colors.cursor,
            });
        let mut cells = Vec::new();
        let mut links = LinkGridBuilder::default();
        let mut rows = state.rows();
        while let Some(mut row) = rows.next() {
            let line = usize::from(row.index());
            let selection = row.selection();
            let wrapped = row.wrapped();
            let mut row_cells = row.cells();
            let mut column = 0;
            while let Some(cell) = row_cells.next() {
                links.push(&cell);
                let index = column;
                column += 1;
                if matches!(cell.width, CellWidth::SpacerTail | CellWidth::SpacerHead) {
                    continue;
                }
                let mut foreground = cell.foreground.unwrap_or(colors.foreground);
                let mut background = cell.background.unwrap_or(colors.background);
                if cell.style.inverse {
                    std::mem::swap(&mut foreground, &mut background);
                }
                if cell.style.faint {
                    foreground = dim(foreground);
                }
                cells.push(RenderCell {
                    line,
                    column: index,
                    text: cell.text.to_owned(),
                    wide: cell.width == CellWidth::Wide,
                    foreground,
                    background,
                    style: cell.style,
                    selected: selection.is_some_and(|(start, end)| {
                        (usize::from(start)..=usize::from(end)).contains(&index)
                    }),
                });
            }
            links.end_row(wrapped);
        }
        state.clean();
        Self {
            cells,
            cursor,
            links: Some(links.finish(&lock(terminal))),
        }
    }
}

/// `color` for gpui.
fn paint_rgb(color: Rgb) -> Hsla {
    rgb(color.packed()).into()
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let font_settings = font_settings(cx);
        let font = typography::font(&font_settings);
        let layout =
            TerminalLayout::measure(window, &font, px(font_size(cx)), font_settings.line_height);
        let size = layout.grid_size(bounds);
        let palette = TerminalPalette::new(theme(cx));
        self.entity.update(cx, |terminal, cx| {
            if terminal.interaction_bounds != bounds || terminal.layout != Some(layout) {
                terminal.cancel_pointer(cx);
            }
            terminal.interaction_bounds = bounds;
            terminal.layout = Some(layout);
            terminal.resize(size, cx);
            if terminal.attachment.is_some() {
                terminal
                    .session
                    .update(cx, |session, _| session.set_palette(palette));
            }
        });
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let scroll_hitbox = hitbox.clone();
        let entity = self.entity.clone();
        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && scroll_hitbox.should_handle_scroll(window)
                && entity.update(cx, |terminal, cx| terminal.scroll(event, cx))
            {
                cx.stop_propagation();
            }
        });
        let press_hitbox = hitbox.clone();
        let entity = self.entity.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && press_hitbox.is_hovered(window)
                && entity.update(cx, |terminal, cx| terminal.mouse_down(event, window, cx))
            {
                cx.stop_propagation();
            }
        });
        let motion_hitbox = hitbox.clone();
        let entity = self.entity.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                let hovered = motion_hitbox.is_hovered(window);
                if entity.update(cx, |terminal, cx| terminal.mouse_move(event, hovered, cx)) {
                    cx.stop_propagation();
                }
            }
        });
        let entity = self.entity.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
            if phase == DispatchPhase::Bubble
                && entity.update(cx, |terminal, cx| terminal.mouse_up(event, cx))
            {
                cx.stop_propagation();
            }
        });

        let modifier_entity = self.entity.clone();
        window.on_modifiers_changed(move |event, _, cx| {
            modifier_entity.update(cx, |terminal, cx| {
                terminal.pointer_modifiers = event.modifiers;
                if !link_modifier(event.modifiers) {
                    terminal.armed_link = None;
                }
                cx.notify();
            });
        });
        let (snapshot, hovered) = self.entity.update(cx, |view, cx| {
            let mut snapshot = TerminalSnapshot::capture(view, cx);
            view.links = snapshot.links.take();
            let hovered = view.hovered_link.clone().filter(|target| {
                view.links
                    .as_ref()
                    .is_some_and(|grid| grid.links_contain(target))
            });
            (snapshot, hovered)
        });
        if hovered.is_some() && link_modifier(window.modifiers()) {
            window.set_cursor_style(CursorStyle::PointingHand, hitbox);
        }
        let Some(layout) = self.entity.read(cx).layout else {
            return;
        };
        let cell_width = layout.cell_width;
        let cell_height = layout.cell_height;
        let mask = Some(ContentMask { bounds });
        let terminal_background = TerminalPalette::new(theme(cx)).background;
        let selection = theme(cx).paint_color(color::TERMINAL_SELECTION);
        let cursor_color = theme(cx).paint_color(color::TERMINAL_CURSOR);
        window.with_content_mask(mask, |window| {
            for cell in &snapshot.cells {
                let origin = layout.origin(bounds, cell.column, cell.line);
                if cell.background != terminal_background || cell.selected {
                    let width = if cell.wide {
                        cell_width * 2
                    } else {
                        cell_width
                    };
                    window.paint_quad(fill(
                        Bounds {
                            origin,
                            size: size(width, cell_height),
                        },
                        if cell.selected {
                            selection
                        } else {
                            paint_rgb(cell.background)
                        },
                    ));
                }
            }

            if let Some(cursor) = &snapshot.cursor {
                let cursor_color = cursor.color.map_or(cursor_color, paint_rgb);
                let origin = layout.origin(bounds, cursor.column, cursor.line);
                let cursor_bounds = match cursor.style {
                    TerminalCursor::Bar => Bounds {
                        origin,
                        size: size(px(2.), cell_height),
                    },
                    TerminalCursor::Underline => Bounds {
                        origin: point(origin.x, origin.y + cell_height - px(2.)),
                        size: size(cell_width, px(2.)),
                    },
                    TerminalCursor::BlockHollow => {
                        window.paint_quad(quad(
                            Bounds {
                                origin,
                                size: size(cell_width, cell_height),
                            },
                            0.,
                            transparent_black(),
                            px(1.),
                            cursor_color,
                            BorderStyle::Solid,
                        ));
                        Bounds {
                            origin,
                            size: size(px(0.), px(0.)),
                        }
                    }
                    TerminalCursor::Block => Bounds {
                        origin,
                        size: size(cell_width, cell_height),
                    },
                };
                if cursor_bounds.size.width > px(0.) {
                    window.paint_quad(fill(cursor_bounds, cursor_color.opacity(0.5)));
                }
            }

            let font_settings = font_settings(cx);
            let base = typography::font(&font_settings);
            let underlined = |cell: &RenderCell| {
                cell.style.underline != Underline::None
                    || hovered
                        .as_ref()
                        .is_some_and(|link| link.contains(cell.column, cell.line))
            };
            for run in shaping_runs(&snapshot.cells, font_settings.ligatures, underlined) {
                let cells = &snapshot.cells[run];
                let cell = &cells[0];
                if cell.style.invisible || cell.text.trim().is_empty() {
                    continue;
                }
                let text: String = cells.iter().map(|cell| cell.text.as_str()).collect();
                let mut font = base.clone();
                if cell.style.bold {
                    font = bold(&font);
                }
                if cell.style.italic {
                    font = italic(&font);
                }
                let run = TextRun {
                    len: text.len(),
                    font,
                    color: paint_rgb(cell.foreground),
                    background_color: None,
                    underline: underlined(cell).then(|| UnderlineStyle {
                        thickness: px(1.),
                        ..Default::default()
                    }),
                    strikethrough: cell.style.strikethrough.then(|| StrikethroughStyle {
                        thickness: px(1.),
                        ..Default::default()
                    }),
                };
                let shaped = window.text_system().shape_line(
                    SharedString::from(text),
                    layout.font_size,
                    &[run],
                    None,
                );
                let origin = layout.origin(bounds, cell.column, cell.line);
                let width = cell_width * if cell.wide { 2 } else { cells.len() };
                window.with_content_mask(
                    Some(ContentMask {
                        bounds: Bounds::new(origin, size(width, cell_height)),
                    }),
                    |window| {
                        let origin = layout.text_origin(origin, shaped.ascent, shaped.descent);
                        let _ =
                            shaped.paint(origin, cell_height, TextAlign::Left, None, window, cx);
                    },
                );
            }
        });
    }
}

#[cfg(all(test, unix))]
#[path = "mouse_tests.rs"]
mod mouse_tests;

use crate::app::commands::declarations::{CommandContext, command};
use crate::app::commands::dispatch::ProductCommandTarget;
use knot_protocol::{Command, CommandArgumentValue, CommandOutcome};

fn claim_terminal(command: &Command, target: &ProductCommandTarget, cx: &App) -> bool {
    let Some(crate::app::workbench::CommandView::Terminal(view)) = target.command_view(cx) else {
        return false;
    };
    let view = view.read(cx);
    if view.attachment.is_none() {
        return false;
    }
    if command.arguments != CommandArgumentValue::Null {
        return true;
    }
    if command.name.as_ref() == crate::app::commands::dispatch::PASTE_COMMAND {
        return view.session.read(cx).status() == TerminalStatus::Running;
    }
    if command.name.as_ref() == crate::app::commands::dispatch::COPY_COMMAND {
        return view
            .session
            .read(cx)
            .with_terminal(|terminal| terminal.selection_text())
            .flatten()
            .is_some_and(|text| !text.is_empty());
    }
    true
}

#[command("copy", title = "Copy", description = "Copy the selection to the clipboard.", id = TERMINAL_COPY_COMMAND, participant = Terminal, claim = claim_terminal)]
pub(crate) async fn terminal_copy(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    execute_terminal(context, TerminalCommand::Copy).await
}
#[command("editor.paste", title = "Paste", description = "Paste from the clipboard.", id = TERMINAL_PASTE_COMMAND, participant = Terminal, claim = claim_terminal)]
pub(crate) async fn terminal_paste(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    execute_terminal(context, TerminalCommand::Paste).await
}
#[command("terminal.toggle-mouse-reporting", title = "Toggle Terminal Mouse Reporting", description = "Let the terminal program receive mouse input, or select text instead.", id = TOGGLE_TERMINAL_MOUSE_REPORTING_COMMAND, participant = Terminal, claim = claim_terminal)]
pub(crate) async fn terminal_toggle(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    execute_terminal(context, TerminalCommand::ToggleMouseReporting).await
}
#[command("terminal.restart", title = "Restart Terminal", description = "Restart the focused terminal session.", id = RESTART_TERMINAL_COMMAND, participant = Terminal, claim = claim_terminal)]
pub(crate) async fn terminal_restart(
    context: CommandContext,
    (): (),
) -> Result<(), CommandOutcome> {
    execute_terminal(context, TerminalCommand::Restart).await
}
async fn execute_terminal(
    context: CommandContext,
    command: TerminalCommand,
) -> Result<(), CommandOutcome> {
    context
        .foreground(move |target, cx| {
            let Some(crate::app::workbench::CommandView::Terminal(view)) = target.command_view(cx)
            else {
                return Err(CommandOutcome::InvalidTarget);
            };
            match view.update(cx, |view, cx| view.execute_terminal_command(command, cx)) {
                crate::app::commands::dispatch::CommandClaim::Finished(
                    CommandOutcome::Completed,
                ) => Ok(()),
                crate::app::commands::dispatch::CommandClaim::Declined => {
                    Err(CommandOutcome::Unavailable)
                }
                _ => unreachable!("terminal commands complete immediately"),
            }
        })
        .await
}
