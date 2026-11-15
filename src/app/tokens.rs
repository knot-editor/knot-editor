//! The application's shared sizes for native surfaces.
//!
//! Every length is in pixels at 100% zoom; surfaces convert one with
//! [`zoom::rems`] (style lengths) or [`zoom::scaled`] (painted lengths), so
//! the chrome zooms as one. Colors come from the theme's color keys.

/// The corner radii.
pub(crate) mod radius {
    pub(crate) const SM: f32 = 4.;
    pub(crate) const MD: f32 = 6.;
    pub(crate) const LG: f32 = 10.;
}

/// Heights and widths of the workbench chrome.
pub(crate) mod chrome {
    pub(crate) const TAB_BAR_HEIGHT: f32 = 30.;
    pub(crate) const STATUS_BAR_HEIGHT: f32 = 24.;
    /// A status or toolbar item, centered in its bar.
    pub(crate) const STATUS_ITEM_HEIGHT: f32 = 20.;
    pub(crate) const STATUS_TEXT_SIZE: f32 = 12.;
    pub(crate) const STRIPE_WIDTH: f32 = 36.;
    pub(crate) const STRIPE_BUTTON: f32 = 30.;
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
    /// The space between a text surface's left edge and its text, where no gutter sits.
    pub(crate) const TEXT_INSET: f32 = 4.;
}

/// Lists, trees, and pickers.
pub(crate) mod list {
    /// A row of a transient list.
    pub(crate) const ROW_HEIGHT: f32 = 24.;
    /// The gap between a picker panel's edge and its rows, and above and below them.
    pub(crate) const PICKER_INSET: f32 = 6.;
    /// The picker query field's text size.
    pub(crate) const PICKER_INPUT_SIZE: f32 = 14.;
    /// The text size of tree and picker rows.
    pub(crate) const TEXT_SIZE: f32 = 13.;
    /// A tree row with a second line.
    pub(crate) const TWO_LINE_ROW_HEIGHT: f32 = 44.;
    /// The gap between a list's edge and its rounded selection.
    pub(crate) const ROW_INSET: f32 = 4.;
    /// A tree row's content inset from the list's edge and the indent per depth level.
    pub(crate) const TREE_INSET: f32 = 10.;
    pub(crate) const TREE_INDENT: f32 = 12.;
    /// A tree row's badge dot.
    pub(crate) const BADGE_SIZE: f32 = 6.;
    /// A picker or transient panel.
    pub(crate) const PANEL_WIDTH: f32 = 560.;
    /// How far below the window top a picker panel starts.
    pub(crate) const PANEL_TOP: f32 = 80.;
}
