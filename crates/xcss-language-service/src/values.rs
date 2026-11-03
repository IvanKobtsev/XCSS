//! Original-source CSS value contexts for editor adapters. No CSS generation.
use super::*;
use xcss_syntax::{
    Node, ParseResult,
    ast::{Value, ValueKind},
};

#[derive(Clone, Debug)]
pub struct CssFunction {
    pub name: String,
    pub range: Range,
}
#[derive(Clone, Debug)]
pub struct CssValue {
    pub property: Option<String>,
    pub text: String,
    pub range: Range,
    pub parts: Vec<CssValuePart>,
    pub checkable: bool,
    pub functions: Vec<CssFunction>,
}
#[derive(Clone, Debug)]
pub struct CssValues {
    pub revision: u64,
    pub values: Vec<CssValue>,
}

impl ServiceSnapshot {
    pub fn css_values(&self, file: &FileId) -> CssValues {
        let mut result = CssValues {
            revision: self.revision(),
            values: vec![],
        };
        if let Some(parse) = self.0.syntax(file) {
            self.value_visit(file, parse, &parse.tree, &mut result.values);
        }
        result
    }
    fn value_visit(
        &self,
        file: &FileId,
        parse: &ParseResult,
        node: &Node,
        out: &mut Vec<CssValue>,
    ) {
        if matches!(node.kind, NodeKind::Raw | NodeKind::Error) {
            return;
        }
        if let Some(d) = parse.declaration(node) {
            self.value_context(file, parse, &d.value, Some(d.property.to_string()), out);
        }
        if let Some(c) = parse.constant(node) {
            self.value_context(file, parse, &c.initial, None, out);
        }
        let initializers = parse
            .variable(node)
            .and_then(|v| v.initial)
            .into_iter()
            .chain(
                parse
                    .injection(node)
                    .into_iter()
                    .flat_map(|i| i.parameters.into_iter().filter_map(|p| p.default)),
            );
        for text in initializers {
            let start = text.as_ptr() as usize - parse.source.text().as_ptr() as usize;
            self.value_context(
                file,
                parse,
                &parse.value(TextRange::new(start, start + text.len())),
                None,
                out,
            );
        }
        for child in &node.children {
            self.value_visit(file, parse, child, out);
        }
    }
    fn value_context(
        &self,
        file: &FileId,
        parse: &ParseResult,
        value: &Value,
        property: Option<String>,
        out: &mut Vec<CssValue>,
    ) {
        let mut parts = vec![];
        let checkable = self.css_parts(file, parse, value, &mut parts);
        let mut functions = vec![];
        self.value_functions(file, parse, value, &mut functions);
        out.push(CssValue {
            property,
            text: parse.slice(value.range).into(),
            range: self.css_range(file, value.range),
            parts,
            checkable,
            functions,
        });
    }
    fn value_functions(
        &self,
        file: &FileId,
        parse: &ParseResult,
        value: &Value,
        out: &mut Vec<CssFunction>,
    ) {
        match &value.kind {
            ValueKind::Function { name, arguments } => {
                let end = value.range.start + parse.slice(value.range).find('(').unwrap_or(0);
                out.push(CssFunction {
                    name: name.clone(),
                    range: self.css_range(file, TextRange::new(value.range.start, end)),
                });
                if !name.eq_ignore_ascii_case("url") {
                    for child in arguments {
                        self.value_functions(file, parse, child, out);
                    }
                }
            }
            ValueKind::Sequence(children) | ValueKind::Group(children) => {
                for child in children {
                    self.value_functions(file, parse, child, out);
                }
            }
            _ => {}
        }
    }
}
