//! Selector inspection shared by analysis and emission.
use crate::{Node, NodeKind, ParseResult, TokenKind};
use raffia::{ParserBuilder, ast::*};
use xcss_source::TextRange;
pub fn selector_needs_content(text: &str) -> bool {
    let mut parser = ParserBuilder::new(text).syntax(raffia::Syntax::Css).build();
    parser
        .parse::<SelectorList>()
        .is_ok_and(|s| selector_list_needs_content(&s))
}
fn selector_list_needs_content(selector: &SelectorList<'_>) -> bool {
    !selector.selectors.is_empty() && selector.selectors.iter().all(selector_ends_in_pseudo)
}

fn selector_ends_in_pseudo(selector: &ComplexSelector<'_>) -> bool {
    let Some(ComplexSelectorChild::CompoundSelector(compound)) = selector.children.last() else {
        return false;
    };
    compound_ends_in_pseudo(compound)
}

fn compound_ends_in_pseudo(compound: &CompoundSelector<'_>) -> bool {
    match compound.children.last() {
        Some(SimpleSelector::PseudoElement(pseudo)) => {
            interpolable_ident_is_one_of(&pseudo.name, &["before", "after"])
        }
        Some(SimpleSelector::PseudoClass(pseudo))
            if interpolable_ident_is_one_of(&pseudo.name, &["is"]) =>
        {
            let Some(argument) = &pseudo.arg else {
                return false;
            };
            match &argument.kind {
                PseudoClassSelectorArgKind::SelectorList(selector_list) => {
                    is_exclusive_pseudo_selector_list(selector_list)
                }
                PseudoClassSelectorArgKind::CompoundSelectorList(selector_list) => {
                    !selector_list.selectors.is_empty()
                        && selector_list.selectors.iter().all(is_bare_before_or_after)
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn is_exclusive_pseudo_selector_list(selector_list: &SelectorList<'_>) -> bool {
    !selector_list.selectors.is_empty()
        && selector_list.selectors.iter().all(|selector| {
            matches!(selector.children.as_slice(), [ComplexSelectorChild::CompoundSelector(compound)] if is_bare_before_or_after(compound))
        })
}

fn is_bare_before_or_after(compound: &CompoundSelector<'_>) -> bool {
    matches!(
        compound.children.as_slice(),
        [SimpleSelector::PseudoElement(pseudo)]
            if interpolable_ident_is_one_of(&pseudo.name, &["before", "after"])
    )
}

fn interpolable_ident_is_one_of(ident: &InterpolableIdent<'_>, expected: &[&str]) -> bool {
    matches!(
        ident,
        InterpolableIdent::Literal(ident)
            if expected.iter().any(|expected| ident.name.eq_ignore_ascii_case(expected))
    )
}

pub fn selector_classes(parse: &ParseResult, node: &Node) -> Vec<(String, TextRange, bool)> {
    let tokens: Vec<_> = parse
        .tokens
        .iter()
        .filter(|t| {
            t.range.start >= node.header.start
                && t.range.end <= node.header.end
                && !matches!(t.kind, TokenKind::Whitespace | TokenKind::Comment)
        })
        .collect();
    let mut result = Vec::new();
    let mut mode = false;
    let mut stack = Vec::new();
    let mut pending = None;
    for (i, token) in tokens.iter().enumerate() {
        let text = &parse.source.text()[token.range.start..token.range.end];
        if text == ":"
            && let Some(next) = tokens.get(i + 1)
        {
            let name = &parse.source.text()[next.range.start..next.range.end];
            if name == "global" || name == "local" {
                pending = Some(name == "global");
                if tokens
                    .get(i + 2)
                    .is_none_or(|t| &parse.source.text()[t.range.start..t.range.end] != "(")
                {
                    mode = name == "global";
                    pending = None;
                }
            }
        }
        if text == "(" {
            stack.push(mode);
            if let Some(next) = pending.take() {
                mode = next;
            }
        }
        if text == ")" {
            mode = stack.pop().unwrap_or(false);
        }
        if text == "," && stack.is_empty() {
            mode = false;
        }
        if text == "."
            && let Some(next) = tokens.get(i + 1)
            && next.kind == TokenKind::Word
        {
            result.push((
                crate::ast::decode_identifier(
                    &parse.source.text()[next.range.start..next.range.end],
                ),
                next.range,
                mode,
            ));
        }
    }
    result
}

pub fn local_class_names(parse: &ParseResult) -> std::collections::BTreeSet<String> {
    parse
        .tree
        .descendants()
        .filter(|n| n.kind == NodeKind::Rule)
        .flat_map(|n| selector_classes(parse, n))
        .filter(|(_, _, global)| !*global)
        .map(|(name, _, _)| name)
        .collect()
}
