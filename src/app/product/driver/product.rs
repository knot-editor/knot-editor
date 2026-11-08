//! The application started headless with chosen packages.

use crate::app::convert::ThemePaint as _;
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{AppContext, Hsla, TestAppContext};
use serde_json::Value;

use super::editor::TestEditor;
use super::setup::{install_globals, install_launch_gate, wait_for_product_startup};
use crate::app::extensions::load::LoadResult;
use crate::app::filesystem::{
    FileSystemProvider, FileSystemProviderRegistry, MemoryFileSystemProvider,
};
use crate::app::model::BufferModel;
use crate::app::product::{ApplicationDocuments, ApplicationFileSystems};
use crate::app::resource::ResourceUri;
use crate::app::settings::{ApplicationSettings, SettingsScope};

/// The product under a personal config with no installed packages, either
/// started without a window or [launched](TestProduct::launch). It keeps
/// the directories startup read from and its durable state.
pub struct TestProduct {
    pub(super) config: tempfile::TempDir,
    pub(super) extensions: tempfile::TempDir,
    pub(super) state: tempfile::TempDir,
}

/// One package's outcome in the startup report.
#[derive(Clone, Debug)]
pub struct LoadEntry {
    pub name: Option<String>,
    pub loaded: bool,
    /// The outcome as the report shows it.
    pub detail: String,
}

/// A registered language and the editing features it declares.
#[derive(Clone, Debug)]
pub struct Language {
    pub id: String,
    pub grammar: String,
    /// It declares line comment tokens.
    pub line_comments: bool,
    /// It declares block comment delimiters.
    pub block_comments: bool,
    /// It registers a non-empty indents query.
    pub indents: bool,
    /// It registers bracket pairs.
    pub brackets: bool,
    /// It registers a folds query.
    pub folds: bool,
}

/// Files served from memory to the product's file-system providers.
pub struct ServedFiles(Arc<MemoryFileSystemProvider>);

impl ServedFiles {
    /// The text the file at `uri` holds now.
    pub fn read(&self, uri: &str) -> String {
        let file = pollster::block_on(self.0.read(ResourceUri::parse(uri).unwrap())).unwrap();
        String::from_utf8(file.bytes).unwrap()
    }
}

impl TestProduct {
    /// Start `packages` with an empty personal config, and require that each
    /// one loaded.
    pub fn start(
        packages: &'static [knot_bundled::BundledSource],
        cx: &mut TestAppContext,
    ) -> Self {
        Self::start_with_post_init(packages, None, cx)
    }

    /// Start `packages` under a personal config whose `post-init.js` is
    /// `post_init`, when given, and require that each one loaded.
    pub fn start_with_post_init(
        packages: &'static [knot_bundled::BundledSource],
        post_init: Option<&str>,
        cx: &mut TestAppContext,
    ) -> Self {
        let product = Self::new_directories();
        if let Some(source) = post_init {
            std::fs::write(product.config.path().join("post-init.js"), source).unwrap();
        }
        install_globals(cx);
        // Product startup binds keys before packages start, so they can bind theirs.
        cx.update(super::super::launch::bind_product_keys);
        install_launch_gate(cx);
        let host = cx.update(crate::app::extensions::host::install);
        host.update(cx, |host, cx| {
            host.set_bundled_packages(packages);
            host.start_product_startup(
                Ok(product.config.path().to_path_buf()),
                Ok(product.extensions.path().to_path_buf()),
                cx,
            )
        });
        assert!(wait_for_product_startup(cx));
        let failures: Vec<_> = product
            .load_report(cx)
            .into_iter()
            .filter(|entry| !entry.loaded)
            .collect();
        assert!(failures.is_empty(), "{failures:?}");
        // Launch opens a window when nothing else does; close it, since a
        // window redraws on every change its session sees.
        for window in cx.windows() {
            cx.update_window(window, |_, window, _| window.remove_window())
                .unwrap();
        }
        cx.run_until_parked();
        product
    }

