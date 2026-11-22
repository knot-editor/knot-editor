//! Immutable source input for one extension lifecycle.

use std::collections::BTreeMap;
use std::sync::Arc;

use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigPhase {
    PreInit,
    PostInit,
}

impl ConfigPhase {
    pub const fn entry(self) -> &'static str {
        match self {
            Self::PreInit => "pre-init.js",
            Self::PostInit => "post-init.js",
        }
    }
}

/// A package's modules within a graph, as URLs: where its root is, its main
/// module, and the module defining its contracts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageModules {
    pub package: Arc<str>,
    pub root: Arc<str>,
    pub main: Arc<str>,
    pub contract: Option<Arc<str>>,
}

/// A package whose contract module a graph may import, with its sources keyed
/// by URL.
pub type PackageDependency = (PackageModules, Vec<(String, Arc<str>)>);

#[derive(Clone, Debug)]
pub struct ModuleGraph {
    root: Arc<str>,
    entry: Arc<str>,
    entries: Arc<BTreeMap<String, Arc<str>>>,
    sources: Arc<BTreeMap<String, Arc<str>>>,
    package: Option<PackageModules>,
    /// The required packages whose contract modules this graph may import.
    dependencies: Arc<Vec<PackageModules>>,
}

impl ModuleGraph {
    /// `root` is a directory file URL; file names and `entry` are relative to it.
    pub fn new(root: &str, entry: &str, files: BTreeMap<String, Arc<str>>) -> Result<Self, String> {
        Self::with_entries(root, &[entry], files)
    }

    pub fn with_entries(
        root: &str,
        entries: &[&str],
        files: BTreeMap<String, Arc<str>>,
    ) -> Result<Self, String> {
        if entries.is_empty() {
            return Err("module graph needs an entry".into());
        }
        let root_url = directory_url(root)?;
        let mut sources = BTreeMap::new();
        for (path, source) in files {
            validate_path(&path)?;
            let url = file_url(&root_url, &path)?;
            if !url.as_str().starts_with(root_url.as_str()) {
                return Err(format!("module path escapes package: {path}"));
            }
            if sources.insert(url.to_string(), source).is_some() {
                return Err(format!("duplicate module URL for {path}"));
            }
        }
        let mut entry_urls = BTreeMap::<String, Arc<str>>::new();
        for entry in entries {
            validate_path(entry)?;
            let url = file_url(&root_url, entry)?.to_string();
            if !sources.contains_key(&url) {
                return Err(format!("entry module is unavailable: {url}"));
            }
            if entry_urls
                .insert((*entry).to_owned(), Arc::from(url))
                .is_some()
            {
                return Err(format!("duplicate module entry: {entry}"));
            }
        }
        let entry = entry_urls[entries[0]].clone();
        Ok(Self {
            root: root_url.to_string().into(),
            entry,
            entries: Arc::new(entry_urls),
            sources: Arc::new(sources),
            package: None,
            dependencies: Arc::default(),
        })
    }

    /// Names the package the graph belongs to, which owns the contracts its
    /// modules define, and the module, relative to the root, that defines the
    /// ones it publishes.
    pub fn owned_by(mut self, package: &str, contract: Option<&str>) -> Result<Self, String> {
        let contract = contract.map(|path| self.module_url(path)).transpose()?;
        self.package = Some(PackageModules {
            package: package.into(),
            root: self.root.clone(),
            main: self.entry.clone(),
            contract,
        });
        Ok(self)
    }

    /// Adds a required package's sources under its own `root`, so this graph
    /// can import the contract module at `contract`, relative to that root.
    pub fn with_dependency(
        mut self,
        package: &str,
        root: &str,
        main: &str,
        contract: &str,
        files: &BTreeMap<String, Arc<str>>,
    ) -> Result<Self, String> {
        let (modules, sources) =
            Self::dependency(&self.root, package, root, main, contract, files)?;
        Arc::make_mut(&mut self.sources).extend(sources);
        Arc::make_mut(&mut self.dependencies).push(modules);
        Ok(self)
    }

    /// The modules and sources of a package a graph rooted at `own_root` may
    /// import the contract of.
    pub fn dependency(
        own_root: &str,
        package: &str,
        root: &str,
        main: &str,
        contract: &str,
        files: &BTreeMap<String, Arc<str>>,
    ) -> Result<PackageDependency, String> {
        let root_url = directory_url(root)?;
        if own_root.starts_with(root_url.as_str()) || root_url.as_str().starts_with(own_root) {
            return Err(format!("dependency {package} shares the package root"));
        }
        let mut sources = Vec::new();
        for (path, source) in files {
            validate_path(path)?;
            sources.push((file_url(&root_url, path)?.to_string(), source.clone()));
        }
        let url = |path: &str| -> Result<Arc<str>, String> {
            validate_path(path)?;
            Ok(file_url(&root_url, path)?.to_string().into())
        };
        let contract = url(contract)?;
        if !sources.iter().any(|(name, _)| **name == *contract) {
            return Err(format!(
                "contract module of {package} is unavailable: {contract}"
            ));
        }
        let modules = PackageModules {
            package: package.into(),
            root: root_url.to_string().into(),
            main: url(main)?,
            contract: Some(contract),
        };
        Ok((modules, sources))
    }

    fn module_url(&self, path: &str) -> Result<Arc<str>, String> {
        validate_path(path)?;
        let url = file_url(&directory_url(&self.root)?, path)?.to_string();
        if !self.sources.contains_key(&url) {
            return Err(format!("contract module is unavailable: {url}"));
        }
        Ok(url.into())
    }

    pub fn package(&self) -> Option<&PackageModules> {
        self.package.as_ref()
    }

