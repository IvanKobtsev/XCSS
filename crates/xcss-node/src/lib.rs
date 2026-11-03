use napi::bindgen_prelude::{JsObjectValue, Object, ToNapiValue};
use napi_derive::napi;
use std::collections::BTreeMap;
use xcss_core::{AnalysisHost, Diagnostic, FileId, GenerateOptions, GeneratedProject, Severity};
pub mod language_service;

#[napi(object)]
pub struct NativeSpan {
    pub start: u32,
    pub end: u32,
}
#[napi(object)]
pub struct NativeLocation {
    pub module_id: String,
    pub span: NativeSpan,
}
#[napi(object)]
pub struct NativeDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: String,
    pub module_id: String,
    pub span: NativeSpan,
    pub line: u32,
    pub column: u32,
    pub related: Vec<NativeLocation>,
}
#[napi(object, object_from_js = false)]
pub struct NativeArtifact {
    pub module_id: String,
    pub css: String,
    pub source_map: String,
    #[napi(ts_type = "Record<string, string>")]
    pub classes: StringMap,
    #[napi(ts_type = "Record<string, string>")]
    pub custom_properties: StringMap,
    pub stylesheet_dependencies: Vec<String>,
    pub native_imports: Vec<String>,
    pub source_dependencies: Vec<String>,
}
#[napi(object, object_from_js = false)]
pub struct NativeCompilation {
    pub success: bool,
    pub artifacts: Vec<NativeArtifact>,
    pub diagnostics: Vec<NativeDiagnostic>,
    pub utilities: Option<NativeUtilities>,
}
#[napi(object, object_from_js = false)]
pub struct NativeUtilities {
    pub css: String,
    pub source_map: String,
    pub source_dependencies: Vec<String>,
    pub rules: Vec<NativeUtilityRule>,
}
#[napi(object, object_from_js = false)]
pub struct NativeUtilityRule {
    pub key: String,
    pub css: String,
    pub source_map: String,
}
/// Define own data properties rather than invoking JavaScript prototype setters.
pub struct StringMap(BTreeMap<String, String>);
impl ToNapiValue for StringMap {
    unsafe fn to_napi_value(
        raw: napi::sys::napi_env,
        value: Self,
    ) -> napi::Result<napi::sys::napi_value> {
        let env = napi::Env::from(raw);
        let mut object = Object::new(&env)?;
        for (key, value) in value.0 {
            let string = env.create_string(value)?;
            object.define_properties(&[napi::Property::new()
                .with_utf8_name(&key)?
                .with_value(&string)])?;
        }
        unsafe { Object::to_napi_value(raw, object) }
    }
}
#[napi(object)]
pub struct NativeInspection {
    pub imports: Vec<String>,
}
#[napi]
pub fn inspect(source: String) -> NativeInspection {
    NativeInspection {
        imports: xcss_core::inspect_project_file(&source).imports,
    }
}

