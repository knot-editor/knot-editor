//! The overlay scrollbar of editors and terminals: a thin rounded thumb at
//! the right edge that shows while its content scrolls and fades out after.

use std::time::{Duration, Instant};

use gpui::{App, Bounds, Hsla, Pixels, Window, fill, point, px, size};

use super::zoom;

/// How long the thumb stays fully visible after the content last moved.
const HOLD: Duration = Duration::from_millis(700);
/// How long the thumb takes to fade out after that.
const FADE: Duration = Duration::from_millis(300);
/// The thumb's width and its gap to the right edge, at 100% zoom.
const THUMB_WIDTH: f32 = 6.;
const THUMB_GAP: f32 = 2.;
/// The shortest thumb, at 100% zoom.
const MIN_THUMB_HEIGHT: f32 = 24.;

/// A viewport's place in its content, in whatever unit the three share.
#[derive(Clone, Copy)]
pub(crate) struct Scroll {
    pub(crate) offset: f32,
    pub(crate) total: f32,
    pub(crate) visible: f32,
}

/// When a view's content last moved, which sets the thumb's opacity.
#[derive(Debug, Default)]
pub(crate) struct ScrollFade {
    position: Option<f32>,
    moved: Option<Instant>,
}

impl ScrollFade {
    /// The thumb's opacity at `now` with the content at `position`, and
    /// whether it is still changing. The first position seen is not a move.
    fn opacity(&mut self, position: f32, now: Instant) -> (f32, bool) {
        if self.position.is_some_and(|last| last != position) {
            self.moved = Some(now);
        }
        self.position = Some(position);
        let Some(elapsed) = self.moved.map(|moved| now.saturating_duration_since(moved)) else {
            return (0., false);
        };
        if elapsed < HOLD {
            (1., true)
        } else if elapsed < HOLD + FADE {
            (
                1. - (elapsed - HOLD).as_secs_f32() / FADE.as_secs_f32(),
                true,
            )
        } else {
            (0., false)
        }
    }

    /// Paint the thumb at the right edge of `bounds` for a viewport showing
    /// `scroll`. Nothing paints while everything fits or the thumb is faded out.
    pub(crate) fn paint(
        &mut self,
        window: &mut Window,
        cx: &App,
        bounds: Bounds<Pixels>,
        scroll: Scroll,
        color: Hsla,
    ) {
        let Scroll {
            offset,
            total,
            visible,
        } = scroll;
        let (opacity, animating) = self.opacity(offset, Instant::now());
        if animating {
            window.request_animation_frame();
        }
        if opacity <= 0. || total <= visible || visible <= 0. {
            return;
        }
        let height = f32::from(bounds.size.height);
        let thumb_height =
            (height * visible / total).max(f32::from(zoom::scaled(MIN_THUMB_HEIGHT, cx)));
        let top = (height * offset / total).min(height - thumb_height).max(0.);
        let width = zoom::scaled(THUMB_WIDTH, cx);
        let thumb = Bounds {
            origin: point(
                bounds.origin.x + bounds.size.width - width - zoom::scaled(THUMB_GAP, cx),
                bounds.origin.y + px(top),
            ),
            size: size(width, px(thumb_height)),
        };
        window.paint_quad(fill(thumb, color.opacity(color.a * opacity)).corner_radii(width / 2.));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thumb_shows_while_scrolling_and_fades_after() {
        let start = Instant::now();
        let at = |millis: u64| start + Duration::from_millis(millis);
        let mut fade = ScrollFade::default();
        assert_eq!(fade.opacity(0., at(0)), (0., false));
        assert_eq!(fade.opacity(0., at(50)), (0., false));
        assert_eq!(fade.opacity(10., at(100)), (1., true));
        assert_eq!(fade.opacity(10., at(500)), (1., true));
        let (halfway, animating) = fade.opacity(10., at(100 + 700 + 150));
        assert!(animating && (halfway - 0.5).abs() < 0.01);
        assert_eq!(fade.opacity(10., at(100 + 700 + 300)), (0., false));
        assert_eq!(fade.opacity(11., at(2000)), (1., true));
    }
}
