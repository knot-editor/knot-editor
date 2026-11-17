//! What typing costs the foreground on the large benchmark documents, in the
//! real editor with the bundled languages. Run with
//! `cargo test --release -p knot-ext-languages --test languages typing_costs -- --ignored --nocapture`.

use std::time::Duration;

use gpui::TestAppContext;
use knot::test_support::{KeyCost, TestProduct, large_documents};

/// Keys typed at each spot and pace.
const KEYS: usize = 60;

fn percentiles(mut samples: Vec<Duration>) -> String {
    samples.sort();
    let at = |p: usize| samples[(samples.len() - 1) * p / 100];
    format!(
        "p50 {:>9.3?} p99 {:>9.3?} max {:>9.3?}",
        at(50),
        at(99),
        at(100)
    )
}

#[gpui::test]
#[ignore]
fn typing_costs_on_large_documents(cx: &mut TestAppContext) {
    let product = TestProduct::start(super::bundled::PACKAGES, cx);
    for document in large_documents::all() {
        let editor = product.windowed_editor(&document.text, cx);
        if let Some(language) = document.language {
            editor.set_language(language, cx);
        }
        println!(
            "{} {} lines",
            document.name(),
            document.text.lines().count()
        );
        // The spots where keys were typed, each once per key.
        let mut typed = Vec::new();
        for &(label, spot) in &document.spots {
            for (pace, settle_every) in [("slow", 1), ("fast", 2)] {
                let offset = spot + typed.iter().filter(|&&earlier| earlier <= spot).count();
                editor.select(offset, offset, cx);
                let costs: Vec<KeyCost> = editor.key_costs(&["x"; KEYS], settle_every, cx);
                typed.extend([spot; KEYS]);
                println!(
                    "  {label:6} {pace}: key {}  settle {}",
                    percentiles(costs.iter().map(|cost| cost.key).collect()),
                    percentiles(costs.iter().filter_map(|cost| cost.settle).collect()),
                );
            }
        }
    }
}
