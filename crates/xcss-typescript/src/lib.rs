//! XCSS-specific TypeScript checking over host-supplied, immutable project inputs.
use std::{collections::BTreeMap, sync::Arc};
use tsrs_compiler::{
    ProgramOptions, ProgramRegions, free_unshared_program, new_compiler_host, new_program,
};
use tsrs_core::{P, Tristate};
use tsrs_tsoptions::ParseConfigHost;
use tsrs_vfs::{FS, bundled, vfstest};
use xcss_analysis::{AnalysisSnapshot, ModuleInterface};
use xcss_source::{Diagnostic, FileId, SourceFile};

mod diagnostics;
mod editor;
use diagnostics::{
    Issues, VARIANT_GROUP_MARKER, contract_errors, missing_modules, semantic_issues, variant_errors,
};
pub use editor::{Completion, EditorIndex, Occurrence, Symbol, editor_index};

/// Paths are project-relative forward-slash paths. Include configs, package metadata,
/// dependencies and declarations as well as TS sources. No disk reads occur here.
#[derive(Clone, Default)]
pub struct Project {
    pub config: String,
    pub files: BTreeMap<FileId, Arc<SourceFile>>,
}

pub fn declaration(interface: &ModuleInterface, precise: bool) -> String {
    let quote = |s: &str| serde_json::to_string(s).unwrap();
    let mut text = String::from("declare const styles: {\n");
    for class in &interface.classes {
        text.push_str(&format!("  {}: string;\n", quote(class)));
    }
    text.push_str("};\nexport default styles;\ntype Groups = {\n");
    for (group, members) in &interface.variants {
        let group = quote(group);
        let values = if precise {
            members
                .iter()
                .map(|m| quote(m))
                .collect::<Vec<_>>()
                .join(" | ")
        } else {
            format!("string & {{ readonly {VARIANT_GROUP_MARKER}?: {group} }}")
        };
        text.push_str(&format!("  {group}: {values};\n"));
    }
    text.push_str("};\nexport type Variants<Group extends keyof Groups> = Groups[Group];\n");
    text
}

struct Host {
    fs: Arc<dyn FS>,
}
impl ParseConfigHost for Host {
    fn fs(&self) -> &dyn FS {
        &*self.fs
    }
    fn get_current_directory(&self) -> &str {
        "/"
    }
}

fn memory_fs(files: &BTreeMap<String, String>) -> Result<Arc<dyn FS>, String> {
    // The pinned tsrs bulk loader can panic while sorting mixed file and
    // directory prefixes. Incremental writes also create parent directories.
    let fs = vfstest::from_map(std::iter::empty::<(&str, &str)>(), true);
    for (path, text) in files {
        fs.write_file(path, text)?;
    }
    Ok(Arc::new(bundled::wrap_fs(fs)))
}

fn with_program<T>(
    project: &Project,
    snapshot: &AnalysisSnapshot,
    precise: bool,
    query: impl FnOnce(&'static tsrs_compiler::Program) -> T,
) -> Result<T, String> {
    // Config parsing and main-thread allocations also need a bounded lifetime in
    // the long-lived language service. Only owned Issue values leave this scope.
    let region = tsrs_core::arena::Region::new(1 << 20);
    let _scope = region.enter();
    let mut files: BTreeMap<String, String> = project
        .files
        .iter()
        .map(|(id, source)| (format!("/{}", id.0), source.text().to_owned()))
        .collect();
    // Discover root files from the real project inputs. Virtual declarations are
    // dependencies only and must not turn an otherwise empty project into a valid one.
    let fs = memory_fs(&files)?;
    let host = Host { fs };
    for file in snapshot.files() {
        if let Some(interface) = snapshot.module_interface(file) {
            let stem = file.0.strip_suffix(".xcss").unwrap_or(&file.0);
            files.insert(
                format!("/{stem}.d.xcss.ts"),
                declaration(interface, precise),
            );
        }
    }
    let fs = memory_fs(&files)?;
    let overrides = tsrs_core::CompilerOptions {
        no_emit: Tristate::True,
        no_check: Tristate::False,
        no_implicit_any: Tristate::True,
        allow_arbitrary_extensions: Tristate::True,
        ..Default::default()
    };
    let (config, errors) = tsrs_tsoptions::get_parsed_command_line_of_config_file(
        &format!("/{}", project.config),
        Some(&overrides),
        None,
        &host,
        None,
    );
    let mut reasons = errors
        .iter()
        .chain(config.iter().flat_map(|config| &config.errors))
        .map(|diagnostic| diagnostic.to_string())
        .collect::<Vec<_>>();
    reasons.sort();
    reasons.dedup();
    if !reasons.is_empty() {
        return Err(reasons.join(" "));
    }
    let config = config.ok_or_else(|| format!("Could not load '{}'.", project.config))?;
    if let Some(missing) = config
        .file_names()
        .iter()
        .find(|path| !host.fs.file_exists(path))
    {
        return Err(format!("File '{missing}' not found."));
    }
    let mut options = ProgramOptions::new(
        P::new(config),
        new_compiler_host("/", fs, &bundled::lib_path(), None, None, None),
    );
    options.regions = Some(Arc::new(ProgramRegions::default()));
    options.checker_regions = true;
    options.checker_diagnostic_regions = true;
    options.canonical_diagnostics = false;
    let program = new_program(options);
    let result = query(program);
    // SAFETY: nothing from tsrs escapes this function; all diagnostics are owned copies.
    unsafe {
        free_unshared_program(program);
    }
    Ok(result)
}

fn run(
    project: &Project,
    snapshot: &AnalysisSnapshot,
    precise_issues: Option<Issues>,
) -> Result<(Vec<Diagnostic>, Issues), String> {
    with_program(project, snapshot, precise_issues.is_none(), |program| {
        let issues = semantic_issues(program, project);
        let diagnostics = if let Some(precise) = precise_issues {
            variant_errors(program, snapshot, precise, &issues)
        } else {
            let mut diagnostics = missing_modules(program, project);
            diagnostics.extend(contract_errors(program, snapshot, &issues));
            diagnostics
        };
        (diagnostics, issues)
    })
}

/// Report XCSS contract diagnostics or return the reason TypeScript setup failed.
pub fn check(project: &Project, snapshot: &AnalysisSnapshot) -> Result<Vec<Diagnostic>, String> {
    let (mut diagnostics, precise) = run(project, snapshot, None)?;
    if precise
        .values()
        .flatten()
        .any(|issue| issue.is_value_error())
    {
        let (variants, _) = run(project, snapshot, Some(precise))?;
        diagnostics.extend(variants);
    }
    diagnostics.sort_by(|a, b| {
        (
            &a.location.file,
            a.location.range.start,
            a.location.range.end,
            a.code,
        )
            .cmp(&(
                &b.location.file,
                b.location.range.start,
                b.location.range.end,
                b.code,
            ))
    });
    diagnostics.dedup();
    Ok(diagnostics)
}
