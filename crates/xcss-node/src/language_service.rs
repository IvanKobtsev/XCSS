use napi_derive::napi;
use xcss_language_service as service;
use xcss_source::{FileId, Position};

#[napi(object)]
pub struct EditorPosition {
    pub line: u32,
    pub character: u32,
}
#[napi(object)]
pub struct EditorRange {
    pub start: EditorPosition,
    pub end: EditorPosition,
}
#[napi(object)]
pub struct EditorLocation {
    pub uri: String,
    pub range: EditorRange,
}
#[napi(object)]
pub struct EditorDiagnostic {
    pub code: String,
    pub label: String,
    pub legacy_code: Option<String>,
    pub message: String,
    pub severity: String,
    pub location: EditorLocation,
    pub related: Vec<EditorLocation>,
}
#[napi(object)]
pub struct SemanticToken {
    pub line: u32,
    pub character: u32,
    pub length: u32,
    pub token_type: u32,
    pub declaration: bool,
}
#[napi(object)]
pub struct EditorDocument {
    pub revision: f64,
    pub diagnostics: Vec<EditorDiagnostic>,
    pub tokens: Vec<SemanticToken>,
}
#[napi(object)]
pub struct CssValuePart {
    pub text: Option<String>,
    pub value_type: Option<String>,
    pub range: EditorRange,
}
#[napi(object)]
pub struct CssDeclaration {
    pub property: String,
    pub property_range: EditorRange,
    pub value_range: EditorRange,
    pub context: String,
    pub checkable: bool,
    pub parts: Vec<CssValuePart>,
}
#[napi(object)]
pub struct CssDeclarations {
    pub revision: f64,
    pub declarations: Vec<CssDeclaration>,
}
#[napi(object)]
pub struct CssFunction {
    pub name: String,
    pub range: EditorRange,
}
#[napi(object)]
pub struct CssValue {
    pub property: Option<String>,
    pub text: String,
    pub range: EditorRange,
    pub parts: Vec<CssValuePart>,
    pub checkable: bool,
    pub functions: Vec<CssFunction>,
}
#[napi(object)]
pub struct CssValues {
    pub revision: f64,
    pub values: Vec<CssValue>,
}
#[napi(object)]
pub struct Specificity {
    pub ids: u32,
    pub classes: u32,
    pub types: u32,
}
#[napi(object)]
pub struct SelectorPart {
    pub kind: String,
    pub range: EditorRange,
}
#[napi(object)]
pub struct Selector {
    pub range: EditorRange,
    pub parts: Vec<SelectorPart>,
    pub specificity: Option<Specificity>,
    pub reason: Option<String>,
}
#[napi(object)]
pub struct Selectors {
    pub revision: f64,
    pub selectors: Vec<Selector>,
}
#[napi(object)]
pub struct EditorHover {
    pub range: EditorRange,
    pub text: String,
}
#[napi(object)]
pub struct EditorDefinitionDetails {
    pub origin: EditorRange,
    pub targets: Vec<EditorLocation>,
}
#[napi(object)]
pub struct EditorCompletion {
    pub label: String,
    pub kind: String,
    pub range: EditorRange,
    pub new_text: String,
}
#[napi(object)]
pub struct EditorExportedSymbol {
    pub module: String,
    pub kind: String,
    pub name: String,
    pub group: Option<String>,
}
#[napi(object)]
pub struct LanguageServiceCapabilities {
    pub api_version: u32,
    pub token_types: Vec<String>,
    pub token_modifiers: Vec<String>,
}
#[napi]
pub fn language_service_capabilities() -> LanguageServiceCapabilities {
    LanguageServiceCapabilities {
        api_version: service::API_VERSION,
        token_types: service::TOKEN_TYPES.iter().map(|s| (*s).into()).collect(),
        token_modifiers: vec!["declaration".into()],
    }
}
fn position(p: Position) -> EditorPosition {
    EditorPosition {
        line: p.line as u32,
        character: p.character as u32,
    }
}
fn range(r: service::Range) -> EditorRange {
    EditorRange {
        start: position(r.start),
        end: position(r.end),
    }
}
fn location(l: service::EditorLocation) -> EditorLocation {
    EditorLocation {
        uri: l.file.0,
        range: range(l.range),
    }
}
fn input(p: EditorPosition) -> Position {
    Position {
        line: p.line as usize,
        character: p.character as usize,
    }
}
#[napi]
pub struct LanguageService {
    inner: service::LanguageService,
}
#[napi]
impl LanguageService {
    #[napi]
    pub fn css_values(&mut self, uri: String) -> CssValues {
        let result = self.inner.snapshot().css_values(&FileId(uri));
        CssValues {
            revision: result.revision as f64,
            values: result
                .values
                .into_iter()
                .map(|v| CssValue {
                    property: v.property,
                    text: v.text,
                    range: range(v.range),
                    checkable: v.checkable,
                    parts: v
                        .parts
                        .into_iter()
                        .map(|p| CssValuePart {
                            text: p.text,
                            value_type: p.value_type,
                            range: range(p.range),
                        })
                        .collect(),
                    functions: v
                        .functions
                        .into_iter()
                        .map(|f| CssFunction {
                            name: f.name,
                            range: range(f.range),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
    #[napi]
    pub fn selectors(&mut self, uri: String) -> Selectors {
        let result = self.inner.snapshot().selectors(&FileId(uri));
        Selectors {
            revision: result.revision as f64,
            selectors: result
                .selectors
                .into_iter()
                .map(|s| Selector {
                    range: range(s.range),
                    parts: s
                        .parts
                        .into_iter()
                        .map(|p| SelectorPart {
                            kind: p.kind,
                            range: range(p.range),
                        })
                        .collect(),
                    specificity: s.specificity.map(|s| Specificity {
                        ids: s.ids,
                        classes: s.classes,
                        types: s.types,
                    }),
                    reason: s.reason,
                })
                .collect(),
        }
    }
    #[napi]
    pub fn css_declarations(&mut self, uri: String) -> CssDeclarations {
        let result = self.inner.snapshot().css_declarations(&FileId(uri));
        CssDeclarations {
            revision: result.revision as f64,
            declarations: result
                .declarations
                .into_iter()
                .map(|d| CssDeclaration {
                    property: d.property,
                    property_range: range(d.property_range),
                    value_range: range(d.value_range),
                    context: d.context,
                    checkable: d.checkable,
                    parts: d
                        .parts
                        .into_iter()
                        .map(|p| CssValuePart {
                            text: p.text,
                            value_type: p.value_type,
                            range: range(p.range),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: service::LanguageService::new(),
        }
    }
    #[napi]
    pub fn set_typescript_project(&mut self, config: Option<String>) {
        self.inner.set_typescript_project(config);
    }

    #[napi]
    pub fn typescript_project_warning(&mut self) -> Option<String> {
        self.inner.snapshot().typescript_project_warning()
    }
    #[napi]
    pub fn set_typescript_file(
        &mut self,
        path: String,
        revision: u32,
        source: String,
    ) -> napi::Result<()> {
        self.inner
            .set_typescript_file(FileId(path), revision.into(), source)
            .map_err(|e| napi::Error::from_reason(format!("{e:?}")))
    }
    #[napi]
    pub fn remove_typescript_file(&mut self, path: String, revision: u32) -> napi::Result<()> {
        self.inner
            .remove_typescript_file(&FileId(path), revision.into())
            .map_err(|e| napi::Error::from_reason(format!("{e:?}")))
    }
    #[napi]
    pub fn set_file(&mut self, uri: String, revision: u32, source: String) -> napi::Result<()> {
        self.inner
            .set_file(FileId(uri), revision.into(), source)
            .map_err(|e| napi::Error::from_reason(format!("{e:?}")))
    }
    #[napi]
    pub fn remove_file(&mut self, uri: String, revision: u32) -> napi::Result<()> {
        self.inner
            .remove_file(&FileId(uri), revision.into())
            .map_err(|e| napi::Error::from_reason(format!("{e:?}")))
    }
    #[napi]
    pub fn set_resolution(&mut self, importer: String, specifier: String, target: Option<String>) {
        self.inner
            .set_resolution(FileId(importer), specifier, target.map(FileId));
    }
    #[napi]
    pub fn document(&mut self, uri: String, include_tokens: Option<bool>) -> EditorDocument {
        let snapshot = self.inner.snapshot();
        let file = FileId(uri);
        let tokens = if include_tokens.unwrap_or(true) {
            snapshot.semantic_tokens(&file)
        } else {
            Vec::new()
        };
        EditorDocument {
            revision: snapshot.revision() as f64,
            diagnostics: snapshot
                .diagnostics(&file)
                .into_iter()
                .map(|d| EditorDiagnostic {
                    code: d.diagnostic.code.into(),
                    label: d.label.into(),
                    legacy_code: d.legacy_code.map(Into::into),
                    message: d.diagnostic.message,
                    severity: match d.diagnostic.severity {
                        xcss_source::Severity::Error => "error",
                        xcss_source::Severity::Warning => "warning",
                        xcss_source::Severity::Information => "information",
                    }
                    .into(),
                    location: location(d.location),
                    related: d.related.into_iter().map(location).collect(),
                })
                .collect(),
            tokens: tokens
                .into_iter()
                .map(|t| SemanticToken {
                    line: t.line as u32,
                    character: t.character as u32,
                    length: t.length as u32,
                    token_type: t.token_type as u32,
                    declaration: t.declaration,
                })
                .collect(),
        }
    }
    #[napi]
    pub fn definition(&mut self, uri: String, position: EditorPosition) -> Vec<EditorLocation> {
        self.inner
            .snapshot()
            .definition(&FileId(uri), input(position))
            .into_iter()
            .map(location)
            .collect()
    }
    #[napi]
    pub fn definition_details(
        &mut self,
        uri: String,
        position: EditorPosition,
    ) -> Option<EditorDefinitionDetails> {
        self.inner
            .snapshot()
            .definition_details(&FileId(uri), input(position))
            .map(|details| EditorDefinitionDetails {
                origin: range(details.origin),
                targets: details.targets.into_iter().map(location).collect(),
            })
    }
    #[napi]
    pub fn references(
        &mut self,
        uri: String,
        position: EditorPosition,
        include_declaration: bool,
    ) -> Vec<EditorLocation> {
        self.inner
            .snapshot()
            .references(&FileId(uri), input(position), include_declaration)
            .into_iter()
            .map(location)
            .collect()
    }
    #[napi]
    pub fn symbols_at(
        &mut self,
        uri: String,
        position: EditorPosition,
    ) -> Vec<EditorExportedSymbol> {
        self.inner
            .snapshot()
            .symbols_at(&FileId(uri), input(position))
            .into_iter()
            .map(|symbol| {
                let (module, kind, name, group) = match symbol {
                    service::ExportedSymbol::Class(module, name) => (module, "class", name, None),
                    service::ExportedSymbol::Group(module, name) => {
                        (module, "variantGroup", name, None)
                    }
                    service::ExportedSymbol::Member(module, group, name) => {
                        (module, "variantMember", name, Some(group))
                    }
                };
                EditorExportedSymbol {
                    module: module.0,
                    kind: kind.into(),
                    name,
                    group,
                }
            })
            .collect()
    }
    #[napi]
    pub fn references_for_symbols(
        &mut self,
        symbols: Vec<EditorExportedSymbol>,
        include_declaration: bool,
    ) -> Vec<EditorLocation> {
        let symbols: Vec<_> = symbols
            .into_iter()
            .filter_map(|symbol| {
                let module = FileId(symbol.module);
                match symbol.kind.as_str() {
                    "class" => Some(service::ExportedSymbol::Class(module, symbol.name)),
                    "variantGroup" => Some(service::ExportedSymbol::Group(module, symbol.name)),
                    "variantMember" => Some(service::ExportedSymbol::Member(
                        module,
                        symbol.group?,
                        symbol.name,
                    )),
                    _ => None,
                }
            })
            .collect();
        self.inner
            .snapshot()
            .exported_references(&symbols, include_declaration)
            .into_iter()
            .map(location)
            .collect()
    }
    #[napi]
    pub fn completion(&mut self, uri: String, position: EditorPosition) -> Vec<EditorCompletion> {
        self.inner
            .snapshot()
            .completion(&FileId(uri), input(position))
            .into_iter()
            .map(|item| EditorCompletion {
                label: item.label,
                kind: item.kind.into(),
                range: range(item.range),
                new_text: item.new_text,
            })
            .collect()
    }
    #[napi]
    pub fn hover(&mut self, uri: String, position: EditorPosition) -> Option<EditorHover> {
        self.inner
            .snapshot()
            .hover(&FileId(uri), input(position))
            .map(|h| EditorHover {
                range: range(h.range),
                text: h.text,
            })
    }
}
impl Default for LanguageService {
    fn default() -> Self {
        Self::new()
    }
}
