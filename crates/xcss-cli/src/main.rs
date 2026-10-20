use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process,
};

use xcss_core::{Syntax, TransformOptions, transform};

fn main() {
    if let Err(message) = run(env::args_os().skip(1).collect()) {
        eprintln!("{message}");
        process::exit(1);
    }
}

fn run(args: Vec<std::ffi::OsString>) -> Result<(), String> {
    let (input, output) = parse_args(args)?;
    let syntax = validate_paths(&input, &output)?;
    let source =
        fs::read_to_string(&input).map_err(|error| format!("{}: {error}", input.display()))?;
    let filename = input.to_string_lossy();
    let module_id = logical_module_id(&input);
    let transformed = transform(
        &source,
        TransformOptions {
            syntax,
            filename: Some(&filename),
            module_id: Some(&module_id),
        },
    )
    .map_err(|diagnostics| {
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
    })?;

    write_complete_file(&output, transformed.code.as_bytes())
        .map_err(|error| format!("{}: {error}", output.display()))
}

fn logical_module_id(path: &Path) -> String {
    let relative = if path.is_absolute() {
        env::current_dir()
            .ok()
            .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    relative.to_string_lossy().replace('\\', "/")
}

fn parse_args(args: Vec<std::ffi::OsString>) -> Result<(PathBuf, PathBuf), String> {
    if args.len() == 4 && args[0] == "compile" && (args[2] == "--output" || args[2] == "-o") {
        return Ok((PathBuf::from(&args[1]), PathBuf::from(&args[3])));
    }
    Err("usage: xcss compile <input.cross.css|input.cross.scss> --output <output.module.css|output.module.scss>".into())
}

fn validate_paths(input: &Path, output: &Path) -> Result<Syntax, String> {
    let input_name = input.to_string_lossy();
    let output_name = output.to_string_lossy();
    let syntax = Syntax::from_input_filename(&input_name).ok_or_else(|| {
        format!(
            "{}: input must end in .cross.css or .cross.scss",
            input.display()
        )
    })?;
    if !output_name.ends_with(syntax.output_suffix()) {
        return Err(format!(
            "{}: output for this input must end in {}",
            output.display(),
            syntax.output_suffix()
        ));
    }
    if input == output {
        return Err("input and output paths must be different".into());
    }
    Ok(syntax)
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
            validate_paths(Path::new("a.cross.css"), Path::new("a.module.css")),
            Ok(Syntax::Css)
        );
        assert_eq!(
            validate_paths(Path::new("a.cross.scss"), Path::new("a.module.scss")),
            Ok(Syntax::Scss)
        );
        assert!(validate_paths(Path::new("a.css"), Path::new("a.module.css")).is_err());
        assert!(validate_paths(Path::new("a.cross.css"), Path::new("a.module.scss")).is_err());
        assert_eq!(
            logical_module_id(Path::new("styles\\a.cross.css")),
            "styles/a.cross.css"
        );
    }
}
