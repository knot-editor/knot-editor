//! Application settings: the global wrapping the plain
//! [`knot_model::settings::SettingsStore`], and typed reads of the native
//! keys for views.

use gpui::{App, Global};
use serde_json::Value;

use knot_protocol::{ExtensionId, ExtensionLifecycleId};

pub(crate) use knot_model::settings::*;

/// The application's settings store.
#[derive(Default)]
pub(crate) struct ApplicationSettings(pub(crate) SettingsStore);

impl Global for ApplicationSettings {}

pub(crate) fn remove_lifecycle(
    extension: ExtensionId,
    lifecycle: ExtensionLifecycleId,
    cx: &mut App,
) {
    if cx.has_global::<ApplicationSettings>()
        && cx
            .global::<ApplicationSettings>()
            .0
            .has_lifecycle(extension, lifecycle)
    {
        cx.global_mut::<ApplicationSettings>()
            .0
            .remove_lifecycle(extension, lifecycle);
    }
}

/// Write a document's own `editor.insertSpaces` value, which beats language
/// and global values.
pub(crate) fn set_document_insert_spaces(document: uuid::Uuid, spaces: bool, cx: &mut App) {
    cx.default_global::<ApplicationSettings>()
        .0
        .set(
            EDITOR_INSERT_SPACES,
            Value::Bool(spaces),
            SettingsScope {
                document: Some(document.to_string()),
                ..Default::default()
            },
        )
        .expect("editor.insertSpaces is a native boolean setting");
}

/// Write a document's own `editor.softWrap` value, which beats language and
/// global values.
pub(crate) fn set_document_soft_wrap(document: uuid::Uuid, wrap: bool, cx: &mut App) {
    cx.default_global::<ApplicationSettings>()
        .0
        .set(
            EDITOR_SOFT_WRAP,
            Value::Bool(wrap),
            SettingsScope {
                document: Some(document.to_string()),
                ..Default::default()
            },
        )
        .expect("editor.softWrap is a native boolean setting");
}

/// Drop a document's own indentation values, so its style and size follow the
/// language and global settings again.
pub(crate) fn reset_document_indentation(document: uuid::Uuid, cx: &mut App) {
    let scope = SettingsScope {
        document: Some(document.to_string()),
        ..Default::default()
    };
    let store = &mut cx.default_global::<ApplicationSettings>().0;
    store.reset(EDITOR_INSERT_SPACES, &scope);
    store.reset(EDITOR_TAB_SIZE, &scope);
}

/// Drop the settings values scoped to a document that was released.
pub(crate) fn release_document(document: uuid::Uuid, cx: &mut App) {
    if cx.has_global::<ApplicationSettings>() {
        cx.global_mut::<ApplicationSettings>()
            .0
            .release_document(&document.to_string());
    }
}

/// The text size in pixels for a view in `context`, within a legible range.
pub(crate) fn font_size(context: &SettingsScope, cx: &App) -> f32 {
    resolve(FONT_SIZE, context, cx, Value::as_f64)
        .expect("font.size is a native number setting")
        .clamp(6., 72.) as f32
}

/// Columns per tab stop for an editor in `context`, within a sane range.
pub(crate) fn editor_tab_size(context: &SettingsScope, cx: &App) -> usize {
    resolve(EDITOR_TAB_SIZE, context, cx, Value::as_i64)
        .expect("editor.tabSize is a native integer setting")
        .clamp(1, 32) as usize
}

/// Whether an editor in `context` indents with spaces by default.
pub(crate) fn editor_insert_spaces(context: &SettingsScope, cx: &App) -> bool {
    resolve(EDITOR_INSERT_SPACES, context, cx, Value::as_bool)
        .expect("editor.insertSpaces is a native boolean setting")
}

/// Whether an editor in `context` auto-closes brackets and quotes.
pub(crate) fn editor_auto_closing_brackets(context: &SettingsScope, cx: &App) -> bool {
    resolve(EDITOR_AUTO_CLOSING_BRACKETS, context, cx, Value::as_bool)
        .expect("editor.autoClosingBrackets is a native boolean setting")
}

/// Whether an editor in `context` wraps a selection in a typed pair.
pub(crate) fn editor_auto_surround(context: &SettingsScope, cx: &App) -> bool {
    resolve(EDITOR_AUTO_SURROUND, context, cx, Value::as_bool)
        .expect("editor.autoSurround is a native boolean setting")
}

