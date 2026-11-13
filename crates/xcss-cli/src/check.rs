use super::*;
use xcss_language_service::{LanguageService, ServiceSnapshot, typescript_warning};

pub(super) fn check_project(args: &[String]) -> Result<(), String> {
    let warning_inputs = args.last().is_some_and(|arg| arg == "--warning-inputs");
    let args = if warning_inputs {
        &args[..args.len() - 1]
    } else {
        args
    };
    let config = match args {
        [] => Ok(None),
        [flag, path] if flag == "--project" => canonical(path)
            .map(Some)
            .map_err(|error| format!("{path}: {error}")),
        _ => return Err("usage: xcss check [--project <tsconfig.json>]".into()),
    };
    let cwd =
        canonical(env::current_dir().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let root = config
        .as_ref()
        .ok()
        .and_then(Option::as_deref)
        .filter(|path| !path.starts_with(&cwd))
        .and_then(Path::parent)
        .unwrap_or(&cwd);
    let resolver =
        xcss_resolver::Resolver::load(root, config.as_ref().map_err(Clone::clone)?.as_deref())?;
    let mut asset_options = GenerateOptions::default();
    let mut paths = Vec::new();
    discover(
        root,
        root,
        &mut Vec::new(),
        &mut paths,
        matches!(config, Ok(Some(_))),
    )?;
    let (stylesheets, typescript): (Vec<_>, Vec<_>) = paths
        .into_iter()
        .partition(|path| path.extension().is_some_and(|ext| ext == "xcss"));
    let mut service = LanguageService::new();
    let mut sources = BTreeMap::new();
    for path in stylesheets {
        let id = module_id(root, &path)?;
        let text = read_source(&path)?;
        service
            .set_file(id.clone(), 1, text.clone())
            .map_err(|e| format!("{e:?}"))?;
        sources.insert(id, text);
    }
    for (id, text) in &sources {
        resolver.assets(root, id, text, &mut asset_options)?;
        for asset in xcss_core::inspect_project_file(text).assets {
            if let Some(Err(error)) = asset_options
                .asset_resolutions
                .get(id)
                .and_then(|m| m.get(&asset.url))
            {
                let prefix = &text[..asset.range.start];
                let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
                let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
                return Err(format!("{}:{line}:{column}: {error}", id.0));
            }
        }
        for specifier in xcss_core::inspect_project_file(text).imports {
            let target = resolver
                .import(&root.join(&id.0), &specifier)
                .map_err(|error| format!("{}: {error}", id.0))?
                .and_then(|p| module_id(root, &p).ok())
                .filter(|id| sources.contains_key(id));
            service.set_resolution(id.clone(), specifier, target);
        }
    }
    let mut failed = report(root, &service.snapshot(), sources.keys());
    let typescript_files = config.as_ref().map_err(Clone::clone).and_then(|config| {
        config
            .as_deref()
            .map(|path| load_typescript(root, path, typescript, &mut service))
            .transpose()
    });
    match typescript_files {
        Ok(Some(files)) => {
            let snapshot = service.snapshot();
            if let Some(warning) = snapshot.typescript_project_warning() {
                eprintln!("Warning: {warning}");
            } else {
                failed |= report(root, &snapshot, &files);
            }
        }
        Err(reason) => eprintln!("Warning: {}", typescript_warning(&reason)),
        Ok(None) => {}
    }
    let snapshot = service.snapshot();
    let range = |r: &xcss_language_service::Range| {
        serde_json::json!({
            "start": { "line": r.start.line, "character": r.start.character },
            "end": { "line": r.end.line, "character": r.end.character },
        })
    };
    let mut inputs = vec![];
    for id in sources.keys() {
        let warnings = snapshot.class_warnings(id);
        if warning_inputs {
            let declarations: Vec<_> = snapshot.css_declarations(id).declarations.iter().map(|d| serde_json::json!({
                "property": d.property, "propertyRange": range(&d.property_range), "valueRange": range(&d.value_range),
                "context": d.context, "checkable": d.checkable,
                "parts": d.parts.iter().map(|p| serde_json::json!({"text": p.text, "valueType": p.value_type, "range": range(&p.range)})).collect::<Vec<_>>()
            })).collect();
            inputs.push(serde_json::json!({
                "file": root.join(&id.0).to_string_lossy(), "declarations": declarations,
                "warnings": warnings.iter().map(|w| serde_json::json!({"message": w.message, "range": range(&w.range)})).collect::<Vec<_>>()
            }));
        } else {
            for warning in warnings {
                eprintln!(
                    "{}:{}:{}: Warning: {}",
                    root.join(&id.0).display(),
                    warning.range.start.line + 1,
                    warning.range.start.character + 1,
                    warning.message
                );
            }
        }
    }
    if warning_inputs {
        println!("{}", serde_json::json!(inputs));
    }
    if failed {
        Err("XCSS checking failed".into())
    } else {
        Ok(())
    }
}

fn report<'a>(
    root: &Path,
    snapshot: &ServiceSnapshot,
    files: impl IntoIterator<Item = &'a FileId>,
) -> bool {
    let mut failed = false;
    for id in files {
        for d in snapshot.diagnostics(id) {
            failed |= d.diagnostic.severity == Severity::Error;
            eprintln!(
                "{}:{}:{}: {} {} — {}",
                root.join(&d.location.file.0).display(),
                d.location.range.start.line + 1,
                d.location.range.start.character + 1,
                d.diagnostic.code,
                d.label,
                d.diagnostic.message
            );
        }
    }
    failed
}

fn read_source(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn load_typescript(
    root: &Path,
    config: &Path,
    paths: Vec<PathBuf>,
    service: &mut LanguageService,
) -> Result<Vec<FileId>, String> {
    let config_id = module_id(root, config)?;
    let mut files = BTreeMap::from([(config_id.clone(), read_source(config)?)]);
    for path in paths {
        let id = module_id(root, &path)?;
        if let std::collections::btree_map::Entry::Vacant(entry) = files.entry(id) {
            entry.insert(read_source(&path)?);
        }
    }
    let mut ids = Vec::with_capacity(files.len());
    for (id, text) in files {
        service
            .set_typescript_file(id.clone(), 1, text)
            .map_err(|error| format!("{}: {error:?}", id.0))?;
        ids.push(id);
    }
    service.set_typescript_project(Some(config_id.0));
    Ok(ids)
}

fn discover(
    root: &Path,
    directory: &Path,
    ancestors: &mut Vec<PathBuf>,
    files: &mut Vec<PathBuf>,
    typescript: bool,
) -> Result<(), String> {
    let real = canonical(directory).map_err(|e| e.to_string())?;
    if ancestors.contains(&real) {
        return Ok(());
    }
    ancestors.push(real);
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            let dependency = path.strip_prefix(root).is_ok_and(|relative| {
                relative
                    .components()
                    .any(|part| part.as_os_str() == "node_modules")
            });
            if [".git", ".cache", ".pnpm"]
                .iter()
                .any(|s| path.file_name().is_some_and(|n| n == *s))
                || !dependency
                    && ["target", "dist", "coverage"]
                        .iter()
                        .any(|s| path.file_name().is_some_and(|n| n == *s))
            {
                continue;
            }
            discover(root, &path, ancestors, files, typescript)?;
        } else if path.extension().is_some_and(|s| {
            s == "xcss"
                || typescript
                    && ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "json"]
                        .iter()
                        .any(|ext| s == *ext)
        }) {
            files.push(path);
        }
    }
    ancestors.pop();
    Ok(())
}
