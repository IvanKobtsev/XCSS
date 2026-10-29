//! Typed views and owned value components, all using original-source ranges.
use crate::{Node, NodeKind, ParseResult, Token, TokenKind};
use xcss_source::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Value {
    pub range: TextRange,
    pub kind: ValueKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Literal,
    CustomProperty(String),
    Parameter(String),
    Operator(char),
    Function { name: String, arguments: Vec<Value> },
    Group(Vec<Value>),
    Sequence(Vec<Value>),
}
/// Arithmetic precedence is explicit without evaluating CSS runtime expressions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArithmeticExpression {
    pub range: TextRange,
    pub kind: ArithmeticKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArithmeticKind {
    Operand(Value),
    Unary {
        operator: char,
        operand: Box<ArithmeticExpression>,
    },
    Binary {
        operator: char,
        left: Box<ArithmeticExpression>,
        right: Box<ArithmeticExpression>,
    },
}
impl Value {
    pub fn arithmetic(&self) -> Option<ArithmeticExpression> {
        let (ValueKind::Sequence(parts) | ValueKind::Group(parts)) = &self.kind else {
            return Some(ArithmeticExpression {
                range: self.range,
                kind: ArithmeticKind::Operand(self.clone()),
            });
        };
        fn expression(
            parts: &[Value],
            cursor: &mut usize,
            minimum: u8,
        ) -> Option<ArithmeticExpression> {
            let head = parts.get(*cursor)?;
            *cursor += 1;
            let mut left = if let ValueKind::Operator(operator @ ('+' | '-')) = head.kind {
                let operand = expression(parts, cursor, 3)?;
                ArithmeticExpression {
                    range: TextRange::new(head.range.start, operand.range.end),
                    kind: ArithmeticKind::Unary {
                        operator,
                        operand: Box::new(operand),
                    },
                }
            } else if matches!(head.kind, ValueKind::Operator(_)) {
                return None;
            } else {
                head.arithmetic()?
            };
            while let Some(Value {
                kind: ValueKind::Operator(operator),
                ..
            }) = parts.get(*cursor)
            {
                let precedence = if matches!(operator, '*' | '/') { 2 } else { 1 };
                if precedence < minimum {
                    break;
                }
                let operator = *operator;
                *cursor += 1;
                let right = expression(parts, cursor, precedence + 1)?;
                left = ArithmeticExpression {
                    range: TextRange::new(left.range.start, right.range.end),
                    kind: ArithmeticKind::Binary {
                        operator,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                };
            }
            Some(left)
        }
        let mut cursor = 0;
        let tree = expression(parts, &mut cursor, 0)?;
        (cursor == parts.len()).then_some(tree)
    }
    pub fn descendants(&self) -> Vec<&Value> {
        let mut values = vec![self];
        match &self.kind {
            ValueKind::Function { arguments, .. }
            | ValueKind::Group(arguments)
            | ValueKind::Sequence(arguments) => {
                for value in arguments {
                    values.extend(value.descendants());
                }
            }
            _ => {}
        }
        values
    }
}
#[derive(Clone, Debug)]
pub struct Declaration<'a> {
    pub property: &'a str,
    pub property_range: TextRange,
    pub value: Value,
}
#[derive(Clone, Debug)]
pub struct Injection<'a> {
    pub name: &'a str,
    pub name_range: TextRange,
    pub parameters: Vec<Parameter<'a>>,
}
#[derive(Clone, Debug)]
pub struct Parameter<'a> {
    pub name: &'a str,
    pub range: TextRange,
    pub annotation: Option<&'a str>,
    pub default: Option<&'a str>,
}
#[derive(Clone, Debug)]
pub struct Variable<'a> {
    pub name: &'a str,
    pub range: TextRange,
    pub inherits: bool,
    pub annotation: Option<&'a str>,
    pub initial: Option<&'a str>,
}
#[derive(Clone, Debug)]
pub struct Call<'a> {
    pub name: &'a str,
    pub name_range: TextRange,
    pub arguments: Vec<Value>,
}
#[derive(Clone, Debug)]
pub struct Import<'a> {
    pub namespace: Option<&'a str>,
    pub names: Vec<(&'a str, TextRange)>,
    pub specifier: &'a str,
}
#[derive(Clone, Debug)]
pub struct Composition<'a> {
    pub names: Vec<(&'a str, TextRange)>,
    pub source: CompositionSource<'a>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositionSource<'a> {
    Local,
    Global,
    Module(&'a str),
}
#[derive(Clone, Debug)]
pub struct Keyframes<'a> {
    pub keyword: &'a str,
    pub name: &'a str,
    pub literal: &'a str,
    pub name_range: TextRange,
    pub global: bool,
    pub scope_switch: bool,
}

impl ParseResult {
    pub fn keyframes<'a>(&'a self, node: &Node) -> Option<Keyframes<'a>> {
        if node.kind != NodeKind::AtRule {
            return None;
        }
        let tokens = self.significant(node.header);
        let keyword = self.token_text(tokens.get(1)?);
        if !keyword.eq_ignore_ascii_case("keyframes")
            && !keyword.eq_ignore_ascii_case("-webkit-keyframes")
        {
            return None;
        }
        let switched = self.token_text(tokens.get(2)?) == ":";
        let global = switched
            && self
                .token_text(tokens.get(3)?)
                .eq_ignore_ascii_case("global");
        let token = *tokens.get(if switched { 5 } else { 2 })?;
        let literal = self.token_text(token);
        Some(Keyframes {
            keyword,
            name: literal.trim_matches(['\'', '"']),
            literal,
            name_range: token.range,
            global,
            scope_switch: switched,
        })
    }
    pub fn composition<'a>(&'a self, node: &Node) -> Option<Composition<'a>> {
        let declaration = self.declaration(node)?;
        if !declaration.property.eq_ignore_ascii_case("composes") {
            return None;
        }
        let tokens = self.significant(declaration.value.range);
        let from = tokens
            .iter()
            .position(|t| t.kind == TokenKind::Word && self.token_text(t) == "from");
        let end = from.unwrap_or(tokens.len());
        let names = tokens[..end]
            .iter()
            .filter(|t| t.kind == TokenKind::Word)
            .map(|t| (self.token_text(t), t.range))
            .collect();
        let source = match from.and_then(|i| tokens.get(i + 1)) {
            Some(token) if self.token_text(token) == "global" => CompositionSource::Global,
            Some(token) => {
                CompositionSource::Module(self.token_text(token).trim_matches(['\'', '"']))
            }
            None => CompositionSource::Local,
        };
        Some(Composition { names, source })
    }
    fn significant(&self, range: TextRange) -> Vec<&Token> {
        self.tokens
            .iter()
            .filter(|t| {
                t.range.start >= range.start
                    && t.range.end <= range.end
                    && !matches!(t.kind, TokenKind::Whitespace | TokenKind::Comment)
            })
            .collect()
    }
    pub fn declaration<'a>(&'a self, node: &Node) -> Option<Declaration<'a>> {
        if node.kind != NodeKind::Declaration {
            return None;
        }
        let tokens = self.significant(node.header);
        let colon = tokens.iter().position(|t| self.token_text(t) == ":")?;
        let range = TextRange::new(
            tokens.first()?.range.start,
            tokens.get(colon.checked_sub(1)?)?.range.end,
        );
        let value_range = TextRange::new(tokens[colon].range.end, node.header.end);
        Some(Declaration {
            property: self.slice(range),
            property_range: range,
            value: self.value(value_range),
        })
    }
    pub fn injection<'a>(&'a self, node: &Node) -> Option<Injection<'a>> {
        if node.kind != NodeKind::Injection {
            return None;
        }
        let tokens = self.significant(node.header);
        let name = *tokens.get(2)?;
        let mut parameters = Vec::new();
        if let Some(open) = tokens.iter().position(|t| self.token_text(t) == "(") {
            let end = tokens
                .iter()
                .enumerate()
                .rfind(|(i, t)| *i > open && self.token_text(t) == ")")
                .map(|(_, t)| t.range.start)
                .unwrap_or(node.header.end);
            for range in self.split_ranges(TextRange::new(tokens[open].range.end, end), ",") {
                let parts = self.significant(range);
                let Some(first) = parts.first() else {
                    continue;
                };
                let end = if self.token_text(first) == "$" {
                    parts.get(1).map(|t| t.range.end).unwrap_or(first.range.end)
                } else {
                    first.range.end
                };
                let name_range = TextRange::new(first.range.start, end);
                let name = self.slice(name_range);
                let (annotation, default) = self.annotation_initial(TextRange::new(end, range.end));
                parameters.push(Parameter {
                    name,
                    range: name_range,
                    annotation,
                    default,
                });
            }
        }
        Some(Injection {
            name: self.token_text(name),
            name_range: name.range,
            parameters,
        })
    }
    pub fn variable<'a>(&'a self, node: &Node) -> Option<Variable<'a>> {
        if node.kind != NodeKind::Variable {
            return None;
        }
        let tokens = self.significant(node.header);
        let inherits = self.token_text(tokens.first()?) != "local";
        let name = *tokens.get(usize::from(!inherits))?;
        let (annotation, initial) =
            self.annotation_initial(TextRange::new(name.range.end, node.header.end));
        Some(Variable {
            name: self.token_text(name),
            range: name.range,
            inherits,
            annotation,
            initial,
        })
    }
    pub fn call<'a>(&'a self, node: &Node) -> Option<Call<'a>> {
        if node.kind != NodeKind::Call {
            return None;
        }
        let tokens = self.significant(node.header);
        let start = tokens.get(2)?.range.start;
        let open = tokens.iter().position(|t| self.token_text(t) == "(");
        let end = open
            .map(|i| tokens[i].range.start)
            .unwrap_or(node.header.end);
        let name = self.source.text()[start..end].trim();
        let arguments = if let Some(open) = open {
            let end = tokens
                .iter()
                .enumerate()
                .rfind(|(i, t)| *i > open && self.token_text(t) == ")")
                .map(|(_, t)| t.range.start)
                .unwrap_or(node.header.end);
            self.split_ranges(TextRange::new(tokens[open].range.end, end), ",")
                .into_iter()
                .filter(|r| !self.slice(*r).trim().is_empty())
                .map(|r| self.value(r))
                .collect()
        } else {
            Vec::new()
        };
        Some(Call {
            name,
            name_range: TextRange::new(start, start + name.len()),
            arguments,
        })
    }
    pub fn injection_import<'a>(&'a self, node: &Node) -> Option<Import<'a>> {
        if node.kind != NodeKind::Import {
            return None;
        }
        let tokens = self.significant(node.header);
        let from = tokens
            .iter()
            .position(|t| t.kind == TokenKind::Word && self.token_text(t) == "from")?;
        let specifier = self
            .token_text(tokens.get(from + 1)?)
            .trim_matches(['\'', '"']);
        let binding = *tokens.get(2)?;
        let named = self.token_text(binding) == "{";
        let names = if named {
            tokens[3..from]
                .iter()
                .filter(|t| t.kind == TokenKind::Word)
                .map(|t| (self.token_text(t), t.range))
                .collect()
        } else {
            Vec::new()
        };
        Some(Import {
            namespace: (!named).then(|| self.token_text(binding)),
            names,
            specifier,
        })
    }
    pub fn value(&self, range: TextRange) -> Value {
        let tokens = self.significant(range);
        let mut cursor = 0;
        let children = self.components(&tokens, &mut cursor, 0);
        Value {
            range,
            kind: ValueKind::Sequence(children),
        }
    }
    fn components(&self, tokens: &[&Token], cursor: &mut usize, depth: usize) -> Vec<Value> {
        let mut values = Vec::new();
        if depth > 128 {
            *cursor = tokens.len();
            return values;
        }
        while let Some(token) = tokens.get(*cursor) {
            let text = self.token_text(token);
            if text == ")" || text == "]" {
                break;
            }
            let start = token.range.start;
            let mut end = token.range.end;
            let kind = if text == "(" || text == "[" {
                *cursor += 1;
                let children = self.components(tokens, cursor, depth + 1);
                if let Some(close) = tokens.get(*cursor) {
                    end = close.range.end;
                    *cursor += 1;
                }
                ValueKind::Group(children)
            } else if token.kind == TokenKind::Word
                && tokens.get(*cursor + 1).is_some_and(|next| {
                    self.token_text(next) == "(" && next.range.start == token.range.end
                })
            {
                *cursor += 2;
                let arguments = self.components(tokens, cursor, depth + 1);
                if let Some(close) = tokens.get(*cursor) {
                    end = close.range.end;
                    *cursor += 1;
                }
                ValueKind::Function {
                    name: decode_identifier(text),
                    arguments,
                }
            } else if text == "$"
                && tokens
                    .get(*cursor + 1)
                    .is_some_and(|next| next.kind == TokenKind::Word)
            {
                *cursor += 1;
                end = tokens[*cursor].range.end;
                let name = format!("${}", decode_identifier(self.token_text(tokens[*cursor])));
                *cursor += 1;
                ValueKind::Parameter(name)
            } else {
                *cursor += 1;
                if text.starts_with("--") {
                    ValueKind::CustomProperty(decode_identifier(text))
                } else if matches!(text, "+" | "-" | "*" | "/") {
                    ValueKind::Operator(text.chars().next().unwrap())
                } else {
                    ValueKind::Literal
                }
            };
            values.push(Value {
                range: TextRange::new(start, end),
                kind,
            });
        }
        values
    }
    pub fn split_ranges(&self, range: TextRange, delimiter: &str) -> Vec<TextRange> {
        let mut ranges = Vec::new();
        let mut start = range.start;
        let mut depth = 0usize;
        for token in self.significant(range) {
            match self.token_text(token) {
                "(" | "[" => depth += 1,
                ")" | "]" => depth = depth.saturating_sub(1),
                value if value == delimiter && depth == 0 => {
                    ranges.push(TextRange::new(start, token.range.start));
                    start = token.range.end;
                }
                _ => {}
            }
        }
        ranges.push(TextRange::new(start, range.end));
        ranges
    }
    pub fn slice(&self, range: TextRange) -> &str {
        &self.source.text()[range.start..range.end]
    }
    pub fn token_text(&self, token: &Token) -> &str {
        self.slice(token.range)
    }
    fn annotation_initial(&self, range: TextRange) -> (Option<&str>, Option<&str>) {
        let tokens = self.significant(range);
        let annotation = if tokens.first().is_some_and(|t| self.token_text(t) == ":") {
            tokens.get(1).map(|t| self.token_text(t))
        } else {
            None
        };
        let initial = tokens
            .iter()
            .position(|t| self.token_text(t) == "=")
            .map(|i| self.source.text()[tokens[i].range.end..range.end].trim());
        (annotation, initial)
    }
}

/// CSS escapes are normalized only for symbol identity; source ranges stay raw.
pub fn decode_identifier(raw: &str) -> String {
    let mut result = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        let mut hex = String::new();
        while hex.len() < 6 && chars.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
            hex.push(chars.next().unwrap());
        }
        if !hex.is_empty() {
            let value = u32::from_str_radix(&hex, 16).unwrap_or(0);
            result.push(
                char::from_u32(value)
                    .filter(|c| *c != '\0')
                    .unwrap_or('\u{fffd}'),
            );
            if chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
        } else if let Some(next) = chars.next() {
            result.push(next);
        }
    }
    result
}
