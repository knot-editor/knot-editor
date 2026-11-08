//! Key notation: the keystrokes and modifiers Knot parses, matches, and shows.
//! The notation is `[ctrl-][alt-][shift-][cmd-][fn-]key[->key_char]`, with
//! `secondary` meaning `cmd` on macOS and `ctrl` elsewhere. Views convert to
//! and from the toolkit's types in [`super::convert`].

use std::fmt;

/// The modifier keys held at some point in time.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub(crate) struct Modifiers {
    pub(crate) control: bool,
    pub(crate) alt: bool,
    pub(crate) shift: bool,
    /// The command key on macOS, the windows key on Windows, super on Linux.
    pub(crate) platform: bool,
    pub(crate) function: bool,
}

impl Modifiers {
    pub(crate) fn none() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn shift() -> Self {
        Self {
            shift: true,
            ..Self::default()
        }
    }

    pub(crate) fn control() -> Self {
        Self {
            control: true,
            ..Self::default()
        }
    }

    /// Whether any modifier is held.
    pub(crate) fn modified(&self) -> bool {
        self.control || self.alt || self.shift || self.platform || self.function
    }
}

/// A key press: the key, the modifiers held, and the character it typed.
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub(crate) struct Keystroke {
    pub(crate) modifiers: Modifiers,
    /// The printed key, such as `s` for option-s. Named keys are lowercase,
    /// like `tab` and `enter`.
    pub(crate) key: String,
    /// The character the press typed, such as `ß` for option-s; `None` for
    /// cmd-s.
    pub(crate) key_char: Option<String>,
}

/// The source of a keystroke that does not parse.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct InvalidKeystroke(pub(crate) String);

impl fmt::Display for InvalidKeystroke {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid keystroke \"{}\"", self.0)
    }
}

impl std::error::Error for InvalidKeystroke {}

impl Keystroke {
    /// Parses `ctrl-shift-a`, `cmd-,`, `secondary-q`, or `a->å` (a key with
    /// the character it typed). A lone uppercase letter means shift plus the
    /// lowercase letter, and a modifier alone names that modifier's key.
    pub(crate) fn parse(source: &str) -> Result<Self, InvalidKeystroke> {
        let invalid = || InvalidKeystroke(source.to_owned());
        let mut modifiers = Modifiers::none();
        let mut key = None;
        let mut key_char = None;

        let mut components = source.split('-').peekable();
        while let Some(component) = components.next() {
            let is = |name: &str| component.eq_ignore_ascii_case(name);
            if is("ctrl") {
                modifiers.control = true;
                continue;
            }
            if is("alt") {
                modifiers.alt = true;
                continue;
            }
            if is("shift") {
                modifiers.shift = true;
                continue;
            }
            if is("fn") {
                modifiers.function = true;
                continue;
            }
            if is("secondary") {
                if cfg!(target_os = "macos") {
                    modifiers.platform = true;
                } else {
                    modifiers.control = true;
                }
                continue;
            }
            if is("cmd") || is("super") || is("win") {
                modifiers.platform = true;
                continue;
            }

            let mut named = component.to_owned();
            if let Some(next) = components.peek() {
                if next.is_empty() && source.ends_with('-') {
                    key = Some(String::from("-"));
                    break;
                } else if next.len() > 1 && next.starts_with('>') {
                    key = Some(named);
                    key_char = Some(String::from(&next[1..]));
                    components.next();
                } else {
                    return Err(invalid());
                }
                continue;
            }

            if component.len() == 1 && component.as_bytes()[0].is_ascii_uppercase() {
                modifiers.shift = true;
            }
            named.make_ascii_lowercase();
            key = Some(named);
        }

        let key = key
            .or_else(|| {
                [
                    (&mut modifiers.shift, "shift"),
                    (&mut modifiers.control, "control"),
                    (&mut modifiers.alt, "alt"),
                    (&mut modifiers.platform, "platform"),
                    (&mut modifiers.function, "function"),
                ]
                .into_iter()
                .find_map(|(held, name)| std::mem::take(held).then(|| name.to_owned()))
            })
            .ok_or_else(invalid)?;

        Ok(Self {
            modifiers,
            key,
            key_char,
        })
    }

    /// The notation [`Keystroke::parse`] reads back, in the canonical modifier
    /// order and spelling of this platform.
    pub(crate) fn unparse(&self) -> String {
        let mut result = String::new();
        let modifiers = &self.modifiers;
        if modifiers.function {
            result.push_str("fn-");
        }
        if modifiers.control {
            result.push_str("ctrl-");
        }
        if modifiers.alt {
            result.push_str("alt-");
        }
        if modifiers.platform {
            result.push_str(if cfg!(target_os = "macos") {
                "cmd-"
            } else if cfg!(windows) {
                "win-"
            } else {
                "super-"
            });
        }
        if modifiers.shift {
            result.push_str("shift-");
        }
        result.push_str(&self.key);
        result
    }

    /// Fills `key_char` the way a keyboard would when only shift modifies
    /// the key, so a keystroke written in notation types its character.
    pub(crate) fn with_simulated_text(mut self) -> Self {
        let modifiers = &self.modifiers;
        if self.key_char.is_none()
            && !modifiers.platform
            && !modifiers.control
            && !modifiers.function
            && !modifiers.alt
        {
            self.key_char = match self.key.as_str() {
                "space" => Some(" ".into()),
                "tab" => Some("\t".into()),
                "enter" => Some("\n".into()),
                key if key.is_empty() || !is_printable(key) => None,
                key if modifiers.shift => Some(key.to_uppercase()),
                key => Some(key.into()),
            };
        }
        self
    }

