use super::*;
#[derive(Clone, Debug, Default)]
pub struct ProjectNames {
    pub local: BTreeMap<(FileId, String), String>,
    pub global: BTreeMap<String, String>,
    pub override_markers: xcss_syntax::OverrideMarkers,
}
/// Plan every module before emitting any entry point.
pub fn project_names(
    snapshot: &AnalysisSnapshot,
    options: &GenerateOptions,
) -> Result<ProjectNames, Vec<Diagnostic>> {
    for (file, classes) in &options.override_uses {
        for (class, levels) in classes {
            if !snapshot
                .module_interface(file)
                .is_some_and(|m| m.classes.contains(class))
                || levels.iter().any(|level| {
                    level.starts_with('0')
                        || level.is_empty()
                        || !level.bytes().all(|b| b.is_ascii_digit())
                })
            {
                return Err(vec![Diagnostic::error(
                    "generate.override-request",
                    file,
                    TextRange::default(),
                    "Override requests require an existing XCSS class and a canonical positive integer level",
                )]);
            }
        }
    }
    if let Some(names) = &options.planned_names {
        return Ok((**names).clone());
    }
    use lightningcss::{
        css_modules::{Config, Pattern},
        stylesheet::{ParserOptions, PrinterOptions, StyleSheet},
    };
    let mut identities = BTreeSet::new();
    let mut reserved = BTreeSet::new();
    let mut local = Vec::new();
    let mut global = Vec::new();
    let mut contributed = BTreeMap::new();
    for symbol in snapshot.symbols().iter().filter(|s| s.global) {
        let ns = if symbol.kind == SymbolKind::Keyframe {
            "keyframe"
        } else {
            "class"
        };
        reserved.insert((ns.to_string(), symbol.name.clone()));
    }
    for file in snapshot.files() {
        // Invalid unrelated modules must not prevent compiling a valid entry.
        if snapshot
            .diagnostics()
            .iter()
            .any(|d| d.location.file == *file && d.severity == Severity::Error)
        {
            continue;
        }
        let mut lower =
            lower::Lowerer::new(snapshot, file, &file.0, options.source_root.as_deref());
        lower.asset_resolutions = Some(&options.asset_resolutions);
        let input = lower.module()?;
        let text = without_line_comments(&input.text);
        let sheet = StyleSheet::parse(
            &text,
            ParserOptions {
                filename: file.0.clone(),
                css_modules: Some(Config {
                    pattern: Pattern::parse("[local]").unwrap(),
                    ..Config::default()
                }),
                ..ParserOptions::default()
            },
        );
        if let Ok(sheet) = sheet
            && let Ok(result) = sheet.to_css(PrinterOptions::default())
        {
            for name in result.exports.unwrap_or_default().keys() {
                if lower.globals.contains_key(name) {
                    continue;
                }
                let ns = if snapshot.symbols().iter().any(|s| {
                    s.location.file == *file && s.kind == SymbolKind::Keyframe && s.name == *name
                }) {
                    "keyframe"
                } else {
                    "class"
                };
                let key = (ns.to_string(), name.clone(), file.0.clone());
                identities.insert(key.clone());
                local.push(((file.clone(), name.clone()), key));
            }
        }
        for (owner, classes) in &lower.utility_classes {
            contributed.insert((file.clone(), owner.clone()), classes.clone());
        }
        for class in lower.utility_classes.values().flatten() {
            let (base, identity) = &lower.utility_names[class];
            let key = (
                "class".to_string(),
                base.clone(),
                format!("utility:{identity}"),
            );
            identities.insert(key.clone());
            global.push((class.clone(), key));
        }
    }
    let names = xcss_source::Names::allocate(snapshot.suffix_length(), identities, &reserved)
        .map_err(|message| {
            vec![Diagnostic::error(
                "naming.capacity",
                snapshot.files().next().unwrap_or(&FileId(String::new())),
                TextRange::default(),
                message,
            )]
        })?;
    let lookup =
        |key: &(String, String, String)| names.get(&key.0, &key.1, &key.2).unwrap().to_owned();
    let mut result = ProjectNames {
        local: local
            .into_iter()
            .map(|(key, identity)| (key, lookup(&identity)))
            .collect(),
        global: global
            .into_iter()
            .map(|(key, identity)| (key, lookup(&identity)))
            .collect(),
        override_markers: BTreeMap::new(),
    };
    let mut used: BTreeSet<String> = result
        .local
        .values()
        .chain(result.global.values())
        .cloned()
        .chain(
            reserved
                .iter()
                .filter(|(ns, _)| ns == "class")
                .map(|(_, name)| name.clone()),
        )
        .collect();
    for (file, classes) in &options.override_uses {
        for (class, levels) in classes {
            let Some(base) = result.local.get(&(file.clone(), class.clone())) else {
                continue;
            };
            let tokens = std::iter::once(base.clone())
                .chain(
                    contributed
                        .get(&(file.clone(), class.clone()))
                        .into_iter()
                        .flatten()
                        .filter_map(|u| result.global.get(u).cloned()),
                )
                .collect::<Vec<_>>();
            for token in tokens {
                for level in levels {
                    if result
                        .override_markers
                        .get(&token)
                        .is_some_and(|m| m.contains_key(level))
                    {
                        continue;
                    }
                    let base = format!("{token}_override_{level}");
                    let mut marker = base.clone();
                    let mut suffix = 0;
                    while !used.insert(marker.clone()) {
                        suffix += 1;
                        marker = format!("{base}_{suffix}");
                    }
                    result
                        .override_markers
                        .entry(token.clone())
                        .or_default()
                        .insert(level.clone(), marker);
                }
            }
        }
    }
    Ok(result)
}
