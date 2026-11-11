//! The case files bundled packages keep beside their sources and the
//! machinery every runner shares: discovery, JSONC parsing with line
//! numbers, name filtering, caret markers, the key notation, and failure
//! reports that name the file and line of the case.
//!
//! A package keeps its cases in `test/cases/<topic>.jsonc`. A file is one
//! group, an object whose arrays hold cases, or an array of such groups.
//! Every case has a `name`; a runner reads the arrays it knows. Setting the
//! `KNOT_CASES` environment variable runs only the cases whose names contain
//! it.
//!
//! Texts are arrays of lines, joined with `\n`, in which `‸` marks the caret,
//! or both ends of a selection, anchor first. A long document can be
//! generated instead: `{ "numbered": 30, "caret": [2, 1] }` is thirty lines
//! ` 00` to ` 29` with the caret at line 2, column 1, and an `"anchor"` of
//! the same shape starts a selection there.
//!
//! In the key notation, characters are typed and `<name>` presses a named
//! key: `<esc>`, `<cr>`, `<bs>`, `<del>`, `<tab>`, `<s-tab>`, the arrows,
//! `<home>`, `<end>`, `<lt>` for `<`, and `<c-x>` for ctrl with a letter.
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use jsonc_parser::common::Ranged;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Marks the caret, or both ends of a selection, in a text.
pub const CARET: char = '‸';

/// The package directories in `root`, such as `extensions/@knot`.
pub fn packages_in(root: &Path) -> Vec<PathBuf> {
    let mut packages: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap_or_else(|error| panic!("{}: {error}", root.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    packages.sort();
    packages
}

/// Each of `packages`' `test/cases/<topic>.jsonc` that exists.
pub fn topic_files(packages: &[PathBuf], topic: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = packages
        .iter()
        .map(|package| package.join("test/cases").join(format!("{topic}.jsonc")))
        .filter(|path| path.exists())
        .collect();
    files.sort();
    files
}

/// Every case file of the package in `package`.
pub fn package_files(package: &Path) -> Vec<PathBuf> {
    let directory = package.join("test/cases");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonc")
        })
        .collect();
    files.sort();
    files
}

/// Where a case is written.
#[derive(Clone)]
pub struct Location {
    file: String,
    line: usize,
}

impl fmt::Display for Location {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}:{}", self.file, self.line)
    }
}

/// A case with its name and where it is written.
pub struct Case<T> {
    pub at: Location,
    pub name: String,
    pub value: T,
}

/// One object of a case file: its fields, and the line of each element of
/// its arrays.
pub struct Group {
    file: String,
    fields: serde_json::Map<String, Value>,
    lines: HashMap<String, Vec<usize>>,
}

impl Group {
    /// The group's field `key`, which must be present.
    pub fn field<T: DeserializeOwned>(&self, key: &str) -> T {
        let value = self
            .fields
            .get(key)
            .unwrap_or_else(|| panic!("{}: no \"{key}\"", self.file))
            .clone();
        serde_json::from_value(value)
            .unwrap_or_else(|error| panic!("{}: {key}: {error}", self.file))
    }

    /// The cases in the array `key`, if any, that the name filter keeps.
    pub fn cases<T: DeserializeOwned>(&self, key: &str) -> Vec<Case<T>> {
        let Some(Value::Array(elements)) = self.fields.get(key) else {
            return Vec::new();
        };
        let filter = std::env::var("KNOT_CASES").ok();
        let mut cases = Vec::new();
        for (element, line) in elements.iter().zip(&self.lines[key]) {
            let at = Location {
                file: self.file.clone(),
                line: *line,
            };
            let Value::Object(object) = element else {
                panic!("{at}: a case is an object");
            };
            let mut object = object.clone();
            let Some(Value::String(name)) = object.remove("name") else {
                panic!("{at}: a case needs a \"name\"");
            };
            if filter
                .as_ref()
                .is_some_and(|filter| !name.contains(filter.as_str()))
            {
                continue;
            }
            let value = serde_json::from_value(Value::Object(object))
                .unwrap_or_else(|error| panic!("{at}: {name}: {error}"));
            cases.push(Case { at, name, value });
        }
        cases
    }
}

