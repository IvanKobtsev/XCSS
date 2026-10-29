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
pub struct EditorHover {
    pub range: EditorRange,
    pub text: String,
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
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: service::LanguageService::new(),
        }
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
    pub fn document(&mut self, uri: String) -> EditorDocument {
        let snapshot = self.inner.snapshot();
        let file = FileId(uri);
        EditorDocument {
            revision: snapshot.revision() as f64,
            diagnostics: snapshot
                .diagnostics(&file)
                .into_iter()
                .map(|d| EditorDiagnostic {
                    code: d.diagnostic.code.into(),
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
            tokens: snapshot
                .semantic_tokens(&file)
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
