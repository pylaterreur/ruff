use std::fmt::Debug;
use std::iter::Peekable;

use ruff_formatter::{SourceCode, SourceCodeSlice};
use ruff_python_ast::helpers::comment_indentation_after;
use ruff_python_ast::{AnyNodeRef, Identifier};
use ruff_python_ast::{Mod, Stmt};
// The interface is designed to only export the members relevant for iterating nodes in
// pre-order.
#[allow(clippy::wildcard_imports)]
use ruff_python_ast::visitor::source_order::*;
use ruff_python_trivia::{
    CommentLinePosition, CommentRanges, SimpleToken, SimpleTokenKind, SimpleTokenizer,
    TriviaRanges, indentation_at_offset,
};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::comments::node_key::NodeRefEqualityKey;
use crate::comments::placement::place_comment;
use crate::comments::{CommentsMap, SourceComment};

/// Collect the preceding, following and enclosing node for each comment without applying
/// [`place_comment`] for debugging.
pub(crate) fn collect_comments<'a>(
    root: &'a Mod,
    source_code: SourceCode<'a>,
    comment_ranges: &'a CommentRanges,
) -> Vec<DecoratedComment<'a>> {
    let mut collector = CommentsVecBuilder::default();
    CommentsVisitor::new(source_code, comment_ranges, &mut collector).visit(AnyNodeRef::from(root));
    collector.comments
}

/// Visitor extracting the comments from an AST.
pub(super) struct CommentsVisitor<'a, 'builder> {
    builder: &'builder mut (dyn PushComment<'a> + 'a),
    source_code: SourceCode<'a>,
    parents: Vec<AnyNodeRef<'a>>,
    preceding_node: Option<AnyNodeRef<'a>>,
    comment_ranges: Peekable<std::slice::Iter<'a, TextRange>>,
    /// The trivia surrounding the last visited comment.
    trivia: SurroundingTrivia,
}

impl<'a, 'builder> CommentsVisitor<'a, 'builder> {
    pub(super) fn new(
        source_code: SourceCode<'a>,
        comment_ranges: &'a CommentRanges,
        builder: &'builder mut (dyn PushComment<'a> + 'a),
    ) -> Self {
        Self {
            builder,
            source_code,
            parents: Vec::new(),
            preceding_node: None,
            comment_ranges: comment_ranges.iter().peekable(),
            trivia: SurroundingTrivia::default(),
        }
    }

    pub(super) fn visit(mut self, root: AnyNodeRef<'a>) {
        if self.enter_node(root).is_traverse() {
            root.visit_source_order(&mut self);
        }

        self.leave_node(root);
    }

    // Try to skip the subtree if
    // * there are no comments
    // * if the next comment comes after this node (meaning, this nodes subtree contains no comments)
    fn can_skip(&mut self, node_end: TextSize) -> bool {
        self.comment_ranges
            .peek()
            .is_none_or(|next_comment| next_comment.start() >= node_end)
    }

    /// Returns the trivia surrounding the comment at `comment_range`.
    ///
    /// Comments are visited in source order, which allows lexing the trivia only once for all
    /// comments between the same two tokens.
    fn surrounding_trivia(&mut self, comment_range: TextRange) -> SurroundingTrivia {
        let source = self.source_code.as_str();

        if comment_range.start() >= self.trivia.range.end() {
            self.trivia = SurroundingTrivia::from_first_comment(comment_range, source);
        }

        if let Some(indentation) = indentation_at_offset(comment_range.start(), source) {
            let indentation = indentation.text_len();
            self.trivia.min_indentation = Some(
                self.trivia
                    .min_indentation
                    .map_or(indentation, |min| min.min(indentation)),
            );
        }

        self.trivia
    }
}

impl<'ast> SourceOrderVisitor<'ast> for CommentsVisitor<'ast, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'ast>) -> TraversalSignal {
        let node_range = node.range();

        let enclosing_node = self.parents.last().copied().unwrap_or(node);

