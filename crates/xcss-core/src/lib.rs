//! Public compilation facade. All language semantics live in the engine crates.
use std::sync::Arc;
pub use xcss_analysis::{AnalysisHost, AnalysisSnapshot, UpdateError};
pub use xcss_codegen::{GenerateOptions, GeneratedModule, GeneratedProject, generate};
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
pub struct ProjectInspection {
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
        if let Some(composition) = parse.composition(node)
            && let xcss_syntax::ast::CompositionSource::Module(specifier) = composition.source
        {
            imports.push(specifier.to_owned());
        }
    }
    imports.sort();
    imports.dedup();
    ProjectInspection { imports }
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
