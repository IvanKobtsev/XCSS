//! Owned, recovering XCSS syntax. No generated text enters this parser.
use std::sync::Arc;
use xcss_source::{Diagnostic, SourceFile, TextRange};
pub mod ast;
mod selector_analysis;
mod selectors;
pub use selector_analysis::{
    SelectorAnalysis, SelectorKind, SelectorPart, Specificity, analyze_selectors,
};
pub use selectors::{local_class_names, selector_classes, selector_needs_content};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Word,
    String,
    Whitespace,
    Comment,
    Punctuation,
    Invalid,
}
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub range: TextRange,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind {
    Stylesheet,
    Rule,
    Declaration,
    Vars,
    Variable,
    Injection,
    Call,
    Import,
    Content,
    AtRule,
    Error,
}
#[derive(Clone, Debug)]
pub struct Node {
    pub kind: NodeKind,
    pub range: TextRange,
    pub header: TextRange,
    pub children: Vec<Node>,
    pub missing: Vec<char>,
    pub block: Option<TextRange>,
}
impl Node {
    pub fn text<'a>(&self, source: &'a SourceFile) -> &'a str {
        &source.text()[self.range.start..self.range.end]
    }
    pub fn header_text<'a>(&self, source: &'a SourceFile) -> &'a str {
        &source.text()[self.header.start..self.header.end]
    }
    pub fn descendants(&self) -> impl Iterator<Item = &Node> {
        let mut nodes = Vec::new();
        fn visit<'a>(node: &'a Node, nodes: &mut Vec<&'a Node>) {
            nodes.push(node);
            for child in &node.children {
                visit(child, nodes);
            }
        }
        visit(self, &mut nodes);
        nodes.into_iter()
    }
}
#[derive(Clone, Debug)]
pub struct ParseResult {
    pub source: Arc<SourceFile>,
    pub tokens: Arc<[Token]>,
    pub tree: Arc<Node>,
    pub diagnostics: Arc<[Diagnostic]>,
}

pub fn parse(source: Arc<SourceFile>) -> ParseResult {
    let (tokens, mut diagnostics) = lex(&source);
    let significant: Vec<_> = tokens
        .iter()
        .filter(|t| !matches!(t.kind, TokenKind::Whitespace | TokenKind::Comment))
        .cloned()
        .collect();
    let mut parser = Parser {
        source: &source,
        tokens: &significant,
        cursor: 0,
        diagnostics: &mut diagnostics,
    };
    let children = parser.block(false, false, 0);
    let tree = Node {
        kind: NodeKind::Stylesheet,
        range: TextRange::new(0, source.text().len()),
        header: TextRange::default(),
        children,
        missing: Vec::new(),
        block: None,
    };
    let mut result = ParseResult {
        source,
        tokens: tokens.into(),
        tree: Arc::new(tree),
        diagnostics: diagnostics.into(),
    };
    validate_fragments(&mut result);
    result
}

