use super::*;
use xcss_language_service::LanguageService;

pub(super) fn check_project(args: &[String]) -> Result<(), String> {
    if args.len() != 2 || args[0] != "--project" {
        return Err("usage: xcss check --project <tsconfig.json>".into());
    }
    let config = canonical(PathBuf::from(&args[1])).map_err(|e| e.to_string())?;
    let cwd =
        canonical(env::current_dir().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let root = if config.starts_with(&cwd) {
        cwd
    } else {
        config.parent().unwrap().to_path_buf()
    };
    let mut service = LanguageService::new();
    let mut sources = BTreeMap::new();
    load(&root, &root, &mut Vec::new(), &mut sources)?;
    for (id, text) in &sources {
        if id.0.ends_with(".xcss") {
            service
                .set_file(id.clone(), 1, text.clone())
                .map_err(|e| format!("{e:?}"))?;
        } else {
            service
                .set_typescript_file(id.clone(), 1, text.clone())
                .map_err(|e| format!("{e:?}"))?;
        }
    }
    for (id, text) in sources.iter().filter(|(id, _)| id.0.ends_with(".xcss")) {
        for specifier in xcss_core::inspect_project_file(text).imports {
            let candidate = if let Some(alias) = specifier.strip_prefix("@/") {
                root.join(alias)
            } else {
                root.join(&id.0).parent().unwrap().join(&specifier)
            };
            let target = canonical(candidate)
                .ok()
                .and_then(|p| module_id(&root, &p).ok())
                .filter(|id| sources.contains_key(id));
            service.set_resolution(id.clone(), specifier, target);
        }
    }
    service.set_typescript_project(Some(module_id(&root, &config)?.0));
    let snapshot = service.snapshot();
    let mut failed = false;
    for id in sources.keys() {
        for d in snapshot.diagnostics(id) {
            failed |= d.diagnostic.severity == Severity::Error;
            eprintln!(
                "{}:{}:{}: {} {}",
                root.join(&d.location.file.0).display(),
                d.location.range.start.line + 1,
                d.location.range.start.character + 1,
                d.diagnostic.code,
                d.diagnostic.message
            );
        }
    }
    if failed {
        Err("XCSS checking failed".into())
    } else {
        Ok(())
    }
}

fn load(
    root: &Path,
    directory: &Path,
    ancestors: &mut Vec<PathBuf>,
    files: &mut BTreeMap<FileId, String>,
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
            load(root, &path, ancestors, files)?;
        } else if path.extension().is_some_and(|s| {
            [
                "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "json", "xcss",
            ]
            .iter()
            .any(|ext| s == *ext)
        }) {
            files.insert(
                module_id(root, &path)?,
                fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?,
            );
        }
    }
    ancestors.pop();
    Ok(())
}
