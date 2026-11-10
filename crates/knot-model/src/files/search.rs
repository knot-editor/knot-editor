//! Ranked matching of file paths against a query.
//!
//! The index and the disk walk offer every visible file to one [`Search`], so
//! both rank alike, across any number of searched directories. A file whose
//! name matches ranks above one that matches only through its directories;
//! within a tier the matcher's order applies, then the order the directories
//! were begun in, then the path order.

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
    /// The searched directory the file was found in.
    pub directory: ResourceUri,
    /// The path below `directory`, with `/` separators.
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
    /// The index of the searched directory, in the order they were begun.
    directory: usize,
    path: String,
}

impl Ord for Rank {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.tier,
            self.style,
            other.score,
            self.directory,
            &self.path,
        )
            .cmp(&(
                other.tier,
                other.style,
                self.score,
                other.directory,
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
pub struct Search<'a> {
    query: &'a Query,
    directories: Vec<ResourceUri>,
    matcher: Matcher,
    limit: usize,
    /// The worst kept candidate is on top.
    kept: BinaryHeap<Candidate>,
    matched: usize,
}

impl<'a> Search<'a> {
    pub fn new(query: &'a Query, limit: usize) -> Self {
        Self {
            query,
            directories: Vec::new(),
            matcher: Matcher::new(),
            limit,
            kept: BinaryHeap::new(),
            matched: 0,
        }
    }

    /// Starts searching `directory`: the files offered next are below it.
    pub(super) fn begin(&mut self, directory: ResourceUri) {
        self.directories.push(directory);
    }

    /// Considers the file at `path` below the directory last begun, whose name
    /// starts at byte `name_start`.
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
        let directory = self.directories.len() - 1;
        if self.kept.len() == self.limit {
            let worst = self.kept.peek().map(|worst| &worst.rank);
            let beats = |worst: &Rank| {
                (tier, found.style, worst.score, directory, path).cmp(&(
                    worst.tier,
                    worst.style,
                    found.score,
                    worst.directory,
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
                directory,
                path: path.to_string(),
            },
            ranges,
        });
    }

    /// The kept files, best first.
    pub fn finish(self) -> SearchResults {
        let directories = self.directories;
        let more = self.matched > self.kept.len();
        let hits = self
            .kept
            .into_sorted_vec()
            .into_iter()
            .map(|candidate| {
                let path = candidate.rank.path;
                let name = path.rsplit('/').next().unwrap_or_default().to_string();
                let directory = &directories[candidate.rank.directory];
                let uri = path
                    .split('/')
                    .fold(directory.clone(), |uri, name| child_uri(&uri, name));
                SearchHit {
                    uri,
                    directory: directory.clone(),
                    path,
                    name,
                    ranges: candidate.ranges,
                }
            })
            .collect();
        SearchResults { hits, more }
    }
}
