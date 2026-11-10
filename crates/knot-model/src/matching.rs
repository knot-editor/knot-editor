//! Candidate matching shared by the picker, input completion, and extensions.
//!
//! A [`MatchStyle`] decides what counts as a match. The user's ordered list of
//! styles (the `completion.styles` setting) applies to every consumer: a
//! candidate matches when any listed style matches it, and candidates order by
//! the first style that matches, then by that style's score, then by the
//! caller's order.

use std::{cmp::Reverse, ops::Range};

use nucleo_matcher::{
    Config, Matcher as Nucleo, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use unicode_segmentation::UnicodeSegmentation;

/// What counts as a match.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchStyle {
    /// The candidate starts with the query.
    Prefix,
    /// The candidate contains the query.
    Substring,
    /// The query's characters appear in order, with gaps; space-separated
    /// parts of the query each match this way.
    Flex,
    /// Every space-separated part of the query is a substring of the
    /// candidate, in any order.
    Orderless,
}

/// The styles used when a list names none that exist.
pub const DEFAULT_STYLES: [MatchStyle; 2] = [MatchStyle::Prefix, MatchStyle::Flex];

impl MatchStyle {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "prefix" => Self::Prefix,
            "substring" => Self::Substring,
            "flex" => Self::Flex,
            "orderless" => Self::Orderless,
            _ => return None,
        })
    }
}

/// The styles a list of names selects. Unknown names are ignored, and a list
/// with no known name selects [`DEFAULT_STYLES`].
pub fn styles_from_names<S: AsRef<str>>(names: &[S]) -> Vec<MatchStyle> {
    let styles: Vec<_> = names
        .iter()
        .filter_map(|name| MatchStyle::parse(name.as_ref()))
        .collect();
    if styles.is_empty() {
        DEFAULT_STYLES.to_vec()
    } else {
        styles
    }
}

/// A query prepared once for many candidates.
pub struct Query {
    text: String,
    styles: Vec<MatchStyle>,
    /// Smart case: a query with an uppercase letter matches case-sensitively.
    fold: bool,
    components: Vec<String>,
    pattern: Pattern,
}

impl Query {
    pub fn new(text: &str, styles: &[MatchStyle]) -> Self {
        Self {
            text: text.to_string(),
            styles: styles.to_vec(),
            fold: !text.chars().any(char::is_uppercase),
            components: text.split_whitespace().map(str::to_string).collect(),
            pattern: Pattern::parse(text, CaseMatching::Smart, Normalization::Smart),
        }
    }

    /// Whether the query holds only whitespace, and so matches everything.
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }
}

/// How one text matched.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Found {
    /// The index in the query's styles of the style that matched.
    pub style: usize,
    /// Higher is better within a style.
    pub score: u32,
    /// The matched byte ranges, sorted and disjoint.
    pub ranges: Vec<Range<usize>>,
}

/// Matches texts against a [`Query`], reusing its scratch space.
pub struct Matcher {
    nucleo: Nucleo,
    buffer: Vec<char>,
    indices: Vec<u32>,
}