    pub(super) fn new_directories() -> Self {
        Self {
            config: tempfile::tempdir().unwrap(),
            extensions: tempfile::tempdir().unwrap(),
            state: tempfile::tempdir().unwrap(),
        }
    }

    /// Each package startup discovered, with its outcome.
    pub fn load_report(&self, cx: &mut TestAppContext) -> Vec<LoadEntry> {
        let report = cx
            .read(crate::app::extensions::host::startup_report)
            .unwrap();
        report
            .report
            .entries
            .iter()
            .map(|entry| LoadEntry {
                name: entry.name.clone(),
                loaded: entry.result == LoadResult::Loaded,
                detail: format!("{:?}", entry.result),
            })
            .collect()
    }

    /// Every registered language.
    pub fn languages(&self, cx: &mut TestAppContext) -> Vec<Language> {
        cx.read(|cx| {
            crate::app::languages::list(cx)
                .into_iter()
                .map(|info| {
                    let id = info.id.to_string();
                    let editing = crate::app::languages::editing_configuration(&id, cx);
                    let editing = editing.as_ref();
                    let language = crate::app::languages::get(&id, cx);
                    let language = language.as_ref();
                    Language {
                        grammar: info.grammar.to_string(),
                        line_comments: editing
                            .is_some_and(|editing| !editing.line_comments.is_empty()),
                        block_comments: editing
                            .is_some_and(|editing| editing.block_comment.is_some()),
                        indents: language
                            .is_some_and(|language| !language.queries.indents.trim().is_empty()),
                        brackets: language.is_some_and(|language| {
                            language
                                .editing
                                .brackets
                                .as_ref()
                                .is_some_and(|brackets| !brackets.is_empty())
                        }),
                        folds: language.is_some_and(|language| !language.queries.folds.is_empty()),
                        id,
                    }
                })
                .collect()
        })
    }

