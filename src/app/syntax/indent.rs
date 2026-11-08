//! Indentation from a language's `indents` query, with nvim-treesitter's
//! indent captures and the semantics of its indent module, so its queries
//! adapt with light edits.
//!
//! A line's indentation comes from the node at its first non-blank byte, or
//! for a blank line from the last node of the previous non-blank line, and
//! from that node's ancestors: `@indent.begin` nodes spanning several lines
//! indent the lines after their first, `@indent.branch` and `@indent.dedent`
//! take a level back, `@indent.align` aligns to an open delimiter, or indents
//! one level when the delimiter ends its line, and `@indent.zero` puts a line
//! at column 0. A row contributes at most one level however many nodes start
//! on it. Lines inside an `@indent.ignore` node, such as a multi-line string,
//! keep their indentation, because it is content; lines inside an
//! `@indent.auto` node, such as a block comment, move with the node's first
//! line, so their alignment under it survives.
//!
//! Inside a parse error, such as a block whose closing brace is not typed
//! yet, each opening bracket of the language's bracket pairs that the error
//! leaves unclosed before the line indents it one level, as the block it
//! starts would, so typing a block before closing it indents its lines.
//!
//! Besides the predicates the query cursor evaluates, patterns may use
//! `has-parent?`, `not-has-parent?`, `kind-eq?`, and `not-kind-eq?`, which
//! compare node kinds and cannot be written as patterns.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use tree_sitter::{Language, Node, Point, Query, QueryCursor, QueryMatch, QueryPredicateArg, Tree};

use super::{StreamingIterator, SyntaxLanguageError, SyntaxQuery, clamp};
use crate::app::languages::editing::Bracket;
use knot_core::buffer::TextSnapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Capture {
    Auto,
    Begin,
    End,
    Dedent,
    Branch,
    Ignore,
    Align,
    Zero,
}

impl Capture {
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "indent.auto" => Self::Auto,
            "indent.begin" => Self::Begin,
            "indent.end" => Self::End,
            "indent.dedent" => Self::Dedent,
            "indent.branch" => Self::Branch,
            "indent.ignore" => Self::Ignore,
            "indent.align" => Self::Align,
            "indent.zero" => Self::Zero,
            _ => return None,
        })
    }
}

/// A pattern's `#set!` properties.
#[derive(Clone, Debug, Default)]
struct Properties {
    /// A `@indent.begin` node indents the next line even when it ends on
    /// the line it starts on, as an unclosed delimiter does.
    immediate: bool,
    /// A `@indent.begin` node indents the line it starts on too.
    start_at_same_line: bool,
    /// The child of an `@indent.align` node that lines align after.
    open_delimiter: Option<Box<str>>,
    /// The child of an `@indent.align` node that closes it.
    close_delimiter: Option<Box<str>>,
    /// Indent the line holding the close delimiter one more level when it
    /// would otherwise line up with the block that follows, as a function
    /// signature's parameters do.
    avoid_last_matching_next: bool,
    /// Columns after the open delimiter that aligned lines start at.
    increment: Option<usize>,
    /// An `@indent.dedent` applies only to lines after the node ends, as
    /// after Python's `return`, whose own continuation lines keep their
    /// level. Knot's addition to nvim's properties.
    after_end: bool,
}

/// A node predicate the query cursor does not evaluate.
struct NodePredicate {
    capture: u32,
    kinds: Vec<Box<str>>,
    /// Compare the parent's kind rather than the node's.
    parent: bool,
    negated: bool,
}

impl NodePredicate {
    fn holds(&self, found: &QueryMatch) -> bool {
        found
            .captures()
            .iter()
            .filter(|capture| capture.index == self.capture)
            .all(|capture| {
                let node = if self.parent {
                    capture.node.parent()
                } else {
                    Some(capture.node)
                };
                let matches =
                    node.is_some_and(|node| self.kinds.iter().any(|kind| &**kind == node.kind()));
                matches != self.negated
            })
    }
}

/// A compiled `indents` query.
pub(crate) struct IndentQuery {
    query: Query,
    /// Indexed by capture index; `None` for names that are not indent
    /// captures, such as `_`-prefixed helpers.
    captures: Vec<Option<Capture>>,
    /// Indexed by pattern index.
    properties: Vec<Properties>,
    /// Indexed by pattern index.
    predicates: Vec<Vec<NodePredicate>>,
}

