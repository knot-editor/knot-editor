//! Theme colors and the highlight-name vocabulary.
//!
//! Capture names follow nvim-treesitter's standard captures, which upstream
//! grammar queries also use, so adapted queries keep their names. A source
//! name resolves to the capture name with the longest dotted prefix
//! (`function.method.call.deep` → `function.method.call`), and names with no
//! such prefix are ignored. The `Theme` resolves every name to a style the
//! same way, so a theme may style `function` alone and have `function.method`
//! follow it.
//!
//! UI colors are named by flat dotted keys. A color definition gives a key a
//! default per appearance, either a color or another key; native surfaces
//! paint only with keys from the native table here. A theme's values win
//! over the defaults, and with no theme installed the dark defaults apply.

use std::collections::HashMap;

use crate::style::{Color, FontStyle, HighlightStyle, Underline};

use knot_protocol::DecorationStyle;

/// Every name a syntax capture or semantic token can resolve to.
pub const CAPTURE_NAMES: &[&str] = &[
    "attribute",
    "attribute.builtin",
    "boolean",
    "character",
    "character.special",
    "comment",
    "comment.documentation",
    "comment.error",
    "comment.note",
    "comment.todo",
    "comment.warning",
    "constant",
    "constant.builtin",
    "constant.macro",
    "constructor",
    "diff.delta",
    "diff.minus",
    "diff.plus",
    "function",
    "function.builtin",
    "function.call",
    "function.macro",
    "function.method",
    "function.method.call",
    "keyword",
    "keyword.conditional",
    "keyword.conditional.ternary",
    "keyword.coroutine",
    "keyword.debug",
    "keyword.directive",
    "keyword.directive.define",
    "keyword.exception",
    "keyword.function",
    "keyword.import",
    "keyword.modifier",
    "keyword.operator",
    "keyword.repeat",
    "keyword.return",
    "keyword.type",
    "label",
    "markup.heading",
    "markup.heading.1",
    "markup.heading.2",
    "markup.heading.3",
    "markup.heading.4",
    "markup.heading.5",
    "markup.heading.6",
    "markup.italic",
    "markup.link",
    "markup.link.label",
    "markup.link.url",
    "markup.list",
    "markup.list.checked",
    "markup.list.unchecked",
    "markup.math",
    "markup.quote",
    "markup.raw",
    "markup.raw.block",
    "markup.strikethrough",
    "markup.strong",
    "markup.underline",
    "module",
    "module.builtin",
    "number",
    "number.float",
    "operator",
    "property",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.documentation",
    "string.escape",
    "string.regexp",
    "string.special",
    "string.special.path",
    "string.special.symbol",
    "string.special.url",
    "tag",
    "tag.attribute",
    "tag.builtin",
    "tag.delimiter",
    "type",
    "type.builtin",
    "type.definition",
    "variable",
    "variable.builtin",
    "variable.member",
    "variable.parameter",
    "variable.parameter.builtin",
];

/// One capture name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Highlight(u16);

impl Highlight {
    /// The capture name with the longest dotted prefix of `name`.
    pub fn resolve(name: &str) -> Option<Self> {
        dotted_prefixes(name).find_map(|prefix| {
            CAPTURE_NAMES
                .iter()
                .position(|known| *known == prefix)
                .map(|index| Self(index as u16))
        })
    }

    /// Whether a theme may style `name`: a capture name or a dotted prefix
    /// of one, such as `markup` for every `markup.*` capture.
    pub fn styles_captures(name: &str) -> bool {
        CAPTURE_NAMES.iter().any(|capture| {
            capture
                .strip_prefix(name)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
        })
    }

    /// The capture name `name`, which must exist exactly.
    #[cfg(any(test, feature = "test-support"))]
    pub fn named(name: &str) -> Self {
        CAPTURE_NAMES
            .iter()
            .position(|known| *known == name)
            .map(|index| Self(index as u16))
            .unwrap_or_else(|| panic!("{name} is not a highlight name"))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn name(self) -> &'static str {
        CAPTURE_NAMES[self.0 as usize]
    }
}

/// `name`, then each shorter prefix ending before a `.`.
fn dotted_prefixes(name: &str) -> impl Iterator<Item = &str> {
    std::iter::successors(Some(name), |prefix| {
        prefix.rfind('.').map(|dot| &prefix[..dot])
    })
}

/// A color definition's default for one appearance.
#[derive(Clone, Debug, PartialEq)]
pub enum ColorDefault {
    Color(Color),
    /// The resolved color of another key.
    Key(String),
}