    /// The grammars compiled into the product, sorted.
    pub fn grammar_ids() -> Vec<&'static str> {
        crate::app::syntax::grammars::grammar_ids().collect()
    }

    /// Resolve the theme as startup does.
    pub fn apply_startup_theme(&self, cx: &mut TestAppContext) {
        cx.update(crate::app::themes::apply_at_startup);
    }

    /// Whether the current theme is dark.
    pub fn theme_is_dark(&self, cx: &mut TestAppContext) -> bool {
        cx.read(|cx| crate::app::theme::theme(cx).appearance == crate::app::theme::Appearance::Dark)
    }

    /// The current theme's value for the color key `key`.
    pub fn theme_color(&self, key: &str, cx: &mut TestAppContext) -> Hsla {
        cx.read(|cx| crate::app::theme::theme(cx).paint_color(key))
    }

    /// A color in theme notation, such as `#282c34`.
    pub fn parse_color(text: &str) -> Option<Hsla> {
        crate::app::style::Color::parse(text).map(crate::app::convert::color_to_gpui)
    }

    /// The highlights of `text` in the language detected for a file named
    /// `name`, as byte ranges and highlight names, or `None` when no
    /// language claims it.
    pub fn highlights(
        &self,
        name: &str,
        text: &str,
        cx: &mut TestAppContext,
    ) -> Option<Vec<(Range<usize>, String)>> {
        use crate::app::syntax::SyntaxState;
        use knot_core::buffer::TextBuffer;
        cx.read(|cx| {
            let registry = crate::app::languages::registry(cx);
            let first_line = text.lines().next().unwrap_or_default();
            let id = registry.detect(Some(name), first_line)?;
            let mut state = SyntaxState::new(registry.resolve(&id).unwrap(), registry.resolver());
            let buffer = TextBuffer::from_text(text);
            assert!(state.parse_in_foreground(&buffer.snapshot(), Duration::from_secs(60)));
            Some(
                state
                    .highlights(&buffer.snapshot(), 0..buffer.len())
                    .into_iter()
                    .map(|(range, highlight)| (range, highlight.name().to_owned()))
                    .collect(),
            )
        })
    }

    /// Each package's heap once every package is between turns, as Show
    /// Extension Memory reports it.
    pub fn memory_report(&self, cx: &mut TestAppContext) -> String {
        let pool = cx.read(crate::app::extensions::host::pool).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while pool.heap_usage().iter().any(|(_, usage)| usage.is_none()) {
            assert!(std::time::Instant::now() < deadline, "packages never idled");
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(10));
        }
        cx.read(crate::app::extensions::host::memory_report)
    }

    /// Restart the package `name`, and require that it loaded again.
    pub fn restart_package(&self, name: &str, cx: &mut TestAppContext) {
        let restarted = Arc::new(Mutex::new(None));
        let result = restarted.clone();
        cx.update(|cx| {
            let host = crate::app::extensions::host::entity(cx).unwrap();
            host.update(cx, |host, cx| {
                let task = host.restart_package(name.into(), cx);
                cx.spawn(async move |_, _| *result.lock().unwrap() = Some(task.await))
                    .detach();
            })
        });
        crate::test_support::wait_until(cx, |_| restarted.lock().unwrap().is_some());
        let entry = restarted.lock().unwrap().take().unwrap();
        assert_eq!(entry.result, LoadResult::Loaded, "{name}");
    }

    /// A windowless session over `text` in `language`, or detected, with the
    /// selection from `anchor` to `caret`, settled.
    pub fn editor(
        &self,
        text: &str,
        language: Option<&str>,
        selection: (usize, usize),
        cx: &mut TestAppContext,
    ) -> TestEditor {
        TestEditor::open(text, language, selection, cx)
    }

    /// A product window over `text` with the caret at its start, for pointer
    /// input, typed input, prompts, and the work a key costs a window.
    pub fn windowed_editor(&self, text: &str, cx: &mut TestAppContext) -> TestEditor {
        TestEditor::windowed(text, cx)
    }

    /// Serve `files`, as names and texts, from the memory directory
    /// `directory` under `mem://product/`, such as `mem://product/dir`, to
    /// every file-system user.
    pub fn serve_files(
        &self,
        directory: &str,
        files: &[(&str, &str)],
        cx: &mut TestAppContext,
    ) -> ServedFiles {
        let root = ResourceUri::parse("mem://product/").unwrap();
        let provider = Arc::new(MemoryFileSystemProvider::new(root).unwrap());
        let mut filesystems = FileSystemProviderRegistry::default();
        filesystems.register("mem", provider.clone()).unwrap();
        provider
            .seed_directory(ResourceUri::parse(directory).unwrap())
            .unwrap();
        for (name, text) in files {
            let uri = ResourceUri::parse(&format!("{directory}/{name}")).unwrap();
            provider.seed_file(uri, text.as_bytes()).unwrap();
        }
        cx.set_global(ApplicationFileSystems(Arc::new(filesystems)));
        ServedFiles(provider)
    }

    /// A windowless session over the served file at `uri`, caret at its
    /// start.
    pub fn open_file(&self, files: &ServedFiles, uri: &str, cx: &mut TestAppContext) -> TestEditor {
        let uri = ResourceUri::parse(uri).unwrap();
        let opened = pollster::block_on(files.0.read(uri.clone())).unwrap();
        let name = uri.to_string().rsplit('/').next().unwrap().to_owned();
        let model = cx.new(|_| BufferModel::from_text(String::from_utf8(opened.bytes).unwrap()));
        let documents = cx.read(|cx| cx.global::<ApplicationDocuments>().0.clone());
        let document = documents.update(cx, |documents, cx| {
            documents.create_persisted(name.as_str(), model.clone(), uri, 0, opened.version, cx)
        });
        TestEditor::over(document, model, (0, 0), cx)
    }

    /// The value of the setting `key` for a view of kind `view`, such as
    /// `editor`, in no particular document or language.
    pub fn setting(&self, key: &str, view: Option<&str>, cx: &mut TestAppContext) -> Option<Value> {
        let scope = SettingsScope {
            view: view.map(str::to_owned),
            ..Default::default()
        };
        cx.read(|cx| {
            cx.global::<ApplicationSettings>()
                .0
                .get(key, &scope)
                .cloned()
        })
    }
}
