use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process,
};

use xcss_core::{ImportResolution, ProjectFile, compile_project, inspect_project_file};

fn main() {
    if let Err(message) = run(env::args_os().skip(1).collect()) {
        eprintln!("{message}");
        process::exit(1);
    }
}

fn run(args: Vec<std::ffi::OsString>) -> Result<(), String> {
    let (input, output, root) = parse_args(args)?;
    validate_paths(&input, &output)?;
    let root =
        fs::canonicalize(root.unwrap_or(env::current_dir().map_err(|error| error.to_string())?))
            .map_err(|error| error.to_string())?;
    let owned = scan_project(&root)?;
    let files: Vec<_> = owned
        .iter()
        .map(|file| ProjectFile {
            module_id: &file.module_id,
            filename: &file.filename,
            source: &file.source,
        })
        .collect();
    let owned_resolutions = resolve_imports(&owned, &root);
    let resolutions: Vec<_> = owned_resolutions
        .iter()
        .map(|resolution| ImportResolution {
            importer: &resolution.importer,
            specifier: &resolution.specifier,
            target: &resolution.target,
        })
        .collect();
    let absolute_input =
        fs::canonicalize(&input).map_err(|error| format!("{}: {error}", input.display()))?;
    let module_id = logical_module_id_from_root(&root, &absolute_input);
    let partial = root.join("node_modules/.xcss/_injections.scss");
    let partial_name = partial.to_string_lossy().replace('\\', "/");
    let transformed = compile_project(&files, &resolutions, &module_id, &partial_name).map_err(
        |diagnostics| {
            diagnostics
                .into_iter()
                .map(|diagnostic| {
                    format!(
                        "{}:{}:{}: error: {}",
                        diagnostic.filename.as_deref().unwrap_or("<input>"),
                        diagnostic.line,
                        diagnostic.column,
                        diagnostic.message
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        },
    )?;

    write_complete_file(&partial, transformed.partial.as_bytes())
        .map_err(|error| format!("{}: {error}", partial.display()))?;
    write_complete_file(&output, transformed.code.as_bytes())
        .map_err(|error| format!("{}: {error}", output.display()))
}

struct OwnedFile {
    module_id: String,
    filename: String,
    source: String,
}
struct OwnedResolution {
    importer: String,
    specifier: String,
    target: String,
}

fn logical_module_id_from_root(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn parse_args(
    args: Vec<std::ffi::OsString>,
) -> Result<(PathBuf, PathBuf, Option<PathBuf>), String> {
    if (args.len() == 4 || args.len() == 6)
        && args[0] == "compile"
        && (args[2] == "--output" || args[2] == "-o")
    {
        let root = if args.len() == 6 && args[4] == "--root" {
            Some(PathBuf::from(&args[5]))
        } else if args.len() == 4 {
            None
        } else {
            return Err(usage());
        };
        return Ok((PathBuf::from(&args[1]), PathBuf::from(&args[3]), root));
    }
    Err(usage())
}

fn usage() -> String {
    "usage: xcss compile <input.xcss> --output <output.module.scss> [--root <dir>]".into()
}

fn validate_paths(input: &Path, output: &Path) -> Result<(), String> {
    let input_name = input.to_string_lossy();
    let output_name = output.to_string_lossy();
    if !input_name.ends_with(".xcss") {
        return Err(format!("{}: input must end in .xcss", input.display()));
    }
    if !output_name.ends_with(".module.scss") {
        return Err(format!(
            "{}: output must end in .module.scss",
            output.display()
        ));
    }
    if input == output {
        return Err("input and output paths must be different".into());
    }
    Ok(())
}

fn scan_project(root: &Path) -> Result<Vec<OwnedFile>, String> {
    fn visit(root: &Path, directory: &Path, files: &mut Vec<OwnedFile>) -> Result<(), String> {
        for entry in
            fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?
        {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name();
                if ["node_modules", ".git", "dist", "target", "coverage"]
                    .iter()
                    .any(|skip| name == *skip)
                {
                    continue;
                }
                visit(root, &path, files)?;
            } else if path.to_string_lossy().ends_with(".xcss") {
                let absolute = fs::canonicalize(&path).map_err(|error| error.to_string())?;
                files.push(OwnedFile {
                    module_id: logical_module_id_from_root(root, &absolute),
                    filename: absolute.to_string_lossy().into_owned(),
                    source: fs::read_to_string(&absolute)
                        .map_err(|error| format!("{}: {error}", absolute.display()))?,
                });
            }
        }
        Ok(())
    }
    let root = fs::canonicalize(root).map_err(|error| format!("{}: {error}", root.display()))?;
    let mut files = Vec::new();
    visit(&root, &root, &mut files)?;
    files.sort_by(|left, right| left.module_id.cmp(&right.module_id));
    Ok(files)
}

fn resolve_imports(files: &[OwnedFile], root: &Path) -> Vec<OwnedResolution> {
    files
        .iter()
        .flat_map(|file| {
            inspect_project_file(&file.source)
                .imports
                .into_iter()
                .map(|specifier| {
                    let target_path = if let Some(path) = specifier.strip_prefix("@/") {
                        root.join(path)
                    } else {
                        Path::new(&file.filename)
                            .parent()
                            .unwrap_or(root)
                            .join(&specifier)
                    };
                    let target = fs::canonicalize(target_path)
                        .ok()
                        .map(|path| logical_module_id_from_root(root, &path))
                        .unwrap_or_default();
                    OwnedResolution {
                        importer: file.module_id.clone(),
                        specifier,
                        target,
                    }
                })
        })
        .collect()
}

fn write_complete_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("xcss-output");
    let temporary = parent.join(format!(".{stem}.{}.tmp", process::id()));
    let backup = parent.join(format!(".{stem}.{}.bak", process::id()));

    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);

        let had_output = path.exists();
        if had_output {
            fs::rename(path, &backup)?;
        }
        if let Err(error) = fs::rename(&temporary, path) {
            if had_output {
                let _ = fs::rename(&backup, path);
            }
            return Err(error);
        }
        if had_output {
            fs::remove_file(&backup)?;
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_matching_extensions() {
        assert_eq!(
            validate_paths(Path::new("a.xcss"), Path::new("a.module.scss")),
            Ok(())
        );
        assert_eq!(
            validate_paths(Path::new("a.xcss"), Path::new("a.module.scss")),
            Ok(())
        );
        assert!(validate_paths(Path::new("a.css"), Path::new("a.module.scss")).is_err());
    }
}
