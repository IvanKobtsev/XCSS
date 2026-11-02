//! Source-to-source formatting. No resolution, evaluation, code generation or host I/O.
use malva::{
    Syntax,
    config::{BlockSelectorLineBreak, FormatOptions as CssOptions, Quotes},
    format_text,
};
use std::sync::Arc;
use xcss_source::{Diagnostic, FileId, Severity, SourceFile, TextRange};
use xcss_syntax::{Node, NodeKind, ParseResult, Token, TokenKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EndOfLine {
    #[default]
    Lf,
    Crlf,
}

#[derive(Clone, Debug)]
pub struct FormatOptions {
    pub tab_width: usize,
    pub use_tabs: bool,
    pub print_width: usize,
    pub end_of_line: EndOfLine,
}
impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            tab_width: 2,
            use_tabs: false,
            print_width: 80,
            end_of_line: EndOfLine::Lf,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FormatResult {
    pub text: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn format(source: &str, options: &FormatOptions) -> FormatResult {
    let parse = parse(source);
    let diagnostics = errors(&parse);
    if !diagnostics.is_empty() {
        return FormatResult {
            text: None,
            diagnostics,
        };
    }
    if options.tab_width == 0
        || options.tab_width > 16
        || options.print_width == 0
        || options.print_width > 1000
    {
        return failure(
            "format.invalid-options",
            "tabWidth must be 1–16 and printWidth 1–1000",
        );
    }
    let printer = Printer {
        parse: &parse,
        options,
    };
    let mut text = printer.sequence(&parse.tree.children, 0, source.len(), 0);
    if !text.is_empty() {
        text.truncate(text.trim_end_matches(['\r', '\n']).len());
        text.push_str(printer.eol());
    }
    let mut diagnostics = errors(&self::parse(&text));
    if !diagnostics.is_empty() {
        return failure(
            "format.invalid-output",
            "Formatter output failed syntax validation; source was left unchanged",
        );
    }
    // Comments are opaque source data. Never silently drop, duplicate or reorder them.
    let output = self::parse(&text);
    let comments = |p: &ParseResult| {
        p.tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Comment)
            .map(|t| p.source.text()[t.range.start..t.range.end].to_owned())
            .collect::<Vec<_>>()
    };
    if comments(&parse) != comments(&output) {
        return failure(
            "format.comment-preservation",
            "Formatter could not preserve comments; source was left unchanged",
        );
    }
    FormatResult {
        text: Some(text),
        diagnostics: std::mem::take(&mut diagnostics),
    }
}
fn parse(source: &str) -> ParseResult {
    xcss_syntax::parse(Arc::new(SourceFile::new(
        FileId("format.xcss".into()),
        0,
        source,
    )))
}
fn errors(parse: &ParseResult) -> Vec<Diagnostic> {
    let mut errors: Vec<_> = parse
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .cloned()
        .collect();
    if errors.is_empty()
        && parse
            .tree
            .descendants()
            .any(|n| n.kind == NodeKind::Error || !n.missing.is_empty())
    {
        errors.push(Diagnostic::error(
            "format.invalid-syntax",
            &parse.source.id,
            parse.tree.range,
            "Incomplete XCSS syntax",
        ));
    }
    errors
}
fn failure(code: &'static str, message: &str) -> FormatResult {
    FormatResult {
        text: None,
        diagnostics: vec![Diagnostic::error(
            code,
            &FileId("format.xcss".into()),
            TextRange::default(),
            message,
        )],
    }
}

struct Printer<'a> {
    parse: &'a ParseResult,
    options: &'a FormatOptions,
}
impl Printer<'_> {
    fn slice(&self, start: usize, end: usize) -> &str {
        &self.parse.source.text()[start..end]
    }
    fn eol(&self) -> &'static str {
        match self.options.end_of_line {
            EndOfLine::Lf => "\n",
            EndOfLine::Crlf => "\r\n",
        }
    }
    fn indent(&self, depth: usize) -> String {
        if self.options.use_tabs {
            "\t".repeat(depth)
        } else {
            " ".repeat(depth * self.options.tab_width)
        }
    }
    fn css_options(&self, depth: usize) -> CssOptions {
        let mut options = CssOptions::default();
        options.layout.indent_width = self.options.tab_width;
        options.layout.use_tabs = self.options.use_tabs;
        options.layout.print_width = self
            .options
            .print_width
            .saturating_sub(depth * self.options.tab_width)
            .max(1);
        options.language.block_selector_linebreak = BlockSelectorLineBreak::Always;
        options.language.quotes = Quotes::PreferDouble;
        options.language.ignore_comment_directive = "prettier-ignore".into();
        options.language.align_comments = false;
        options
    }
    /// Indent generated CSS; opaque source is never passed through this function.
    fn generated(&self, text: &str, depth: usize) -> String {
        text.trim_end_matches(['\r', '\n'])
            .split('\n')
            .map(|line| {
                if line.is_empty() {
                    String::new()
                } else {
                    format!("{}{}", self.indent(depth), line.trim_end_matches('\r'))
                }
            })
            .collect::<Vec<_>>()
            .join(self.eol())
    }
    fn standard(&self, node: &Node) -> bool {
        if !matches!(
            node.kind,
            NodeKind::Rule | NodeKind::Declaration | NodeKind::AtRule | NodeKind::Import
        ) {
            return false;
        }
        if self.slice(node.header.start, node.header.end).contains('$') {
            return false;
        }
        if self.parse.tokens.iter().any(|t| {
            t.kind == TokenKind::Comment
                && t.range.start >= node.range.start
                && t.range.end <= node.range.end
        }) {
            return false;
        }
        if node.kind == NodeKind::AtRule
            && !known_at_rule(self.slice(node.header.start, node.header.end))
        {
            return false;
        }
        if node.kind == NodeKind::Import && self.parse.injection_import(node).is_some() {
            return false;
        }
        if let Some(decl) = self.parse.declaration(node)
            && xcss_expression(&decl.value)
        {
            return false;
        }
        node.children.iter().all(|n| self.standard(n))
    }
    fn sequence(&self, nodes: &[Node], start: usize, end: usize, depth: usize) -> String {
        let mut out = String::new();
        let mut cursor = start;
        let mut ignored = false;
        for node in nodes {
            self.gap(&mut out, cursor, node.range.start, depth, &mut ignored);
            if !out.is_empty() && !out.ends_with('\n') {
                out.push_str(self.eol());
            }
            if ignored {
                out.push_str(&self.indent(depth));
                out.push_str(self.slice(node.range.start, node.range.end));
            } else {
                out.push_str(&self.node(node, depth));
            }
            ignored = false;
            cursor = node.range.end;
        }
        self.gap(&mut out, cursor, end, depth, &mut ignored);
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(self.eol());
        }
        out
    }
    fn gap(&self, out: &mut String, start: usize, end: usize, depth: usize, ignored: &mut bool) {
        let mut cursor = start;
        for token in self.parse.tokens.iter().filter(|t| {
            t.range.start >= start && t.range.end <= end && t.kind == TokenKind::Comment
        }) {
            self.space(out, self.slice(cursor, token.range.start), depth);
            let comment = self.slice(token.range.start, token.range.end);
            out.push_str(comment);
            *ignored = comment
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .trim()
                == "prettier-ignore";
            cursor = token.range.end;
        }
        let gap = self.slice(cursor, end);
        if gap.contains('\n') && !out.is_empty() {
            if !out.ends_with('\n') {
                out.push_str(self.eol());
            }
            if gap.bytes().filter(|c| *c == b'\n').count() > 1
                && !out.ends_with(&format!("{}{}", self.eol(), self.eol()))
            {
                out.push_str(self.eol());
            }
        }
    }
    fn space(&self, out: &mut String, gap: &str, depth: usize) {
        if !out.is_empty() && !gap.contains('\n') {
            out.push(' ');
        } else {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push_str(self.eol());
            }
            if gap.bytes().filter(|c| *c == b'\n').count() > 1
                && !out.is_empty()
                && !out.ends_with(&format!("{}{}", self.eol(), self.eol()))
            {
                out.push_str(self.eol());
            }
            out.push_str(&self.indent(depth));
        }
    }
    fn node(&self, node: &Node, depth: usize) -> String {
        if node.kind == NodeKind::Raw {
            let block = node.block.expect("validated raw block");
            return format!(
                "{}@raw {}",
                self.indent(depth),
                self.slice(block.start, node.range.end)
            );
        }
        let options = self.css_options(depth);
        if node.block.is_none() && self.standard(node) {
            let text = self.slice(node.range.start, node.range.end);
            if node.kind == NodeKind::Declaration {
                let mut inner = options.clone();
                inner.layout.print_width += self.options.tab_width;
                if let Ok(formatted) = format_text(&format!("a{{{text}}}"), Syntax::Css, &inner)
                    && let Some(body) = formatted
                        .strip_prefix("a {\n")
                        .and_then(|s| s.strip_suffix("}\n"))
                {
                    let unit = self.indent(1);
                    let body = body
                        .lines()
                        .map(|l| l.strip_prefix(&unit).unwrap_or(l))
                        .collect::<Vec<_>>()
                        .join("\n");
                    return self.generated(&body, depth);
                }
            } else if let Ok(formatted) = format_text(text, Syntax::Css, &options) {
                return self.generated(&formatted, depth);
            }
        }
        if node.kind == NodeKind::AtRule {
            // Unknown at-rule headers can define arbitrary whitespace-sensitive grammars.
            let header = self.slice(node.header.start, node.header.end).trim();
            if !known_at_rule(header) {
                return format!(
                    "{}{}",
                    self.indent(depth),
                    self.slice(node.range.start, node.range.end)
                );
            }
        }
        let mut header = self.header(node, depth);
        if let Some(block) = node.block {
            header.push_str(" {");
            header.push_str(self.eol());
            header.push_str(&self.sequence(
                &node.children,
                block.start + 1,
                block.end - 1,
                depth + 1,
            ));
            header.push_str(&self.indent(depth));
            header.push('}');
        } else {
            header.push(';');
        }
        header
    }
    fn header(&self, node: &Node, depth: usize) -> String {
        let text = self.slice(node.header.start, node.header.end).trim();
        if matches!(node.kind, NodeKind::Rule | NodeKind::AtRule)
            && let Ok(formatted) = format_text(
                &format!("{text} {{}}"),
                Syntax::Css,
                &self.css_options(depth),
            )
            && let Some(header) = formatted
                .strip_suffix(" {\n}\n")
                .or_else(|| formatted.strip_suffix(" {}\n"))
        {
            return self.generated(header, depth);
        }
        let tokens: Vec<_> = self
            .parse
            .tokens
            .iter()
            .filter(|t| {
                t.range.start >= node.header.start
                    && t.range.end <= node.header.end
                    && t.kind != TokenKind::Whitespace
            })
            .collect();
        let (doc, _) = self.tokens(&tokens, 0, None, node.kind == NodeKind::Rule);
        let mut out = self.indent(depth);
        doc.render(&mut out, depth, self, false);
        out
    }
    fn tokens(
        &self,
        tokens: &[&Token],
        mut index: usize,
        end: Option<&str>,
        selector: bool,
    ) -> (Doc, usize) {
        let mut docs = Vec::new();
        let mut previous: Option<&Token> = None;
        while index < tokens.len() {
            let token = tokens[index];
            let text = self.slice(token.range.start, token.range.end);
            if Some(text) == end {
                break;
            }
            if text == "(" || text == "[" {
                if let Some(prev) = previous {
                    let gap = self.slice(prev.range.end, token.range.start);
                    if !gap.is_empty() && self.slice(prev.range.start, prev.range.end) != ":" {
                        docs.push(Doc::Text(" ".into()));
                    }
                }
                if text == "("
                    && previous.is_some_and(|prev| {
                        self.slice(prev.range.start, prev.range.end)
                            .eq_ignore_ascii_case("url")
                    })
                {
                    let mut next = index + 1;
                    let mut nesting = 1;
                    while next < tokens.len() {
                        match self.slice(tokens[next].range.start, tokens[next].range.end) {
                            "(" => nesting += 1,
                            ")" => nesting -= 1,
                            _ => (),
                        }
                        if nesting == 0 {
                            break;
                        }
                        next += 1;
                    }
                    if next < tokens.len() {
                        docs.push(Doc::Text(
                            self.slice(token.range.start, tokens[next].range.end).into(),
                        ));
                        previous = Some(tokens[next]);
                        index = next + 1;
                        continue;
                    }
                }
                let close = if text == "(" { ")" } else { "]" };
                let (inner, next) = self.tokens(tokens, index + 1, Some(close), selector);
                docs.push(Doc::Group(Box::new(Doc::Concat(vec![
                    Doc::Text(text.into()),
                    Doc::Indent(Box::new(Doc::Concat(vec![Doc::Soft, inner]))),
                    Doc::Soft,
                    Doc::Text(close.into()),
                ]))));
                index = next;
            } else {
                if let Some(prev) = previous {
                    let p = self.slice(prev.range.start, prev.range.end);
                    let gap = self.slice(prev.range.end, token.range.start);
                    if p == "," {
                        docs.push(if selector && end.is_none() {
                            Doc::Hard
                        } else {
                            Doc::Line
                        });
                    } else {
                        let operator = matches!(text, "=" | "+" | "*" | "/")
                            || matches!(p, "=" | "+" | "*" | "/")
                            || (text == "-" || p == "-") && !gap.is_empty();
                        let colon = p == ":"
                            && !selector
                            && !(docs.len() <= 2 && node_call_prefix(tokens, self));
                        let gap = !gap.is_empty()
                            && !matches!(text, "," | ":" | "." | ")" | "]")
                            && !matches!(p, "@" | "$" | ".");
                        if operator || colon || gap {
                            docs.push(Doc::Text(" ".into()));
                        }
                    }
                }
                let normalized = if token.kind == TokenKind::String
                    || text.starts_with(|c: char| c.is_ascii_digit())
                {
                    format_text(
                        &format!("a{{v:{text}}}"),
                        Syntax::Css,
                        &CssOptions {
                            language: malva::config::LanguageOptions {
                                quotes: Quotes::PreferDouble,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    )
                    .ok()
                    .and_then(|s| {
                        s.split_once("v: ")
                            .and_then(|(_, value)| value.strip_suffix(";\n}\n"))
                            .map(str::to_owned)
                    })
                } else {
                    None
                };
                docs.push(Doc::Text(normalized.unwrap_or_else(|| text.into())));
            }
            previous = Some(tokens[index]);
            index += 1;
        }
        (Doc::Group(Box::new(Doc::Concat(docs))), index)
    }
}
fn node_call_prefix(tokens: &[&Token], printer: &Printer<'_>) -> bool {
    tokens.len() > 1
        && printer.slice(tokens[0].range.start, tokens[0].range.end) == "@"
        && printer.slice(tokens[1].range.start, tokens[1].range.end) == ":"
}
fn known_at_rule(header: &str) -> bool {
    [
        "@media",
        "@supports",
        "@container",
        "@layer",
        "@keyframes",
        "@-webkit-keyframes",
        "@font-face",
        "@scope",
        "@starting-style",
        "@import",
        "@namespace",
        "@charset",
        "@property",
        "@page",
    ]
    .iter()
    .any(|p| {
        header
            .strip_prefix(p)
            .is_some_and(|s| s.is_empty() || s.starts_with(|c: char| c.is_whitespace() || c == '('))
    })
}
fn xcss_expression(value: &xcss_syntax::ast::Value) -> bool {
    use xcss_syntax::ast::ValueKind;
    match &value.kind {
        ValueKind::Operator(_) | ValueKind::Group(_) => true,
        ValueKind::Sequence(values) => values.iter().any(xcss_expression),
        // Native CSS function internals are handled by the CSS printer, including URLs.
        _ => false,
    }
}

enum Doc {
    Text(String),
    Concat(Vec<Doc>),
    Group(Box<Doc>),
    Indent(Box<Doc>),
    Line,
    Soft,
    Hard,
}
impl Doc {
    fn flat_width(&self) -> usize {
        match self {
            Self::Text(s) => s.chars().count(),
            Self::Concat(parts) => parts.iter().map(Self::flat_width).sum(),
            Self::Group(d) | Self::Indent(d) => d.flat_width(),
            Self::Line => 1,
            Self::Soft => 0,
            Self::Hard => 1_000_000,
        }
    }
    fn render(&self, out: &mut String, depth: usize, p: &Printer<'_>, flat: bool) {
        match self {
            Self::Text(s) => out.push_str(s),
            Self::Concat(parts) => {
                for part in parts {
                    part.render(out, depth, p, flat);
                }
            }
            Self::Group(doc) => {
                let column = out
                    .rsplit('\n')
                    .next()
                    .unwrap_or("")
                    .chars()
                    .map(|c| if c == '\t' { p.options.tab_width } else { 1 })
                    .sum::<usize>();
                doc.render(
                    out,
                    depth,
                    p,
                    flat || column + doc.flat_width() <= p.options.print_width,
                );
            }
            Self::Indent(doc) => doc.render(out, depth + 1, p, flat),
            Self::Hard => {
                out.push_str(p.eol());
                out.push_str(&p.indent(depth));
            }
            Self::Line | Self::Soft if !flat => {
                out.push_str(p.eol());
                out.push_str(&p.indent(depth));
            }
            Self::Line => out.push(' '),
            Self::Soft => (),
        }
    }
}
