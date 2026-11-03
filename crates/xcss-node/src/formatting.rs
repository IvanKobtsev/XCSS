use napi_derive::napi;
use xcss_format::{EndOfLine, FormatOptions};

#[napi(object)]
pub struct SourceFormatOptions {
    pub tab_width: Option<u32>,
    pub use_tabs: Option<bool>,
    pub print_width: Option<u32>,
    pub end_of_line: Option<String>,
}
#[napi(object)]
pub struct FormatDiagnostic {
    pub code: String,
    pub message: String,
    /// UTF-16 positions in the original source, not Rust byte offsets.
    pub range: crate::language_service::EditorRange,
}
#[napi(object)]
pub struct SourceFormatResult {
    pub text: Option<String>,
    pub diagnostics: Vec<FormatDiagnostic>,
}
#[napi]
pub fn format_source(
    source: String,
    options: Option<SourceFormatOptions>,
) -> napi::Result<SourceFormatResult> {
    let mut settings = FormatOptions::default();
    if let Some(options) = options {
        if let Some(width) = options.tab_width {
            settings.tab_width = width as usize;
        }
        if let Some(tabs) = options.use_tabs {
            settings.use_tabs = tabs;
        }
        if let Some(width) = options.print_width {
            settings.print_width = width as usize;
        }
        if let Some(eol) = options.end_of_line {
            settings.end_of_line = match eol.as_str() {
                "lf" => EndOfLine::Lf,
                "crlf" => EndOfLine::Crlf,
                _ => return Err(napi::Error::from_reason("endOfLine must be lf or crlf")),
            };
        }
    }
    let result = xcss_format::format(&source, &settings);
    let file = xcss_source::SourceFile::new(xcss_source::FileId("format.xcss".into()), 0, source);
    let position = |offset| {
        let p = file
            .byte_to_utf16(offset)
            .expect("diagnostic is on a source boundary");
        crate::language_service::EditorPosition {
            line: p.line as u32,
            character: p.character as u32,
        }
    };
    Ok(SourceFormatResult {
        text: result.text,
        diagnostics: result
            .diagnostics
            .into_iter()
            .map(|d| FormatDiagnostic {
                code: d.code.into(),
                message: d.message,
                range: crate::language_service::EditorRange {
                    start: position(d.location.range.start),
                    end: position(d.location.range.end),
                },
            })
            .collect(),
    })
}