/// Whether an editor in `context` wraps long lines.
pub(crate) fn editor_soft_wrap(context: &SettingsScope, cx: &App) -> bool {
    resolve(EDITOR_SOFT_WRAP, context, cx, Value::as_bool)
        .expect("editor.softWrap is a native boolean setting")
}

/// How an editor in `context` numbers its lines.
pub(crate) fn editor_line_numbers(context: &SettingsScope, cx: &App) -> LineNumbers {
    match resolve(EDITOR_LINE_NUMBERS, context, cx, |value| {
        value.as_str().map(str::to_owned)
    })
    .as_deref()
    {
        Some("off") => LineNumbers::Off,
        Some("relative") => LineNumbers::Relative,
        _ => LineNumbers::On,
    }
}

/// Whether saving a document in `context` trims trailing whitespace.
pub(crate) fn files_trim_trailing_whitespace(context: &SettingsScope, cx: &App) -> bool {
    resolve(FILES_TRIM_TRAILING_WHITESPACE, context, cx, |value| {
        value.as_str().map(str::to_owned)
    })
    .as_deref()
        != Some("off")
}

/// Whether saving a document in `context` ends it with a line terminator.
pub(crate) fn files_insert_final_newline(context: &SettingsScope, cx: &App) -> bool {
    resolve(FILES_INSERT_FINAL_NEWLINE, context, cx, Value::as_bool)
        .expect("files.insertFinalNewline is a native boolean setting")
}

/// The fold lane of an editor in `context`.
pub(crate) fn editor_folding(context: &SettingsScope, cx: &App) -> Folding {
    if !resolve(EDITOR_FOLDING, context, cx, Value::as_bool)
        .expect("editor.folding is a native boolean setting")
    {
        return Folding::Off;
    }
    match resolve(EDITOR_SHOW_FOLDING_CONTROLS, context, cx, |value| {
        value.as_str().map(str::to_owned)
    })
    .as_deref()
    {
        Some("always") => Folding::Always,
        _ => Folding::Mouseover,
    }
}

/// What the editor gutter's fold lane shows.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Folding {
    Off,
    /// Chevrons on foldable lines while the pointer is over the gutter.
    #[default]
    Mouseover,
    Always,
}

/// What the editor gutter's line-number lane shows.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum LineNumbers {
    #[default]
    On,
    Off,
    /// Each line's distance from the cursor's line, which shows its own number.
    Relative,
}

fn resolve<T>(
    key: &str,
    context: &SettingsScope,
    cx: &App,
    read: impl Fn(&Value) -> Option<T>,
) -> Option<T> {
    match cx.try_global::<ApplicationSettings>() {
        Some(settings) => settings.0.get(key, context).and_then(read),
        None => SettingsStore::default().get(key, context).and_then(read),
    }
}

/// The value of a native boolean setting, unscoped.
pub(crate) fn boolean(key: &str, cx: &App) -> bool {
    resolve(key, &SettingsScope::default(), cx, Value::as_bool).expect("a native boolean setting")
}

/// The value of a native text setting, unscoped.
pub(crate) fn text(key: &str, cx: &App) -> String {
    resolve(key, &SettingsScope::default(), cx, |value| {
        value.as_str().map(str::to_owned)
    })
    .expect("a native text setting")
}

/// Where product windows place the bottom dock.
pub(crate) fn bottom_dock_layout(cx: &App) -> knot_model::workbench::BottomDockLayout {
    knot_model::workbench::BottomDockLayout::parse(&text(WORKBENCH_BOTTOM_DOCK_LAYOUT, cx))
}

/// Write a native boolean setting for every context.
pub(crate) fn set_boolean(key: &str, value: bool, cx: &mut App) {
    cx.default_global::<ApplicationSettings>()
        .0
        .set(key, Value::Bool(value), SettingsScope::default())
        .expect("a native boolean setting");
}

/// The ignore policy the `files.*` settings select.
pub(crate) fn files_ignore_policy(cx: &App) -> knot_model::files::IgnorePolicy {
    knot_model::files::IgnorePolicy {
        hidden: strings(FILES_HIDDEN, cx),
        ignored: strings(FILES_IGNORED, cx),
        respect_gitignore: resolve(
            FILES_RESPECT_GITIGNORE,
            &SettingsScope::default(),
            cx,
            Value::as_bool,
        )
        .expect("files.respectGitignore is a native boolean setting"),
    }
}

/// The value of a native text-list setting, unscoped.
pub(crate) fn strings(key: &str, cx: &App) -> Vec<String> {
    resolve(key, &SettingsScope::default(), cx, |value| {
        value.as_array().map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
    })
    .expect("a native text-list setting")
}
