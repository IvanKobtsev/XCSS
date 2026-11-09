use super::*;
use xcss_language_service::{LanguageService, ServiceSnapshot, typescript_warning};

pub(super) fn check_project(args: &[String]) -> Result<(), String> {
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
        for specifier in xcss_core::inspect_project_file(text).imports {
            let candidate = if let Some(alias) = specifier.strip_prefix("@/") {
                root.join(alias)
            } else {
                root.join(&id.0).parent().unwrap().join(&specifier)
            };
            let target = canonical(candidate)
                .ok()
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
