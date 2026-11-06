//! XCSS-specific TypeScript checking over host-supplied, immutable project inputs.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tsrs_compiler::{
    Context, ProgramOptions, ProgramRegions, free_unshared_program, new_compiler_host, new_program,
};
use tsrs_core::{P, Tristate};
use tsrs_tsoptions::ParseConfigHost;
use tsrs_vfs::{FS, bundled, vfstest};
use xcss_analysis::{AnalysisSnapshot, ModuleInterface};
use xcss_source::{Diagnostic, FileId, SourceFile, TextRange};

/// Paths are project-relative forward-slash paths. Include configs, package metadata,
/// dependencies and declarations as well as TS sources. No disk reads occur here.
#[derive(Clone, Default)]
pub struct Project {
    pub config: String,
    pub files: BTreeMap<FileId, Arc<SourceFile>>,
}

pub fn declaration(interface: &ModuleInterface, precise: bool) -> String {
    if !precise {
        return "declare const styles: Record<string, string>; export default styles; export type Variants<Group extends string> = string & { readonly __xcssVariantGroup__?: Group };".into();
    }
    let quote = |s: &str| serde_json::to_string(s).unwrap();
    let mut text = String::from("declare const styles: {\n");
    for class in &interface.classes {
        text.push_str(&format!("  {}: string;\n", quote(class)));
    }
    text.push_str("};\nexport default styles;\ntype Groups = {\n");
    for (group, members) in &interface.variants {
        text.push_str(&format!(
            "  {}: {};\n",
            quote(group),
            members
                .iter()
                .map(|m| quote(m))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
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

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Issue {
    file: String,
    start: usize,
    end: usize,
    code: i32,
    message: String,
}

fn copy_issue(d: P<tsrs_ast::Diagnostic>, config: &str) -> Issue {
    Issue {
        file: d
            .file()
            .map(|f| f.file_name().to_owned())
            .unwrap_or_else(|| format!("/{config}")),
        start: d.pos().unwrap_or(0) as usize,
        end: d.end().unwrap_or(0) as usize,
        code: d.code(),
        message: d.to_string(),
    }
}

fn run(project: &Project, snapshot: &AnalysisSnapshot, precise: bool) -> (Vec<Issue>, Vec<Issue>) {
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
    let host = Host {
        fs: Arc::new(bundled::wrap_fs(vfstest::from_map(files.iter(), true))),
    };
    for file in snapshot.files() {
        if let Some(interface) = snapshot.module_interface(file) {
            let stem = file.0.strip_suffix(".xcss").unwrap_or(&file.0);
            files.insert(
                format!("/{stem}.d.xcss.ts"),
                declaration(interface, precise),
            );
        }
    }
    let fs: Arc<dyn FS> = Arc::new(bundled::wrap_fs(vfstest::from_map(files, true)));
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
    let mut configuration: Vec<_> = errors
        .into_iter()
        .map(|d| copy_issue(d, &project.config))
        .collect();
    let Some(config) = config else {
        return (configuration, Vec::new());
    };
    configuration.extend(
        config
            .errors
            .iter()
            .map(|&d| copy_issue(d, &project.config)),
    );
    let mut options = ProgramOptions::new(
        P::new(config),
        new_compiler_host("/", fs, &bundled::lib_path(), None, None, None),
    );
    options.regions = Some(Arc::new(ProgramRegions::default()));
    options.checker_regions = true;
    options.checker_diagnostic_regions = true;
    options.canonical_diagnostics = false;
    let program = new_program(options);
    // Ambient wildcard declarations must not conceal deleted/missing XCSS modules.
    for &file in program.source_files() {
        for &specifier in file.imports() {
            if specifier.text().ends_with(".xcss")
                && program
                    .get_resolved_module_from_module_specifier(file, specifier)
                    .is_none_or(|m| !m.is_resolved())
            {
                configuration.push(Issue {
                    file: file.file_name().to_owned(),
                    start: specifier.loc().source_pos().unwrap_or(0) as usize,
                    end: specifier.loc().source_end().unwrap_or(0) as usize,
                    code: 2307,
                    message: format!("Cannot resolve XCSS module '{}'.", specifier.text()),
                });
            }
        }
    }
    configuration.extend(
        program
            .get_program_diagnostics()
            .into_iter()
            .map(|d| copy_issue(d, &project.config)),
    );
    let diagnostics = program.get_semantic_diagnostics(&Context::default(), None);
    configuration.extend(
        diagnostics
            .iter()
            .filter(|d| matches!(d.code(), 2307 | 2688 | 7016))
            .map(|&d| copy_issue(d, &project.config)),
    );
    let issues = diagnostics
        .into_iter()
        .map(|d| copy_issue(d, &project.config))
        .collect();
    // SAFETY: nothing from tsrs escapes this function; all diagnostics are owned copies.
    unsafe {
        free_unshared_program(program);
    }
    (configuration, issues)
}

/// Check twice so diagnostics unrelated to XCSS's stricter module contracts are omitted.
pub fn check(project: &Project, snapshot: &AnalysisSnapshot) -> Vec<Diagnostic> {
    let (configuration, precise) = run(project, snapshot, true);
    let (_, permissive) = run(project, snapshot, false);
    let keys: BTreeSet<_> = permissive
        .iter()
        .map(|d| (&d.file, d.start, d.end, d.code))
        .collect();
    let issues: BTreeSet<_> = configuration
        .into_iter()
        .chain(
            precise
                .into_iter()
                .filter(|d| !keys.contains(&(&d.file, d.start, d.end, d.code))),
        )
        .collect();
    issues
        .into_iter()
        .filter_map(|issue| {
            let file = FileId(issue.file.trim_start_matches('/').to_owned());
            let source = project.files.get(&file);
            if source.is_none() && file.0 != project.config {
                return None;
            }
            if file.0.ends_with(".d.xcss.ts") {
                return None;
            }
            let len = source.map_or(0, |s| s.text().len());
            let mut diagnostic = Diagnostic::error(
                "typescript.xcss",
                &file,
                TextRange::new(issue.start.min(len), issue.end.min(len)),
                format!("TS{}: {}", issue.code, issue.message),
            );
            diagnostic.related = Vec::new();
            Some(diagnostic)
        })
        .collect()
}
