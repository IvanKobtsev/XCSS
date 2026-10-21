use napi::{Either, bindgen_prelude::Null};
use napi_derive::napi;
use xcss_core::{Diagnostic, ImportResolution, ProjectFile, Severity};

#[napi(object)]
pub struct NativeSpan {
    pub start: u32,
    pub end: u32,
}

#[napi(object)]
pub struct NativeDiagnostic {
    pub message: String,
    pub severity: String,
    pub span: NativeSpan,
    pub line: u32,
    pub column: u32,
    pub filename: Option<String>,
}

#[napi(object)]
pub struct NativeTransformResult {
    pub code: Either<String, Null>,
    pub partial: Either<String, Null>,
    pub diagnostics: Vec<NativeDiagnostic>,
}

#[napi(object)]
pub struct NativeProjectFile {
    pub module_id: String,
    pub filename: String,
    pub source: String,
}

#[napi(object)]
pub struct NativeImportResolution {
    pub importer: String,
    pub specifier: String,
    pub target: String,
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
pub fn transform(
    files: Vec<NativeProjectFile>,
    resolutions: Vec<NativeImportResolution>,
    target_module_id: String,
    partial_path: String,
) -> NativeTransformResult {
    let borrowed_files: Vec<_> = files
        .iter()
        .map(|file| ProjectFile {
            module_id: &file.module_id,
            filename: &file.filename,
            source: &file.source,
        })
        .collect();
    let borrowed_resolutions: Vec<_> = resolutions
        .iter()
        .map(|resolution| ImportResolution {
            importer: &resolution.importer,
            specifier: &resolution.specifier,
            target: &resolution.target,
        })
        .collect();
    match xcss_core::compile_project(
        &borrowed_files,
        &borrowed_resolutions,
        &target_module_id,
        &partial_path,
    ) {
        Ok(output) => NativeTransformResult {
            code: Either::A(output.code),
            partial: Either::A(output.partial),
            diagnostics: Vec::new(),
        },
        Err(diagnostics) => NativeTransformResult {
            code: Either::B(Null),
            partial: Either::B(Null),
            diagnostics: diagnostics.into_iter().map(Into::into).collect(),
        },
    }
}

impl From<Diagnostic> for NativeDiagnostic {
    fn from(diagnostic: Diagnostic) -> Self {
        Self {
            message: diagnostic.message,
            severity: match diagnostic.severity {
                Severity::Error => "error",
            }
            .into(),
            span: NativeSpan {
                start: to_u32(diagnostic.span.start),
                end: to_u32(diagnostic.span.end),
            },
            line: to_u32(diagnostic.line),
            column: to_u32(diagnostic.column),
            filename: diagnostic.filename,
        }
    }
}

fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