        // Process all remaining comments that end before this node's start position.
        // If the `preceding` node is set, then it process all comments ending after the `preceding` node
        // and ending before this node's start position
        while let Some(comment_range) = self.comment_ranges.peek().copied() {
            // Exit if the comment is enclosed by this node or comes after it
            if comment_range.end() > node_range.start() {
                break;
            }

            let comment = DecoratedComment {
                enclosing: enclosing_node,
                preceding: self.preceding_node,
                following: Some(node),
                parent: self.parents.iter().rev().nth(1).copied(),
                line_position: CommentLinePosition::for_range(
                    *comment_range,
                    self.source_code.as_str(),
                ),
                slice: self.source_code.slice(*comment_range),
                trivia: self.surrounding_trivia(*comment_range),
            };

            self.builder.push_comment(comment);
            self.comment_ranges.next();
        }

        // From here on, we're inside of `node`, meaning, we're passed the preceding node.
        self.preceding_node = None;
        self.parents.push(node);

        if self.can_skip(node_range.end()) {
            TraversalSignal::Skip
        } else {
            TraversalSignal::Traverse
        }
    }

    fn leave_node(&mut self, node: AnyNodeRef<'ast>) {
        // We are leaving this node, pop it from the parent stack.
        self.parents.pop();

        let node_end = node.end();
        let is_root = self.parents.is_empty();

        // Process all comments that start after the `preceding` node and end before this node's end.
        while let Some(comment_range) = self.comment_ranges.peek().copied() {
            // If the comment starts after this node, break.
            // If this is the root node and there are comments after the node, attach them to the root node
            // anyway because there's no other node we can attach the comments to (RustPython should include the comments in the node's range)
            if comment_range.start() >= node_end && !is_root {
                break;
            }

            let comment = DecoratedComment {
                enclosing: node,
                parent: self.parents.last().copied(),
                preceding: self.preceding_node,
                following: None,
                line_position: CommentLinePosition::for_range(
                    *comment_range,
                    self.source_code.as_str(),
                ),
                slice: self.source_code.slice(*comment_range),
                trivia: self.surrounding_trivia(*comment_range),
            };

            self.builder.push_comment(comment);

            self.comment_ranges.next();
        }

        self.preceding_node = Some(node);
    }

    fn visit_body(&mut self, body: &'ast [Stmt]) {
        match body {
            [] => {
                // no-op
            }
            [only] => self.visit_stmt(only),
            [first, .., last] => {
                if self.can_skip(last.end()) {
                    // Skip traversing the body when there's no comment between the first and last statement.
                    // It is still necessary to visit the first statement to process all comments between
                    // the previous node and the first statement.
                    self.visit_stmt(first);
                    self.preceding_node = Some(last.into());
                } else {
                    walk_body(self, body);
                }
            }
        }
    }

    fn visit_identifier(&mut self, _identifier: &'ast Identifier) {
        // TODO: Visit and associate comments with identifiers
    }
}

/// A comment decorated with additional information about its surrounding context in the source document.
///
/// Used by [`place_comment`] to determine if this should become a [leading](self#leading-comments),
/// [dangling](self#dangling-comments), or [trailing](self#trailing-comments) comment.
#[derive(Debug, Clone)]
pub(crate) struct DecoratedComment<'a> {
    enclosing: AnyNodeRef<'a>,
    preceding: Option<AnyNodeRef<'a>>,
    following: Option<AnyNodeRef<'a>>,
    parent: Option<AnyNodeRef<'a>>,
    line_position: CommentLinePosition,
    slice: SourceCodeSlice,
    trivia: SurroundingTrivia,
}