/// A color key with a description and a default per appearance.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorDefinition {
    pub key: String,
    pub description: String,
    pub dark: ColorDefault,
    pub light: ColorDefault,
}

impl ColorDefinition {
    fn default_for(&self, appearance: Appearance) -> &ColorDefault {
        match appearance {
            Appearance::Dark => &self.dark,
            Appearance::Light => &self.light,
        }
    }
}

enum NativeDefault {
    Rgb(u32),
    Rgba(u32),
    Key(&'static str),
}

impl NativeDefault {
    fn definition(&self) -> ColorDefault {
        match self {
            Self::Rgb(color) => ColorDefault::Color(Color::rgb(*color)),
            Self::Rgba(color) => ColorDefault::Color(Color::rgba(*color)),
            Self::Key(key) => ColorDefault::Key((*key).to_owned()),
        }
    }
}

macro_rules! native_colors {
    ($($name:ident = $key:literal, $description:literal, $dark:expr, $light:expr;)*) => {
        /// Color keys native surfaces paint with.
        pub mod color {
            $(pub const $name: &str = $key;)*
        }

        fn native_color_table() -> Vec<(&'static str, &'static str, NativeDefault, NativeDefault)> {
            use NativeDefault::{Key, Rgb, Rgba};
            use color::*;
            vec![$(($key, $description, $dark, $light),)*]
        }
    };
}

native_colors! {
    BACKGROUND = "background", "Window and pane background.", Rgb(0x2b2d30), Rgb(0xffffff);
    BORDER = "border", "Dividers and panel borders.", Rgb(0x393b40), Rgb(0xebecf0);
    BORDER_HOVER = "border.hover", "A resizable divider under the pointer.", Rgb(0x3574f0), Rgb(0x3574f0);
    BUTTON_BACKGROUND = "button.background", "Inline action buttons.", Rgb(0x393b40), Rgb(0xebecf0);
    BUTTON_HOVER_BACKGROUND = "button.hover.background", "An inline action button under the pointer.", Rgb(0x43454a), Rgb(0xdfe1e5);
    DIFF_ADDED = "diff.added", "Lines a text added, in the gutter and in diffs.", Key(SUCCESS), Key(SUCCESS);
    DIFF_ADDED_BACKGROUND = "diff.added.background", "Lines a diff view's other side lacks, behind the text.", Rgba(0x4caf5030), Rgba(0x4caf5038);
    DIFF_MODIFIED = "diff.modified", "Lines a text changed, in the gutter and in diffs.", Key(INFO), Key(INFO);
    DIFF_MODIFIED_BACKGROUND = "diff.modified.background", "Lines a diff view's sides differ in, behind the text.", Rgba(0x3574f038), Rgba(0x3574f030);
    DIFF_REMOVED = "diff.removed", "Lines a text removed, in the gutter and in diffs.", Key(ERROR), Key(ERROR);
    DIFF_REMOVED_BACKGROUND = "diff.removed.background", "Lines only a diff view's original side has, behind the text.", Rgba(0xf4433638), Rgba(0xf4433630);
    DROP_TARGET_BACKGROUND = "dropTarget.background", "Where a dragged tab would land.", Rgba(0x3574f055), Rgba(0x3574f040);
    EDITOR_BRACKET_MATCH = "editor.bracketMatch", "Background of the bracket at the cursor and its partner.", Rgba(0x3574f040), Rgba(0x3574f033);
    EDITOR_BACKGROUND = "editor.background", "Text editor background.", Key(BACKGROUND), Key(BACKGROUND);
    EDITOR_BAR_BACKGROUND = "editor.bar.background", "Bars extensions open above an editor's text, such as Find.", Key(TAB_BAR_BACKGROUND), Key(TAB_BAR_BACKGROUND);
    EDITOR_BAR_BORDER = "editor.bar.border", "The line below each editor bar.", Key(BORDER), Key(BORDER);
    EDITOR_CURSOR = "editor.cursor", "Text cursor.", Rgb(0xced0d6), Rgb(0x000000);
    EDITOR_FIND_MATCH = "editor.findMatch", "Background of each text match of an open Find bar.", Rgb(0x5c4d1f), Rgb(0xffeb9b);
    EDITOR_FIND_MATCH_CURRENT = "editor.findMatchCurrent", "Background of the match Find has selected.", Rgb(0x7a5a1a), Rgb(0xffc66d);
    EDITOR_FIND_SCOPE = "editor.findScope", "Background of the range Find in Selection limits its search to.", Key(EDITOR_SELECTION), Key(EDITOR_SELECTION);
    EDITOR_FOLD_PLACEHOLDER = "editor.foldPlaceholder", "The placeholder after the first line of a collapsed fold.", Rgb(0x6f737a), Rgb(0x818594);
    EDITOR_FOREGROUND = "editor.foreground", "Unhighlighted editor text.", Rgb(0xbcbec4), Rgb(0x080808);
    EDITOR_GUTTER_BACKGROUND = "editor.gutter.background", "The editor gutter beside the text.", Key(EDITOR_BACKGROUND), Key(EDITOR_BACKGROUND);
    EDITOR_LINE_HIGHLIGHT = "editor.lineHighlight", "Background of the cursor's line while nothing is selected.", Rgba(0xffffff0a), Rgb(0xf5f8fe);
    EDITOR_LINE_NUMBER = "editor.lineNumber", "Line numbers.", Rgb(0x4b5059), Rgb(0xaeb3c1);
    EDITOR_LINE_NUMBER_ACTIVE = "editor.lineNumber.active", "The cursor line's number.", Rgb(0xa1a3ab), Rgb(0x767a8a);
    EDITOR_MARKED_TEXT = "editor.markedText", "Underline of text being composed by an input method.", Rgba(0xbcbec499), Rgba(0x08080899);
    EDITOR_SELECTION = "editor.selection", "Selected text.", Rgba(0x21428399), Rgb(0xa6d2ff);
    EDITOR_SELECTION_HIGHLIGHT = "editor.selectionHighlight", "Background of the other occurrences of the selected text.", Rgba(0x3574f033), Rgba(0x3574f026);
    EDITOR_WORD_HIGHLIGHT = "editor.wordHighlight", "Background of the occurrences of the word at the cursor.", Rgba(0x3574f026), Rgba(0x3574f01f);
    ERROR = "error", "Errors and failures.", Rgb(0xf75464), Rgb(0xdb5c5c);
    ICON = "icon", "Icons that choose no color.", Key(TEXT), Key(TEXT);
    INFO = "info", "Informational markers.", Rgb(0x548af7), Rgb(0x3574f0);
    INPUT_BACKGROUND = "input.background", "Text fields in editor bars.", Key(EDITOR_BACKGROUND), Key(EDITOR_BACKGROUND);
    INPUT_BORDER = "input.border", "Border of a text field.", Key(BORDER), Key(BORDER);
    INPUT_FOCUSED_BORDER = "input.focused.border", "Border of the text field with focus.", Key(BORDER_HOVER), Key(BORDER_HOVER);
    LIST_GROUP_BACKGROUND = "list.group.background", "Group headers in trees.", Rgb(0x2b2d30), Rgb(0xf7f8fa);
    LIST_MATCH_FOREGROUND = "list.match.foreground", "Characters a filter query matched in a list row.", Key(TEXT_ACCENT), Key(TEXT_ACCENT);
    LIST_HOVER_BACKGROUND = "list.hover.background", "A list or tree row under the pointer.", Rgb(0x35373b), Rgb(0xf2f3f6);
    LIST_SELECTED_BACKGROUND = "list.selected.background", "The selected list or tree row.", Rgb(0x2e436e), Rgb(0xd4e2ff);
    LIST_SELECTED_FOREGROUND = "list.selected.foreground", "Text of the selected list or tree row.", Rgb(0xdfe1e5), Key(TEXT);
    NOTIFICATION_BACKGROUND = "notification.background", "Notification popup cards.", Key(POPOVER_BACKGROUND), Key(POPOVER_BACKGROUND);
    NOTIFICATION_BORDER = "notification.border", "Border of notification popup cards.", Key(POPOVER_BORDER), Key(POPOVER_BORDER);
    NOTIFICATION_DESCRIPTION_FOREGROUND = "notification.description.foreground", "The message of a notification popup.", Key(TEXT_MUTED), Key(TEXT_MUTED);
    NOTIFICATION_ERROR = "notification.error", "The icon of an error notification.", Key(ERROR), Key(ERROR);
    NOTIFICATION_FOREGROUND = "notification.foreground", "The title of a notification popup.", Key(TEXT), Key(TEXT);
    NOTIFICATION_INFO = "notification.info", "The icon of an informational notification.", Key(INFO), Key(INFO);
    NOTIFICATION_WARNING = "notification.warning", "The icon of a warning notification.", Key(WARNING), Key(WARNING);
    POPOVER_BACKGROUND = "popover.background", "Pickers, the command palette, completion, and reports.", Rgb(0x2b2d30), Rgb(0xffffff);
    POPOVER_BORDER = "popover.border", "Border of popovers.", Rgb(0x4e5157), Rgb(0xc9ccd6);
    SCRIM = "scrim", "Dims the window behind a modal surface.", Rgba(0x00000080), Rgba(0x00000040);
    SCROLLBAR_THUMB = "scrollbar.thumb", "Scrollbar thumb.", Rgba(0x6f737a80), Rgba(0x00000040);
    STATUS_BAR_BACKGROUND = "statusBar.background", "The status bar below the workbench.", Key(WINDOW_BACKGROUND), Key(WINDOW_BACKGROUND);
    STATUS_BAR_BORDER = "statusBar.border", "The line above the status bar.", Key(WINDOW_BACKGROUND), Key(WINDOW_BACKGROUND);
    STATUS_BAR_FOREGROUND = "statusBar.foreground", "Status bar items that choose no color.", Key(TEXT_MUTED), Key(TEXT_MUTED);
    STATUS_BAR_ITEM_HOVER_BACKGROUND = "statusBar.item.hover.background", "A status bar item that runs a command, under the pointer.", Key(LIST_HOVER_BACKGROUND), Key(LIST_HOVER_BACKGROUND);
    STRIPE_SELECTED_BACKGROUND = "stripe.selected.background", "The button of the focused tool window in a stripe.", Rgba(0x3574f04d), Rgba(0x3574f04d);
    STRIPE_SHOWN_BACKGROUND = "stripe.shown.background", "The button of a tool window that shows without focus.", Rgb(0x4a4c50), Rgb(0xdfe1e5);
    STRIPE_SEPARATOR = "stripe.separator", "The line between groups of tool buttons in a stripe.", Rgb(0x424649), Rgb(0xc9ccd6);
    SUCCESS = "success", "Success markers.", Rgb(0x5fb865), Rgb(0x369650);
    TAB_ACTIVE_BACKGROUND = "tab.active.background", "The active tab.", Key(EDITOR_BACKGROUND), Key(EDITOR_BACKGROUND);
    TAB_ACTIVE_BORDER = "tab.active.border", "Border around the active tab when tabs are rounded pills.", Key(TAB_ACTIVE_BACKGROUND), Key(TAB_ACTIVE_BACKGROUND);
    TAB_ACTIVE_FOREGROUND = "tab.active.foreground", "Title of the active tab.", Rgb(0xdfe1e5), Key(TEXT);
    TAB_ACTIVE_UNFOCUSED_BACKGROUND = "tab.active.unfocused.background", "The active tab of a pane that does not have focus.", Key(TAB_ACTIVE_BACKGROUND), Key(TAB_ACTIVE_BACKGROUND);
    TAB_ACTIVE_UNFOCUSED_BORDER = "tab.active.unfocused.border", "Border around the active tab of a pane that does not have focus, when tabs are rounded pills.", Key(TAB_ACTIVE_BORDER), Key(TAB_ACTIVE_BORDER);
    TAB_BAR_BACKGROUND = "tab.bar.background", "The tab bar behind the tabs.", Key(BACKGROUND), Key(BACKGROUND);
    TAB_INACTIVE_BACKGROUND = "tab.inactive.background", "Inactive tabs.", Key(TAB_BAR_BACKGROUND), Key(TAB_BAR_BACKGROUND);
    TAB_INACTIVE_FOREGROUND = "tab.inactive.foreground", "Titles of inactive tabs.", Rgb(0x868a91), Key(TEXT_MUTED);
    TERMINAL_ANSI_BLACK = "terminal.ansi.black", "ANSI black.", Rgb(0x000000), Rgb(0x000000);
    TERMINAL_ANSI_BLUE = "terminal.ansi.blue", "ANSI blue.", Rgb(0x2472c8), Rgb(0x0451a5);
    TERMINAL_ANSI_BRIGHT_BLACK = "terminal.ansi.brightBlack", "ANSI bright black.", Rgb(0x666666), Rgb(0x666666);
    TERMINAL_ANSI_BRIGHT_BLUE = "terminal.ansi.brightBlue", "ANSI bright blue.", Rgb(0x3b8eea), Rgb(0x0451a5);
    TERMINAL_ANSI_BRIGHT_CYAN = "terminal.ansi.brightCyan", "ANSI bright cyan.", Rgb(0x29b8db), Rgb(0x0598bc);
    TERMINAL_ANSI_BRIGHT_GREEN = "terminal.ansi.brightGreen", "ANSI bright green.", Rgb(0x23d18b), Rgb(0x14ce14);
    TERMINAL_ANSI_BRIGHT_MAGENTA = "terminal.ansi.brightMagenta", "ANSI bright magenta.", Rgb(0xd670d6), Rgb(0xbc05bc);
    TERMINAL_ANSI_BRIGHT_RED = "terminal.ansi.brightRed", "ANSI bright red.", Rgb(0xf14c4c), Rgb(0xcd3131);
    TERMINAL_ANSI_BRIGHT_WHITE = "terminal.ansi.brightWhite", "ANSI bright white.", Rgb(0xffffff), Rgb(0xa5a5a5);
    TERMINAL_ANSI_BRIGHT_YELLOW = "terminal.ansi.brightYellow", "ANSI bright yellow.", Rgb(0xf5f543), Rgb(0xb5ba00);
    TERMINAL_ANSI_CYAN = "terminal.ansi.cyan", "ANSI cyan.", Rgb(0x11a8cd), Rgb(0x0598bc);
    TERMINAL_ANSI_GREEN = "terminal.ansi.green", "ANSI green.", Rgb(0x0dbc79), Rgb(0x00bc00);
    TERMINAL_ANSI_MAGENTA = "terminal.ansi.magenta", "ANSI magenta.", Rgb(0xbc3fbc), Rgb(0xbc05bc);
    TERMINAL_ANSI_RED = "terminal.ansi.red", "ANSI red.", Rgb(0xcd3131), Rgb(0xcd3131);
    TERMINAL_ANSI_WHITE = "terminal.ansi.white", "ANSI white.", Rgb(0xe5e5e5), Rgb(0x555555);
    TERMINAL_ANSI_YELLOW = "terminal.ansi.yellow", "ANSI yellow.", Rgb(0xe5e510), Rgb(0x949800);
    TERMINAL_BACKGROUND = "terminal.background", "Terminal background and default cell background.", Key(BACKGROUND), Key(BACKGROUND);
    TERMINAL_CURSOR = "terminal.cursor", "Terminal cursor.", Key(TERMINAL_FOREGROUND), Key(TERMINAL_FOREGROUND);
    TERMINAL_FOREGROUND = "terminal.foreground", "Default terminal text.", Key(EDITOR_FOREGROUND), Key(EDITOR_FOREGROUND);
    TERMINAL_SELECTION = "terminal.selection", "Selected terminal cells.", Rgb(0x214283), Rgb(0xa6d2ff);
    TEXT = "text", "Default interface text.", Rgb(0xbcbec4), Rgb(0x080808);
    TEXT_ACCENT = "text.accent", "Interface text that acts, such as buttons and links.", Rgb(0x548af7), Rgb(0x3574f0);
    TEXT_MUTED = "text.muted", "Secondary interface text.", Rgb(0x868a91), Rgb(0x6c707e);
    CARD_BACKGROUND = "card.background", "The cards of the center and the docks.", Key(BACKGROUND), Key(BACKGROUND);
    CARD_FOCUSED_BORDER = "card.focusedBorder", "The outline of the focused pane.", Key(TEXT_ACCENT), Key(TEXT_ACCENT);
    TAB_ACCENT = "tab.accent", "The line under the active tab.", Key(BORDER_HOVER), Key(BORDER_HOVER);
    TOOLBAR_BACKGROUND = "toolbar.background", "The main toolbar above the workbench.", Key(WINDOW_BACKGROUND), Key(WINDOW_BACKGROUND);
    WINDOW_BACKGROUND = "window.background", "The window behind the cards, the toolbar, and the status bar.", Rgb(0x1e1f22), Rgb(0xdfe1e5);
    WARNING = "warning", "Warnings.", Rgb(0xf2c55c), Rgb(0xa46a00);
}

macro_rules! chrome_values {
    ($($field:ident = $key:literal, $default:expr, $min:expr, $max:expr, $description:literal;)*) => {
        /// The shape and weight of the workbench chrome, in pixels at 100%
        /// zoom except the weight. A theme sets some of them; the rest keep
        /// the Islands values.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct Chrome {
            $(pub $field: f32,)*
        }

        impl Default for Chrome {
            fn default() -> Self {
                Self { $($field: $default,)* }
            }
        }

        /// A chrome value's key, default, and range.
        pub struct ChromeKey {
            pub key: &'static str,
            pub default: f32,
            pub min: f32,
            pub max: f32,
            pub description: &'static str,
        }

        /// Every chrome value, named as a theme's `chrome` field and, behind
        /// a `chrome.` prefix, as the setting that overrides it.
        pub const CHROME_KEYS: &[ChromeKey] = &[
            $(ChromeKey {
                key: $key,
                default: $default,
                min: $min,
                max: $max,
                description: $description,
            },)*
        ];

        impl Chrome {
            /// Set the value named `key`, clamped to its range; false for an
            /// unknown key.
            pub fn set(&mut self, key: &str, value: f32) -> bool {
                match key {
                    $($key => self.$field = value.clamp($min, $max),)*
                    _ => return false,
                }
                true
            }
        }
    };
}