impl Default for Matcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Matcher {
    pub fn new() -> Self {
        Self {
            nucleo: Nucleo::new(Config::DEFAULT),
            buffer: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// How `text` matches by the first style that matches it.
    pub fn find(&mut self, query: &Query, text: &str) -> Option<Found> {
        if query.is_empty() {
            return Some(Found {
                style: 0,
                score: 0,
                ranges: Vec::new(),
            });
        }
        query.styles.iter().enumerate().find_map(|(style, kind)| {
            let (score, ranges) = self.find_with(*kind, query, text)?;
            Some(Found {
                style,
                score,
                ranges,
            })
        })
    }

    fn find_with(
        &mut self,
        style: MatchStyle,
        query: &Query,
        text: &str,
    ) -> Option<(u32, Vec<Range<usize>>)> {
        match style {
            MatchStyle::Prefix => {
                let range = find_text(text, &query.text, query.fold, true)?;
                // An exact match outranks a longer candidate with the prefix.
                Some((u32::from(range.end == text.len()), vec![range]))
            }
            MatchStyle::Substring => {
                let range = find_text(text, &query.text, query.fold, false)?;
                Some((u32::MAX - range.start as u32, vec![range]))
            }
            MatchStyle::Orderless => {
                let mut ranges = query
                    .components
                    .iter()
                    .map(|part| find_text(text, part, query.fold, false))
                    .collect::<Option<Vec<_>>>()?;
                ranges.sort_by_key(|range| range.start);
                Some((0, merge(ranges)))
            }
            MatchStyle::Flex => {
                self.indices.clear();
                let score = query.pattern.indices(
                    Utf32Str::new(text, &mut self.buffer),
                    &mut self.nucleo,
                    &mut self.indices,
                )?;
                Some((score, grapheme_ranges(text, &mut self.indices)))
            }
        }
    }

    /// The candidates that match, best first, as indices into `candidates`.
    pub fn rank<S: AsRef<str>>(&mut self, query: &Query, candidates: &[S]) -> Vec<(usize, Found)> {
        let mut ranked: Vec<_> = candidates
            .iter()
            .enumerate()
            .filter_map(|(index, text)| Some((index, self.find(query, text.as_ref())?)))
            .collect();
        // A stable sort keeps the caller's order among equal scores.
        ranked.sort_by_key(|(_, found)| (found.style, Reverse(found.score)));
        ranked
    }
}

/// The first occurrence of `needle` in `haystack` (at its start if
/// `at_start`), comparing characters case-insensitively when `fold`.
fn find_text(haystack: &str, needle: &str, fold: bool, at_start: bool) -> Option<Range<usize>> {
    let same = |a: char, b: char| a == b || (fold && a.to_lowercase().eq(b.to_lowercase()));
    for (start, _) in haystack.char_indices() {
        let mut rest = haystack[start..].char_indices();
        let mut end = start;
        let matched = needle.chars().all(|wanted| match rest.next() {
            Some((offset, found)) if same(found, wanted) => {
                end = start + offset + found.len_utf8();
                true
            }
            _ => false,
        });
        if matched {
            return Some(start..end);
        }
        if at_start {
            break;
        }
    }
    None
}

/// Merge sorted ranges that overlap or touch.
fn merge(ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

/// Convert grapheme indices from the matcher into merged byte ranges.
fn grapheme_ranges(text: &str, indices: &mut Vec<u32>) -> Vec<Range<usize>> {
    indices.sort_unstable();
    indices.dedup();
    let mut wanted = indices.iter().copied().peekable();
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for (index, (start, grapheme)) in text.grapheme_indices(true).enumerate() {
        if wanted.peek() != Some(&(index as u32)) {
            continue;
        }
        wanted.next();
        let end = start + grapheme.len();
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    ranges
}

/// Byte ranges of `text` as UTF-16 code unit ranges, the units of JavaScript
/// strings.
pub fn utf16_ranges(text: &str, ranges: &[Range<usize>]) -> Vec<[usize; 2]> {
    let units = |byte: usize| text[..byte].encode_utf16().count();
    ranges
        .iter()
        .map(|range| [units(range.start), units(range.end)])
        .collect()
}

/// UTF-16 code unit ranges of `text` as byte ranges, clamped to the text and
/// to character boundaries.
pub fn byte_ranges(text: &str, ranges: &[[usize; 2]]) -> Vec<Range<usize>> {
    let byte = |units: usize| {
        let mut seen = 0;
        for (index, character) in text.char_indices() {
            if seen >= units {
                return index;
            }
            seen += character.len_utf16();
        }
        text.len()
    };
    ranges
        .iter()
        .map(|[start, end]| byte(*start)..byte(*end))
        .filter(|range| range.start < range.end)
        .collect()
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    fn matches(query: &str, styles: &[&str], candidates: &[&str]) -> Vec<usize> {
        let query = Query::new(query, &styles_from_names(styles));
        Matcher::new()
            .rank(&query, candidates)
            .into_iter()
            .map(|(index, _)| index)
            .collect()
    }

    #[test]
    fn prefix_requires_the_start() {
        let words = ["write", "rewrite", "wq"];
        assert_eq!(matches("w", &["prefix"], &words), [0, 2]);
        assert_eq!(matches("rite", &["prefix"], &words), Vec::<usize>::new());
    }

    #[test]
    fn an_exact_match_outranks_a_longer_prefix_match() {
        let words = ["setup", "set", "settings"];
        assert_eq!(matches("set", &["prefix"], &words), [1, 0, 2]);
    }

    #[test]
    fn substring_prefers_earlier_matches() {
        let words = ["xxab", "abxx", "xab"];
        assert_eq!(matches("ab", &["substring"], &words), [1, 2, 0]);
    }

    #[test]
    fn flex_matches_a_subsequence() {
        let words = ["expandtab", "number", "exit"];
        assert_eq!(matches("eta", &["flex"], &words), [0]);
        assert_eq!(matches("xt", &["flex"], &words), [2, 0]);
    }

    #[test]
    fn orderless_takes_the_parts_in_any_order() {
        let words = ["go to line", "line", "toggle"];
        assert_eq!(matches("line go", &["orderless"], &words), [0]);
        assert_eq!(matches("to go", &["orderless"], &words), [0]);
    }

    #[test]
    fn smart_case_is_sensitive_only_for_an_uppercase_query() {
        let words = ["Alpha", "alpha"];
        assert_eq!(matches("alp", &["prefix"], &words), [0, 1]);
        assert_eq!(matches("Alp", &["prefix"], &words), [0]);
    }

    #[test]
    fn earlier_styles_rank_first_and_later_styles_add_matches() {
        let words = ["xset", "set", "sxt"];
        assert_eq!(matches("set", &["prefix", "flex"], &words), [1, 0]);
        assert_eq!(matches("set", &["prefix"], &words), [1]);
    }

    #[test]
    fn an_empty_query_keeps_the_callers_order_without_ranges() {
        let query = Query::new("  ", &DEFAULT_STYLES);
        let ranked = Matcher::new().rank(&query, &["b", "a"]);
        assert_eq!(
            ranked
                .iter()
                .map(|(index, found)| (*index, found.ranges.len()))
                .collect::<Vec<_>>(),
            [(0, 0), (1, 0)]
        );
    }

    #[test]
    fn unknown_style_names_are_ignored_and_none_selects_the_default() {
        assert_eq!(styles_from_names(&["nope", "prefix"]), [MatchStyle::Prefix]);
        assert_eq!(styles_from_names(&["nope"]), DEFAULT_STYLES);
        assert_eq!(styles_from_names::<&str>(&[]), DEFAULT_STYLES);
    }

    #[test]
    fn ranges_are_byte_ranges_on_multibyte_text() {
        let query = Query::new("é", &[MatchStyle::Substring]);
        let found = Matcher::new().find(&query, "caféine").unwrap();
        assert_eq!(found.ranges, [3..5]);
        let query = Query::new("ÉI", &[MatchStyle::Substring]);
        assert!(Matcher::new().find(&query, "caféine").is_none());
        let query = Query::new("éi", &[MatchStyle::Substring]);
        assert_eq!(
            Matcher::new().find(&query, "caféine").unwrap().ranges,
            [3..6]
        );
        let query = Query::new("fn", &[MatchStyle::Flex]);
        assert_eq!(
            Matcher::new().find(&query, "f👩‍🚀n").unwrap().ranges,
            [0..1, 12..13]
        );
    }

    #[test]
    fn orderless_ranges_merge() {
        let query = Query::new("ab bc", &[MatchStyle::Orderless]);
        assert_eq!(Matcher::new().find(&query, "abc").unwrap().ranges, [0..3]);
    }

    #[test]
    fn javascript_ranges_convert_both_ways() {
        let text = "a😀bc";
        assert_eq!(utf16_ranges(text, &[5..6]), [[3, 4]]);
        assert_eq!(byte_ranges(text, &[[3, 4]]), [5..6]);
        assert_eq!(byte_ranges(text, &[[0, 99]]), [0..text.len()]);
    }
}