impl<'a> DecoratedComment<'a> {
    /// The closest parent node that fully encloses the comment.
    ///
    /// A node encloses a comment when the comment is between two of its direct children (ignoring lists).
    ///
    /// # Examples
    ///
    /// ```python
    /// [
    ///     a,
    ///     # comment
    ///      b
    /// ]
    /// ```
    ///
    /// The enclosing node is the list expression and not the name `b` because
    /// `a` and `b` are children of the list expression and `comment` is between the two nodes.
    pub(crate) fn enclosing_node(&self) -> AnyNodeRef<'a> {
        self.enclosing
    }

    /// Returns the parent of the enclosing node, if any
    pub(super) fn enclosing_parent(&self) -> Option<AnyNodeRef<'a>> {
        self.parent
    }

    /// Returns the comment's preceding node.
    ///
    /// The direct child node (ignoring lists) of the [`enclosing_node`](DecoratedComment::enclosing_node) that precedes this comment.
    ///
    /// Returns [None] if the [`enclosing_node`](DecoratedComment::enclosing_node) only consists of tokens or if
    /// all preceding children of the [`enclosing_node`](DecoratedComment::enclosing_node) have been tokens.
    ///
    /// The Preceding node is guaranteed to be a sibling of [`following_node`](DecoratedComment::following_node).
    ///
    /// # Examples
    ///
    /// ## Preceding tokens only
    ///
    /// ```python
    /// [
    ///     # comment
    /// ]
    /// ```
    /// Returns [None] because the comment has no preceding node, only a preceding `[` token.
    ///
    /// ## Preceding node
    ///
    /// ```python
    /// a # comment
    /// b
    /// ```
    ///
    /// Returns `Some(a)` because `a` directly precedes the comment.
    ///
    /// ## Preceding token and node
    ///
    /// ```python
    /// [
    ///     a, # comment
    ///     b
    /// ]
    /// ```
    ///
    ///  Returns `Some(a)` because `a` is the preceding node of `comment`. The presence of the `,` token
    /// doesn't change that.
    pub(crate) fn preceding_node(&self) -> Option<AnyNodeRef<'a>> {
        self.preceding
    }

    /// Returns the node following the comment.
    ///
    /// The direct child node (ignoring lists) of the [`enclosing_node`](DecoratedComment::enclosing_node) that follows this comment.
    ///
    /// Returns [None] if the [`enclosing_node`](DecoratedComment::enclosing_node) only consists of tokens or if
    /// all children children of the [`enclosing_node`](DecoratedComment::enclosing_node) following this comment are tokens.
    ///
    /// The following node is guaranteed to be a sibling of [`preceding_node`](DecoratedComment::preceding_node).
    ///
    /// # Examples
    ///
    /// ## Following tokens only
    ///
    /// ```python
    /// [
    ///     # comment
    /// ]
    /// ```
    ///
    /// Returns [None] because there's no node following the comment, only the `]` token.
    ///
    /// ## Following node
    ///
    /// ```python
    /// [ # comment
    ///     a
    /// ]
    /// ```
    ///
    /// Returns `Some(a)` because `a` is the node directly following the comment.
    ///
    /// ## Following token and node
    ///
    /// ```python
    /// [
    ///     a # comment
    ///     , b
    /// ]
    /// ```
    ///
    /// Returns `Some(b)` because the `b` identifier is the first node following `comment`.
    ///
    /// ## Following parenthesized expression
    ///
    /// ```python
    /// (
    ///     a
    ///     # comment
    /// )
    /// b
    /// ```
    ///
    /// Returns `None` because `comment` is enclosed inside the parenthesized expression and it has no children
    /// following `# comment`.
    pub(crate) fn following_node(&self) -> Option<AnyNodeRef<'a>> {
        self.following
    }

    /// The position of the comment in the text.
    pub(super) fn line_position(&self) -> CommentLinePosition {
        self.line_position
    }

    /// Returns the slice into the source code.
    pub(crate) fn slice(&self) -> &SourceCodeSlice {
        &self.slice
    }

    /// Returns the non-trivia tokens in `range`, like
    /// `SimpleTokenizer::new(source, range).skip_trivia()`, but without lexing the trivia
    /// surrounding this comment.
    ///
    /// Prefer this method when `range` overlaps with the comment's surrounding trivia, e.g. when
    /// `range` ends at the comment. Lexing the trivia again for every comment makes placing many
    /// consecutive comments quadratic.
    ///
    /// `range` must not start or end inside a comment.
    pub(super) fn non_trivia_tokens<'s>(
        &self,
        range: TextRange,
        source: &'s str,
    ) -> impl Iterator<Item = SimpleToken> + use<'s> {
        let trivia = self.trivia.range;
        let before = TextRange::new(
            range.start(),
            trivia.start().clamp(range.start(), range.end()),
        );
        let after = TextRange::new(trivia.end().clamp(before.end(), range.end()), range.end());

        SimpleTokenizer::new(source, before)
            .skip_trivia()
            .chain(SimpleTokenizer::new(source, after).skip_trivia())
    }

    /// Returns `true` if there's an empty line between this comment and the next token.
    pub(super) fn has_empty_line_after(&self) -> bool {
        self.trivia
            .last_comment_before_empty_line
            .is_some_and(|comment_end| comment_end >= self.end())
    }

    /// Returns the indentation of the least indented own-line comment between `preceding` and this
    /// comment, including this comment.
    ///
    /// See [`comment_indentation_after`].
    pub(super) fn indentation_after(&self, preceding: AnyNodeRef, source: &str) -> TextSize {
        // If the trivia starts right after `preceding`, then it contains all comments between
        // `preceding` and this comment (and none before `preceding`).
        if self.trivia.range.start() == preceding.end() {
            self.trivia.min_indentation.unwrap_or_default()
        } else {
            comment_indentation_after(preceding, self.range(), source)
        }
    }
}

