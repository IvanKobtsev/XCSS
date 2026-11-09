//! Protocol-independent editor queries. No filesystem, generation, or editor SDK dependencies.
use xcss_analysis::{AnalysisHost, AnalysisSnapshot, ReferenceRole, SymbolKind, ValueType};
use xcss_source::{Diagnostic, FileId, Location, Position, TextRange};
use xcss_syntax::{NodeKind, TokenKind};

pub use xcss_analysis::UpdateError;
pub const API_VERSION: u32 = 5;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Default)]
struct TypescriptState {
    generation: u64,
    config: Option<String>,
    files: BTreeMap<FileId, Arc<xcss_source::SourceFile>>,
    revisions: BTreeMap<FileId, u64>,
}
mod selectors;
pub use selectors::{Selector, SelectorPart, Selectors};
pub use xcss_syntax::Specificity;
mod css;
mod diagnostic_catalog;
mod editor;
pub use editor::{Completion, ExportedSymbol};
mod values;
pub use css::{CssDeclaration, CssDeclarations, CssValuePart};
pub use values::{CssFunction, CssValue, CssValues};
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
    "selectorId",
    "selectorTag",
    "selectorAttribute",
    "pseudoClass",
    "pseudoElement",
    "selectorUniversal",
    "selectorNesting",
    "selectorCombinator",
    "selectorNamespace",
    "cssFunction",
    "cssKeyword",
    "cssColor",
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
    pub label: &'static str,
    pub legacy_code: Option<&'static str>,
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
    typescript: TypescriptState,
    cached: Option<ServiceSnapshot>,
}
impl LanguageService {
    /// Enable TS checking using a project-relative tsconfig path; None disables it.
    pub fn set_typescript_project(&mut self, config: Option<String>) {
        self.typescript.config = config;
        self.typescript.generation += 1;
    }
    /// Supply TS sources, configs, package metadata, and dependency declarations.
    pub fn set_typescript_file(
        &mut self,
        file: FileId,
        revision: u64,
        text: String,
    ) -> Result<(), UpdateError> {
        if file.0.is_empty()
            || file.0.starts_with('/')
            || file.0.contains('\\')
            || file.0.split('/').any(|part| part == "..")
        {
            return Err(UpdateError::InvalidModuleId);
        }
        if self
            .typescript
            .revisions
            .get(&file)
            .is_some_and(|&old| revision <= old)
        {
            return Err(UpdateError::StaleRevision);
        }
        self.typescript.revisions.insert(file.clone(), revision);
        self.typescript.files.insert(
            file.clone(),
            Arc::new(xcss_source::SourceFile::new(file, revision, text)),
        );
        self.typescript.generation += 1;
        Ok(())
    }
    pub fn remove_typescript_file(
        &mut self,
        file: &FileId,
        revision: u64,
    ) -> Result<(), UpdateError> {
        if self
            .typescript
            .revisions
            .get(file)
            .is_some_and(|&old| revision <= old)
        {
            return Err(UpdateError::StaleRevision);
        }
        self.typescript.revisions.insert(file.clone(), revision);
        self.typescript.files.remove(file);
        self.typescript.generation += 1;
        Ok(())
    }
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
        let analysis = self.host.snapshot();
        let revision = analysis.revision() + self.typescript.generation;
        if let Some(cached) = &self.cached
            && cached.revision() == revision
        {
            return cached.clone();
        }
        let snapshot = ServiceSnapshot(
            analysis,
            Arc::new(self.typescript.clone()),
            Arc::new(OnceLock::new()),
            Arc::new(OnceLock::new()),
        );
        self.cached = Some(snapshot.clone());
        snapshot
    }
}
#[derive(Clone)]
pub struct ServiceSnapshot(
    AnalysisSnapshot,
    Arc<TypescriptState>,
    Arc<OnceLock<Result<Vec<Diagnostic>, String>>>,
    Arc<OnceLock<xcss_typescript::EditorIndex>>,
);

