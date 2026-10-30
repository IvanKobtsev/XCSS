//! Selector classification and specificity over Raffia's CSS AST.
use raffia::{ParserBuilder, Spanned, ast::*};
use xcss_source::TextRange;

/// Ordered as CSS compares specificity, with no decimal carry between columns.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct Specificity {
    pub ids: u32,
    pub classes: u32,
    pub types: u32,
}
impl std::ops::AddAssign for Specificity {
    fn add_assign(&mut self, rhs: Self) {
        self.ids = self.ids.saturating_add(rhs.ids);
        self.classes = self.classes.saturating_add(rhs.classes);
        self.types = self.types.saturating_add(rhs.types);
    }
}
const CLASS: Specificity = Specificity {
    ids: 0,
    classes: 1,
    types: 0,
};
const TYPE: Specificity = Specificity {
    ids: 0,
    classes: 0,
    types: 1,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectorKind {
    Class,
    Id,
    Tag,
    Attribute,
    PseudoClass,
    PseudoElement,
    Universal,
    Nesting,
    Combinator,
    Namespace,
}
impl SelectorKind {
    pub fn token_type(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Id => "selectorId",
            Self::Tag => "selectorTag",
            Self::Attribute => "selectorAttribute",
            Self::PseudoClass => "pseudoClass",
            Self::PseudoElement => "pseudoElement",
            Self::Universal => "selectorUniversal",
            Self::Nesting => "selectorNesting",
            Self::Combinator => "selectorCombinator",
            Self::Namespace => "selectorNamespace",
        }
    }
}
#[derive(Clone, Debug)]
pub struct SelectorPart {
    pub kind: SelectorKind,
    pub range: TextRange,
}
#[derive(Clone, Debug)]
pub struct SelectorAnalysis {
    pub range: TextRange,
    pub parts: Vec<SelectorPart>,
    pub specificity: Option<Specificity>,
    pub reason: Option<String>,
}

/// Parent specificity is the maximum of the associated parent selector list.
/// None is a top-level selector; Some(None) means an unresolved parent.
pub fn analyze_selectors(
    text: &str,
    offset: usize,
    parent: Option<Option<Specificity>>,
) -> Vec<SelectorAnalysis> {
    let mut result = vec![];
    let mut start = 0;
    let mut depth = 0usize;
    let mut quote = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            chars.next();
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        if c == '/' && chars.peek().is_some_and(|(_, c)| *c == '*') {
            chars.next();
            while let Some((_, c)) = chars.next() {
                if c == '*' && chars.peek().is_some_and(|(_, c)| *c == '/') {
                    chars.next();
                    break;
                }
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                result.extend(analyze_member(&text[start..i], offset + start, parent));
                start = i + 1;
            }
            _ => {}
        }
    }
    result.extend(analyze_member(&text[start..], offset + start, parent));
    result
}
fn analyze_member(
    text: &str,
    offset: usize,
    parent: Option<Option<Specificity>>,
) -> Vec<SelectorAnalysis> {
    let mut parser = ParserBuilder::new(text).build();
    let parsed = parser.parse::<SelectorList>();
    let Ok(list) = parsed else {
        return vec![SelectorAnalysis {
            range: TextRange::new(offset, offset + text.len()),
            parts: vec![],
            specificity: None,
            reason: Some("Invalid or incomplete selector".into()),
        }];
    };
    let tail = &text[list.span.end..];
    let recovered = !parser.recoverable_errors().is_empty() || !only_trivia(tail);
    list.selectors
        .iter()
        .map(|selector| {
            let mut visitor = Visitor {
                text,
                offset,
                parent,
                parts: vec![],
                reason: None,
                nesting: false,
            };
            let mut specificity = visitor.complex(selector);
            if parent.is_some() && !visitor.nesting {
                visitor.add_parent(&mut specificity);
            }
            if recovered {
                visitor.reason = Some("Recovered selector syntax".into());
            }
            visitor.parts.sort_by_key(|p| p.range.start);
            SelectorAnalysis {
                range: TextRange::new(offset + selector.span.start, offset + selector.span.end),
                parts: visitor.parts,
                specificity: visitor.reason.is_none().then_some(specificity),
                reason: visitor.reason,
            }
        })
        .collect()
}

