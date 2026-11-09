//! Language registrations, document language detection, and the syntax state
//! each buffer's language attaches.
//!
//! Extensions and personal config register languages: a language ID, the
//! compiled-in grammar it parses with, the file names, extensions, and shebang
//! interpreters it claims, its queries, and the editing data native editing
//! commands read (see [`editing`]). An association claims patterns for
//! an ID without defining the language. The latest registration of an ID
//! defines that language, and the latest registration or association claiming
//! a pattern decides detection, so removing either reveals the previous one.
//!
//! A buffer records a language ID, explicit or detected, independently of
//! registration: an unregistered ID paints and reports as `plaintext` until
//! something registers it. Every registration change re-detects open documents whose
//! language is not explicit and reattaches syntax wherever the resolved
//! language changed.

use std::{collections::HashMap, sync::Arc};

use serde::Serialize;

use knot_protocol::{ExtensionId, ExtensionLifecycleId, LanguageInfo};

use crate::{
    buffer::BufferModel,
    editing::EditingDefinition,
    syntax::{LanguageResolver, SyntaxLanguage, SyntaxLanguageError},
};

/// The language a buffer reports when its ID is missing or unregistered,
/// as LSP names it.
pub const PLAIN_TEXT: &str = "plaintext";

/// How far detection reads into a first line for a shebang.
const SHEBANG_LIMIT: usize = 256;

/// The extension lifecycle that owns a registration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LanguageOwner {
    pub extension: ExtensionId,
    pub lifecycle: ExtensionLifecycleId,
}

/// A language as an extension registers it and `languages.get` returns it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LanguageDefinition {
    #[serde(flatten)]
    pub info: LanguageInfo,
    pub queries: LanguageQueries,
    #[serde(flatten)]
    pub editing: EditingDefinition,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LanguageQueries {
    pub highlights: String,
    pub injections: String,
    pub indents: String,
    pub folds: String,
}

impl LanguageDefinition {
    /// Compile the queries against the grammar. This is the expensive part
    /// of registering a language, and needs no application state, so callers
    /// run it off the foreground.
    pub fn compile(self) -> Result<CompiledLanguage, SyntaxLanguageError> {
        let syntax = SyntaxLanguage::new(
            &self.info.grammar,
            &self.queries.highlights,
            &self.queries.injections,
        )?
        .with_indents(&self.queries.indents)?
        .with_folds(&self.queries.folds)?
        .with_editing(self.editing.resolve());
        Ok(CompiledLanguage {
            definition: self,
            syntax: Arc::new(syntax),
        })
    }
}

/// A language definition whose queries compiled.
pub struct CompiledLanguage {
    definition: LanguageDefinition,
    syntax: Arc<SyntaxLanguage>,
}

/// File patterns claimed for a language without defining it.
#[derive(Clone, Debug, PartialEq)]
pub struct LanguageAssociation {
    pub language: String,
    pub file_extensions: Vec<String>,
    pub file_names: Vec<String>,
    pub interpreters: Vec<String>,
}

/// A language registration, or an association, which claims patterns for
/// `info.id` and has no name, grammar, or language of its own.
struct Registration {
    id: u64,
    owner: LanguageOwner,
    info: LanguageInfo,
    language: Option<CompiledLanguage>,
}

/// The languages injections and buffers resolve, rebuilt on every change.
#[derive(Default)]
struct LanguageTable {
    languages: HashMap<String, Arc<SyntaxLanguage>>,
    /// Lowercase file extension to language ID.
    extensions: HashMap<String, String>,
}

impl LanguageTable {
    /// The language an injection names: a language ID, compared exactly and
    /// then in lowercase, or else a file extension, as Markdown fences often
    /// name `js` or `py`.
    fn resolve_injection(&self, name: &str) -> Option<Arc<SyntaxLanguage>> {
        let lowercase = name.to_lowercase();
        self.languages
            .get(name)
            .or_else(|| self.languages.get(&lowercase))
            .or_else(|| {
                self.extensions
                    .get(&lowercase)
                    .and_then(|id| self.languages.get(id))
            })
            .cloned()
    }
}

#[derive(Default)]
pub struct LanguageRegistry {
    /// In registration order, so a later index wins.
    registrations: Vec<Registration>,
    next_id: u64,
    table: Arc<LanguageTable>,
}

impl LanguageRegistry {
    /// Add a registration, returning its ID.
    pub fn register(&mut self, owner: LanguageOwner, language: CompiledLanguage) -> u64 {
        self.next_id += 1;
        self.registrations.push(Registration {
            id: self.next_id,
            owner,
            info: language.definition.info.clone(),
            language: Some(language),
        });
        self.rebuild();
        self.next_id
    }

    /// Add an association, returning its ID. It shares registration IDs and
    /// order, so the latest claim of a pattern wins either way.
    pub fn associate(&mut self, owner: LanguageOwner, association: LanguageAssociation) -> u64 {
        self.next_id += 1;
        self.registrations.push(Registration {
            id: self.next_id,
            owner,
            info: LanguageInfo {
                id: association.language,
                name: String::new(),
                grammar: String::new(),
                file_extensions: association.file_extensions,
                file_names: association.file_names,
                interpreters: association.interpreters,
            },
            language: None,
        });
        self.rebuild();
        self.next_id
    }

    /// Remove registration `id` of one extension lifecycle. Returns false when
    /// it owns none.
    pub fn unregister(
        &mut self,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
        id: u64,
    ) -> bool {
        self.remove(|registration| {
            registration.id == id
                && registration.owner.extension == extension
                && registration.owner.lifecycle == lifecycle
        })
    }

