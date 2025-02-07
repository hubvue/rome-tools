use std::{cmp::Ordering, collections::BinaryHeap};

use rome_diagnostics::file::FileId;
use rome_rowan::{Language, TextRange};

use crate::{
    AnalyzerOptions, AnalyzerSignal, Phases, QueryMatch, Rule, RuleFilter, RuleGroup, ServiceBag,
};

/// The [QueryMatcher] trait is responsible of running lint rules on
/// [QueryMatch] instances emitted by the various [Visitor](crate::Visitor)
/// and push signals wrapped in [SignalEntry] to the signal queue
pub trait QueryMatcher<L: Language> {
    /// Execute a single query match
    fn match_query(&mut self, params: MatchQueryParams<L>);
}

/// Parameters provided to [QueryMatcher::match_query] and require to run lint rules
pub struct MatchQueryParams<'phase, 'query, L: Language> {
    pub phase: Phases,
    pub file_id: FileId,
    pub root: &'phase L::Root,
    pub query: QueryMatch<L>,
    pub services: &'phase ServiceBag,
    pub signal_queue: &'query mut BinaryHeap<SignalEntry<'phase, L>>,
    pub options: &'query AnalyzerOptions,
}

/// Opaque identifier for a group of rule
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct GroupKey {
    group: &'static str,
}

impl GroupKey {
    pub(crate) fn new(group: &'static str) -> Self {
        Self { group }
    }

    pub fn group<G: RuleGroup>() -> Self {
        Self::new(G::NAME)
    }
}

impl From<GroupKey> for RuleFilter<'static> {
    fn from(key: GroupKey) -> Self {
        RuleFilter::Group(key.group)
    }
}

/// Opaque identifier for a single rule
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleKey {
    group: &'static str,
    rule: &'static str,
}

impl RuleKey {
    pub(crate) fn new(group: &'static str, rule: &'static str) -> Self {
        Self { group, rule }
    }

    pub fn rule<R: Rule>() -> Self {
        Self::new(<R::Group as RuleGroup>::NAME, R::METADATA.name)
    }

    pub fn group(&self) -> &'static str {
        self.group
    }

    pub fn rule_name(&self) -> &'static str {
        self.rule
    }
}

impl From<RuleKey> for RuleFilter<'static> {
    fn from(key: RuleKey) -> Self {
        RuleFilter::Rule(key.group, key.rule)
    }
}

impl PartialEq<RuleKey> for RuleFilter<'static> {
    fn eq(&self, other: &RuleKey) -> bool {
        match *self {
            RuleFilter::Group(group) => group == other.group,
            RuleFilter::Rule(group, rule) => group == other.group && rule == other.rule,
        }
    }
}

/// Entry for a pending signal in the `signal_queue`
pub struct SignalEntry<'phase, L: Language> {
    /// Boxed analyzer signal to be emitted
    pub signal: Box<dyn AnalyzerSignal<L> + 'phase>,
    /// Unique identifier for the rule that emitted this signal
    pub rule: RuleKey,
    /// Text range in the document this signal covers
    pub text_range: TextRange,
}

// SignalEntry is ordered based on the starting point of its `text_range`
impl<'phase, L: Language> Ord for SignalEntry<'phase, L> {
    fn cmp(&self, other: &Self) -> Ordering {
        other.text_range.start().cmp(&self.text_range.start())
    }
}

impl<'phase, L: Language> PartialOrd for SignalEntry<'phase, L> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'phase, L: Language> Eq for SignalEntry<'phase, L> {}

impl<'phase, L: Language> PartialEq for SignalEntry<'phase, L> {
    fn eq(&self, other: &Self) -> bool {
        self.text_range.start() == other.text_range.start()
    }
}

/// Adapter type wrapping a [QueryMatcher] type with a function that can be
/// used to inspect the query matches emitted by the analyzer
pub struct InspectMatcher<F, I> {
    func: F,
    inner: I,
}

