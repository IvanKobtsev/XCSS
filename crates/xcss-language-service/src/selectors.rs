//! Source selector queries shared by semantic highlighting and future lint rules.
use super::*;
use xcss_syntax::{Node, ParseResult, SelectorAnalysis, Specificity};

#[derive(Clone, Debug)]
pub struct SelectorPart {
    pub kind: String,
    pub range: Range,
}
#[derive(Clone, Debug)]
pub struct Selector {
    pub range: Range,
    pub parts: Vec<SelectorPart>,
    pub specificity: Option<Specificity>,
    pub reason: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Selectors {
    pub revision: u64,
    pub selectors: Vec<Selector>,
}

impl ServiceSnapshot {
    pub fn selectors(&self, file: &FileId) -> Selectors {
        let mut result = Selectors {
            revision: self.revision(),
            selectors: vec![],
        };
        if let Some(parse) = self.0.syntax(file) {
            let mut raw = vec![];
            visit(parse, &parse.tree, None, false, &mut raw, &mut 100_000);
            for item in raw {
                let convert = |range| {
                    self.location(&Location {
                        file: file.clone(),
                        range,
                    })
                    .unwrap()
                    .range
                };
                result.selectors.push(Selector {
                    range: convert(item.range),
                    specificity: item.specificity,
                    reason: item.reason,
                    parts: item
                        .parts
                        .into_iter()
                        .map(|p| SelectorPart {
                            kind: p.kind.token_type().into(),
                            range: convert(p.range),
                        })
                        .collect(),
                });
            }
        }
        result
    }
}

pub(super) fn raw_selectors(parse: &ParseResult) -> Vec<SelectorAnalysis> {
    let mut result = vec![];
    visit(parse, &parse.tree, None, false, &mut result, &mut 100_000);
    result
}

fn visit(
    parse: &ParseResult,
    node: &Node,
    parent: Option<Option<&str>>,
    keyframes: bool,
    out: &mut Vec<SelectorAnalysis>,
    budget: &mut usize,
) {
    let header = parse.slice(node.header).trim_start();
    let keyframes = keyframes
        || node.kind == NodeKind::AtRule
            && header
                .split_whitespace()
                .next()
                .is_some_and(|s| s == "@keyframes" || s.ends_with("-keyframes"));
    if keyframes || matches!(node.kind, NodeKind::Vars | NodeKind::Variable) {
        return;
    }
    let mut parent = parent;
    if matches!(node.kind, NodeKind::Injection | NodeKind::Util) {
        // Definition-site selectors may have an unknown call-site nesting context.
        parent = Some(None);
    }
    let mut expanded_parent = String::new();
    if node.kind == NodeKind::Rule {
        let mut analyzed = xcss_syntax::analyze_selectors(
            parse.slice(node.header),
            node.header.start,
            parent.map(|_| None),
        );
        let mut valid = parent != Some(None);
        for item in &mut analyzed {
            if parent == Some(None) {
                continue;
            }
            match xcss_syntax::expand_selectors(parent.flatten(), parse.slice(item.range), budget) {
                Ok(expanded) => {
                    let scores = xcss_syntax::analyze_selectors(&expanded.text, 0, None);
                    let first = scores.first().and_then(|s| s.specificity);
                    if first.is_some() && scores.iter().all(|s| s.specificity == first) {
                        item.specificity = first;
                        item.reason = None;
                    } else {
                        item.specificity = None;
                        item.reason =
                            Some(scores.iter().find_map(|s| s.reason.clone()).unwrap_or_else(
                                || "Specificity varies by expanded selector branch".into(),
                            ));
                    }
                    valid &= scores.iter().all(|s| s.specificity.is_some());
                    if !expanded_parent.is_empty() {
                        expanded_parent.push_str(", ");
                    }
                    expanded_parent.push_str(&expanded.text);
                }
                Err(reason) => {
                    valid = false;
                    item.specificity = None;
                    item.reason = Some(reason.into());
                }
            }
        }
        parent = Some(valid.then_some(expanded_parent.as_str()));
        out.extend(analyzed);
    }
    for child in &node.children {
        visit(parse, child, parent, keyframes, out, budget);
    }
}
