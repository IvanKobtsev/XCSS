use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process,
};
use xcss_core::{AnalysisHost, FileId, GenerateOptions, Severity};
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = env::args().skip(1).collect();
    if !matches!(args.len(), 4 | 6)
        || args[0] != "compile"
        || !matches!(args[2].as_str(), "--output" | "-o")
        || args.len() == 6 && args[4] != "--root"
    {
        return Err(
            "usage: xcss compile <input.xcss> --output <output.css> [--root <directory>]".into(),
        );
    }
    let input = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[3]);
    if input.extension().is_none_or(|s| s != "xcss") {
        return Err("input must end in .xcss".into());
    }
    if output.extension().is_none_or(|s| s != "css")
        || output.to_string_lossy().ends_with(".module.css")
    {
        return Err(
            "output must end in .css, without .module.css (XCSS already scopes names)".into(),
        );
    }
    let root = canonical(if args.len() == 6 {
        PathBuf::from(&args[5])
    } else {
        env::current_dir().map_err(|e| e.to_string())?
    })
    .map_err(|e| e.to_string())?;
    let input = canonical(input).map_err(|e| e.to_string())?;
    let output = portable_path(if output.is_absolute() {
        output
    } else {
        env::current_dir().map_err(|e| e.to_string())?.join(output)
    });
    if input == output {
        return Err("input and output must differ".into());
    }
    let mut paths = Vec::new();
    scan(&root, &mut paths)?;
    paths.sort();
    let mut host = AnalysisHost::new();
    let mut files = BTreeMap::new();
    for path in paths {
        let module = module_id(&root, &path)?;
        let source = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        host.set_file(module.clone(), 1, source.clone())
            .map_err(|e| format!("{e:?}"))?;
        files.insert(module, (path, source));
    }
    for (module, (path, source)) in &files {
        for specifier in xcss_core::inspect_project_file(source).imports {
            let candidate = if let Some(alias) = specifier.strip_prefix("@/") {
                root.join(alias)
            } else {
                path.parent().unwrap().join(&specifier)
            };
            let target = canonical(candidate)
                .ok()
                .and_then(|p| module_id(&root, &p).ok())
                .filter(|id| files.contains_key(id));
            host.set_resolution(module.clone(), specifier, target);
        }
    }
    let entry = module_id(&root, &input)?;
    let destinations: BTreeMap<_, _> = files
        .keys()
        .map(|id| {
            let path = if *id == entry {
                output.clone()
            } else {
                output
                    .parent()
                    .unwrap()
                    .join(".xcss")
                    .join(&id.0)
                    .with_extension("css")
            };
            (id.clone(), path)
        })
        .collect();
    let output_paths = destinations
        .iter()
        .map(|(id, path)| (id.clone(), path.to_string_lossy().replace('\\', "/")))
        .collect();
    let snapshot = host.snapshot();
    let result = xcss_core::generate(
        &snapshot,
        &entry,
        &GenerateOptions {
            output_paths,
            source_root: Some(root.to_string_lossy().replace('\\', "/")),
            ..GenerateOptions::default()
        },
    )
    .map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|d| {
                let pos = snapshot
                    .syntax(&d.location.file)
                    .and_then(|p| p.source.byte_to_utf16(d.location.range.start));
                format!(
                    "{}:{}:{}: {}: {}",
                    d.location.file.0,
                    pos.map(|p| p.line + 1).unwrap_or(1),
                    pos.map(|p| p.character + 1).unwrap_or(1),
                    d.code,
                    d.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    for warning in &result.diagnostics {
        if warning.severity == Severity::Warning {
            eprintln!("{}: warning: {}", warning.location.file.0, warning.message);
        }
    }
    let mut writes = Vec::new();
    for module in result.modules {
        let path = &destinations[&module.module_id];
        let parent = path.parent().unwrap();
        let mut imports = String::new();
        for dependency in &module.stylesheet_dependencies {
            let relative = pathdiff::diff_paths(&destinations[dependency], parent)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            imports.push_str(&format!(
                "@import {};\n",
                serde_json::to_string(&format!("./{relative}")).unwrap()
            ));
        }
        let mut css = module.css;
        if css.starts_with("@charset ") {
            if let Some(end) = css.find(';') {
                css.insert_str(end + 1, &format!("\n{imports}"));
            }
        } else {
            css = format!("{imports}{css}");
        }
        let map_path = PathBuf::from(format!("{}.map", path.display()));
        css.push_str(&format!(
            "\n/*# sourceMappingURL={} */\n",
            map_path.file_name().unwrap().to_string_lossy()
        ));
        let mut map: serde_json::Value =
            serde_json::from_str(&module.source_map).map_err(|e| e.to_string())?;
        map["sourceRoot"] = serde_json::Value::String(
            pathdiff::diff_paths(&root, parent)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
        );
        map["file"] =
            serde_json::Value::String(path.file_name().unwrap().to_string_lossy().into_owned());
        // @import insertion shifts generated lines in the CSS artifact.
        if !imports.is_empty() {
            let count = imports.lines().count();
            let mappings = map["mappings"].as_str().unwrap_or("");
            map["mappings"] = serde_json::Value::String(format!("{}{mappings}", ";".repeat(count)));
        }
        writes.push((path.clone(), css));
        writes.push((map_path, serde_json::to_string(&map).unwrap()));
        writes.push((
            path.with_extension("exports.json"),
            serde_json::to_string_pretty(&module.classes).unwrap(),
        ));
    }
    write_transaction(writes)
}
fn module_id(root: &Path, path: &Path) -> Result<FileId, String> {
    path.strip_prefix(root)
        .map(|p| FileId(p.to_string_lossy().replace('\\', "/")))
        .map_err(|_| format!("{} is outside project root", path.display()))
}
fn scan(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            if [
                "node_modules",
                ".git",
                "dist",
                "target",
                "coverage",
                ".xcss",
            ]
            .iter()
            .any(|name| entry.file_name() == *name)
            {
                continue;
            }
            scan(&path, files)?;
        } else if path.extension().is_some_and(|s| s == "xcss") {
            files.push(canonical(path).map_err(|e| e.to_string())?);
        }
    }
    Ok(())
}
fn write_transaction(writes: Vec<(PathBuf, String)>) -> Result<(), String> {
    let mut staged = Vec::new();
    for (index, (path, text)) in writes.into_iter().enumerate() {
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let temp = path.with_extension(format!("xcss-{}-{index}.tmp", process::id()));
        let backup = path.with_extension(format!("xcss-{}-{index}.bak", process::id()));
        if let Err(error) = fs::write(&temp, text) {
            for (_, temp, _) in &staged {
                let _ = fs::remove_file(temp);
            }
            return Err(error.to_string());
        }
        staged.push((path, temp, backup));
    }
    let mut installed = Vec::new();
    for (path, temp, backup) in &staged {
        let had_output = path.exists();
        let result = (|| {
            if had_output {
                fs::rename(path, backup)?;
            }
            fs::rename(temp, path)
        })();
        if let Err(error) = result {
            if had_output && backup.exists() {
                let _ = fs::rename(backup, path);
            }
            for (path, backup, had) in installed.iter().rev() {
                let _ = fs::remove_file(path);
                if *had {
                    let _ = fs::rename(backup, path);
                }
            }
            for (_, temp, _) in &staged {
                let _ = fs::remove_file(temp);
            }
            return Err(error.to_string());
        }
        installed.push((path.clone(), backup.clone(), had_output));
    }
    for (_, backup, had) in installed {
        if had {
            fs::remove_file(backup).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn canonical(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    fs::canonicalize(path).map(portable_path)
}
fn portable_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy().replace('\\', "/");
        if let Some(rest) = text.strip_prefix("//?/UNC/") {
            PathBuf::from(format!("//{rest}"))
        } else {
            PathBuf::from(text.strip_prefix("//?/").unwrap_or(&text))
        }
    }
    #[cfg(not(windows))]
    {
        path
    }
}