impl IndentQuery {
    /// Compile `source` against `grammar`; `None` when it has no patterns.
    pub(super) fn new(
        grammar: &Language,
        source: &str,
    ) -> Result<Option<Self>, SyntaxLanguageError> {
        let query =
            Query::new(grammar, source).map_err(|error| SyntaxLanguageError::InvalidQuery {
                query: SyntaxQuery::Indents,
                message: error.message,
                row: error.row,
                column: error.column,
            })?;
        if query.pattern_count() == 0 {
            return Ok(None);
        }
        let captures = query
            .capture_names()
            .iter()
            .map(|name| Capture::named(name))
            .collect();
        let properties = (0..query.pattern_count())
            .map(|pattern| {
                let mut properties = Properties::default();
                for property in query.property_settings(pattern) {
                    let value = property.value.as_deref();
                    match &*property.key {
                        "indent.immediate" => properties.immediate = true,
                        "indent.start_at_same_line" => properties.start_at_same_line = true,
                        "indent.open_delimiter" => {
                            properties.open_delimiter = value.map(Into::into)
                        }
                        "indent.close_delimiter" => {
                            properties.close_delimiter = value.map(Into::into)
                        }
                        "indent.avoid_last_matching_next" => {
                            properties.avoid_last_matching_next = true
                        }
                        "indent.after_end" => properties.after_end = true,
                        "indent.increment" => {
                            properties.increment = value.and_then(|value| value.parse().ok())
                        }
                        _ => {}
                    }
                }
                properties
            })
            .collect();
        let predicates = (0..query.pattern_count())
            .map(|pattern| {
                query
                    .general_predicates(pattern)
                    .iter()
                    .map(|predicate| {
                        let (parent, negated) = match &*predicate.operator {
                            "has-parent?" => (true, false),
                            "not-has-parent?" => (true, true),
                            "kind-eq?" => (false, false),
                            "not-kind-eq?" => (false, true),
                            operator => {
                                return Err(unsupported(
                                    &query,
                                    source,
                                    pattern,
                                    format!("unsupported predicate #{operator}"),
                                ));
                            }
                        };
                        let mut arguments = predicate.args.iter();
                        let Some(QueryPredicateArg::Capture(capture)) = arguments.next() else {
                            return Err(unsupported(
                                &query,
                                source,
                                pattern,
                                format!("#{} takes a capture first", predicate.operator),
                            ));
                        };
                        let kinds = arguments
                            .map(|argument| match argument {
                                QueryPredicateArg::String(kind) => Ok(kind.clone()),
                                QueryPredicateArg::Capture(_) => Err(unsupported(
                                    &query,
                                    source,
                                    pattern,
                                    format!("#{} takes node kinds", predicate.operator),
                                )),
                            })
                            .collect::<Result<_, _>>()?;
                        Ok(NodePredicate {
                            capture: *capture,
                            kinds,
                            parent,
                            negated,
                        })
                    })
                    .collect::<Result<_, _>>()
            })
            .collect::<Result<_, _>>()?;
        Ok(Some(Self {
            query,
            captures,
            properties,
            predicates,
        }))
    }
}

/// A compile error at the start of `pattern`.
fn unsupported(
    query: &Query,
    source: &str,
    pattern: usize,
    message: String,
) -> SyntaxLanguageError {
    let before = &source[..query.start_byte_for_pattern(pattern)];
    let row = before.matches('\n').count();
    let column = before.len() - before.rfind('\n').map_or(0, |newline| newline + 1);
    SyntaxLanguageError::InvalidQuery {
        query: SyntaxQuery::Indents,
        message,
        row,
        column,
    }
}

/// How a line indents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineIndent {
    /// Indent to this many columns.
    Columns(usize),
    /// Leave the indentation as it is, because it is content, as inside a
    /// multi-line string.
    Keep,
    /// Move with the line at this row, where the node holding this line
    /// starts, as a block comment's lines move with its first.
    Follow(usize),
}

/// The lines of the text a tree parsed.
pub(crate) trait LineSource {
    /// `row`'s text, without its line terminator.
    fn line(&self, row: usize) -> String;
    /// The byte offset where `row` starts.
    fn line_start(&self, row: usize) -> usize;
}

/// The lines being indented: the source's text, with the indentation each
/// row has been given so far.
pub(super) struct Lines<'a> {
    source: &'a dyn LineSource,
    /// Columns per indentation level and per tab stop.
    size: usize,
    /// Rows whose indentation changed, with their new columns.
    changed: HashMap<usize, usize>,
}

impl<'a> Lines<'a> {
    pub(super) fn new(source: &'a dyn LineSource, size: usize) -> Self {
        Self {
            source,
            size: size.max(1),
            changed: HashMap::new(),
        }
    }

    pub(super) fn line(&self, row: usize) -> String {
        self.source.line(row)
    }