impl Ranged for DecoratedComment<'_> {
    #[inline]
    fn range(&self) -> TextRange {
        self.slice.range()
    }
}

impl From<DecoratedComment<'_>> for SourceComment {
    fn from(decorated: DecoratedComment) -> Self {
        Self::new(decorated.slice, decorated.line_position)
    }
}

/// The whitespace and comments between the token preceding a comment and the token following it.
///
/// ```python
/// a = 1  # comment 1
///
/// # comment 2
/// # comment 3
/// b = 2
/// ```
///
/// All three comments share the same surrounding trivia, from the end of `1` to the start of `b`.
/// A line continuation ends the trivia like a token, and so does the end of the file.
#[derive(Debug, Clone, Copy, Default)]
struct SurroundingTrivia {
    range: TextRange,
    /// The end of the last comment in `range` that is followed by an empty line.
    last_comment_before_empty_line: Option<TextSize>,
    /// The indentation of the least indented own-line comment in `range`, up to the current
    /// comment.
    min_indentation: Option<TextSize>,
}

impl SurroundingTrivia {
    /// Lexes the trivia surrounding the first comment between two tokens.
    fn from_first_comment(comment_range: TextRange, source: &str) -> Self {
        // Python's whitespace and newline characters are exactly the ASCII whitespace characters.
        let start = source[TextRange::up_to(comment_range.start())]
            .trim_ascii_end()
            .text_len();

        let mut end = source.text_len();
        let mut comment_end = comment_range.end();
        let mut newlines = 0u32;
        let mut last_comment_before_empty_line = None;

        for token in SimpleTokenizer::starts_at(comment_range.end(), source) {
            match token.kind() {
                SimpleTokenKind::Whitespace => {}
                SimpleTokenKind::Newline => {
                    newlines += 1;
                    if newlines == 2 {
                        last_comment_before_empty_line = Some(comment_end);
                    }
                }
                SimpleTokenKind::Comment => {
                    comment_end = token.end();
                    newlines = 0;
                }
                _ => {
                    end = token.start();
                    break;
                }
            }
        }

        Self {
            range: TextRange::new(start, end),
            last_comment_before_empty_line,
            min_indentation: None,
        }
    }
}