fn lex(source: &SourceFile) -> (Vec<Token>, Vec<Diagnostic>) {
    let text = source.text();
    let mut out = Vec::new();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let start = i;
        let c = text[i..].chars().next().unwrap();
        let kind;
        if text[i..].starts_with("/*") {
            kind = TokenKind::Comment;
            if let Some(end) = text[i + 2..].find("*/") {
                i += end + 4;
            } else {
                i = text.len();
                diagnostics.push(Diagnostic::error(
                    "syntax.unclosed-comment",
                    &source.id,
                    TextRange::new(start, i),
                    "Unclosed comment",
                ));
            }
        } else if c == '\'' || c == '"' {
            kind = TokenKind::String;
            i += 1;
            let mut closed = false;
            while i < text.len() {
                let next = text[i..].chars().next().unwrap();
                if next == '\n' || next == '\r' {
                    break;
                }
                i += next.len_utf8();
                if next == '\\' {
                    if i < text.len() {
                        i += text[i..].chars().next().unwrap().len_utf8();
                    }
                } else if next == c {
                    closed = true;
                    break;
                }
            }
            if !closed {
                diagnostics.push(Diagnostic::error(
                    "syntax.unclosed-string",
                    &source.id,
                    TextRange::new(start, i),
                    "Unclosed string",
                ));
            }
        } else if c.is_whitespace() {
            kind = TokenKind::Whitespace;
            i += c.len_utf8();
            while i < text.len() && text[i..].chars().next().unwrap().is_whitespace() {
                i += text[i..].chars().next().unwrap().len_utf8();
            }
        } else if c.is_ascii_digit()
            || c == '.' && text[i + 1..].starts_with(|c: char| c.is_ascii_digit())
            || matches!(c, '+' | '-')
                && text[i + 1..].starts_with(|c: char| c.is_ascii_digit() || c == '.')
        {
            kind = TokenKind::Word;
            if matches!(c, '+' | '-') {
                i += 1;
            }
            while i < text.len() && text.as_bytes()[i].is_ascii_digit() {
                i += 1;
            }
            if i < text.len() && text.as_bytes()[i] == b'.' {
                i += 1;
                while i < text.len() && text.as_bytes()[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < text.len() && matches!(text.as_bytes()[i], b'e' | b'E') {
                let exponent = i;
                i += 1;
                if i < text.len() && matches!(text.as_bytes()[i], b'+' | b'-') {
                    i += 1;
                }
                let digits = i;
                while i < text.len() && text.as_bytes()[i].is_ascii_digit() {
                    i += 1;
                }
                if digits == i {
                    i = exponent;
                }
            }
            while i < text.len()
                && (text[i..].starts_with(|c: char| c.is_alphabetic())
                    || text.as_bytes()[i] == b'%')
            {
                i += text[i..].chars().next().unwrap().len_utf8();
            }
        } else if c.is_alphanumeric() || matches!(c, '_' | '-' | '\\') || !c.is_ascii() {
            kind = TokenKind::Word;
            while i < text.len() {
                let next = text[i..].chars().next().unwrap();
                if next == '\\' {
                    i += 1;
                    let mut digits = 0;
                    while i < text.len() && digits < 6 && text.as_bytes()[i].is_ascii_hexdigit() {
                        i += 1;
                        digits += 1;
                    }
                    if digits > 0 {
                        if i < text.len() && text[i..].starts_with(char::is_whitespace) {
                            let c = text[i..].chars().next().unwrap();
                            i += c.len_utf8();
                            if c == '\r' && text[i..].starts_with('\n') {
                                i += 1;
                            }
                        }
                    } else if i < text.len() {
                        i += text[i..].chars().next().unwrap().len_utf8();
                    }
                } else if next.is_alphanumeric() || matches!(next, '_' | '-') || !next.is_ascii() {
                    i += next.len_utf8();
                } else {
                    break;
                }
            }
        } else {
            kind = TokenKind::Punctuation;
            i += c.len_utf8();
        }
        out.push(Token {
            kind,
            range: TextRange::new(start, i),
        });
    }
    (out, diagnostics)
}

struct Parser<'a> {
    source: &'a SourceFile,
    tokens: &'a [Token],
    cursor: usize,
    diagnostics: &'a mut Vec<Diagnostic>,
}
impl Parser<'_> {
    fn token(&self, i: usize) -> &str {
        let r = self.tokens[i].range;
        &self.source.text()[r.start..r.end]
    }
    fn block(&mut self, nested: bool, vars: bool, depth: usize) -> Vec<Node> {
        let mut nodes = Vec::new();
        if depth > 128 {
            self.diagnostics.push(Diagnostic::error(
                "syntax.depth",
                &self.source.id,
                self.tokens
                    .get(self.cursor)
                    .map(|t| t.range)
                    .unwrap_or_default(),
                "Maximum nesting depth exceeded",
            ));
            self.cursor = self.tokens.len();
            return nodes;
        }
        while self.cursor < self.tokens.len() {
            if self.token(self.cursor) == "}" {
                if nested {
                    break;
                }
                self.diagnostics.push(Diagnostic::error(
                    "syntax.unexpected-close",
                    &self.source.id,
                    self.tokens[self.cursor].range,
                    "Unexpected closing brace",
                ));
                self.cursor += 1;
                continue;
            }
            let first = self.cursor;
            let start = self.tokens[first].range.start;
            let mut parens = 0usize;
            let mut brackets = 0usize;
            let named_import = self.source.text()[start..].starts_with("@import");
            let mut import_braces = 0usize;
            let custom_value = self.token(first).starts_with("--")
                && first + 1 < self.tokens.len()
                && self.token(first + 1) == ":";
            let mut value_braces = 0usize;
            while self.cursor < self.tokens.len() {
                let value = self.token(self.cursor);
                if custom_value && value == "{" {
                    value_braces += 1;
                    self.cursor += 1;
                    continue;
                }
                if custom_value && value == "}" && value_braces > 0 {
                    value_braces -= 1;
                    self.cursor += 1;
                    continue;
                }
                if custom_value && value_braces > 0 {
                    self.cursor += 1;
                    continue;
                }
                if named_import && value == "{" {
                    import_braces += 1;
                    self.cursor += 1;
                    continue;
                }
                if named_import && value == "}" && import_braces > 0 {
                    import_braces -= 1;
                    self.cursor += 1;
                    continue;
                }
                if matches!(value, ";" | "}") || value == "{" && brackets == 0 {
                    break;
                }
                // Recover a missing semicolon at the next declaration on another line.
                if self.cursor > first
                    && self.tokens[self.cursor].kind == TokenKind::Word
                    && self.tokens.get(self.cursor + 1).is_some()
                    && self.token(self.cursor + 1) == ":"
                {
                    let gap = &self.source.text()[self.tokens[self.cursor - 1].range.end
                        ..self.tokens[self.cursor].range.start];
                    if gap.contains('\n') {
                        break;
                    }
                }
                match value {
                    "(" => parens += 1,
                    ")" => parens = parens.saturating_sub(1),
                    "[" => brackets += 1,
                    "]" => brackets = brackets.saturating_sub(1),
                    _ => {}
                }
                self.cursor += 1;
            }
            let end = self
                .tokens
                .get(self.cursor)
                .map(|t| t.range.start)
                .unwrap_or(self.source.text().len());
            let header = TextRange::new(start, end);
            let raw = self.source.text()[start..end].trim();
            let has_block = self.cursor < self.tokens.len() && self.token(self.cursor) == "{";
            let at_name = if self.token(first) == "@" && first + 1 < self.cursor {
                self.token(first + 1)
            } else {
                ""
            };
            let kind = match at_name {
                "vars" => NodeKind::Vars,
                "injection" => NodeKind::Injection,
                ":" => NodeKind::Call,
                "import" => NodeKind::Import,
                "content" => NodeKind::Content,
                _ if raw.starts_with('@') => NodeKind::AtRule,
                _ if vars => NodeKind::Variable,
                _ if has_block => NodeKind::Rule,
                _ if raw.contains(':') => NodeKind::Declaration,
                _ => NodeKind::Error,
            };
            let forbidden = [
                "@mixin",
                "@include",
                "@use",
                "@forward",
                "@if",
                "@else",
                "@for",
                "@each",
                "@while",
                "@function",
                "@return",
                "@extend",
                "@debug",
                "@warn",
                "@error",
                "@at-root",
            ];
            if forbidden.iter().any(|s| {
                raw.strip_prefix(s).is_some_and(|tail| {
                    tail.is_empty() || tail.starts_with(|c: char| !c.is_alphanumeric() && c != '-')
                })
            }) || raw.starts_with('$')
                || raw.contains("#{")
            {
                self.diagnostics.push(Diagnostic::error(
                    "syntax.sass",
                    &self.source.id,
                    header,
                    "Sass syntax is not part of XCSS",
                ));
            }
            if raw.contains("#{") {
                self.diagnostics.push(Diagnostic::error(
                    "syntax.interpolation",
                    &self.source.id,
                    header,
                    "Sass interpolation is not supported",
                ));
            }
            let mut missing = Vec::new();
            if parens > 0 {
                missing.push(')');
            }
            if brackets > 0 {
                missing.push(']');
            }
            let mut children = Vec::new();
            let mut node_end = end;
            let block_start = has_block.then_some(end);
            if has_block {
                self.cursor += 1;
                children = self.block(true, kind == NodeKind::Vars, depth + 1);
                if self.cursor < self.tokens.len() && self.token(self.cursor) == "}" {
                    node_end = self.tokens[self.cursor].range.end;
                    self.cursor += 1;
                } else {
                    missing.push('}');
                    node_end = self.source.text().len();
                }
            } else if self.cursor < self.tokens.len() && self.token(self.cursor) == ";" {
                node_end = self.tokens[self.cursor].range.end;
                self.cursor += 1;
            } else if kind != NodeKind::Error
                && (self.cursor < self.tokens.len() && self.token(self.cursor) != "}"
                    || matches!(kind, NodeKind::Injection | NodeKind::Vars | NodeKind::Rule))
            {
                missing.push(';');
            }
            if !missing.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    "syntax.missing-token",
                    &self.source.id,
                    TextRange::new(end, end),
                    format!("Missing {}", missing.iter().collect::<String>()),
                ));
            }
            if kind == NodeKind::Error {
                self.diagnostics.push(Diagnostic::error(
                    "syntax.invalid-statement",
                    &self.source.id,
                    header,
                    "Expected a declaration or rule",
                ));
            }
            nodes.push(Node {
                kind,
                range: TextRange::new(start, node_end),
                header,
                children,
                missing,
                block: block_start.map(|start| TextRange::new(start, node_end)),
            });
            if self.cursor == first {
                self.cursor += 1;
            }
        }
        nodes
    }
}