    /// Remove every registration of one extension lifecycle.
    pub fn remove_lifecycle(
        &mut self,
        extension: ExtensionId,
        lifecycle: ExtensionLifecycleId,
    ) -> bool {
        self.remove(|registration| {
            registration.owner.extension == extension && registration.owner.lifecycle == lifecycle
        })
    }

    fn remove(&mut self, matches: impl Fn(&Registration) -> bool) -> bool {
        let before = self.registrations.len();
        self.registrations
            .retain(|registration| !matches(registration));
        let changed = self.registrations.len() != before;
        if changed {
            self.rebuild();
        }
        changed
    }

    /// The registration that defines each language ID, sorted by ID.
    fn effective(&self) -> Vec<&Registration> {
        let mut effective: HashMap<&str, &Registration> = HashMap::new();
        for registration in self
            .registrations
            .iter()
            .filter(|registration| registration.language.is_some())
        {
            effective.insert(&registration.info.id, registration);
        }
        let mut effective: Vec<_> = effective.into_values().collect();
        effective.sort_by(|a, b| a.info.id.cmp(&b.info.id));
        effective
    }

    fn rebuild(&mut self) {
        let mut languages = HashMap::new();
        for registration in self.effective() {
            let language = registration.language.as_ref().expect("effective languages");
            languages.insert(registration.info.id.clone(), Arc::clone(&language.syntax));
        }
        let mut extensions = HashMap::new();
        for registration in &self.registrations {
            for extension in &registration.info.file_extensions {
                extensions.insert(extension.to_lowercase(), registration.info.id.clone());
            }
        }
        self.table = Arc::new(LanguageTable {
            languages,
            extensions,
        });
    }

    /// The language each registered ID resolves to, sorted by ID.
    pub fn list(&self) -> Vec<LanguageInfo> {
        self.effective()
            .into_iter()
            .map(|registration| registration.info.clone())
            .collect()
    }

    /// The definition of the registration that defines `id`.
    pub fn get(&self, id: &str) -> Option<&LanguageDefinition> {
        self.registrations
            .iter()
            .rev()
            .filter(|registration| registration.info.id == id)
            .find_map(|registration| registration.language.as_ref())
            .map(|language| &language.definition)
    }

    pub fn resolve(&self, id: &str) -> Option<Arc<SyntaxLanguage>> {
        self.table.languages.get(id).cloned()
    }

    /// The editing configuration of the registration that defines `id`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn editing(&self, id: &str) -> Option<Arc<crate::editing::EditingConfiguration>> {
        self.resolve(id)
            .map(|language| Arc::clone(language.editing()))
    }

    /// Resolves injected language names against the registrations as they
    /// are now.
    pub fn resolver(&self) -> LanguageResolver {
        let table = Arc::clone(&self.table);
        Arc::new(move |name| table.resolve_injection(name))
    }

    /// The language a file called `file_name` whose text starts with
    /// `first_line` should use: a registered file name, then a registered
    /// extension, then the shebang interpreter. The longest extension or an
    /// exact interpreter wins, and then the latest registration claiming it.
    pub fn detect(&self, file_name: Option<&str>, first_line: &str) -> Option<String> {
        let best = |claim: &dyn Fn(&LanguageInfo) -> Option<usize>| {
            self.registrations
                .iter()
                .enumerate()
                .filter_map(|(index, registration)| {
                    claim(&registration.info)
                        .map(|specificity| ((specificity, index), &registration.info.id))
                })
                .max_by_key(|(rank, _)| *rank)
                .map(|(_, id)| id.clone())
        };
        if let Some(name) = file_name {
            let lowercase = name.to_lowercase();
            let detected = best(&|info| {
                info.file_names
                    .iter()
                    .any(|known| known == name)
                    .then_some(0)
            })
            .or_else(|| {
                best(&|info| {
                    info.file_extensions
                        .iter()
                        .filter(|extension| {
                            let extension = extension.to_lowercase();
                            lowercase.len() > extension.len()
                                && lowercase.ends_with(&extension)
                                && lowercase[..lowercase.len() - extension.len()].ends_with('.')
                        })
                        .map(String::len)
                        .max()
                })
            });
            if detected.is_some() {
                return detected;
            }
        }
        let interpreter = interpreter(first_line)?;
        // `python3.12` also matches a registered `python`.
        let unversioned = interpreter.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        best(&|info| {
            info.interpreters
                .iter()
                .any(|known| known == interpreter)
                .then_some(1)
                .or_else(|| {
                    info.interpreters
                        .iter()
                        .any(|known| known == unversioned)
                        .then_some(0)
                })
        })
    }
}

/// The program a `#!` line runs, looking through `env` and its options.
fn interpreter(first_line: &str) -> Option<&str> {
    let mut words = first_line.strip_prefix("#!")?.split_whitespace();
    let program = words.next()?.rsplit('/').next()?;
    if program != "env" {
        return Some(program);
    }
    words.find(|word| !word.starts_with('-') && !word.contains('='))
}

/// The first line when it is a shebang, read no further than the limit.
pub fn shebang_line(model: &BufferModel) -> String {
    if model.len() < 2 || !model.is_char_boundary(2) || model.read(0..2) != "#!" {
        return String::new();
    }
    let mut end = model.line_range(0).end.min(SHEBANG_LIMIT);
    while !model.is_char_boundary(end) {
        end -= 1;
    }
    model.read(0..end)
}

#[cfg(test)]
mod tests;
