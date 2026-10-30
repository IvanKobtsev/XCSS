//! Source-preserving CSS validation inputs. This module never generates CSS.
use super::*;
use xcss_syntax::{
    Node, ParseResult,
    ast::{Value, ValueKind, decode_identifier},
};

#[derive(Clone, Debug)]
pub struct CssValuePart {
    pub text: Option<String>,
    pub value_type: Option<String>,
    pub range: Range,
}
#[derive(Clone, Debug)]
pub struct CssDeclaration {
    pub property: String,
    pub property_range: Range,
    pub value_range: Range,
    pub context: String,
    pub checkable: bool,
    pub parts: Vec<CssValuePart>,
}
#[derive(Clone, Debug)]
pub struct CssDeclarations {
    pub revision: u64,
    pub declarations: Vec<CssDeclaration>,
}

impl ServiceSnapshot {
    pub fn css_declarations(&self, file: &FileId) -> CssDeclarations {
        let mut result = CssDeclarations {
            revision: self.revision(),
            declarations: vec![],
        };
        if let Some(parse) = self.0.syntax(file) {
            self.css_visit(
                file,
                parse,
                &parse.tree,
                ("stylesheet", false),
                &mut result.declarations,
            );
        }
        result
    }

    fn css_range(&self, file: &FileId, range: TextRange) -> Range {
        self.location(&Location {
            file: file.clone(),
            range,
        })
        .expect("parser ranges are source boundaries")
        .range
    }

    fn css_visit(
        &self,
        file: &FileId,
        parse: &ParseResult,
        node: &Node,
        context: (&str, bool),
        out: &mut Vec<CssDeclaration>,
    ) {
        let (context, recovered) = context;
        if matches!(
            node.kind,
            NodeKind::Vars | NodeKind::Variable | NodeKind::Error
        ) {
            return;
        }
        let recovered = recovered || !node.missing.is_empty();
        if let Some(declaration) = parse.declaration(node) {
            let property = decode_identifier(declaration.property);
            let raw = parse.slice(declaration.value.range);
            let significant: String = parse
                .tokens
                .iter()
                .filter(|t| {
                    t.range.start >= declaration.value.range.start
                        && t.range.end <= declaration.value.range.end
                })
                .filter(|t| !matches!(t.kind, TokenKind::Whitespace | TokenKind::Comment))
                .map(|t| parse.slice(t.range))
                .collect();
            if property.eq_ignore_ascii_case("composes")
                || property.eq_ignore_ascii_case("content")
                    && significant.eq_ignore_ascii_case("preserve")
            {
                return;
            }
            let mut parts = vec![];
            let mut checkable = !recovered && !raw.trim().is_empty();
            // Transition names and groups have XCSS-specific semantics; defer their values.
            if property.eq_ignore_ascii_case("transition")
                || property.eq_ignore_ascii_case("transition-property")
            {
                checkable &=
                    !declaration.value.descendants().iter().any(|v| {
                        matches!(v.kind, ValueKind::Group(_) | ValueKind::CustomProperty(_))
                    });
            }
            checkable &= !parse.diagnostics.iter().any(|d| {
                d.location.range.start < node.range.end && d.location.range.end > node.range.start
            });
            let expression =
                self.0.expressions().iter().find(|e| {
                    e.location.file == *file && e.location.range == declaration.value.range
                });
            if let Some(expression) =
                expression.filter(|e| e.kind == xcss_analysis::ExpressionKind::Arithmetic)
            {
                if let ValueType::Named(name) = &expression.value_type {
                    parts.push(CssValuePart {
                        text: None,
                        value_type: Some(name.clone()),
                        range: self.css_range(file, declaration.value.range),
                    });
                } else {
                    checkable = false;
                }
            } else {
                checkable &= self.css_parts(file, parse, &declaration.value, &mut parts);
            }
            out.push(CssDeclaration {
                property,
                property_range: self.css_range(file, declaration.property_range),
                value_range: self.css_range(file, declaration.value.range),
                context: context.into(),
                checkable,
                parts,
            });
            return;
        }
        let next = match node.kind {
            NodeKind::Rule => "rule",
            NodeKind::Injection => "injection",
            NodeKind::AtRule => {
                // At-rule children that are declarations are descriptors. Nested rules
                // remain eligible, including inside unknown extension at-rules.
                if parse.keyframes(node).is_some() {
                    "keyframes"
                } else {
                    "at-rule"
                }
            }
            _ => context,
        };
        for child in &node.children {
            if node.kind == NodeKind::AtRule && child.kind == NodeKind::Declaration {
                continue;
            }
            self.css_visit(file, parse, child, (next, recovered), out);
        }
    }

    fn css_parts(
        &self,
        file: &FileId,
        parse: &ParseResult,
        value: &Value,
        out: &mut Vec<CssValuePart>,
    ) -> bool {
        let typed = |kind: &ValueType, out: &mut Vec<CssValuePart>| {
            if let ValueType::Named(name) = kind {
                out.push(CssValuePart {
                    text: None,
                    value_type: Some(name.clone()),
                    range: self.css_range(file, value.range),
                });
                true
            } else {
                false
            }
        };
        match &value.kind {
            ValueKind::Parameter(_) | ValueKind::CustomProperty(_) => {
                return self
                    .0
                    .symbol_at(file, value.range.start)
                    .is_some_and(|s| typed(&s.value_type, out));
            }
            ValueKind::Function { name, .. } if name.eq_ignore_ascii_case("var") => return false,
            ValueKind::Group(parts) | ValueKind::Sequence(parts)
                if parts
                    .iter()
                    .any(|p| matches!(p.kind, ValueKind::Operator(op) if op != '/')) =>
            {
                return self
                    .0
                    .expressions()
                    .iter()
                    .find(|e| e.location.file == *file && e.location.range == value.range)
                    .is_some_and(|e| typed(&e.value_type, out));
            }
            ValueKind::Group(_) => return false,
            ValueKind::Sequence(children)
            | ValueKind::Function {
                arguments: children,
                ..
            } => {
                let mut cursor = value.range.start;
                for child in children {
                    self.css_literal(file, parse, TextRange::new(cursor, child.range.start), out);
                    if !self.css_parts(file, parse, child, out) {
                        return false;
                    }
                    cursor = child.range.end;
                }
                self.css_literal(file, parse, TextRange::new(cursor, value.range.end), out);
            }
            _ => self.css_literal(file, parse, value.range, out),
        }
        true
    }

    fn css_literal(
        &self,
        file: &FileId,
        parse: &ParseResult,
        range: TextRange,
        out: &mut Vec<CssValuePart>,
    ) {
        if range.start < range.end {
            out.push(CssValuePart {
                text: Some(parse.slice(range).into()),
                value_type: None,
                range: self.css_range(file, range),
            });
        }
    }
}