    pub(super) fn line_start(&self, row: usize) -> usize {
        self.source.line_start(row)
    }

    /// The columns `row` is indented by now.
    pub(super) fn columns(&self, row: usize) -> usize {
        self.changed.get(&row).copied().unwrap_or_else(|| {
            let line = self.source.line(row);
            width(&line[..leading_whitespace(&line)], 0, self.size)
        })
    }

    /// The column of byte `col` on `row`, after any change to the row's
    /// indentation.
    fn column_at(&self, row: usize, col: usize) -> usize {
        let line = self.source.line(row);
        let indent = leading_whitespace(&line);
        let start = self.columns(row);
        match line.get(indent.min(col)..col) {
            Some(text) => start + width(text, start, self.size),
            None => start,
        }
    }
}

/// The byte length of `line`'s leading spaces and tabs.
pub(super) fn leading_whitespace(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// The columns `text` spans from column `start`, with tabs advancing to
/// the next multiple of `size`.
fn width(text: &str, start: usize, size: usize) -> usize {
    text.chars().fold(start, |column, character| {
        if character == '\t' {
            column + size - column % size
        } else {
            column + 1
        }
    }) - start
}

/// The indent captures of the nodes a query finds, by node identity, each
/// with its pattern and where its match ends.
struct Captures<'q> {
    query: &'q IndentQuery,
    nodes: HashMap<usize, Vec<(Capture, usize, Point)>>,
}

impl<'q> Captures<'q> {
    /// The captures of the matches intersecting `rows` of `root`'s tree.
    fn find(query: &'q IndentQuery, root: Node, text: &TextSnapshot, rows: Range<usize>) -> Self {
        let mut captures = Self {
            query,
            nodes: HashMap::new(),
        };
        captures.extend(
            root,
            text,
            Point::new(rows.start, 0)..Point::new(rows.end, 0),
        );
        captures
    }

    fn extend(&mut self, root: Node, text: &TextSnapshot, points: Range<Point>) {
        let mut cursor = QueryCursor::new();
        cursor.set_point_range(points);
        let node_text = |node: Node| {
            text.chunks(clamp(node.byte_range(), text))
                .map(str::as_bytes)
        };
        let mut matches = cursor.matches(&self.query.query, root, node_text);
        while let Some(found) = matches.next() {
            let pattern = found.pattern_index;
            if !self.query.predicates[pattern]
                .iter()
                .all(|predicate| predicate.holds(found))
            {
                continue;
            }
            let end = found
                .captures()
                .iter()
                .map(|capture| capture.node.end_position())
                .max()
                .unwrap_or_default();
            for capture in found.captures() {
                if let Some(kind) = self.query.captures[capture.index as usize] {
                    self.nodes
                        .entry(capture.node.id())
                        .or_default()
                        .push((kind, pattern, end));
                }
            }
        }
    }

    /// The properties of the last pattern capturing `node` as `kind`.
    fn get(&self, node: Node, kind: Capture) -> Option<&'q Properties> {
        let query = self.query;
        self.nodes.get(&node.id()).and_then(|captures| {
            captures
                .iter()
                .rev()
                .find(|(captured, ..)| *captured == kind)
                .map(|(_, pattern, _)| &query.properties[*pattern])
        })
    }

    /// Whether a match capturing `node` as `kind` ends at or after `point`.
    fn reaches(&self, node: Node, kind: Capture, point: Point) -> bool {
        self.nodes.get(&node.id()).is_some_and(|captures| {
            captures
                .iter()
                .any(|(captured, _, end)| *captured == kind && *end >= point)
        })
    }

    fn has(&self, node: Node, kind: Capture) -> bool {
        self.get(node, kind).is_some()
    }
}

/// The `@indent.align` properties of `node`, captured or promoted from a
/// child.
fn aligned<'q>(
    captures: &Captures<'q>,
    promoted: &HashMap<usize, &'q Properties>,
    node: Node,
) -> Option<&'q Properties> {
    captures
        .get(node, Capture::Align)
        .or_else(|| promoted.get(&node.id()).copied())
}

/// The smallest node of `root`'s tree covering the byte at `col` on `row`.
fn node_at(root: Node, row: usize, col: usize) -> Node {
    root.descendant_for_point_range(Point::new(row, col), Point::new(row, col + 1))
        .unwrap_or(root)
}

