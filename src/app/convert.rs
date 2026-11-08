//! The edge between Knot's own vocabulary types and the toolkit's. Views and
//! the shell convert here; the rest of the application uses Knot's types.

use super::keys;

pub(crate) fn modifiers_from_gpui(modifiers: gpui::Modifiers) -> keys::Modifiers {
    keys::Modifiers {
        control: modifiers.control,
        alt: modifiers.alt,
        shift: modifiers.shift,
        platform: modifiers.platform,
        function: modifiers.function,
    }
}

pub(crate) fn modifiers_to_gpui(modifiers: keys::Modifiers) -> gpui::Modifiers {
    gpui::Modifiers {
        control: modifiers.control,
        alt: modifiers.alt,
        shift: modifiers.shift,
        platform: modifiers.platform,
        function: modifiers.function,
    }
}

pub(crate) fn keystroke_from_gpui(keystroke: &gpui::Keystroke) -> keys::Keystroke {
    keys::Keystroke {
        modifiers: modifiers_from_gpui(keystroke.modifiers),
        key: keystroke.key.clone(),
        key_char: keystroke.key_char.clone(),
    }
}

pub(crate) fn keystroke_to_gpui(keystroke: &keys::Keystroke) -> gpui::Keystroke {
    gpui::Keystroke {
        modifiers: modifiers_to_gpui(keystroke.modifiers),
        key: keystroke.key.clone(),
        key_char: keystroke.key_char.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keystrokes_round_trip_through_gpui() {
        for source in ["ctrl-shift-a", "alt-s->ß", "cmd-,", "f5", "ctrl--"] {
            let knot = keys::Keystroke::parse(source).unwrap();
            let gpui = keystroke_to_gpui(&knot);
            assert_eq!(keystroke_from_gpui(&gpui), knot, "{source}");
        }
    }

    #[test]
    fn parsing_agrees_with_gpui() {
        for source in [
            "ctrl-shift-a",
            "Shift-CTRL-a",
            "A",
            "shift",
            "ctrl",
            "secondary-q",
            "alt-s->ß",
            "cmd-,",
            "ctrl--",
            "ENTER",
            "fn-f1",
            "super-x",
        ] {
            let knot = keys::Keystroke::parse(source).unwrap();
            let gpui = gpui::Keystroke::parse(source).unwrap();
            assert_eq!(keystroke_to_gpui(&knot), gpui, "{source}");
            assert_eq!(knot.unparse(), gpui.unparse(), "{source}");
        }
        for source in ["ctrl-ab-c", "a-b"] {
            assert!(gpui::Keystroke::parse(source).is_err());
            assert!(keys::Keystroke::parse(source).is_err());
        }
    }

    #[test]
    fn simulated_text_and_matching_agree_with_gpui() {
        for source in [
            "a", "A", "space", "tab", "left", "f5", "ctrl-a", "alt-x", "shift-/",
        ] {
            let knot = keys::Keystroke::parse(source)
                .unwrap()
                .with_simulated_text();
            let gpui = gpui::Keystroke::parse(source).unwrap().with_simulated_ime();
            assert_eq!(keystroke_to_gpui(&knot), gpui, "{source}");
        }
    }
}
