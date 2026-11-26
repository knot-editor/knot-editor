//! How pointer input opens links: the modifier that arms them and the opener
//! views call, which tests replace.

use std::rc::Rc;

use gpui::{App, Modifiers};

/// Opens a link's URL.
pub(crate) type LinkOpener = Rc<dyn Fn(&str, &mut App)>;

/// The platform browser.
pub(crate) fn browser_opener() -> LinkOpener {
    Rc::new(|uri, cx| cx.open_url(uri))
}

/// Command-click on macOS and Control-click elsewhere open links; Shift
/// selects instead.
pub(crate) fn link_modifier(modifiers: Modifiers) -> bool {
    !modifiers.shift
        && if cfg!(target_os = "macos") {
            modifiers.platform
        } else {
            modifiers.control
        }
}
