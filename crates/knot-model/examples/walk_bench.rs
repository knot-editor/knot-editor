//! Times `files::walk` over a local directory with the default ignore policy.
//!
//! Usage: `cargo run --release -p knot-model --example walk_bench -- <directory> [runs]`

use std::{sync::Arc, time::Instant};

use knot_model::{
    files::{IgnorePolicy, walk},
    filesystem::{FileSystemProviderRegistry, LocalFileSystemProvider},
};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let path = arguments
        .next()
        .expect("usage: walk_bench <directory> [runs]");
    let runs: usize = arguments
        .next()
        .map_or(5, |runs| runs.parse().expect("runs must be a number"));
    let path = std::fs::canonicalize(path).expect("the directory must exist");
    let uri = url::Url::from_directory_path(&path)
        .expect("an absolute directory path")
        .to_string();
    let mut providers = FileSystemProviderRegistry::default();
    providers
        .register("file", Arc::new(LocalFileSystemProvider::unrestricted()))
        .expect("register the local provider");
    let providers = Arc::new(providers);
    let policy = IgnorePolicy::default();
    for run in 1..=runs {
        let start = Instant::now();
        let files = pollster::block_on(walk(providers.clone(), &uri, &policy)).expect("walk");
        println!(
            "run {run}: {:.3} s, {} files",
            start.elapsed().as_secs_f64(),
            files.len()
        );
    }
}
