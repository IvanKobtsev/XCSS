//! Selector-list expansion without native nesting's maximum-parent specificity.
use crate::{Token, TokenKind, lex};
use xcss_source::{FileId, SourceFile, TextRange};

#[derive(Clone, Debug)]
pub struct SelectorExpansionSpan {
    pub generated: TextRange,
    pub source: TextRange,
    pub parent: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SelectorExpansion {
    pub text: String,
    pub spans: Vec<SelectorExpansionSpan>,
}

impl SelectorExpansion {
    fn literal(&mut self, text: &str) {
        self.text.push_str(text);
    }
    fn source(&mut self, text: &str, range: TextRange, parent: bool) {
        let start = self.text.len();
        self.text.push_str(&text[range.start..range.end]);
        self.spans.push(SelectorExpansionSpan {
            generated: TextRange::new(start, self.text.len()),
            source: range,
            parent,
        });
    }
    fn append(&mut self, other: &Self) {
        let offset = self.text.len();
        self.text.push_str(&other.text);
        self.spans
            .extend(other.spans.iter().map(|span| SelectorExpansionSpan {
                generated: TextRange::new(
                    span.generated.start + offset,
                    span.generated.end + offset,
                ),
                ..span.clone()
            }));
    }
}

fn charge(budget: &mut usize, count: usize) -> Result<(), &'static str> {
    *budget = budget
        .checked_sub(count)
        .ok_or("Selector expansion exceeds the generation limit")?;
    Ok(())
}

fn check_bytes(size: usize) -> Result<(), &'static str> {
    if size > 16 * 1024 * 1024 {
        Err("Selector expansion exceeds the output size limit")
    } else {
        Ok(())
    }
}

pub(crate) fn tokens(text: &str) -> Vec<Token> {
    lex(&SourceFile::new(FileId("selector".into()), 0, text)).0
}

pub(crate) fn members(text: &str, tokens: &[Token]) -> Vec<TextRange> {
    let mut ranges = Vec::new();
    let mut depth = 0usize;
    let mut start = tokens.first().map_or(0, |t| t.range.start);
    let end = tokens.last().map_or(start, |t| t.range.end);
    for token in tokens {
        if token.kind != TokenKind::Punctuation {
            continue;
        }
        match &text[token.range.start..token.range.end] {
            "(" | "[" => depth += 1,
            ")" | "]" => depth = depth.saturating_sub(1),
            "," if depth == 0 => {
                ranges.push(trim(text, TextRange::new(start, token.range.start)));
                start = token.range.end;
            }
            _ => {}
        }
    }
    ranges.push(trim(text, TextRange::new(start, end)));
    ranges
}

fn trim(text: &str, range: TextRange) -> TextRange {
    let raw = &text[range.start..range.end];
    let start = range.start + raw.len() - raw.trim_start().len();
    TextRange::new(start, start + raw.trim().len())
}

/// Expand existing CSS nesting syntax. Each bare `&` independently selects a
/// parent-list member; selector-function arguments retain their list structure.
/// Spans let callers copy parent and child fragments with their original maps.
pub fn expand_selectors(
    parent: Option<&str>,
    child: &str,
    budget: &mut usize,
) -> Result<SelectorExpansion, &'static str> {
    let child_tokens = tokens(child);
    let Some(parent) = parent else {
        charge(budget, members(child, &child_tokens).len())?;
        let mut result = SelectorExpansion::default();
        result.source(child, TextRange::new(0, child.len()), false);
        return Ok(result);
    };
    let parents = members(parent, &tokens(parent));
    expand_list(parent, &parents, child, &child_tokens, true, budget, 0)
}