/// The groups of the file at `path`, each holding only the `allowed` fields.
pub fn read_groups(path: &Path, allowed: &[&str]) -> Vec<Group> {
    // Name the file from its package scope on, such as `@knot/vim/test/…`.
    let shown = path.to_string_lossy();
    let file = match shown.find("/@") {
        Some(start) => shown[start + 1..].to_owned(),
        None => shown.into_owned(),
    };
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{file}: {error}"));
    let parsed = jsonc_parser::parse_to_ast(&text, &Default::default(), &Default::default())
        .unwrap_or_else(|error| panic!("{file}: {error}"));
    let line_of = |offset: usize| text[..offset].matches('\n').count() + 1;
    let objects = match parsed.value {
        Some(jsonc_parser::ast::Value::Object(object)) => vec![object],
        Some(jsonc_parser::ast::Value::Array(array)) => array
            .elements
            .into_iter()
            .map(|element| match element {
                jsonc_parser::ast::Value::Object(object) => object,
                _ => panic!("{file}: a group is an object"),
            })
            .collect(),
        _ => panic!("{file}: expected a group or a list of groups"),
    };
    objects
        .into_iter()
        .map(|object| {
            let mut fields = serde_json::Map::new();
            let mut lines = HashMap::new();
            for property in object.properties {
                let key = property.name.as_str().to_owned();
                assert!(
                    allowed.contains(&key.as_str()),
                    "{file}: unknown field \"{key}\""
                );
                if let jsonc_parser::ast::Value::Array(array) = &property.value {
                    let starts = array
                        .elements
                        .iter()
                        .map(|element| line_of(element.start()));
                    lines.insert(key.clone(), starts.collect());
                }
                fields.insert(key, property.value.into());
            }
            Group {
                file: file.clone(),
                fields,
                lines,
            }
        })
        .collect()
}

/// What went wrong in a run, with the location of each case.
#[derive(Default)]
pub struct Failures(Vec<String>);

impl Failures {
    /// Record that `actual` is not `expected` for `case`, labelled with
    /// `context` such as a language.
    pub fn check<T>(&mut self, case: &Case<T>, context: &str, expected: &str, actual: &str) {
        if expected != actual {
            self.fail(
                case,
                context,
                format!("expected:\n{expected}\n  actual:\n{actual}"),
            );
        }
    }

    pub fn fail<T>(&mut self, case: &Case<T>, context: &str, detail: String) {
        let context = if context.is_empty() {
            String::new()
        } else {
            format!("{context}: ")
        };
        self.0
            .push(format!("{} {context}{}\n  {detail}", case.at, case.name));
    }

    /// Fail the test if anything was recorded.
    pub fn assert_none(self) {
        assert!(self.0.is_empty(), "\n{}", self.0.join("\n"));
    }
}

/// A text in a case: lines, or a generated numbered document.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum Text {
    Lines(Vec<String>),
    Numbered(Numbered),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Numbered {
    numbered: usize,
    caret: Option<(usize, usize)>,
    anchor: Option<(usize, usize)>,
}

impl Text {
    /// The lines, with the caret markers in them.
    pub fn lines(&self) -> Vec<String> {
        match self {
            Text::Lines(lines) => lines.clone(),
            Text::Numbered(numbered) => {
                let mut lines: Vec<String> = (0..numbered.numbered)
                    .map(|index| format!(" {index:02}"))
                    .collect();
                // The later marker goes first, so the earlier one's column holds.
                let mut marks: Vec<(usize, usize)> =
                    numbered.anchor.into_iter().chain(numbered.caret).collect();
                marks.sort();
                for (line, column) in marks.into_iter().rev() {
                    lines[line].insert(column, CARET);
                }
                lines
            }
        }
    }
}