impl<F, I> InspectMatcher<F, I> {
    ///  Create a new instance of [InspectMatcher] from an existing [QueryMatcher]
    /// object and an inspection function
    pub fn new<L>(inner: I, func: F) -> Self
    where
        L: Language,
        F: FnMut(&MatchQueryParams<L>),
        I: QueryMatcher<L>,
    {
        Self { func, inner }
    }
}

impl<L, F, I> QueryMatcher<L> for InspectMatcher<F, I>
where
    L: Language,
    F: FnMut(&MatchQueryParams<L>),
    I: QueryMatcher<L>,
{
    fn match_query(&mut self, params: MatchQueryParams<L>) {
        (self.func)(&params);
        self.inner.match_query(params);
    }
}

#[cfg(test)]
mod tests {
    use rome_diagnostics::v2::{Diagnostic, Error, Severity};
    use rome_diagnostics::{file::FileId, v2::category};
    use rome_rowan::{
        raw_language::{RawLanguage, RawLanguageKind, RawLanguageRoot, RawSyntaxTreeBuilder},
        AstNode, TextRange, TextSize, TriviaPiece, TriviaPieceKind,
    };

    use crate::{
        signals::DiagnosticSignal, Analyzer, AnalyzerContext, AnalyzerDiagnostic, AnalyzerOptions,
        AnalyzerSignal, ControlFlow, MetadataRegistry, Never, Phases, QueryMatch, QueryMatcher,
        RuleKey, ServiceBag, SignalEntry, SyntaxVisitor,
    };

    use super::MatchQueryParams;

    struct SuppressionMatcher;

    #[derive(Debug, Diagnostic)]
    #[diagnostic(category = "args/fileNotFound", message = "test_suppression")]
    struct TestDiagnostic {
        #[location(resource)]
        location: FileId,
        #[location(span)]
        span: TextRange,
    }

    impl QueryMatcher<RawLanguage> for SuppressionMatcher {
        /// Emits a warning diagnostic for all literal expressions
        fn match_query(&mut self, params: MatchQueryParams<RawLanguage>) {
            let node = match params.query {
                QueryMatch::Syntax(node) => node,
                _ => unreachable!(),
            };

            if node.kind() != RawLanguageKind::LITERAL_EXPRESSION {
                return;
            }

            let span = node.text_trimmed_range();
            params.signal_queue.push(SignalEntry {
                signal: Box::new(DiagnosticSignal::new(move || {
                    AnalyzerDiagnostic::from_error(Error::from(TestDiagnostic {
                        span,
                        location: FileId::zero(),
                    }))
                })),
                rule: RuleKey::new("group", "rule"),
                text_range: span,
            });
        }
    }