fn expand_list(
    parent: &str,
    parents: &[TextRange],
    child: &str,
    tokens: &[Token],
    implicit: bool,
    budget: &mut usize,
    depth: usize,
) -> Result<SelectorExpansion, &'static str> {
    if depth > 128 {
        return Err("Selector expansion exceeds the depth limit");
    }
    let mut result = SelectorExpansion::default();
    for member in members(child, tokens) {
        let start = tokens.partition_point(|t| t.range.end <= member.start);
        let end = tokens.partition_point(|t| t.range.start < member.end);
        let tokens = &tokens[start..end];
        // Each replacement is either a single resolved function, or a choice of
        // parents for an actual nesting token. Strings and attributes are opaque.
        let mut replacements = Vec::new();
        let mut i = 0;
        let mut brackets = 0usize;
        let mut nesting = false;
        while i < tokens.len() {
            let token = &tokens[i];
            if token.range.start < member.start
                || token.range.end > member.end
                || token.kind != TokenKind::Punctuation
            {
                i += 1;
                continue;
            }
            match &child[token.range.start..token.range.end] {
                "[" => brackets += 1,
                "]" => brackets = brackets.saturating_sub(1),
                "&" if brackets == 0 => {
                    if tokens.get(i + 1).is_some_and(|next| {
                        next.kind == TokenKind::Word && next.range.start == token.range.end
                    }) {
                        return Err("Sass parent-selector suffix syntax is not supported");
                    }
                    nesting = true;
                    let mut choices = Vec::new();
                    charge(budget, parents.len())?;
                    for range in parents {
                        let mut choice = SelectorExpansion::default();
                        choice.source(parent, *range, true);
                        choices.push(choice);
                    }
                    replacements.push((token.range, choices));
                }
                "(" if brackets == 0 => {
                    let mut level = 1;
                    let mut end = i + 1;
                    while end < tokens.len() {
                        if tokens[end].kind == TokenKind::Punctuation {
                            match &child[tokens[end].range.start..tokens[end].range.end] {
                                "(" => level += 1,
                                ")" => {
                                    level -= 1;
                                    if level == 0 {
                                        break;
                                    }
                                }
                                _ => {}
                            }
                        }
                        end += 1;
                    }
                    if end == tokens.len() {
                        return Err("Unclosed selector function");
                    }
                    let inner = &tokens[i + 1..end];
                    if inner.iter().any(|t| {
                        t.kind == TokenKind::Punctuation
                            && &child[t.range.start..t.range.end] == "&"
                    }) {
                        // nth-child's formula precedes its selector-list argument.
                        let nth = tokens[..i]
                            .iter()
                            .rev()
                            .find(|t| !matches!(t.kind, TokenKind::Whitespace | TokenKind::Comment))
                            .is_some_and(|t| {
                                let name = &child[t.range.start..t.range.end];
                                name.eq_ignore_ascii_case("nth-child")
                                    || name.eq_ignore_ascii_case("nth-last-child")
                            });
                        let of = nth
                            .then(|| {
                                inner.iter().position(|t| {
                                    t.kind == TokenKind::Word
                                        && child[t.range.start..t.range.end]
                                            .eq_ignore_ascii_case("of")
                                })
                            })
                            .flatten();
                        let selectors = of.map_or(inner, |n| &inner[n + 1..]);
                        let resolved = expand_list(
                            parent,
                            parents,
                            child,
                            selectors,
                            false,
                            budget,
                            depth + 1,
                        )?;
                        nesting |= resolved.spans.iter().any(|s| s.parent);
                        let start = selectors
                            .iter()
                            .find(|t| !matches!(t.kind, TokenKind::Whitespace | TokenKind::Comment))
                            .map_or(token.range.end, |t| t.range.start);
                        replacements.push((
                            TextRange::new(start, tokens[end].range.start),
                            vec![resolved],
                        ));
                    }
                    i = end;
                }
                _ => {}
            }
            i += 1;
        }
        let mut variants = vec![SelectorExpansion::default()];
        let mut cursor = member.start;
        for (range, choices) in replacements {
            let size = variants
                .len()
                .checked_mul(choices.len())
                .ok_or("Selector expansion exceeds the generation limit")?;
            charge(budget, size)?;
            let bytes = variants
                .iter()
                .try_fold(0usize, |total, variant| {
                    choices.iter().try_fold(total, |total, choice| {
                        total.checked_add(
                            variant.text.len() + range.start - cursor + choice.text.len(),
                        )
                    })
                })
                .ok_or("Selector expansion exceeds the output size limit")?;
            check_bytes(bytes)?;
            let mut next = Vec::with_capacity(size);
            for variant in &variants {
                for choice in &choices {
                    let mut expanded = variant.clone();
                    expanded.source(child, TextRange::new(cursor, range.start), false);
                    expanded.append(choice);
                    next.push(expanded);
                }
            }
            cursor = range.end;
            variants = next;
        }
        if implicit && !nesting {
            charge(budget, parents.len())?;
            variants.clear();
            for range in parents {
                let mut variant = SelectorExpansion::default();
                variant.source(parent, *range, true);
                variant.literal(" ");
                variants.push(variant);
            }
        }
        charge(budget, variants.len())?;
        for mut variant in variants {
            check_bytes(result.text.len() + variant.text.len() + member.end - cursor + 2)?;
            variant.source(child, TextRange::new(cursor, member.end), false);
            if !result.text.is_empty() {
                result.literal(", ");
            }
            result.append(&variant);
        }
    }
    Ok(result)
}
