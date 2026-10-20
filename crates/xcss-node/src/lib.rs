use napi::{Either, bindgen_prelude::Null};
use napi_derive::napi;
use xcss_core::{Diagnostic, Severity, Syntax, TransformOptions};

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
    pub diagnostics: Vec<NativeDiagnostic>,
}

#[napi]
pub fn transform(
    source: String,
    syntax: String,
    filename: Option<String>,
    module_id: Option<String>,
) -> NativeTransformResult {
    let syntax = match syntax.as_str() {
        "css" => Syntax::Css,
        "scss" => Syntax::Scss,
        _ => {
            return NativeTransformResult {
                code: Either::B(Null),
                diagnostics: vec![NativeDiagnostic {
                    message: format!("unsupported XCSS syntax `{syntax}`"),
                    severity: "error".into(),
                    span: NativeSpan { start: 0, end: 0 },
                    line: 1,
                    column: 1,
                    filename,
                }],
            };
        }
    };

    match xcss_core::transform(
        &source,
        TransformOptions {
            syntax,
            filename: filename.as_deref(),
            module_id: module_id.as_deref(),
        },
    ) {
        Ok(output) => NativeTransformResult {
            code: Either::A(output.code),
            diagnostics: Vec::new(),
        },
        Err(diagnostics) => NativeTransformResult {
            code: Either::B(Null),
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