#[derive(Debug)]
pub(super) enum CommentPlacement<'a> {
    /// Makes `comment` a [leading comment](self#leading-comments) of `node`.
    Leading {
        node: AnyNodeRef<'a>,
        comment: SourceComment,
    },
    /// Makes `comment` a [trailing comment](self#trailing-comments) of `node`.
    Trailing {
        node: AnyNodeRef<'a>,
        comment: SourceComment,
    },

    /// Makes `comment` a [dangling comment](self#dangling-comments) of `node`.
    Dangling {
        node: AnyNodeRef<'a>,
        comment: SourceComment,
    },

    /// Uses the default heuristic to determine the placement of the comment.
    ///
    /// # End of line comments
    ///
    /// Makes the comment a...
    ///
    /// * [trailing comment] of the [`preceding_node`] if both the [`following_node`] and [`preceding_node`] are not [None]
    ///   and the comment and [`preceding_node`] are only separated by a space (there's no token between the comment and [`preceding_node`]).
    /// * [leading comment] of the [`following_node`] if the [`following_node`] is not [None]
    /// * [trailing comment] of the [`preceding_node`] if the [`preceding_node`] is not [None]
    /// * [dangling comment] of the [`enclosing_node`].
    ///
    /// ## Examples
    /// ### Comment with preceding and following nodes
    ///
    /// ```python
    /// [
    ///     a, # comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a`.
    ///
    /// ### Comment with preceding node only
    ///
    /// ```python
    /// [
    ///     a # comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a`.
    ///
    /// ### Comment with following node only
    ///
    /// ```python
    /// [ # comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b`.
    ///
    /// ### Dangling comment
    ///
    /// ```python
    /// [ # comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [dangling comment] of the enclosing list expression because both the [`preceding_node`] and [`following_node`] are [None].
    ///
    /// # Own line comments
    ///
    /// Makes the comment a...
    ///
    /// * [leading comment] of the [`following_node`] if the [`following_node`] is not [None]
    /// * or a [trailing comment] of the [`preceding_node`] if the [`preceding_node`] is not [None]
    /// * or a [dangling comment] of the [`enclosing_node`].
    ///
    /// ## Examples
    ///
    /// ### Comment with leading and preceding nodes
    ///
    /// ```python
    /// [
    ///     a,
    ///     # comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b`.
    ///
    /// ### Comment with preceding node only
    ///
    /// ```python
    /// [
    ///     a
    ///     # comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a`.
    ///
    /// ### Comment with following node only
    ///
    /// ```python
    /// [
    ///     # comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b`.
    ///
    /// ### Dangling comment
    ///
    /// ```python
    /// [
    ///     # comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [dangling comment] of the list expression because both [`preceding_node`] and [`following_node`] are [None].
    ///
    /// [`preceding_node`]: DecoratedComment::preceding_node
    /// [`following_node`]: DecoratedComment::following_node
    /// [`enclosing_node`]: DecoratedComment::enclosing_node
    /// [trailing comment]: self#trailing-comments
    /// [leading comment]: self#leading-comments
    /// [dangling comment]: self#dangling-comments
    Default(DecoratedComment<'a>),
}

impl<'a> CommentPlacement<'a> {
    /// Makes `comment` a [leading comment](self#leading-comments) of `node`.
    #[inline]
    pub(super) fn leading(node: impl Into<AnyNodeRef<'a>>, comment: DecoratedComment) -> Self {
        Self::Leading {
            node: node.into(),
            comment: comment.into(),
        }
    }

    /// Makes `comment` a [dangling comment](self::dangling-comments) of `node`.
    pub(super) fn dangling(node: impl Into<AnyNodeRef<'a>>, comment: DecoratedComment) -> Self {
        Self::Dangling {
            node: node.into(),
            comment: comment.into(),
        }
    }

    /// Makes `comment` a [trailing comment](self::trailing-comments) of `node`.
    #[inline]
    pub(super) fn trailing(node: impl Into<AnyNodeRef<'a>>, comment: DecoratedComment) -> Self {
        Self::Trailing {
            node: node.into(),
            comment: comment.into(),
        }
    }

    /// Chains the placement with the given function.
    ///
    /// Returns `self` when the placement is non-[`CommentPlacement::Default`]. Otherwise, calls the
    /// function with the comment and returns the result.
    pub(super) fn or_else<F: FnOnce(DecoratedComment<'a>) -> Self>(self, f: F) -> Self {
        match self {
            Self::Default(comment) => f(comment),
            _ => self,
        }
    }
}

pub(super) trait PushComment<'a> {
    fn push_comment(&mut self, placement: DecoratedComment<'a>);
}

/// A storage for the [`CommentsVisitor`] that just pushes the decorated comments to a [`Vec`] for
/// debugging purposes.
#[derive(Debug, Default)]
struct CommentsVecBuilder<'a> {
    comments: Vec<DecoratedComment<'a>>,
}

impl<'a> PushComment<'a> for CommentsVecBuilder<'a> {
    fn push_comment(&mut self, placement: DecoratedComment<'a>) {
        self.comments.push(placement);
    }
}

/// A storage for the [`CommentsVisitor`] that fixes the placement and stores the comments in a
/// [`CommentsMap`].
pub(super) struct CommentsMapBuilder<'a> {
    comments: CommentsMap<'a>,
    /// We need those for backwards lexing
    trivia: &'a TriviaRanges,
    source: &'a str,
}

