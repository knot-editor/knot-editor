//! Ranked matching of file paths against a query.
//!
//! The index and the disk walk offer every visible file to one [`Ranker`], so
//! both rank alike. A file whose name matches ranks above one that matches only
//! through its directories; within a tier the matcher's order applies, then the
//! path order.

use std::{cmp::Ordering, collections::BinaryHeap, ops::Range};

use crate::{
    filesystem::child_uri,
    matching::{Matcher, Query},
    resource::ResourceUri,
};

/// A file that matched.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    pub uri: ResourceUri,
    /// The path below the searched directory, with `/` separators.
    pub path: String,
    /// The last segment of `path`.
    pub name: String,
    /// The matched byte ranges of `path`, sorted and disjoint.
    pub ranges: Vec<Range<usize>>,
}

/// The best matches, and whether others matched too.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub more: bool,
}

/// Orders better matches first.
#[derive(Eq, PartialEq)]
struct Rank {
    /// 0 when the name matched, 1 when only the path did.
    tier: u8,
    style: usize,
    score: u32,
    path: String,
}

impl Ord for Rank {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.tier, self.style, other.score, &self.path).cmp(&(
            other.tier,
            other.style,
            self.score,
            &other.path,
        ))
    }
}

impl PartialOrd for Rank {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Eq, PartialEq)]
struct Candidate {
    rank: Rank,
    ranges: Vec<Range<usize>>,
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank.cmp(&other.rank)
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Keeps the best `limit` of the files offered to it.
pub(super) struct Ranker<'a> {
    query: &'a Query,
    matcher: Matcher,
    limit: usize,
    /// The worst kept candidate is on top.
    kept: BinaryHeap<Candidate>,
    matched: usize,
}

impl<'a> Ranker<'a> {
    pub(super) fn new(query: &'a Query, limit: usize) -> Self {
        Self {
            query,
            matcher: Matcher::new(),
            limit,
            kept: BinaryHeap::new(),
            matched: 0,
        }
    }

    /// Considers the file at `path`, whose name starts at byte `name_start`.
    pub(super) fn offer(&mut self, path: &str, name_start: usize) {
        let name = &path[name_start..];
        let (tier, found) = match self.matcher.find(self.query, name) {
            Some(found) => (0, found),
            None => match self.matcher.find(self.query, path) {
                Some(found) => (1, found),
                None => return,
            },
        };
        self.matched += 1;
        if self.kept.len() == self.limit {
            let worst = self.kept.peek().map(|worst| &worst.rank);
            let beats = |worst: &Rank| {
                (tier, found.style, worst.score, path).cmp(&(
                    worst.tier,
                    worst.style,
                    found.score,
                    worst.path.as_str(),
                )) == Ordering::Less
            };
            if !worst.is_some_and(beats) {
                return;
            }
            self.kept.pop();
        }
        // Ranges of a name match are relative to the name.
        let ranges = if tier == 0 {
            found
                .ranges
                .iter()
                .map(|range| range.start + name_start..range.end + name_start)
                .collect()
        } else {
            found.ranges
        };
        self.kept.push(Candidate {
            rank: Rank {
                tier,
                style: found.style,
                score: found.score,
                path: path.to_string(),
            },
            ranges,
        });
    }

    /// The kept files, best first, with URIs below `directory`.
    pub(super) fn finish(self, directory: &ResourceUri) -> SearchResults {
        let more = self.matched > self.kept.len();
        let hits = self
            .kept
            .into_sorted_vec()
            .into_iter()
            .map(|candidate| {
                let path = candidate.rank.path;
                let name = path.rsplit('/').next().unwrap_or_default().to_string();
                let uri = path
                    .split('/')
                    .fold(directory.clone(), |uri, name| child_uri(&uri, name));
                SearchHit {
                    uri,
                    path,
                    name,
                    ranges: candidate.ranges,
                }
            })
            .collect();
        SearchResults { hits, more }
    }
}