chrome_values! {
    card_radius = "cardRadius", 6., 0., 24., "Corner radius of the center and dock cards.";
    card_gap = "cardGap", 8., 0., 24., "Gap between cards, which is also their resize handle.";
    card_inset = "cardInset", 4., 0., 24., "Margin around the cards at the window edges.";
    card_border = "cardBorder", 1., 0., 4., "Width of the card border, painted in the `border` color.";
    tab_bar_height = "tabBarHeight", 30., 20., 48., "Height of tab bars and dock headers.";
    stripe_width = "stripeWidth", 36., 24., 64., "Width of the tool stripes.";
    accent_width = "accentWidth", 2., 0., 6., "Thickness of the accent line under the active tab.";
    tab_inset = "tabInset", 0., 0., 12., "Space above and below the tabs inside the tab bar.";
    tab_radius = "tabRadius", 0., 0., 12., "Corner radius of a tab, so the active tab can be a rounded pill.";
}

/// The definitions of every native color key.
pub fn native_color_definitions() -> Vec<ColorDefinition> {
    native_color_table()
        .into_iter()
        .map(|(key, description, dark, light)| ColorDefinition {
            key: key.to_owned(),
            description: description.to_owned(),
            dark: dark.definition(),
            light: light.definition(),
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Dark,
    Light,
}

/// Resolved colors by key and a resolved style for every capture name.
pub struct Theme {
    pub appearance: Appearance,
    pub chrome: Chrome,
    colors: HashMap<String, Color>,
    /// Indexed by `Highlight`.
    highlights: Vec<HighlightStyle>,
}

impl Default for Theme {
    fn default() -> Self {
        Self::defaults(Appearance::Dark, &native_color_definitions())
    }
}

/// Styles every theme shares for markup that carries no color; a theme's own
/// styles for these names win.
pub fn markup_styles() -> [(&'static str, HighlightStyle); 4] {
    [
        (
            "markup.italic",
            HighlightStyle {
                font_style: Some(FontStyle::Italic),
                ..Default::default()
            },
        ),
        (
            "markup.strikethrough",
            HighlightStyle {
                strikethrough: true,
                ..Default::default()
            },
        ),
        (
            "markup.strong",
            HighlightStyle {
                font_weight: Some(700),
                ..Default::default()
            },
        ),
        (
            "markup.underline",
            HighlightStyle {
                underline: Some(Underline {
                    thickness: 1.,
                    color: None,
                    wavy: false,
                }),
                ..Default::default()
            },
        ),
    ]
}

impl Theme {
    /// Color defaults and a minimal syntax palette, used when no registered
    /// theme applies.
    pub fn defaults(appearance: Appearance, definitions: &[ColorDefinition]) -> Self {
        let syntax: [(&str, u32); 8] = match appearance {
            Appearance::Dark => [
                ("comment", 0x6a9955),
                ("constant", 0x4fc1ff),
                ("function", 0xdcdcaa),
                ("keyword", 0x569cd6),
                ("markup.heading", 0x569cd6),
                ("number", 0xb5cea8),
                ("string", 0xce9178),
                ("type", 0x4ec9b0),
            ],
            Appearance::Light => [
                ("comment", 0x008000),
                ("constant", 0x0070c1),
                ("function", 0x795e26),
                ("keyword", 0x0000ff),
                ("markup.heading", 0x0000ff),
                ("number", 0x098658),
                ("string", 0xa31515),
                ("type", 0x267f99),
            ],
        };
        let syntax = syntax
            .into_iter()
            .map(|(name, color)| (name, HighlightStyle::color(Color::rgb(color))))
            .chain(markup_styles())
            .collect::<Vec<_>>();
        Self::new(appearance, &HashMap::new(), definitions, &syntax)
    }

    /// A theme whose `values` win over each definition's default for
    /// `appearance` and whose capture styles resolve against `syntax` by
    /// longest dotted prefix.
    pub fn new(
        appearance: Appearance,
        values: &HashMap<String, Color>,
        definitions: &[ColorDefinition],
        syntax: &[(&str, HighlightStyle)],
    ) -> Self {
        let colors = resolve_colors(appearance, values, definitions);
        let highlights = CAPTURE_NAMES
            .iter()
            .map(|name| {
                dotted_prefixes(name)
                    .find_map(|prefix| {
                        syntax
                            .iter()
                            .find(|(styled, _)| *styled == prefix)
                            .map(|(_, style)| *style)
                    })
                    .unwrap_or_default()
            })
            .collect();
        Self {
            appearance,
            chrome: Chrome::default(),
            colors,
            highlights,
        }
    }

    /// The resolved color of `key`; an undefined key paints as interface
    /// text, since its definition may arrive later.
    pub fn color(&self, key: &str) -> Color {
        self.colors
            .get(key)
            .or_else(|| self.colors.get(color::TEXT))
            .copied()
            .unwrap_or_default()
    }

    pub fn style(&self, highlight: Highlight) -> HighlightStyle {
        self.highlights[highlight.0 as usize]
    }

    /// The text style of an underline decoration naming color `key`: a
    /// straight or wavy underline in that color. Background styles paint
    /// behind the text instead and add nothing here.
    pub fn decoration(&self, key: &str, style: DecorationStyle) -> HighlightStyle {
        let (thickness, wavy) = match style {
            DecorationStyle::Underline => (1., false),
            DecorationStyle::Wavy => (1.5, true),
            DecorationStyle::Background | DecorationStyle::Line => {
                return HighlightStyle::default();
            }
        };
        HighlightStyle {
            underline: Some(Underline {
                thickness,
                color: Some(self.color(key)),
                wavy,
            }),
            ..Default::default()
        }
    }
}

/// Each defined key's color: its value in `values`, else its default for
/// `appearance`. A default naming an undefined key, or a reference cycle,
/// leaves the key unresolved.
fn resolve_colors(
    appearance: Appearance,
    values: &HashMap<String, Color>,
    definitions: &[ColorDefinition],
) -> HashMap<String, Color> {
    let definitions = definitions
        .iter()
        .map(|definition| (definition.key.as_str(), definition))
        .collect::<HashMap<_, _>>();
    let resolve = |key: &str| {
        let mut key = key;
        for _ in 0..=definitions.len() {
            if let Some(value) = values.get(key) {
                return Some(*value);
            }
            match definitions.get(key)?.default_for(appearance) {
                ColorDefault::Color(color) => return Some(*color),
                ColorDefault::Key(next) => key = next,
            }
        }
        None
    };
    definitions
        .keys()
        .filter_map(|key| Some(((*key).to_owned(), resolve(key)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve_to_the_longest_capture_prefix() {
        let name = |name| Highlight::resolve(name).map(Highlight::name);
        assert_eq!(
            name("function.method.call.deep"),
            Some("function.method.call")
        );
        assert_eq!(name("function.method"), Some("function.method"));
        assert_eq!(name("tag.attribute"), Some("tag.attribute"));
        assert_eq!(name("keyword.control.flow"), Some("keyword"));
        assert_eq!(name("spell"), None);
        assert_eq!(name(""), None);
    }

    /// The in-memory extension test host checks queries against the same names.
    #[test]
    fn fake_host_lists_every_capture_name() {
        let fake = include_str!("../../../scripts/extensions/testing/host.ts")
            .split("export const CAPTURE_NAMES: readonly string[] = [")
            .nth(1)
            .unwrap()
            .split("];")
            .next()
            .unwrap()
            .split('"')
            .skip(1)
            .step_by(2)
            .collect::<Vec<_>>();
        assert_eq!(fake, CAPTURE_NAMES);
    }

    #[test]
    fn themes_style_capture_names_and_their_prefixes() {
        for name in ["markup", "markup.heading", "function.method.call", "type"] {
            assert!(Highlight::styles_captures(name), "{name}");
        }
        for name in ["", "mark", "function.method.call.deep", "diagnostic.error"] {
            assert!(!Highlight::styles_captures(name), "{name}");
        }
    }

    /// The declarations and the in-memory extension test host list the
    /// native color keys the table defines.
    #[test]
    fn declarations_and_fake_host_list_every_native_color_key() {
        let mut native = native_color_definitions()
            .into_iter()
            .map(|definition| definition.key)
            .collect::<Vec<_>>();
        native.sort_unstable();
        let fake = include_str!("../../../scripts/extensions/testing/host.ts")
            .split("export const NATIVE_COLOR_KEYS: readonly string[] = [")
            .nth(1)
            .unwrap()
            .split("];")
            .next()
            .unwrap()
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(fake, native);
        let declared = include_str!("../../knot-host/src/js/knot.d.ts")
            .split("export interface ThemeColors {")
            .nth(1)
            .unwrap()
            .split("\n  }")
            .next()
            .unwrap()
            .lines()
            .filter_map(|line| line.trim().strip_prefix("readonly \""))
            .filter_map(|line| line.split('"').next())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(declared, native);
    }

    #[test]
    fn names_are_sorted_and_unique() {
        assert!(CAPTURE_NAMES.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn theme_styles_follow_the_longest_styled_prefix() {
        let theme = Theme::new(
            Appearance::Dark,
            &HashMap::new(),
            &native_color_definitions(),
            &[
                ("function", HighlightStyle::color(Color::rgb(0x111111))),
                (
                    "function.macro",
                    HighlightStyle::color(Color::rgb(0x222222)),
                ),
            ],
        );
        let color = |name| theme.style(Highlight::named(name)).color;
        assert_eq!(color("function.method"), Some(Color::rgb(0x111111)));
        assert_eq!(color("function.macro"), Some(Color::rgb(0x222222)));
        assert_eq!(color("keyword"), None);
    }

    #[test]
    fn native_keys_are_unique_and_every_default_resolves() {
        let definitions = native_color_definitions();
        let mut keys = definitions
            .iter()
            .map(|definition| definition.key.as_str())
            .collect::<Vec<_>>();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), definitions.len());
        for appearance in [Appearance::Dark, Appearance::Light] {
            let colors = resolve_colors(appearance, &HashMap::new(), &definitions);
            for key in &keys {
                assert!(
                    colors.contains_key(*key),
                    "{key} has no {appearance:?} default"
                );
            }
        }
    }

    #[test]
    fn theme_values_win_over_defaults_and_references_follow_them() {
        let values = HashMap::from([(color::BACKGROUND.to_owned(), Color::rgb(0x123456))]);
        let theme = Theme::new(Appearance::Dark, &values, &native_color_definitions(), &[]);
        assert_eq!(theme.color(color::BACKGROUND), Color::rgb(0x123456));
        // editor.background defaults to background.
        assert_eq!(theme.color(color::EDITOR_BACKGROUND), Color::rgb(0x123456));
        assert_eq!(theme.color("undefined.key"), theme.color(color::TEXT));
    }

    #[test]
    fn reference_cycles_and_dangling_references_stay_unresolved() {
        let definition = |key: &str, target: &str| ColorDefinition {
            key: key.to_owned(),
            description: String::new(),
            dark: ColorDefault::Key(target.to_owned()),
            light: ColorDefault::Key(target.to_owned()),
        };
        let colors = resolve_colors(
            Appearance::Dark,
            &HashMap::new(),
            &[
                definition("a", "b"),
                definition("b", "a"),
                definition("c", "missing"),
            ],
        );
        assert!(colors.is_empty());
    }

    #[test]
    fn diff_colors_follow_the_status_colors_until_a_theme_sets_them() {
        let definitions = native_color_definitions();
        for appearance in [Appearance::Dark, Appearance::Light] {
            let theme = Theme::defaults(appearance, &definitions);
            assert_eq!(theme.color(color::DIFF_ADDED), theme.color(color::SUCCESS));
            assert_eq!(theme.color(color::DIFF_MODIFIED), theme.color(color::INFO));
            assert_eq!(theme.color(color::DIFF_REMOVED), theme.color(color::ERROR));
        }
        let values = HashMap::from([(color::DIFF_ADDED.to_owned(), Color::rgb(0x123456))]);
        let theme = Theme::new(Appearance::Dark, &values, &definitions, &[]);
        assert_eq!(theme.color(color::DIFF_ADDED), Color::rgb(0x123456));
        assert_eq!(theme.color(color::DIFF_MODIFIED), theme.color(color::INFO));
    }

    #[test]
    fn decorations_underline_in_their_key_color() {
        let mut definitions = native_color_definitions();
        definitions.push(ColorDefinition {
            key: "diff.add".into(),
            description: String::new(),
            dark: ColorDefault::Color(Color::rgb(0x00ff00)),
            light: ColorDefault::Key(color::SUCCESS.into()),
        });
        for appearance in [Appearance::Dark, Appearance::Light] {
            let theme = Theme::defaults(appearance, &definitions);
            assert_eq!(theme.appearance, appearance);
            let underline = |key| {
                theme
                    .decoration(key, DecorationStyle::Wavy)
                    .underline
                    .unwrap()
                    .color
                    .unwrap()
            };
            assert_eq!(underline(color::ERROR), theme.color(color::ERROR));
            assert_eq!(underline("diff.add"), theme.color("diff.add"));
            assert_ne!(underline("diff.add"), theme.color(color::TEXT));
            assert_eq!(underline("undefined"), theme.color(color::TEXT));
        }
    }

    #[test]
    #[should_panic(expected = "function.calls is not a highlight name")]
    fn named_requires_an_exact_vocabulary_entry() {
        Highlight::named("function.calls");
    }
}