/// The child of `node` whose kind is `delimiter`, and whether nothing but
/// whitespace and delimiter characters follows it on its line.
fn find_delimiter<'t>(node: Node<'t>, delimiter: &str, lines: &Lines) -> Option<(Node<'t>, bool)> {
    let mut walk = node.walk();
    let child = node
        .children(&mut walk)
        .find(|child| child.kind() == delimiter)?;
    let line = lines.source.line(child.start_position().row);
    let rest = line.get(child.end_position().column..).unwrap_or("");
    let last = rest
        .chars()
        .all(|character| character.is_whitespace() || delimiter.contains(character));
    Some((child, last))
}

/// The rows of the opening brackets among `error`'s children that no later
/// child before `end` closes, in order and without repeats.
fn unclosed_rows(error: Node, brackets: &[Bracket], end: Point) -> Vec<usize> {
    let mut open: Vec<(&str, usize)> = Vec::new();
    let mut walk = error.walk();
    for child in error.children(&mut walk) {
        if child.start_position() > end {
            break;
        }
        let kind = child.kind();
        if let Some(bracket) = brackets
            .iter()
            .find(|bracket| bracket.open != bracket.close && bracket.open == kind)
        {
            open.push((&bracket.close, child.start_position().row));
        } else if open.last().is_some_and(|(close, _)| *close == kind) {
            open.pop();
        }
    }
    let mut rows: Vec<usize> = open.into_iter().map(|(_, row)| row).collect();
    rows.dedup();
    rows
}

/// How `row` indents in `tree`, whose `query` is its language's and
/// `brackets` its bracket pairs. An `injected` tree indents from the line
/// where it starts.
pub(super) fn evaluate(
    tree: &Tree,
    query: &IndentQuery,
    brackets: &[Bracket],
    text: &TextSnapshot,
    lines: &Lines,
    row: usize,
    injected: bool,
) -> LineIndent {
    let size = lines.size as isize;
    let root = tree.root_node();
    let line = lines.source.line(row);
    let first = leading_whitespace(&line);
    let previous = (first == line.len())
        .then(|| {
            (0..row)
                .rev()
                .find(|&row| !lines.source.line(row).trim().is_empty())
        })
        .flatten();
    let mut captures = Captures::find(query, root, text, previous.unwrap_or(row)..row + 1);

    let mut node = match previous {
        Some(previous) => {
            // The last node of the previous non-blank line, ignoring a
            // trailing comment and trailing whitespace.
            let text = lines.source.line(previous);
            let indent = leading_whitespace(&text);
            let end = text.trim_end().len();
            let mut node = node_at(root, previous, end - 1);
            if node.kind().contains("comment") && node.start_position().row == previous {
                let start = node.start_position().column;
                if node_at(root, previous, indent).id() != node.id() {
                    let before = text[..start.min(text.len())].trim_end().len();
                    node = node_at(root, previous, before.saturating_sub(1));
                }
            }
            if captures.has(node, Capture::End) {
                node_at(root, row, first)
            } else {
                node
            }
        }
        None => node_at(root, row, first),
    };

    if captures.has(node, Capture::Zero) {
        return LineIndent::Columns(0);
    }
    let mut indent = if injected {
        lines.columns(root.start_position().row) as isize
    } else {
        0
    };
    // Rows a node has already indented or dedented the line for.
    let mut processed_rows = HashSet::new();
    // `@indent.align` captures moved from a child to its ERROR parent.
    let mut promoted: HashMap<usize, &Properties> = HashMap::new();

    loop {
        let start = node.start_position().row;
        let end = node.end_position().row;
        let inside = start < row && row <= end;
        let begin = captures.get(node, Capture::Begin);
        if begin.is_none()
            && aligned(&captures, &promoted, node).is_none()
            && captures.has(node, Capture::Auto)
            && inside
        {
            return LineIndent::Follow(start);
        }
        if begin.is_none() && captures.has(node, Capture::Ignore) && inside {
            return LineIndent::Keep;
        }

        let mut processed = false;
        if !processed_rows.contains(&start)
            && ((captures.has(node, Capture::Branch) && start == row)
                || (captures
                    .get(node, Capture::Dedent)
                    .is_some_and(|dedent| !dedent.after_end || end < row)
                    && start != row))
        {
            indent -= size;
            processed = true;
        }

        let unprocessed = !processed_rows.contains(&start);
        let in_error = unprocessed && node.parent().is_some_and(|parent| parent.has_error());
        if let Some(begin) = begin
            && unprocessed
            && (start != end || in_error || begin.immediate)
            && (start != row || begin.start_at_same_line)
        {
            indent += size;
            processed = true;
        }

        // An ERROR node takes the alignment of its first aligned child, so
        // `(ERROR "(" @indent.align …)` can name the delimiter it aligns to.
        if in_error && aligned(&captures, &promoted, node).is_none() {
            let mut walk = node.walk();
            for child in node.children(&mut walk) {
                captures.extend(root, text, child.start_position()..child.end_position());
                if let Some(properties) = captures.get(child, Capture::Align) {
                    promoted.insert(node.id(), properties);
                    break;
                }
            }
        }

        if let Some(properties) = aligned(&captures, &promoted, node)
            && unprocessed
            && (start != end || in_error)
            && start != row
        {
            let delimiter = |name: &Option<Box<str>>| match name {
                Some(name) => find_delimiter(node, name, lines),
                None => Some((node, false)),
            };
            if let Some((open, open_last)) = delimiter(&properties.open_delimiter) {
                let close = delimiter(&properties.close_delimiter);
                let open_row = open.start_position().row;
                let close_row = close.map(|(close, _)| close.start_position().row);
                let close_last = close.is_some_and(|(_, last)| last);
                let mut absolute = false;
                if open_last {
                    // A hanging indent, the delimiter ending its line.
                    indent += size;
                    if close_last && close_row.is_some_and(|close| close < row) {
                        indent = (indent - size).max(0);
                    }
                } else if close_last
                    && close_row.is_some_and(|close| close != open_row && close < row)
                {
                    indent = (indent - size).max(0);
                } else {
                    indent = (lines.column_at(open_row, open.start_position().column)
                        + properties.increment.unwrap_or(1)) as isize;
                    absolute = true;
                }
                if properties.avoid_last_matching_next
                    && close_row.is_some_and(|close| close != open_row && close == row)
                    && indent <= lines.columns(open_row) as isize + size
                {
                    indent += size;
                }
                processed = true;
                if absolute {
                    return LineIndent::Columns(indent.max(0) as usize);
                }
            }
        }

        if node.is_error() && !processed {
            for open in unclosed_rows(node, brackets, Point::new(row, first)) {
                if open < row && processed_rows.insert(open) {
                    indent += size;
                }
            }
        }

        if processed {
            processed_rows.insert(start);
        }
        match node.parent() {
            Some(parent) => node = parent,
            None => break,
        }
    }
    LineIndent::Columns(indent.max(0) as usize)
}

