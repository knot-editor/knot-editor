//! Fold regions: the line ranges a view may collapse.

use std::ops::Range;
use std::sync::Arc;

use super::BufferModel;

/// Columns a tab counts for when comparing indentation.
const TAB_WIDTH: usize = 4;

/// The regions computed for one text and syntax state.
pub(super) struct FoldRegions {
    revision: u64,
    syntax: Option<u64>,
    regions: Arc<[Range<usize>]>,
}

impl BufferModel {
    /// The regions of the text, as line ranges from the first line to one
    /// past the last, ordered by first line and then outermost first. They
    /// come from the language's `folds` query, or from indentation for a
    /// language without one.
    pub(crate) fn fold_regions(&self) -> Arc<[Range<usize>]> {
        let revision = self.revision();
        let syntax = self.syntax_generation();
        if let Some(cached) = &*self.fold_regions.borrow()
            && cached.revision == revision
            && cached.syntax == syntax
        {
            return Arc::clone(&cached.regions);
        }
        let queried = self
            .syntax
            .as_ref()
            .filter(|syntax| syntax.has_fold_query())
            .map(|syntax| syntax.fold_regions(&self.buffer.snapshot()));
        let text = self.buffer.read_range(0..self.buffer.len());
        let lines: Vec<&str> = text.split('\n').collect();
        let mut regions = match queried {
            Some(mut regions) => {
                for region in &mut regions {
                    // Trailing blank lines stay visible.
                    while region.end > region.start + 1
                        && lines
                            .get(region.end - 1)
                            .is_some_and(|l| l.trim().is_empty())
                    {
                        region.end -= 1;
                    }
                }
                regions
            }
            None => indentation_regions(&lines),
        };
        regions.retain(|region| region.len() >= 2);
        regions.sort_by_key(|region| (region.start, std::cmp::Reverse(region.end)));
        regions.dedup_by_key(|region| region.start);
        let regions: Arc<[Range<usize>]> = regions.into();
        *self.fold_regions.borrow_mut() = Some(FoldRegions {
            revision,
            syntax,
            regions: Arc::clone(&regions),
        });
        regions
    }
}

fn indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { TAB_WIDTH } else { 1 })
        .sum()
}

/// A line followed by more-indented lines starts a region, which runs
/// through the last non-blank line before the next line indented no more.
fn indentation_regions(lines: &[&str]) -> Vec<Range<usize>> {
    let mut regions = Vec::new();
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut last = 0;
    for (row, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let width = indent_width(line);
        while let Some(&(indent, start)) = open.last() {
            if indent < width {
                break;
            }
            regions.push(start..last + 1);
            open.pop();
        }
        open.push((width, row));
        last = row;
    }
    regions.extend(open.into_iter().map(|(_, start)| start..last + 1));
    regions
}