struct Visitor<'a> {
    text: &'a str,
    offset: usize,
    parent: Option<Option<Specificity>>,
    parts: Vec<SelectorPart>,
    reason: Option<String>,
    nesting: bool,
}
impl Visitor<'_> {
    fn part(&mut self, kind: SelectorKind, start: usize, end: usize) {
        if start < end {
            self.parts.push(SelectorPart {
                kind,
                range: TextRange::new(self.offset + start, self.offset + end),
            });
        }
    }
    fn add_parent(&mut self, total: &mut Specificity) {
        if let Some(Some(parent)) = self.parent {
            *total += parent;
        } else {
            self.reason = Some("Nesting selector has no resolved parent".into());
        }
    }
    fn complex(&mut self, selector: &ComplexSelector<'_>) -> Specificity {
        let mut total = Specificity::default();
        for child in &selector.children {
            match child {
                ComplexSelectorChild::CompoundSelector(c) => total += self.compound(c),
                ComplexSelectorChild::Combinator(c) => {
                    self.part(SelectorKind::Combinator, c.span.start, c.span.end)
                }
            }
        }
        total
    }
    fn list(&mut self, list: &SelectorList<'_>) -> Specificity {
        list.selectors
            .iter()
            .map(|s| self.complex(s))
            .max()
            .unwrap_or_default()
    }
    fn compound(&mut self, selector: &CompoundSelector<'_>) -> Specificity {
        let mut total = Specificity::default();
        for simple in &selector.children {
            total += self.simple(simple);
        }
        total
    }
    fn qualified(&mut self, name: &WqName<'_>, kind: SelectorKind) {
        if let Some(prefix) = &name.prefix {
            self.part(SelectorKind::Namespace, prefix.span.start, prefix.span.end);
        }
        let span = name.name.span();
        self.part(kind, span.start, span.end);
    }
    fn simple(&mut self, simple: &SimpleSelector<'_>) -> Specificity {
        let span = simple.span();
        match simple {
            SimpleSelector::Class(_) => {
                self.part(SelectorKind::Class, span.start + 1, span.end);
                CLASS
            }
            SimpleSelector::Id(_) => {
                self.part(SelectorKind::Id, span.start + 1, span.end);
                Specificity {
                    ids: 1,
                    ..Specificity::default()
                }
            }
            SimpleSelector::Type(t) => match t {
                TypeSelector::TagName(t) => {
                    self.qualified(&t.name, SelectorKind::Tag);
                    TYPE
                }
                TypeSelector::Universal(u) => {
                    if let Some(prefix) = &u.prefix {
                        self.part(SelectorKind::Namespace, prefix.span.start, prefix.span.end);
                    }
                    self.part(SelectorKind::Universal, span.end - 1, span.end);
                    Specificity::default()
                }
            },
            SimpleSelector::Attribute(a) => {
                self.qualified(&a.name, SelectorKind::Attribute);
                CLASS
            }
            SimpleSelector::Nesting(_) => {
                self.part(SelectorKind::Nesting, span.start, span.end);
                self.nesting = true;
                let mut total = Specificity::default();
                self.add_parent(&mut total);
                total
            }
            SimpleSelector::PseudoClass(p) => {
                let name_span = p.name.span();
                let name = ident_name(&p.name);
                let legacy = matches!(
                    name.as_str(),
                    "before" | "after" | "first-line" | "first-letter"
                );
                self.part(
                    if legacy {
                        SelectorKind::PseudoElement
                    } else {
                        SelectorKind::PseudoClass
                    },
                    span.start,
                    name_span.end,
                );
                let mut argument = Specificity::default();
                if let Some(arg) = &p.arg {
                    argument = match &arg.kind {
                        PseudoClassSelectorArgKind::SelectorList(l) => self.list(l),
                        PseudoClassSelectorArgKind::CompoundSelector(c) => self.compound(c),
                        PseudoClassSelectorArgKind::CompoundSelectorList(l) => l
                            .selectors
                            .iter()
                            .map(|c| self.compound(c))
                            .max()
                            .unwrap_or_default(),
                        PseudoClassSelectorArgKind::RelativeSelectorList(l) => l
                            .selectors
                            .iter()
                            .map(|s| {
                                if let Some(c) = &s.combinator {
                                    self.part(SelectorKind::Combinator, c.span.start, c.span.end);
                                }
                                self.complex(&s.complex_selector)
                            })
                            .max()
                            .unwrap_or_default(),
                        PseudoClassSelectorArgKind::Nth(n) => n
                            .matcher
                            .as_ref()
                            .and_then(|m| m.selector.as_ref())
                            .map(|l| self.list(l))
                            .unwrap_or_default(),
                        PseudoClassSelectorArgKind::TokenSeq(_)
                            if matches!(name.as_str(), "local" | "global") =>
                        {
                            // Raffia treats CSS Modules wrappers as opaque; parse their content as selectors.
                            let start = arg.l_paren.end;
                            let end = arg.r_paren.start;
                            let mut parser = ParserBuilder::new(&self.text[start..end]).build();
                            if let Ok(list) = parser.parse::<SelectorList>() {
                                if !parser.recoverable_errors().is_empty()
                                    || !only_trivia(&self.text[start + list.span.end..end])
                                {
                                    self.reason = Some("Invalid scoping selector".into());
                                }
                                let mut child = Visitor {
                                    text: &self.text[start..end],
                                    offset: self.offset + start,
                                    parent: self.parent,
                                    parts: vec![],
                                    reason: None,
                                    nesting: false,
                                };
                                let total = child.list(&list);
                                self.parts.extend(child.parts);
                                self.nesting |= child.nesting;
                                if child.reason.is_some() {
                                    self.reason = child.reason;
                                }
                                total
                            } else {
                                self.reason = Some("Invalid scoping selector".into());
                                Specificity::default()
                            }
                        }
                        PseudoClassSelectorArgKind::TokenSeq(_)
                            if matches!(
                                name.as_str(),
                                "state" | "active-view-transition-type" | "heading"
                            ) =>
                        {
                            Specificity::default()
                        }
                        PseudoClassSelectorArgKind::TokenSeq(_) => {
                            self.reason =
                                Some(format!("Unsupported functional pseudo-class :{name}"));
                            Specificity::default()
                        }
                        _ => Specificity::default(),
                    };
                }
                if legacy {
                    return TYPE;
                }
                match name.as_str() {
                    "where" => Specificity::default(),
                    "is" | "not" | "has" | "matches" | "-webkit-any" | "-moz-any" | "local"
                    | "global" => argument,
                    "host" | "host-context" | "nth-child" | "nth-last-child" => {
                        argument += CLASS;
                        argument
                    }
                    _ => CLASS,
                }
            }
            SimpleSelector::PseudoElement(p) => {
                let name_span = p.name.span();
                self.part(SelectorKind::PseudoElement, span.start, name_span.end);
                let name = ident_name(&p.name);
                let mut total = TYPE;
                if let Some(arg) = &p.arg {
                    match &arg.kind {
                        PseudoElementSelectorArgKind::CompoundSelector(c) => {
                            let argument = self.compound(c);
                            if name == "slotted" {
                                total += argument;
                            } else {
                                self.reason =
                                    Some(format!("Unsupported functional pseudo-element ::{name}"));
                            }
                        }
                        PseudoElementSelectorArgKind::Ident(_)
                            if matches!(
                                name.as_str(),
                                "part"
                                    | "highlight"
                                    | "view-transition-group"
                                    | "view-transition-image-pair"
                                    | "view-transition-old"
                                    | "view-transition-new"
                            ) => {}
                        _ if matches!(
                            name.as_str(),
                            "part"
                                | "highlight"
                                | "view-transition-group"
                                | "view-transition-image-pair"
                                | "view-transition-old"
                                | "view-transition-new"
                        ) => {}
                        _ => {
                            self.reason =
                                Some(format!("Unsupported functional pseudo-element ::{name}"))
                        }
                    }
                }
                total
            }
            _ => {
                self.reason = Some("Unsupported selector".into());
                Specificity::default()
            }
        }
    }
}
fn ident_name(ident: &InterpolableIdent<'_>) -> String {
    match ident {
        InterpolableIdent::Literal(i) => i.name.to_ascii_lowercase(),
        _ => String::new(),
    }
}
fn only_trivia(mut text: &str) -> bool {
    loop {
        text = text.trim_start();
        if text.is_empty() {
            return true;
        }
        if let Some(comment) = text.strip_prefix("/*")
            && let Some(end) = comment.find("*/")
        {
            text = &comment[end + 2..];
            continue;
        }
        return false;
    }
}
