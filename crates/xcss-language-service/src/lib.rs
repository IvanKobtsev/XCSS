//! Protocol-independent editor queries. No filesystem, generation, or editor SDK dependencies.
use xcss_analysis::{AnalysisHost, AnalysisSnapshot, ReferenceRole, SymbolKind, ValueType};
use xcss_source::{Diagnostic, FileId, Location, Position, TextRange};
use xcss_syntax::{NodeKind, TokenKind};

pub use xcss_analysis::UpdateError;
pub const API_VERSION: u32 = 2;
mod css;
pub use css::{CssDeclaration, CssDeclarations, CssValuePart};
/// Stable order shared by adapters; append new entries rather than reordering.
pub const TOKEN_TYPES: &[&str] = &[
    "function",
    "parameter",
    "variable",
    "namespace",
    "type",
    "class",
    "enumMember",
    "property",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorLocation {
    pub file: FileId,
    pub range: Range,
}
#[derive(Clone, Debug)]
pub struct EditorDiagnostic {
    pub diagnostic: Diagnostic,
    pub location: EditorLocation,
    pub related: Vec<EditorLocation>,
}
#[derive(Clone, Debug)]
pub struct Hover {
    pub range: Range,
    /// Plain text, deliberately not untrusted Markdown.
    pub text: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticToken {
    pub line: usize,
    pub character: usize,
    pub length: usize,
    pub token_type: usize,
    pub declaration: bool,
}
#[derive(Default)]
pub struct LanguageService {
    host: AnalysisHost,
}
impl LanguageService {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_file(
        &mut self,
        file: FileId,
        revision: u64,
        text: String,
    ) -> Result<(), UpdateError> {
        self.host.set_file(file, revision, text)
    }
    pub fn remove_file(&mut self, file: &FileId, revision: u64) -> Result<(), UpdateError> {
        self.host.remove_file(file, revision)
    }
    pub fn set_resolution(&mut self, file: FileId, specifier: String, target: Option<FileId>) {
        self.host.set_resolution(file, specifier, target);
    }
    pub fn snapshot(&mut self) -> ServiceSnapshot {
        ServiceSnapshot(self.host.snapshot())
    }
}
#[derive(Clone)]
pub struct ServiceSnapshot(AnalysisSnapshot);
impl ServiceSnapshot {
    pub fn revision(&self) -> u64 {
        self.0.revision()
    }
    fn location(&self, location: &Location) -> Option<EditorLocation> {
        let source = &self.0.syntax(&location.file)?.source;
        Some(EditorLocation {
            file: location.file.clone(),
            range: Range {
                start: source.byte_to_utf16(location.range.start)?,
                end: source.byte_to_utf16(location.range.end)?,
            },
        })
    }
    fn offset(&self, file: &FileId, position: Position) -> Option<usize> {
        self.0.syntax(file)?.source.utf16_to_byte(position)
    }
    pub fn diagnostics(&self, file: &FileId) -> Vec<EditorDiagnostic> {
        self.0
            .diagnostics()
            .iter()
            .filter(|d| &d.location.file == file)
            .filter_map(|d| {
                Some(EditorDiagnostic {
                    diagnostic: d.clone(),
                    location: self.location(&d.location)?,
                    related: d.related.iter().filter_map(|l| self.location(l)).collect(),
                })
            })
            .collect()
    }
    pub fn definition(&self, file: &FileId, position: Position) -> Vec<EditorLocation> {
        let Some(offset) = self.offset(file, position) else {
            return vec![];
        };
        let Some(symbol) = self.0.symbol_at(file, offset) else {
            return vec![];
        };
        self.location(&symbol.location).into_iter().collect()
    }
    pub fn references(
        &self,
        file: &FileId,
        position: Position,
        include_declaration: bool,
    ) -> Vec<EditorLocation> {
        let Some(offset) = self.offset(file, position) else {
            return vec![];
        };
        let Some(symbol) = self.0.symbol_at(file, offset) else {
            return vec![];
        };
        let mut locations: Vec<_> = self
            .0
            .references_to(symbol.id)
            .filter(|r| include_declaration || r.role != ReferenceRole::Declaration)
            .filter_map(|r| self.location(&r.location))
            .collect();
        if include_declaration && let Some(location) = self.location(&symbol.location) {
            locations.push(location);
        }
        locations.sort_by_key(|l| (l.file.clone(), l.range.start.line, l.range.start.character));
        locations.dedup();
        locations
    }
    pub fn hover(&self, file: &FileId, position: Position) -> Option<Hover> {
        let offset = self.offset(file, position)?;
        let symbol = self.0.symbol_at(file, offset)?;
        let occurrence = self
            .0
            .references()
            .iter()
            .filter(|r| r.location.file == *file && r.location.range.contains(offset))
            .min_by_key(|r| r.location.range.end - r.location.range.start)
            .map(|r| &r.location)
            .unwrap_or(&symbol.location);
        let mut text = format!("{:?} {}", symbol.kind, symbol.name);
        if let ValueType::Named(kind) = &symbol.value_type {
            text.push_str(&format!(": {kind}"));
        }
        if let Some(signature) = self.0.signature(symbol.id) {
            let parameters: Vec<_> = signature
                .parameters
                .iter()
                .map(|p| {
                    let name = &self.0.symbol(p.symbol).unwrap().name;
                    let ty = match &p.value_type {
                        ValueType::Unknown => "unknown",
                        ValueType::Named(name) => name,
                    };
                    format!(
                        "{name}: {ty}{}",
                        p.default
                            .as_ref()
                            .map(|v| format!(" = {v}"))
                            .unwrap_or_default()
                    )
                })
                .collect();
            text = format!(
                "@injection {}({}){}",
                symbol.name,
                parameters.join(", "),
                if signature.accepts_content {
                    " { @content }"
                } else {
                    ""
                }
            );
        }
        if let Some(variable) = self.0.variable_registration(symbol.id) {
            text.push_str(if variable.inherits {
                "\ninherits: true"
            } else {
                "\ninherits: false (local)"
            });
            if let Some(initial) = variable.initial {
                text.push_str(&format!("\ninitial: {initial}"));
            }
        }
        text.push_str(&format!("\n{}", symbol.location.file.0));
        Some(Hover {
            range: self.location(occurrence)?.range,
            text,
        })
    }
    pub fn semantic_tokens(&self, file: &FileId) -> Vec<SemanticToken> {
        let Some(parse) = self.0.syntax(file) else {
            return vec![];
        };
        // Priority resolves broad contextual candidates against resolved identifiers.
        let mut candidates: Vec<(TextRange, usize, bool, u8)> = Vec::new();
        let kind = |kind| match kind {
            SymbolKind::Injection => 0,
            SymbolKind::Parameter => 1,
            SymbolKind::Variable => 2,
            SymbolKind::Namespace => 3,
            SymbolKind::Class => 5,
            SymbolKind::Keyframe => 6,
        };
        for symbol in self.0.symbols().iter().filter(|s| s.location.file == *file) {
            candidates.push((symbol.location.range, kind(symbol.kind), true, 3));
        }
        for reference in self
            .0
            .references()
            .iter()
            .filter(|r| r.location.file == *file)
        {
            let ty = reference
                .target
                .and_then(|id| self.0.symbol(id))
                .map(|s| kind(s.kind))
                .unwrap_or_else(|| {
                    if parse.tree.descendants().any(|n| {
                        parse
                            .call(n)
                            .is_some_and(|c| c.name_range == reference.location.range)
                    }) {
                        0
                    } else if reference.name.starts_with('$') {
                        1
                    } else {
                        2
                    }
                });
            let mut range = reference.location.range;
            if ty == 0
                && let Some(dot) = parse.slice(range).find('.')
            {
                range.start += dot + 1;
            }
            candidates.push((range, ty, reference.role == ReferenceRole::Declaration, 3));
        }
        for node in parse.tree.descendants() {
            if let Some(declaration) = parse.declaration(node) {
                let ty = if declaration.property.starts_with("--") {
                    2
                } else {
                    7
                };
                candidates.push((declaration.property_range, ty, true, 1));
            }
            if let Some(call) = parse.call(node) {
                let mut range = call.name_range;
                if let Some(dot) = call.name.find('.') {
                    candidates.push((TextRange::new(range.start, range.start + dot), 3, false, 2));
                    range.start += dot + 1;
                }
                candidates.push((range, 0, false, 2));
            }
            if node.kind == NodeKind::Rule {
                for (_, range, _) in xcss_syntax::selector_classes(parse, node) {
                    candidates.push((range, 5, true, 2));
                }
                let tokens: Vec<_> = parse
                    .tokens
                    .iter()
                    .filter(|t| {
                        t.range.start >= node.header.start && t.range.end <= node.header.end
                    })
                    .collect();
                for pair in tokens.windows(2) {
                    if parse.slice(pair[0].range) == "#" && pair[1].kind == TokenKind::Word {
                        candidates.push((pair[1].range, 2, true, 2));
                    }
                }
            }
            let annotation = parse
                .variable(node)
                .and_then(|v| v.annotation)
                .into_iter()
                .chain(
                    parse
                        .injection(node)
                        .into_iter()
                        .flat_map(|v| v.parameters.into_iter().filter_map(|p| p.annotation)),
                );
            for name in annotation {
                let start = name.as_ptr() as usize - parse.source.text().as_ptr() as usize;
                candidates.push((TextRange::new(start, start + name.len()), 4, false, 2));
            }
        }
        candidates.sort_by_key(|(r, _, _, priority)| {
            (std::cmp::Reverse(*priority), r.end - r.start, r.start)
        });
        let mut accepted: Vec<(TextRange, usize, bool)> = Vec::new();
        for (range, ty, declaration, _) in candidates {
            if range.start >= range.end
                || accepted
                    .iter()
                    .any(|(r, _, _)| range.start < r.end && r.start < range.end)
            {
                continue;
            }
            // Identifier candidates may never recolor comments or string literals.
            if parse.tokens.iter().any(|t| {
                matches!(t.kind, TokenKind::Comment | TokenKind::String)
                    && range.start < t.range.end
                    && t.range.start < range.end
            }) {
                continue;
            }
            accepted.push((range, ty, declaration));
        }
        accepted.sort_by_key(|(r, _, _)| r.start);
        let mut result = Vec::new();
        for (range, token_type, declaration) in accepted {
            let mut start = range.start;
            for line in parse.slice(range).split_inclusive('\n') {
                let content = line.trim_end_matches(['\n', '\r']);
                if !content.is_empty()
                    && let Some(position) = parse.source.byte_to_utf16(start)
                {
                    result.push(SemanticToken {
                        line: position.line,
                        character: position.character,
                        length: content.encode_utf16().count(),
                        token_type,
                        declaration,
                    });
                }
                start += line.len();
            }
        }
        result
    }
}
