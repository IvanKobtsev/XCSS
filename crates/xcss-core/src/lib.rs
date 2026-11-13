//! Public compilation facade. All language semantics live in the engine crates.
use std::sync::Arc;
pub use xcss_analysis::{
    AnalysisHost, AnalysisSnapshot, ModuleInterface, ProvidedVariable, UpdateError,
};
pub use xcss_codegen::{
    GenerateOptions, GeneratedModule, GeneratedProject, GeneratedUtilities, GeneratedUtilityRule,
    generate, merge_utilities, positional_class,
};
pub use xcss_source::{Diagnostic, FileId, Location, Severity, SourceFile, TextRange};

#[derive(Clone, Debug)]
pub struct ProjectFile<'a> {
    pub module_id: &'a str,
    pub source: &'a str,
}
#[derive(Clone, Debug)]
pub struct ImportResolution<'a> {
    pub importer: &'a str,
    pub specifier: &'a str,
    pub target: Option<&'a str>,
}
#[derive(Clone, Debug)]
pub struct AssetReference {
    pub url: String,
    pub range: TextRange,
}
#[derive(Clone, Debug)]
pub struct ProjectInspection {
    pub assets: Vec<AssetReference>,
    pub imports: Vec<String>,
}

pub fn inspect_project_file(source: &str) -> ProjectInspection {
    let parse = xcss_syntax::parse(Arc::new(SourceFile::new(
        FileId("inspection.xcss".into()),
        0,
        source,
    )));
    let mut imports = Vec::new();
    for node in parse.tree.descendants() {
        if let Some(import) = parse.injection_import(node) {
            imports.push(import.specifier.to_owned());
        }
    }
    imports.sort();
    imports.dedup();
    let mut assets = Vec::new();
    let tokens = &parse.tokens;
    for (index, token) in tokens.iter().enumerate() {
        if token.kind != xcss_syntax::TokenKind::Word
            || !parse.token_text(token).eq_ignore_ascii_case("url")
        {
            continue;
        }
        let Some(open) = tokens.get(index + 1).filter(|t| parse.token_text(t) == "(") else {
            continue;
        };
        let Some(close) = tokens[index + 2..]
            .iter()
            .find(|t| parse.token_text(t) == ")")
        else {
            continue;
        };
        let inside = parse.source.text()[open.range.end..close.range.start].trim();
        let url = xcss_syntax::ast::decode_identifier(inside.trim_matches(['\'', '"']));
        assets.push(AssetReference {
            url,
            range: TextRange::new(token.range.start, close.range.end),
        });
    }
    ProjectInspection { imports, assets }
}

pub fn compile_project(
    files: &[ProjectFile<'_>],
    resolutions: &[ImportResolution<'_>],
    entry: &str,
    options: &GenerateOptions,
) -> Result<GeneratedProject, Vec<Diagnostic>> {
    let mut host = AnalysisHost::new();
    for file in files {
        host.set_file(FileId(file.module_id.into()), 1, file.source)
            .map_err(|error| {
                vec![Diagnostic::error(
                    "project.update",
                    &FileId(file.module_id.into()),
                    TextRange::default(),
                    format!("{error:?}"),
                )]
            })?;
    }
    for resolution in resolutions {
        host.set_resolution(
            FileId(resolution.importer.into()),
            resolution.specifier,
            resolution.target.map(|s| FileId(s.into())),
        );
    }
    generate(&host.snapshot(), &FileId(entry.into()), options)
}
