//! Terminal mouse gesture ownership and wheel routing; the terminal encodes
//! the reports.

use super::input::InputModes;
use gpui::{Modifiers, MouseButton, NavigationDirection, Pixels, Point, ScrollDelta};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PointerOwner {
    Local,
    Application,
    Link,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WheelRoute {
    Local,
    Application,
    Alternate,
}

pub(crate) fn wheel_route(mode: InputModes, shift: bool) -> WheelRoute {
    if shift {
        WheelRoute::Local
    } else if mode.mouse_reporting() {
        WheelRoute::Application
    } else if mode.alternate_screen && mode.alternate_scroll {
        WheelRoute::Alternate
    } else {
        WheelRoute::Local
    }
}

#[derive(Default)]
pub(crate) struct PointerState {
    pressed: Vec<(MouseButton, PointerOwner)>,
    /// The last motion reported, or the drag a press would report.
    last_motion: Option<Vec<u8>>,
    wheel: Point<f32>,
    wheel_route: Option<WheelRoute>,
}

impl PointerState {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
    pub(crate) fn press(&mut self, button: MouseButton, owner: PointerOwner) {
        self.pressed.retain(|(pressed, _)| *pressed != button);
        self.pressed.push((button, owner));
        self.last_motion = None;
    }
    pub(crate) fn application_buttons(&self) -> Vec<MouseButton> {
        self.pressed
            .iter()
            .filter_map(|(button, owner)| (*owner == PointerOwner::Application).then_some(*button))
            .collect()
    }
    pub(crate) fn owner(&self, button: MouseButton) -> Option<PointerOwner> {
        self.pressed
            .iter()
            .find(|(pressed, _)| *pressed == button)
            .map(|(_, owner)| *owner)
    }
    pub(crate) fn release(&mut self, button: MouseButton) -> Option<PointerOwner> {
        let owner = self.owner(button);
        self.pressed.retain(|(pressed, _)| *pressed != button);
        self.last_motion = None;
        owner
    }
    /// Whether `report` differs from the last motion reported, which it
    /// replaces.
    pub(crate) fn motion_changed(&mut self, report: &[u8]) -> bool {
        if self.last_motion.as_deref() == Some(report) {
            return false;
        }
        self.last_motion = Some(report.to_vec());
        true
    }
    pub(crate) fn leave(&mut self) {
        self.last_motion = None;
    }
    pub(crate) fn wheel_steps(
        &mut self,
        delta: ScrollDelta,
        cell_width: Pixels,
        cell_height: Pixels,
        route: WheelRoute,
    ) -> (i32, i32) {
        if self.wheel_route != Some(route) {
            self.wheel = Point::default();
            self.wheel_route = Some(route);
        }
        let delta = match delta {
            ScrollDelta::Lines(delta) => delta,
            ScrollDelta::Pixels(delta) => {
                let divisor = if route == WheelRoute::Local {
                    gpui::px(4.)
                } else {
                    cell_height
                };
                gpui::point(delta.x / cell_width, delta.y / divisor)
            }
        };
        // Reversing direction starts a new accumulation instead of fighting old residue.
        if self.wheel.x * delta.x < 0. {
            self.wheel.x = 0.;
        }
        if self.wheel.y * delta.y < 0. {
            self.wheel.y = 0.;
        }
        self.wheel += delta;
        let horizontal = self.wheel.x.trunc() as i32;
        let vertical = self.wheel.y.trunc() as i32;
        self.wheel.x -= horizontal as f32;
        self.wheel.y -= vertical as f32;
        (horizontal, vertical)
    }
}

pub(crate) fn report_button(button: MouseButton) -> knot_terminal::MouseButton {
    match button {
        MouseButton::Left => knot_terminal::MouseButton::Left,
        MouseButton::Middle => knot_terminal::MouseButton::Middle,
        MouseButton::Right => knot_terminal::MouseButton::Right,
        MouseButton::Navigate(NavigationDirection::Back) => knot_terminal::MouseButton::Back,
        MouseButton::Navigate(NavigationDirection::Forward) => knot_terminal::MouseButton::Forward,
    }
}

/// The modifiers mouse reports carry; Command and Super are never reported.
pub(crate) fn report_modifiers(modifiers: Modifiers) -> knot_terminal::Modifiers {
    knot_terminal::Modifiers {
        shift: modifiers.shift,
        control: modifiers.control,
        alt: modifiers.alt,
        super_key: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

    #[test]
    fn gesture_ownership_motion_deduplication_and_reset() {
        let mut state = PointerState::default();
        assert_eq!(state.release(MouseButton::Left), None);
        state.press(MouseButton::Left, PointerOwner::Local);
        state.press(MouseButton::Right, PointerOwner::Application);
        assert_eq!(state.owner(MouseButton::Left), Some(PointerOwner::Local));
        assert_eq!(
            state.release(MouseButton::Right),
            Some(PointerOwner::Application)
        );
        assert!(state.motion_changed(b"one"));
        assert!(!state.motion_changed(b"one"));
        assert!(state.motion_changed(b"two"));
        state.leave();
        assert!(state.motion_changed(b"two"));
        state.clear();
        assert_eq!(state.owner(MouseButton::Left), None);
    }

    #[test]
    fn wheel_routes_fractional_deltas_direction_changes_and_route_changes() {
        let alternate = InputModes {
            alternate_screen: true,
            alternate_scroll: true,
            ..InputModes::default()
        };
        let tracking = InputModes {
            mouse: crate::app::terminal::input::MouseTracking::Click,
            ..alternate
        };
        assert_eq!(wheel_route(tracking, false), WheelRoute::Application);
        assert_eq!(wheel_route(tracking, true), WheelRoute::Local);
        assert_eq!(wheel_route(alternate, false), WheelRoute::Alternate);
        assert_eq!(
            wheel_route(
                InputModes {
                    alternate_scroll: false,
                    ..alternate
                },
                false
            ),
            WheelRoute::Local
        );
        let mut state = PointerState::default();
        let step = |state: &mut PointerState, delta, route| {
            state.wheel_steps(delta, px(8.), px(16.), route)
        };
        for _ in 0..3 {
            assert_eq!(
                step(
                    &mut state,
                    ScrollDelta::Pixels(point(px(2.), px(4.))),
                    WheelRoute::Application
                ),
                (0, 0)
            );
        }
        assert_eq!(
            step(
                &mut state,
                ScrollDelta::Pixels(point(px(2.), px(4.))),
                WheelRoute::Application
            ),
            (1, 1)
        );
        assert_eq!(
            step(
                &mut state,
                ScrollDelta::Lines(point(0., -2.5)),
                WheelRoute::Application
            ),
            (0, -2)
        );
        assert_eq!(
            step(
                &mut state,
                ScrollDelta::Lines(point(0., 0.5)),
                WheelRoute::Application
            ),
            (0, 0)
        );
        assert_eq!(
            step(
                &mut state,
                ScrollDelta::Lines(point(0., 0.5)),
                WheelRoute::Local
            ),
            (0, 0)
        );
        assert_eq!(
            step(
                &mut state,
                ScrollDelta::Lines(point(0., 0.5)),
                WheelRoute::Local
            ),
            (0, 1)
        );
        assert_eq!(
            step(
                &mut state,
                ScrollDelta::Pixels(point(px(0.), px(4.))),
                WheelRoute::Local
            ),
            (0, 1)
        );
        state.clear();
        assert_eq!(
            step(
                &mut state,
                ScrollDelta::Lines(point(0., 0.5)),
                WheelRoute::Alternate
            ),
            (0, 0)
        );
    }
}