    #[test]
    fn suppressions() {
        let root = {
            let mut builder = RawSyntaxTreeBuilder::new();

            builder.start_node(RawLanguageKind::ROOT);
            builder.start_node(RawLanguageKind::SEPARATED_EXPRESSION_LIST);

            builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
            builder.token_with_trivia(
                RawLanguageKind::STRING_TOKEN,
                "//group\n\"warn_here\"",
                &[
                    TriviaPiece::new(TriviaPieceKind::SingleLineComment, 7),
                    TriviaPiece::new(TriviaPieceKind::Newline, 1),
                ],
                &[],
            );
            builder.finish_node();

            builder.token_with_trivia(
                RawLanguageKind::SEMICOLON_TOKEN,
                ";\n",
                &[],
                &[TriviaPiece::new(TriviaPieceKind::Newline, 1)],
            );

            builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
            builder.token_with_trivia(
                RawLanguageKind::STRING_TOKEN,
                "//group/rule\n\"warn_here\"",
                &[
                    TriviaPiece::new(TriviaPieceKind::SingleLineComment, 12),
                    TriviaPiece::new(TriviaPieceKind::Newline, 1),
                ],
                &[],
            );
            builder.finish_node();

            builder.token_with_trivia(
                RawLanguageKind::SEMICOLON_TOKEN,
                ";\n",
                &[],
                &[TriviaPiece::new(TriviaPieceKind::Newline, 1)],
            );

            builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
            builder.token_with_trivia(
                RawLanguageKind::STRING_TOKEN,
                "//unknown_group\n\"warn_here\"",
                &[
                    TriviaPiece::new(TriviaPieceKind::SingleLineComment, 15),
                    TriviaPiece::new(TriviaPieceKind::Newline, 1),
                ],
                &[],
            );
            builder.finish_node();

            builder.token_with_trivia(
                RawLanguageKind::SEMICOLON_TOKEN,
                ";\n",
                &[],
                &[TriviaPiece::new(TriviaPieceKind::Newline, 1)],
            );

            builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
            builder.token_with_trivia(
                RawLanguageKind::STRING_TOKEN,
                "//group/unknown_rule\n\"warn_here\"",
                &[
                    TriviaPiece::new(TriviaPieceKind::SingleLineComment, 20),
                    TriviaPiece::new(TriviaPieceKind::Newline, 1),
                ],
                &[],
            );
            builder.finish_node();

            builder.token_with_trivia(
                RawLanguageKind::SEMICOLON_TOKEN,
                ";\n",
                &[],
                &[TriviaPiece::new(TriviaPieceKind::Newline, 1)],
            );

            builder.token_with_trivia(
                RawLanguageKind::SEMICOLON_TOKEN,
                "//group/rule\n;\n",
                &[
                    TriviaPiece::new(TriviaPieceKind::SingleLineComment, 12),
                    TriviaPiece::new(TriviaPieceKind::Newline, 1),
                ],
                &[TriviaPiece::new(TriviaPieceKind::Newline, 1)],
            );

            builder.finish_node();
            builder.finish_node();

            RawLanguageRoot::unwrap_cast(builder.finish())
        };

        let mut diagnostics = Vec::new();
        let mut emit_signal = |signal: &dyn AnalyzerSignal<RawLanguage>| -> ControlFlow<Never> {
            let mut diag = signal.diagnostic().expect("diagnostic");
            diag.set_severity(Severity::Warning);
            let code = diag.category().expect("code");
            let range = diag.get_span().expect("range");

            diagnostics.push((code, range));
            ControlFlow::Continue(())
        };

        fn parse_suppression_comment(comment: &str) -> Vec<Option<&str>> {
            comment
                .trim_start_matches("//")
                .split(' ')
                .map(Some)
                .collect()
        }

        let mut metadata = MetadataRegistry::default();
        metadata.insert_rule("group", "rule");

        let mut analyzer = Analyzer::new(
            &metadata,
            SuppressionMatcher,
            parse_suppression_comment,
            &mut emit_signal,
        );

        analyzer.add_visitor(Phases::Syntax, SyntaxVisitor::default());

        let ctx: AnalyzerContext<RawLanguage> = AnalyzerContext {
            file_id: FileId::zero(),
            root,
            range: None,
            services: ServiceBag::default(),
            options: &AnalyzerOptions::default(),
        };

        let result: Option<Never> = analyzer.run(ctx);
        assert!(result.is_none());

        assert_eq!(
            diagnostics.as_slice(),
            &[
                (
                    category!("suppressions/unknownGroup"),
                    TextRange::new(TextSize::from(47), TextSize::from(62))
                ),
                (
                    category!("args/fileNotFound"),
                    TextRange::new(TextSize::from(63), TextSize::from(74))
                ),
                (
                    category!("suppressions/unknownRule"),
                    TextRange::new(TextSize::from(76), TextSize::from(96))
                ),
                (
                    category!("args/fileNotFound"),
                    TextRange::new(TextSize::from(97), TextSize::from(108))
                ),
                (
                    category!("suppressions/unused"),
                    TextRange::new(TextSize::from(110), TextSize::from(122))
                ),
            ]
        );
    }
}