/// Parse a bounded standard CSS value with Raffia, without exposing its AST.
pub fn valid_css_value(value: &str) -> bool {
    let mut parser = raffia::ParserBuilder::new(value)
        .syntax(raffia::Syntax::Css)
        .build();
    parser.parse::<raffia::ast::ComponentValues>().is_ok() && parser.recoverable_errors().is_empty()
}

fn validate_fragments(result: &mut ParseResult) {
    let mut diagnostics = result.diagnostics.to_vec();
    for node in result.tree.descendants() {
        if !node.missing.is_empty() {
            continue;
        }
        let raw = node.header_text(&result.source);
        let invalid_shape = match node.kind {
            NodeKind::Injection => {
                node.block.is_none()
                    || result.injection(node).is_none_or(|i| {
                        i.name.is_empty()
                            || !i.name.starts_with(|c: char| {
                                c.is_alphabetic() || matches!(c, '_' | '-' | '\\')
                            })
                    })
            }
            NodeKind::Vars => node.block.is_none(),
            NodeKind::Call => result.call(node).is_none_or(|c| c.name.is_empty()),
            _ => false,
        };
        if invalid_shape {
            diagnostics.push(Diagnostic::error(
                "syntax.invalid-construct",
                &result.source.id,
                node.header,
                "Incomplete XCSS construct",
            ));
            continue;
        }
        let mut parser = raffia::ParserBuilder::new(raw)
            .syntax(raffia::Syntax::Css)
            .build();
        let error = match node.kind {
            NodeKind::Rule if !raw.trim_start().starts_with(|c: char| c.is_ascii_digit()) => {
                parser.parse::<raffia::ast::SelectorList>().err()
            }
            NodeKind::Declaration
                if !raw.contains('$')
                    && result.declaration(node).is_some_and(|d| {
                        !d.value.descendants().iter().any(|v| {
                            matches!(
                                v.kind,
                                ast::ValueKind::Operator(_) | ast::ValueKind::Group(_)
                            )
                        })
                    }) =>
            {
                parser.parse::<raffia::ast::Declaration>().err()
            }
            _ => None,
        };
        if let Some(error) = error {
            // Fragment offsets are relative to the original header, never transformed text.
            let range = TextRange::new(
                node.header.start + error.span.start.min(raw.len()),
                node.header.start + error.span.end.min(raw.len()),
            );
            diagnostics.push(Diagnostic::error(
                "syntax.css",
                &result.source.id,
                range,
                format!("Invalid CSS fragment: {:?}", error.kind),
            ));
        }
    }
    result.diagnostics = diagnostics.into();
}

#[cfg(test)]
mod tests {
    use super::*;
    use xcss_source::FileId;
    #[test]
    fn preserves_tokens_and_recovers() {
        let source = Arc::new(SourceFile::new(
            FileId("a".into()),
            1,
            "/*é*/ .a { color: 'bad\n; width: 2px; } .b { @:missing(; height: 1px; }",
        ));
        let parsed = parse(source.clone());
        assert_eq!(
            parsed
                .tokens
                .iter()
                .map(|t| &source.text()[t.range.start..t.range.end])
                .collect::<String>(),
            source.text()
        );
        assert!(
            parsed
                .tree
                .descendants()
                .any(|n| n.header_text(&source).starts_with("height:"))
        );
        assert!(!parsed.diagnostics.is_empty());
    }
}