/// `lines` joined, without markers, and the selection they mark.
pub fn unmark(lines: &[String]) -> (String, Option<(usize, usize)>) {
    let marked = lines.join("\n");
    let markers: Vec<usize> = marked
        .match_indices(CARET)
        .enumerate()
        .map(|(index, (offset, _))| offset - index * CARET.len_utf8())
        .collect();
    let text = marked.replace(CARET, "");
    let selection = match markers[..] {
        [] => None,
        [caret] => Some((caret, caret)),
        [anchor, caret] => Some((anchor, caret)),
        _ => panic!("more than two carets: {marked:?}"),
    };
    (text, selection)
}

/// gpui keystrokes for the key notation.
pub fn parse_keys(keys: &str) -> Vec<String> {
    let mut parsed = Vec::new();
    let mut rest = keys;
    while let Some(character) = rest.chars().next() {
        if character == '<'
            && let Some(end) = rest.find('>')
        {
            let mut name = &rest[1..end];
            let mut keystroke = String::new();
            // Modifier prefixes: c- control, d- command, s- shift, a- alt.
            while let Some((modifier, remainder)) = name.split_once('-')
                && !remainder.is_empty()
            {
                keystroke.push_str(match modifier {
                    "c" => "ctrl-",
                    "d" => "cmd-",
                    "s" => "shift-",
                    "a" => "alt-",
                    _ => panic!("unknown key <{name}>"),
                });
                name = remainder;
            }
            keystroke.push_str(match name {
                "esc" => "escape",
                "cr" => "enter",
                "bs" => "backspace",
                "del" => "delete",
                "lt" => "<",
                "tab" | "left" | "right" | "up" | "down" | "home" | "end" => name,
                _ if name.chars().count() == 1 => name,
                _ if name.len() > 1
                    && name.starts_with('f')
                    && name[1..].chars().all(|c| c.is_ascii_digit()) =>
                {
                    name
                }
                _ => panic!("unknown key <{name}>"),
            });
            parsed.push(keystroke);
            rest = &rest[end + 1..];
        } else {
            parsed.push(match character {
                ' ' => "space".to_owned(),
                upper if upper.is_ascii_uppercase() => {
                    format!("shift-{}", upper.to_ascii_lowercase())
                }
                other => other.to_string(),
            });
            rest = &rest[character.len_utf8()..];
        }
    }
    parsed
}

/// A test per topic file of `package`, run by `runner(topic, cx)`, so the
/// topics run in parallel, and a test that every file has one.
#[macro_export]
macro_rules! topics {
    ($package:expr, $runner:ident; $($topic:ident),* $(,)?) => {
        mod topics {
            use super::*;
            $(
                #[gpui::test]
                fn $topic(cx: &mut TestAppContext) {
                    $runner(stringify!($topic), cx);
                }
            )*

            #[test]
            fn every_topic_file_has_a_test() {
                let package = $package;
                let package: &std::path::Path = package.as_ref();
                let files: Vec<String> = $crate::test_support::cases::package_files(package)
                    .iter()
                    .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
                    .collect();
                assert_eq!(
                    files,
                    [$(stringify!($topic)),*],
                    "list each file in {}/test/cases in `topics!`",
                    package.display()
                );
            }
        }
    };
}
pub use crate::topics;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_give_the_text_and_the_selection() {
        let lines = |text: &[&str]| text.iter().map(|line| line.to_string()).collect::<Vec<_>>();
        assert_eq!(unmark(&lines(&["ab", "c"])), ("ab\nc".into(), None));
        assert_eq!(unmark(&lines(&["a‸b"])), ("ab".into(), Some((1, 1))));
        assert_eq!(
            unmark(&lines(&["‸ab", "c‸"])),
            ("ab\nc".into(), Some((0, 4)))
        );
    }

    #[test]
    fn a_numbered_text_places_its_markers() {
        let text = Text::Numbered(Numbered {
            numbered: 3,
            caret: Some((2, 1)),
            anchor: Some((0, 0)),
        });
        assert_eq!(text.lines(), ["‸ 00", " 01", " ‸02"]);
    }

    #[test]
    fn keys_parse_to_keystrokes() {
        assert_eq!(
            parse_keys("2G<c-d>A<esc> <lt>"),
            ["2", "shift-g", "ctrl-d", "shift-a", "escape", "space", "<"]
        );
    }
}
