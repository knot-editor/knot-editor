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
    pub fn fold_regions(&self) -> Arc<[Range<usize>]> {
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
        let mut regions = match queried {
            Some(mut regions) => {
                for region in &mut regions {
                    self.trim_trailing_blank_lines(region);
                }
                regions
            }
            None => {
                let text = self.buffer.read_range(0..self.buffer.len());
                let lines: Vec<&str> = text.split('\n').collect();
                indentation_regions(&lines)
            }
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

    /// The first lines of the regions of [`Self::fold_regions`] that start
    /// among `lines`, in order. The work grows with `lines`, not with the
    /// text.
    pub fn fold_starts(&self, lines: Range<usize>) -> Vec<usize> {
        let lines = lines.start..lines.end.min(self.line_count());
        let queried = self
            .syntax
            .as_ref()
            .filter(|syntax| syntax.has_fold_query())
            .map(|syntax| {
                let start = self.byte_of_line(lines.start);
                let end = self.byte_of_line(lines.end + 1);
                syntax.fold_regions_starting_in(&self.buffer.snapshot(), lines.clone(), start..end)
            });
        let mut starts: Vec<usize> = match queried {
            Some(regions) => regions
                .into_iter()
                .filter_map(|mut region| {
                    self.trim_trailing_blank_lines(&mut region);
                    (region.len() >= 2).then_some(region.start)
                })
                .collect(),
            // A line starts an indentation region when the next non-blank
            // line is indented more.
            None => lines
                .filter(|&row| {
                    let line = self.line_text(row);
                    !line.trim().is_empty()
                        && (row + 1..self.line_count())
                            .map(|next| self.line_text(next))
                            .find(|next| !next.trim().is_empty())
                            .is_some_and(|next| indent_width(&next) > indent_width(&line))
                })
                .collect(),
        };
        starts.sort_unstable();
        starts.dedup();
        starts
    }

    /// Where `line` starts, or the end of the text past the last line.
    fn byte_of_line(&self, line: usize) -> usize {
        if line < self.line_count() {
            self.buffer.line_start(line)
        } else {
            self.buffer.len()
        }
    }

    /// Leave the trailing blank lines of `region` visible.
    fn trim_trailing_blank_lines(&self, region: &mut Range<usize>) {
        while region.end > region.start + 1
            && region.end <= self.line_count()
            && self.line_text(region.end - 1).trim().is_empty()
        {
            region.end -= 1;
        }
    }

    fn line_text(&self, line: usize) -> String {
        self.buffer.read_range(self.line_range(line))
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
