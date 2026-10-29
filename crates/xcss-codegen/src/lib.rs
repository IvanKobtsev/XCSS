//! Scoped CSS emission from an immutable XCSS analysis snapshot.
use std::collections::{BTreeMap, BTreeSet};
use xcss_analysis::{AnalysisSnapshot, DependencyKind, SymbolId, SymbolKind};
use xcss_source::{Diagnostic, FileId, Location, Severity, TextRange};
use xcss_syntax::{Node, NodeKind, ParseResult};
mod lower;
mod mapped;
mod registration;
mod values;
use mapped::Mapped;

#[derive(Clone, Debug, Default)]
pub struct GenerateOptions {
    pub minify: bool,
    pub output_paths: BTreeMap<FileId, String>,
    pub bundle: bool,
    /// Optional host path coordinate system for asset rebasing; never used for hashes.
    pub source_root: Option<String>,
}
#[derive(Clone, Debug)]
pub struct GeneratedModule {
    pub module_id: FileId,
    pub css: String,
    pub source_map: String,
    pub classes: BTreeMap<String, String>,
    pub custom_properties: BTreeMap<String, String>,
    pub stylesheet_dependencies: Vec<FileId>,
    pub native_imports: Vec<String>,
    pub source_dependencies: Vec<FileId>,
}
#[derive(Clone, Debug)]
pub struct GeneratedProject {
    pub entry: FileId,
    pub modules: Vec<GeneratedModule>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn generate(
    snapshot: &AnalysisSnapshot,
    entry: &FileId,
    options: &GenerateOptions,
) -> Result<GeneratedProject, Vec<Diagnostic>> {
    if snapshot.syntax(entry).is_none() {
        return Err(vec![Diagnostic::error(
            "generate.missing-entry",
            entry,
            TextRange::default(),
            "Entry module is not loaded",
        )]);
    }
    let mut required = BTreeSet::from([entry.clone()]);
    let mut changed = true;
    while changed {
        changed = false;
        for dep in snapshot.dependencies() {
            if required.contains(&dep.from) {
                changed |= required.insert(dep.to.clone());
            }
        }
    }
    for file in &required {
        if let Some(parsed) = snapshot.syntax(file) {
            for node in parsed.tree.descendants() {
                if let Some(composition) = parsed.composition(node)
                    && let xcss_syntax::ast::CompositionSource::Module(specifier) =
                        composition.source
                    && specifier
                        .split('?')
                        .next()
                        .unwrap_or(specifier)
                        .ends_with(".css")
                {
                    return Err(vec![Diagnostic::error(
                        "generate.external-composition",
                        file,
                        node.header,
                        "Composition from external CSS Modules requires an adapter; use a project XCSS module",
                    )]);
                }
            }
        }
    }
    let errors: Vec<_> = snapshot
        .diagnostics()
        .iter()
        .filter(|d| required.contains(&d.location.file) && d.severity == Severity::Error)
        .cloned()
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    for file in &required {
        if file.0.starts_with('/') || file.0.contains(':') {
            return Err(vec![Diagnostic::error(
                "generate.module-id",
                file,
                TextRange::default(),
                "Generation requires project-relative module identities",
            )]);
        }
    }
    let mut emitted = BTreeSet::from([entry.clone()]);
    let mut changed = true;
    while changed {
        changed = false;
        for dep in snapshot.dependencies() {
            if dep.kind == DependencyKind::Composition && emitted.contains(&dep.from) {
                if !dep.to.0.ends_with(".xcss") {
                    return Err(vec![Diagnostic::error(
                        "generate.external-composition",
                        &dep.from,
                        TextRange::default(),
                        "Composition from external CSS Modules is not supported",
                    )]);
                }
                changed |= emitted.insert(dep.to.clone());
            }
        }
    }
    let mut modules = Vec::new();
    let mut diagnostics = Vec::new();
    for file in &emitted {
        let output_file = if options.bundle { entry } else { file };
        let output_identity = options
            .output_paths
            .get(output_file)
            .cloned()
            .unwrap_or_else(|| {
                options
                    .source_root
                    .as_ref()
                    .map(|root| format!("{root}/{}", output_file.0))
                    .unwrap_or_else(|| output_file.0.clone())
            });
        let mut lower = lower::Lowerer::new(
            snapshot,
            file,
            &output_identity,
            options.source_root.as_deref(),
        );
        let mapped = lower.module()?;
        diagnostics.extend(lower.diagnostics);
        let (css, source_map, exports) = backend(snapshot, file, &mapped, options, &lower.globals)?;
        let mut classes = BTreeMap::new();
        // Parse the expanded module so classes introduced by injections belong to the caller.
        let expanded = xcss_syntax::parse(std::sync::Arc::new(xcss_source::SourceFile::new(
            file.clone(),
            0,
            mapped.text.clone(),
        )));
        let class_names = xcss_syntax::local_class_names(&expanded);
        for name in class_names {
            if let Some(compiled) = exports.get(&name) {
                classes.insert(name, compiled.clone());
            }
        }
        modules.push(GeneratedModule {
            module_id: file.clone(),
            css,
            source_map,
            classes,
            custom_properties: lower.properties,
            stylesheet_dependencies: snapshot
                .dependencies()
                .iter()
                .filter(|d| {
                    d.from == *file && d.kind == DependencyKind::Composition && d.to != *file
                })
                .map(|d| d.to.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            native_imports: lower.native_imports.into_iter().collect(),
            source_dependencies: lower.sources.into_iter().collect(),
        });
    }
    let base: BTreeMap<_, _> = modules
        .iter()
        .map(|m| (m.module_id.clone(), m.classes.clone()))
        .collect();
    for module in &mut modules {
        for (name, value) in &mut module.classes {
            if let Some(symbol) = snapshot.symbols().iter().find(|s| {
                s.location.file == module.module_id
                    && s.kind == SymbolKind::Class
                    && s.name == *name
                    && !s.global
            }) {
                let mut names = vec![value.clone()];
                compose(snapshot, symbol.id, &base, &mut names, &mut BTreeSet::new());
                let mut seen = BTreeSet::new();
                *value = names
                    .into_iter()
                    .filter(|n| seen.insert(n.clone()))
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
    }
    if options.bundle {
        modules = vec![bundle_modules(entry, &modules)?];
    }
    Ok(GeneratedProject {
        entry: entry.clone(),
        modules,
        diagnostics,
    })
}
fn bundle_modules(
    entry: &FileId,
    modules: &[GeneratedModule],
) -> Result<GeneratedModule, Vec<Diagnostic>> {
    fn visit<'a>(
        file: &FileId,
        modules: &'a [GeneratedModule],
        seen: &mut BTreeSet<FileId>,
        out: &mut Vec<&'a GeneratedModule>,
    ) {
        if !seen.insert(file.clone()) {
            return;
        }
        let module = modules.iter().find(|m| m.module_id == *file).unwrap();
        for dep in &module.stylesheet_dependencies {
            visit(dep, modules, seen, out);
        }
        out.push(module);
    }
    let mut ordered = Vec::new();
    visit(entry, modules, &mut BTreeSet::new(), &mut ordered);
    let mut artifact = modules
        .iter()
        .find(|m| m.module_id == *entry)
        .unwrap()
        .clone();
    artifact.css.clear();
    artifact.stylesheet_dependencies.clear();
    let mut map = parcel_sourcemap::SourceMap::new("/");
    let mut sources = BTreeSet::new();
    let mut imports = Vec::new();
    let mut bodies = Vec::new();
    let mut native_imports = BTreeSet::new();
    for module in &ordered {
        let parsed = xcss_syntax::parse(std::sync::Arc::new(xcss_source::SourceFile::new(
            module.module_id.clone(),
            0,
            module.css.clone(),
        )));
        let mut cursor = 0;
        for node in parsed
            .tree
            .children
            .iter()
            .filter(|n| n.kind == NodeKind::Import)
        {
            if cursor < node.range.start {
                bodies.push((*module, TextRange::new(cursor, node.range.start)));
            }
            imports.push((*module, node.range));
            cursor = node.range.end;
        }
        if cursor < module.css.len() {
            bodies.push((*module, TextRange::new(cursor, module.css.len())));
        }
        sources.extend(module.source_dependencies.iter().cloned());
        native_imports.extend(module.native_imports.iter().cloned());
    }
    for (module, range) in imports.into_iter().chain(bodies) {
        let source =
            parcel_sourcemap::SourceMap::from_json("/", &module.source_map).map_err(|e| {
                vec![Diagnostic::error(
                    "generate.source-map",
                    entry,
                    TextRange::default(),
                    e.to_string(),
                )]
            })?;
        let base = artifact.css.len();
        artifact.css.push_str(&module.css[range.start..range.end]);
        if !artifact.css.ends_with('\n') {
            artifact.css.push('\n');
        }
        let original =
            xcss_source::SourceFile::new(module.module_id.clone(), 0, module.css.clone());
        let generated = xcss_source::SourceFile::new(entry.clone(), 0, artifact.css.clone());
        let indexes: Vec<_> = source
            .get_sources()
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let index = map.add_source(name);
                if let Ok(content) = source.get_source_content(i as u32) {
                    let _ = map.set_source_content(index as usize, content);
                }
                index
            })
            .collect();
        for mapping in source.get_mappings() {
            let Some(offset) = original.utf16_to_byte(xcss_source::Position {
                line: mapping.generated_line as usize,
                character: mapping.generated_column as usize,
            }) else {
                continue;
            };
            if !range.contains(offset) {
                continue;
            }
            let position = generated
                .byte_to_utf16(base + offset - range.start)
                .unwrap();
            let origin = mapping.original.map(|mut origin| {
                origin.source = indexes[origin.source as usize];
                origin.name = origin
                    .name
                    .and_then(|i| source.get_name(i).ok())
                    .map(|name| map.add_name(name));
                origin
            });
            map.add_mapping(position.line as u32, position.character as u32, origin);
        }
    }
    artifact.source_dependencies = sources.into_iter().collect();
    artifact.native_imports = native_imports.into_iter().collect();
    artifact.source_map = map.to_json(None).map_err(|e| {
        vec![Diagnostic::error(
            "generate.source-map",
            entry,
            TextRange::default(),
            e.to_string(),
        )]
    })?;
    Ok(artifact)
}
fn compose(
    snapshot: &AnalysisSnapshot,
    id: SymbolId,
    base: &BTreeMap<FileId, BTreeMap<String, String>>,
    out: &mut Vec<String>,
    seen: &mut BTreeSet<SymbolId>,
) {
    if !seen.insert(id) {
        return;
    }
    for composition in snapshot.compositions().iter().filter(|c| c.owner == id) {
        for member in &composition.members {
            if let Some(target) = member.target {
                let symbol = snapshot.symbol(target).unwrap();
                if let Some(name) = base
                    .get(&symbol.location.file)
                    .and_then(|m| m.get(&symbol.name))
                {
                    out.push(name.clone());
                }
                compose(snapshot, target, base, out, seen);
            } else {
                out.push(member.name.clone());
            }
        }
    }
}
pub fn scoped_property(file: &FileId, name: &str) -> String {
    let hash = file.0.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    format!("{name}-xcss-{hash:016x}")
}
fn backend(
    snapshot: &AnalysisSnapshot,
    file: &FileId,
    input: &Mapped,
    options: &GenerateOptions,
    globals: &BTreeMap<String, String>,
) -> Result<(String, String, BTreeMap<String, String>), Vec<Diagnostic>> {
    use lightningcss::{
        css_modules::{Config, Pattern},
        stylesheet::{ParserOptions, PrinterOptions, StyleSheet},
    };
    let css_modules = Config {
        pattern: Pattern::parse("xcss_[hash]_[local]").unwrap(),
        dashed_idents: false,
        ..Config::default()
    };
    let sheet = StyleSheet::parse(
        &input.text,
        ParserOptions {
            filename: file.0.clone(),
            css_modules: Some(css_modules),
            ..ParserOptions::default()
        },
    )
    .map_err(|error| {
        vec![input.error(
            snapshot,
            file,
            "generate.css-parse",
            error.to_string(),
            error.loc.map(|l| (l.line, l.column.saturating_sub(1))),
        )]
    })?;
    let mut map = parcel_sourcemap::SourceMap::new("/");
    let mut result = sheet
        .to_css(PrinterOptions {
            minify: options.minify,
            source_map: Some(&mut map),
            ..PrinterOptions::default()
        })
        .map_err(|error| {
            vec![input.error(
                snapshot,
                file,
                "generate.css-print",
                error.to_string(),
                error.loc.map(|l| (l.line, l.column.saturating_sub(1))),
            )]
        })?;
    let mut original = input.source_map(snapshot);
    map.extends(&mut original).map_err(|e| {
        vec![Diagnostic::error(
            "generate.source-map",
            file,
            TextRange::default(),
            e.to_string(),
        )]
    })?;
    if !globals.is_empty() {
        use lightningcss::traits::ToCss;
        let replacements: BTreeMap<_, _> = globals
            .iter()
            .filter_map(|(placeholder, name)| {
                result
                    .exports
                    .as_ref()?
                    .get(placeholder)
                    .map(|export| (export.name.clone(), name.clone()))
            })
            .collect();
        let printed = xcss_syntax::parse(std::sync::Arc::new(xcss_source::SourceFile::new(
            file.clone(),
            0,
            result.code.clone(),
        )));
        for token in printed
            .tokens
            .iter()
            .rev()
            .filter(|t| t.kind == xcss_syntax::TokenKind::Word)
        {
            if let Some(name) = replacements.get(printed.token_text(token)) {
                let name = lightningcss::values::ident::CustomIdent(name.as_str().into())
                    .to_css_string(PrinterOptions::default())
                    .unwrap();
                let position = printed.source.byte_to_utf16(token.range.end).unwrap();
                map.offset_columns(
                    position.line as u32,
                    position.character as u32,
                    name.encode_utf16().count() as i64
                        - printed.token_text(token).encode_utf16().count() as i64,
                )
                .map_err(|e| {
                    vec![Diagnostic::error(
                        "generate.source-map",
                        file,
                        TextRange::default(),
                        e.to_string(),
                    )]
                })?;
                result
                    .code
                    .replace_range(token.range.start..token.range.end, &name);
            }
        }
    }
    input.map_printed(snapshot, &result.code, &mut map);
    let source_map = map.to_json(None).map_err(|e| {
        vec![Diagnostic::error(
            "generate.source-map",
            file,
            TextRange::default(),
            e.to_string(),
        )]
    })?;
    let exports = result
        .exports
        .unwrap_or_default()
        .into_iter()
        .map(|(name, value)| (name, value.name))
        .collect();
    Ok((result.code, source_map, exports))
}
