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
            visit(parse, &parse.tree, None, false, &mut raw);
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
    visit(parse, &parse.tree, None, false, &mut result);
    result
}

fn visit(
    parse: &ParseResult,
    node: &Node,
    parent: Option<Option<Specificity>>,
    keyframes: bool,
    out: &mut Vec<SelectorAnalysis>,
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
    if node.kind == NodeKind::Injection {
        // Definition-site selectors may have an unknown call-site nesting context.
        parent = Some(None);
    }
    if node.kind == NodeKind::Rule {
        let analyzed =
            xcss_syntax::analyze_selectors(parse.slice(node.header), node.header.start, parent);
        parent = Some(if analyzed.iter().any(|s| s.specificity.is_none()) {
            None
        } else {
            analyzed.iter().filter_map(|s| s.specificity).max()
        });
        out.extend(analyzed);
    }
    for child in &node.children {
        visit(parse, child, parent, keyframes, out);
    }
}
