//! The application's shared sizes for native surfaces.
//!
//! Every length is in pixels at 100% zoom; surfaces convert one with
//! [`zoom::rems`] (style lengths) or [`zoom::scaled`] (painted lengths), so
//! the chrome zooms as one. Colors come from the theme's color keys.

/// The corner radii.
pub(crate) mod radius {
    pub(crate) const SM: f32 = 4.;
    pub(crate) const MD: f32 = 6.;
}

/// Heights and widths of the workbench chrome.
pub(crate) mod chrome {
    pub(crate) const TAB_BAR_HEIGHT: f32 = 30.;
    pub(crate) const STATUS_BAR_HEIGHT: f32 = 22.;
    pub(crate) const STRIPE_WIDTH: f32 = 30.;
    pub(crate) const STRIPE_BUTTON: f32 = 26.;
    pub(crate) const STRIPE_GROUP_MIN_HEIGHT: f32 = 60.;
    /// The grab area of a dock or split resize handle.
    /// The gap between cards and the hit width of a split divider.
    pub(crate) const RESIZE_HANDLE: f32 = 8.;
    /// The margin around the cards at the window edges.
    pub(crate) const WINDOW_INSET: f32 = 4.;
    /// The toolbar row's height; unscaled on macOS, where it matches the native window buttons.
    /// The width the native window buttons take at the toolbar row's start.
    pub(crate) const WINDOW_BUTTONS_WIDTH: f32 = 80.;
    pub(crate) const TOOLBAR_HEIGHT: f32 = 40.;
}

/// Lists, trees, and pickers.
pub(crate) mod list {
    /// A row of a transient list.
    pub(crate) const ROW_HEIGHT: f32 = 26.;
    /// A tree row with a second line.
    pub(crate) const TWO_LINE_ROW_HEIGHT: f32 = 44.;
    /// A tree row's left inset and the indent per depth level.
    pub(crate) const TREE_INSET: f32 = 8.;
    pub(crate) const TREE_INDENT: f32 = 14.;
    /// A tree row's badge dot.
    pub(crate) const BADGE_SIZE: f32 = 6.;
    /// A picker or transient panel.
    pub(crate) const PANEL_WIDTH: f32 = 560.;
    /// How far below the window top a picker panel starts.
    pub(crate) const PANEL_TOP: f32 = 80.;
}