    pub fn dependencies(&self) -> &[PackageModules] {
        &self.dependencies
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn entry(&self) -> &str {
        &self.entry
    }

    pub fn entry_url(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(AsRef::as_ref)
    }

    pub fn entries(&self) -> &BTreeMap<String, Arc<str>> {
        &self.entries
    }

    pub fn sources(&self) -> &BTreeMap<String, Arc<str>> {
        &self.sources
    }
}

fn directory_url(root: &str) -> Result<Url, String> {
    let url = Url::parse(root).map_err(|error| format!("invalid package URL: {error}"))?;
    if url.scheme() != "file"
        || !url.as_str().ends_with('/')
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("package root must be a directory file URL".into());
    }
    Ok(url)
}

fn validate_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path.starts_with('/')
    {
        return Err(format!("invalid package module path: {path}"));
    }
    Ok(())
}

fn file_url(root: &Url, path: &str) -> Result<Url, String> {
    let mut url = root.clone();
    url.path_segments_mut()
        .map_err(|_| format!("invalid package module path: {path}"))?
        .pop_if_empty()
        .extend(path.split('/'));
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_entry_and_rejects_invalid_paths() {
        let files = BTreeMap::from([("dist/main.js".into(), Arc::from("export {};"))]);
        let graph = ModuleGraph::new(
            "file:///extensions/%40example/tools/",
            "dist/main.js",
            files.clone(),
        )
        .unwrap();
        assert_eq!(
            graph.entry(),
            "file:///extensions/%40example/tools/dist/main.js"
        );
        assert_eq!(graph.sources().len(), 1);
        assert!(
            ModuleGraph::new("file:///extensions/tools/", "../main.js", files.clone()).is_err()
        );
        assert!(
            ModuleGraph::new("file:///extensions/tools/", "missing.js", files.clone()).is_err()
        );
        assert!(
            ModuleGraph::new("https://example.com/tools/", "dist/main.js", files.clone()).is_err()
        );
        assert!(
            ModuleGraph::new(
                "file:///extensions/tools/",
                "dist/main.js",
                BTreeMap::from([
                    ("../outside.js".into(), Arc::from("")),
                    ("dist/main.js".into(), Arc::from("")),
                ])
            )
            .is_err()
        );
    }

    #[test]
    fn encodes_file_name_punctuation_without_changing_package_root() {
        let graph = ModuleGraph::new(
            "file:///extensions/%40example/tools/",
            "main #1.js",
            BTreeMap::from([("main #1.js".into(), Arc::from("export {};"))]),
        )
        .unwrap();
        assert_eq!(
            graph.entry(),
            "file:///extensions/%40example/tools/main%20%231.js"
        );
    }

    #[test]
    fn multiple_entries_must_exist_and_keep_distinct_urls() {
        let files = BTreeMap::from([
            ("pre-init.js".into(), Arc::from("export {};")),
            ("post-init.js".into(), Arc::from("export {};")),
        ]);
        let graph = ModuleGraph::with_entries(
            "file:///config/knot/",
            &["pre-init.js", "post-init.js"],
            files.clone(),
        )
        .unwrap();
        assert_eq!(graph.entry(), "file:///config/knot/pre-init.js");
        assert_eq!(
            graph.entry_url("post-init.js"),
            Some("file:///config/knot/post-init.js")
        );
        assert!(ModuleGraph::with_entries("file:///config/knot/", &[], files.clone()).is_err());
        assert!(
            ModuleGraph::with_entries(
                "file:///config/knot/",
                &["pre-init.js", "pre-init.js"],
                files.clone()
            )
            .is_err()
        );
        assert!(ModuleGraph::with_entries("file:///config/knot/", &["missing.js"], files).is_err());
    }

    #[test]
    fn dependencies_add_their_sources_under_their_own_root() {
        let own = BTreeMap::from([
            ("main.js".into(), Arc::from("")),
            ("contract.js".into(), Arc::from("")),
        ]);
        let dependency = BTreeMap::from([
            ("dist/main.js".into(), Arc::from("")),
            ("dist/contract.js".into(), Arc::from("")),
        ]);
        let graph = ModuleGraph::new("file:///extensions/consumer/", "main.js", own.clone())
            .unwrap()
            .owned_by("@example/consumer", Some("contract.js"))
            .unwrap()
            .with_dependency(
                "@example/provider",
                "file:///extensions/provider/",
                "dist/main.js",
                "dist/contract.js",
                &dependency,
            )
            .unwrap();
        let package = graph.package().unwrap();
        assert_eq!(&*package.main, "file:///extensions/consumer/main.js");
        assert_eq!(
            package.contract.as_deref(),
            Some("file:///extensions/consumer/contract.js")
        );
        let provider = &graph.dependencies()[0];
        assert_eq!(&*provider.main, "file:///extensions/provider/dist/main.js");
        assert!(
            graph
                .sources()
                .contains_key("file:///extensions/provider/dist/contract.js")
        );
        assert_eq!(graph.sources().len(), 4);

        let base = || ModuleGraph::new("file:///extensions/consumer/", "main.js", own.clone());
        assert!(
            base()
                .unwrap()
                .owned_by("@example/consumer", Some("missing.js"))
                .is_err()
        );
        assert!(
            base()
                .unwrap()
                .with_dependency(
                    "@example/provider",
                    "file:///extensions/provider/",
                    "dist/main.js",
                    "dist/missing.js",
                    &dependency,
                )
                .is_err()
        );
        assert!(
            base()
                .unwrap()
                .with_dependency(
                    "@example/inner",
                    "file:///extensions/consumer/inner/",
                    "dist/main.js",
                    "dist/contract.js",
                    &dependency,
                )
                .is_err()
        );
    }
}