impl<'a> PushComment<'a> for CommentsMapBuilder<'a> {
    fn push_comment(&mut self, placement: DecoratedComment<'a>) {
        let placement = place_comment(placement, self.trivia, self.source);
        match placement {
            CommentPlacement::Leading { node, comment } => {
                self.push_leading_comment(node, comment);
            }
            CommentPlacement::Trailing { node, comment } => {
                self.push_trailing_comment(node, comment);
            }
            CommentPlacement::Dangling { node, comment } => {
                self.push_dangling_comment(node, comment);
            }
            CommentPlacement::Default(comment) => {
                match comment.line_position() {
                    CommentLinePosition::EndOfLine => {
                        match (comment.preceding_node(), comment.following_node()) {
                            (Some(preceding), Some(_)) => {
                                // Attach comments with both preceding and following node to the preceding
                                // because there's a line break separating it from the following node.
                                // ```python
                                // a; # comment
                                // b
                                // ```
                                self.push_trailing_comment(preceding, comment);
                            }
                            (Some(preceding), None) => {
                                self.push_trailing_comment(preceding, comment);
                            }
                            (None, Some(following)) => {
                                self.push_leading_comment(following, comment);
                            }
                            (None, None) => {
                                self.push_dangling_comment(comment.enclosing_node(), comment);
                            }
                        }
                    }
                    CommentLinePosition::OwnLine => {
                        match (comment.preceding_node(), comment.following_node()) {
                            // Following always wins for a leading comment
                            // ```python
                            // a
                            // // python
                            // b
                            // ```
                            // attach the comment to the `b` expression statement
                            (_, Some(following)) => {
                                self.push_leading_comment(following, comment);
                            }
                            (Some(preceding), None) => {
                                self.push_trailing_comment(preceding, comment);
                            }
                            (None, None) => {
                                self.push_dangling_comment(comment.enclosing_node(), comment);
                            }
                        }
                    }
                }
            }
        }
    }
}

impl<'a> CommentsMapBuilder<'a> {
    pub(crate) fn new(source: &'a str, trivia: &'a TriviaRanges) -> Self {
        Self {
            comments: CommentsMap::default(),
            trivia,
            source,
        }
    }

    pub(crate) fn finish(self) -> CommentsMap<'a> {
        self.comments
    }

    fn push_leading_comment(&mut self, node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) {
        self.comments
            .push_leading(NodeRefEqualityKey::from_ref(node), comment.into());
    }

    fn push_dangling_comment(&mut self, node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) {
        self.comments
            .push_dangling(NodeRefEqualityKey::from_ref(node), comment.into());
    }

    fn push_trailing_comment(&mut self, node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) {
        self.comments
            .push_trailing(NodeRefEqualityKey::from_ref(node), comment.into());
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use ruff_formatter::SourceCode;
    use ruff_python_ast::PySourceType;
    use ruff_python_parser::{ParseOptions, parse};
    use ruff_python_trivia::TriviaRanges;

    use crate::comments::visitor::{DecoratedComment, collect_comments};

    #[test]
    fn empty_line_after_comment() -> Result<()> {
        let source = r"
x = 1  # trailing comment

# own line comment

# another own line comment
# block
y = 2
";
        let parsed = parse(source, ParseOptions::from(PySourceType::Python))?;
        let trivia = TriviaRanges::from(parsed.tokens());
        let comments =
            collect_comments(parsed.syntax(), SourceCode::new(source), trivia.comments());

        let empty_line_after: Vec<_> = comments
            .iter()
            .map(DecoratedComment::has_empty_line_after)
            .collect();
        assert_eq!(empty_line_after, [true, true, false, false]);

        Ok(())
    }

    #[test]
    fn indentation_after_preceding_node() -> Result<()> {
        // The line continuation separates the last comment from the other comments, but the other
        // comments still count because they are between the comment and `pass`.
        let source = r"
if True:
    pass
        # indented
    # same as `pass`
        # indented again
\
        # after a line continuation
else:
    pass
";
        let parsed = parse(source, ParseOptions::from(PySourceType::Python))?;
        let trivia = TriviaRanges::from(parsed.tokens());
        let comments =
            collect_comments(parsed.syntax(), SourceCode::new(source), trivia.comments());

        let indentations: Vec<_> = comments
            .iter()
            .filter_map(|comment| {
                let preceding = comment.preceding_node()?;
                Some(comment.indentation_after(preceding, source).to_u32())
            })
            .collect();
        assert_eq!(indentations, [8, 4, 4, 4]);

        Ok(())
    }
}