/// Whether typing up to byte `caret` of `row` completed a node that starts
/// at the row's first non-blank byte and closes or branches an indented
/// node: an `@indent.end`, `@indent.branch`, or `@indent.dedent` capture
/// whose match reaches the caret, such as a typed `}`, or Python's `else`
/// once a pattern capturing the `:` after it as `@_end` matches.
pub(super) fn closes(
    tree: &Tree,
    query: &IndentQuery,
    text: &TextSnapshot,
    lines: &Lines,
    row: usize,
    caret: usize,
) -> bool {
    let line = lines.source.line(row);
    let first = leading_whitespace(&line);
    if first >= caret {
        return false;
    }
    let root = tree.root_node();
    let captures = Captures::find(query, root, text, row..row + 1);
    let start = Point::new(row, first);
    let mut node = node_at(root, row, first);
    while node.start_position() == start {
        if [Capture::End, Capture::Branch, Capture::Dedent]
            .into_iter()
            .any(|kind| captures.reaches(node, kind, Point::new(row, caret)))
        {
            return true;
        }
        match node.parent() {
            Some(parent) => node = parent,
            None => break,
        }
    }
    false
}

/// The rows of `rows` whose indentation changes, with their new columns,
/// each row seeing the indentation given to the rows before it. `indent`
/// evaluates a row, or returns `None` when its layer has no indent query.
/// Blank rows change only when they are the only row.
pub(super) fn reindent(
    lines: &mut Lines,
    rows: Range<usize>,
    mut indent: impl FnMut(&Lines, usize) -> Option<LineIndent>,
) -> Vec<(usize, usize)> {
    let several = rows.len() > 1;
    let mut changes = Vec::new();
    for row in rows {
        let line = lines.source.line(row);
        if several && line.trim().is_empty() {
            continue;
        }
        let current = lines.columns(row);
        let columns = match indent(lines, row) {
            None | Some(LineIndent::Keep) => continue,
            Some(LineIndent::Columns(columns)) => columns,
            Some(LineIndent::Follow(start)) => {
                let Some(&moved) = lines.changed.get(&start) else {
                    continue;
                };
                let before = {
                    let text = lines.source.line(start);
                    width(&text[..leading_whitespace(&text)], 0, lines.size)
                };
                (current as isize + moved as isize - before as isize).max(0) as usize
            }
        };
        if columns != current {
            lines.changed.insert(row, columns);
            changes.push((row, columns));
        }
    }
    changes
}

#[cfg(test)]
mod tests;