pub fn typescript_warning(reason: &str) -> String {
    let reason = reason.lines().map(str::trim).collect::<Vec<_>>().join(" ");
    format!("Couldn't analyze XCSS TypeScript integration: {reason}")
}

impl ServiceSnapshot {
    pub fn revision(&self) -> u64 {
        self.0.revision() + self.1.generation
    }
    fn location(&self, location: &Location) -> Option<EditorLocation> {
        let source = self
            .0
            .syntax(&location.file)
            .map(|p| p.source.as_ref())
            .or_else(|| self.1.files.get(&location.file).map(Arc::as_ref))?;
        Some(EditorLocation {
            file: location.file.clone(),
            range: Range {
                start: source.byte_to_utf16(location.range.start)?,
                end: source.byte_to_utf16(location.range.end)?,
            },
        })
    }
    fn offset(&self, file: &FileId, position: Position) -> Option<usize> {
        self.0
            .syntax(file)
            .map(|p| p.source.as_ref())
            .or_else(|| self.1.files.get(file).map(Arc::as_ref))?
            .utf16_to_byte(position)
    }

    fn typescript_check(&self) -> &Result<Vec<Diagnostic>, String> {
        self.2.get_or_init(|| {
            let Some(config) = &self.1.config else {
                return Ok(Vec::new());
            };
            xcss_typescript::check(
                &xcss_typescript::Project {
                    config: config.clone(),
                    files: self.1.files.clone(),
                },
                &self.0,
            )
        })
    }

    pub fn typescript_project_warning(&self) -> Option<String> {
        self.typescript_check()
            .as_ref()
            .err()
            .map(|reason| typescript_warning(reason))
    }

    pub fn diagnostics(&self, file: &FileId) -> Vec<EditorDiagnostic> {
        let typescript = if self.0.syntax(file).is_none() && self.1.files.contains_key(file) {
            self.typescript_check().as_deref().unwrap_or_default()
        } else {
            &[]
        };
        self.0
            .diagnostics()
            .iter()
            .chain(typescript)
            .filter(|d| &d.location.file == file)
            .filter_map(|d| {
                let (code, label, legacy_code) = diagnostic_catalog::metadata(d.code);
                let mut diagnostic = d.clone();
                diagnostic.code = code;
                Some(EditorDiagnostic {
                    diagnostic,
                    label,
                    legacy_code,
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
        if let Some(symbol) = self.0.symbol_at(file, offset) {
            return self.location(&symbol.location).into_iter().collect();
        }
        let mut symbols = self.exported_symbols_at(file, offset);
        if self.0.syntax(file).is_some() {
            for symbol in &mut symbols {
                if let ExportedSymbol::Member(module, _, member) = symbol {
                    *symbol = ExportedSymbol::Class(module.clone(), member.clone());
                }
            }
        }
        self.exported_definitions(&symbols)
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
        let symbols = self.exported_symbols_at(file, offset);
        if !symbols.is_empty() {
            return self.exported_references(&symbols, include_declaration);
        }
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
                "@{} {}({}){}",
                if symbol.kind == SymbolKind::Util {
                    "util"
                } else {
                    "injection"
                },
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
        if let Some((parse, initial)) = self.0.constant_initializer(symbol.id) {
            text.push_str(&format!("\nvalue: {}", parse.slice(initial.range).trim()));
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
            SymbolKind::Injection | SymbolKind::Util => 0,
            SymbolKind::Parameter => 1,
            SymbolKind::Variable | SymbolKind::Constant => 2,
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
            let annotation = parse
                .variable(node)
                .and_then(|v| v.annotation)
                .into_iter()
                .chain(parse.constant(node).and_then(|c| c.annotation))
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
        for selector in selectors::raw_selectors(parse) {
            for part in selector.parts {
                let ty = TOKEN_TYPES
                    .iter()
                    .position(|name| *name == part.kind.token_type())
                    .unwrap();
                candidates.push((part.range, ty, false, 4));
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