#[napi]
pub struct Compiler {
    host: AnalysisHost,
}
#[napi]
impl Compiler {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            host: AnalysisHost::new(),
        }
    }
    #[napi]
    pub fn set_file(
        &mut self,
        module_id: String,
        revision: u32,
        source: String,
    ) -> napi::Result<()> {
        self.host
            .set_file(FileId(module_id), u64::from(revision), source)
            .map_err(|e| napi::Error::from_reason(format!("{e:?}")))
    }
    #[napi]
    pub fn remove_file(&mut self, module_id: String, revision: u32) -> napi::Result<()> {
        self.host
            .remove_file(&FileId(module_id), u64::from(revision))
            .map_err(|e| napi::Error::from_reason(format!("{e:?}")))
    }
    #[napi]
    pub fn set_resolution(&mut self, importer: String, specifier: String, target: Option<String>) {
        self.host
            .set_resolution(FileId(importer), specifier, target.map(FileId));
    }
    #[napi]
    pub fn compile_utilities(&mut self, entries: Vec<String>) -> NativeCompilation {
        let snapshot = self.host.snapshot();
        let mut artifacts = Vec::new();
        let mut diagnostics = Vec::new();
        for entry in entries {
            match xcss_core::generate(&snapshot, &FileId(entry), &GenerateOptions::default()) {
                Ok(project) => {
                    diagnostics.extend(project.diagnostics);
                    if let Some(shared) = project.utilities {
                        artifacts.push(shared);
                    }
                }
                Err(errors) => diagnostics.extend(errors),
            }
        }
        let mut success = !diagnostics.iter().any(|d| d.severity == Severity::Error);
        let utilities = if success {
            match xcss_core::merge_utilities(artifacts) {
                Ok(shared) => shared.map(convert_utilities),
                Err(errors) => {
                    diagnostics.extend(errors);
                    success = false;
                    None
                }
            }
        } else {
            None
        };
        NativeCompilation {
            success,
            artifacts: Vec::new(),
            utilities,
            diagnostics: diagnostics
                .into_iter()
                .map(|d| convert_diagnostic(d, &snapshot))
                .collect(),
        }
    }
    #[napi]
    pub fn compile(
        &mut self,
        entry: String,
        minify: Option<bool>,
        bundle: Option<bool>,
    ) -> NativeCompilation {
        let snapshot = self.host.snapshot();
        let result = xcss_core::generate(
            &snapshot,
            &FileId(entry),
            &GenerateOptions {
                minify: minify.unwrap_or(false),
                bundle: bundle.unwrap_or(false),
                ..GenerateOptions::default()
            },
        );
        let (success, artifacts, diagnostics, utilities) = match result {
            Ok(GeneratedProject {
                modules,
                diagnostics,
                utilities,
                ..
            }) => (
                true,
                modules
                    .into_iter()
                    .map(|m| NativeArtifact {
                        module_id: m.module_id.0,
                        css: m.css,
                        source_map: m.source_map,
                        classes: StringMap(m.classes),
                        custom_properties: StringMap(m.custom_properties),
                        stylesheet_dependencies: m
                            .stylesheet_dependencies
                            .into_iter()
                            .map(|s| s.0)
                            .collect(),
                        native_imports: m.native_imports,
                        source_dependencies: m
                            .source_dependencies
                            .into_iter()
                            .map(|s| s.0)
                            .collect(),
                    })
                    .collect(),
                diagnostics,
                utilities.map(convert_utilities),
            ),
            Err(diagnostics) => (false, Vec::new(), diagnostics, None),
        };
        NativeCompilation {
            success,
            artifacts,
            utilities,
            diagnostics: diagnostics
                .into_iter()
                .map(|d| convert_diagnostic(d, &snapshot))
                .collect(),
        }
    }
}
fn convert_utilities(mut u: xcss_core::GeneratedUtilities) -> NativeUtilities {
    NativeUtilities {
        css: u.css,
        source_map: u.source_map,
        source_dependencies: u.source_dependencies.into_iter().map(|f| f.0).collect(),
        rules: u
            .rule_order
            .into_iter()
            .map(|key| {
                let r = u.rules.remove(&key).unwrap();
                NativeUtilityRule {
                    key,
                    css: r.css,
                    source_map: r.source_map,
                }
            })
            .collect(),
    }
}
impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}
fn convert_diagnostic(d: Diagnostic, snapshot: &xcss_core::AnalysisSnapshot) -> NativeDiagnostic {
    let position = snapshot
        .syntax(&d.location.file)
        .and_then(|p| p.source.byte_to_utf16(d.location.range.start));
    NativeDiagnostic {
        code: d.code.into(),
        message: d.message,
        severity: match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Information => "information",
        }
        .into(),
        module_id: d.location.file.0,
        span: NativeSpan {
            start: d.location.range.start as u32,
            end: d.location.range.end as u32,
        },
        line: position.map(|p| p.line as u32 + 1).unwrap_or(1),
        column: position.map(|p| p.character as u32 + 1).unwrap_or(1),
        related: d
            .related
            .into_iter()
            .map(|l| NativeLocation {
                module_id: l.file.0,
                span: NativeSpan {
                    start: l.range.start as u32,
                    end: l.range.end as u32,
                },
            })
            .collect(),
    }
}

pub mod formatting;