    /// Whether this typed keystroke triggers a binding written as `bound`:
    /// directly, or through the character it typed, which already reflects
    /// its other modifiers.
    pub(crate) fn triggers(&self, bound: &Keystroke) -> bool {
        if let Some(key_char) = self
            .key_char
            .as_ref()
            .filter(|key_char| **key_char != self.key)
        {
            let modifiers = if cfg!(windows) {
                Modifiers::none()
            } else {
                Modifiers {
                    control: self.modifiers.control,
                    platform: self.modifiers.platform,
                    ..Modifiers::none()
                }
            };
            if bound.key == *key_char && bound.modifiers == modifiers {
                return true;
            }
        }
        bound.modifiers == self.modifiers && bound.key == self.key
    }
}

/// Whether a key names a character rather than a function or navigation key.
fn is_printable(key: &str) -> bool {
    let function_key = key
        .strip_prefix('f')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=35).contains(&number));
    !function_key
        && !matches!(
            key,
            "backspace"
                | "delete"
                | "left"
                | "right"
                | "up"
                | "down"
                | "pageup"
                | "pagedown"
                | "insert"
                | "home"
                | "end"
                | "back"
                | "forward"
                | "escape"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Keystroke {
        Keystroke::parse(source).unwrap()
    }

    #[test]
    fn parses_modifiers_and_keys_in_any_order_and_case() {
        let stroke = parse("Shift-CTRL-a");
        assert_eq!(
            stroke.modifiers,
            Modifiers {
                control: true,
                shift: true,
                ..Modifiers::none()
            }
        );
        assert_eq!(stroke.key, "a");
        assert_eq!(parse("ENTER").key, "enter");
        assert_eq!(parse("cmd-,").key, ",");
        assert_eq!(parse("ctrl--").key, "-");
    }

    #[test]
    fn an_uppercase_letter_means_shift_and_the_lowercase_letter() {
        let stroke = parse("A");
        assert_eq!(stroke.key, "a");
        assert_eq!(stroke.modifiers, Modifiers::shift());
    }

    #[test]
    fn a_modifier_alone_is_its_own_key() {
        let stroke = parse("shift");
        assert_eq!(stroke.key, "shift");
        assert_eq!(stroke.modifiers, Modifiers::none());
        assert_eq!(parse("ctrl").key, "control");
    }

    #[test]
    fn secondary_is_command_on_macos_and_control_elsewhere() {
        let modifiers = parse("secondary-q").modifiers;
        assert_eq!(modifiers.platform, cfg!(target_os = "macos"));
        assert_eq!(modifiers.control, !cfg!(target_os = "macos"));
    }

    #[test]
    fn a_key_char_follows_an_arrow() {
        let stroke = parse("alt-s->ß");
        assert_eq!(stroke.key, "s");
        assert_eq!(stroke.key_char.as_deref(), Some("ß"));
        assert!(stroke.modifiers.alt);
    }

    #[test]
    fn rejects_text_that_is_not_a_keystroke() {
        for source in ["ctrl-ab-c", "a-b"] {
            assert_eq!(
                Keystroke::parse(source),
                Err(InvalidKeystroke(source.to_owned())),
                "{source:?}"
            );
        }
    }

    #[test]
    fn unparse_round_trips_through_parse() {
        for source in [
            "ctrl-shift-a",
            "alt-tab",
            "fn-f1",
            "shift-left",
            "ctrl--",
            "x",
        ] {
            let stroke = parse(source);
            assert_eq!(parse(&stroke.unparse()), stroke, "{source}");
        }
    }

    #[test]
    fn unparse_orders_modifiers_canonically() {
        assert_eq!(parse("shift-alt-ctrl-a").unparse(), "ctrl-alt-shift-a");
    }

    #[test]
    fn simulated_text_follows_shift_and_ignores_command_keys() {
        assert_eq!(
            parse("a").with_simulated_text().key_char.as_deref(),
            Some("a")
        );
        assert_eq!(
            parse("A").with_simulated_text().key_char.as_deref(),
            Some("A")
        );
        assert_eq!(
            parse("space").with_simulated_text().key_char.as_deref(),
            Some(" ")
        );
        assert_eq!(
            parse("enter").with_simulated_text().key_char.as_deref(),
            Some("\n")
        );
        assert_eq!(parse("left").with_simulated_text().key_char, None);
        assert_eq!(parse("f5").with_simulated_text().key_char, None);
        assert_eq!(parse("ctrl-a").with_simulated_text().key_char, None);
    }

    #[test]
    fn a_typed_keystroke_triggers_a_binding_by_key_or_by_typed_character() {
        assert!(parse("ctrl-a").triggers(&parse("ctrl-a")));
        assert!(!parse("ctrl-a").triggers(&parse("ctrl-b")));
        assert!(!parse("a").triggers(&parse("shift-a")));

        let question = Keystroke {
            key: "/".into(),
            modifiers: Modifiers::shift(),
            key_char: Some("?".into()),
        };
        if !cfg!(windows) {
            assert!(question.triggers(&parse("?")));
        }
        assert!(question.triggers(&parse("shift-/")));
    }
}
