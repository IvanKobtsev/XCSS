use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[test]
fn checks_typescript_without_emitting_and_reports_original_locations() {
    let root = sandbox();
    fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true},"files":["index.ts"]}"#,
    )
    .unwrap();
    fs::write(
        root.join("a.xcss"),
        "@variants V: .one; .one { color: red; }",
    )
    .unwrap();
    let imports = "import { styles, type Variants } from './a.xcss'; import 'app/helpers'; const n: number = 'ordinary type error';\n";
    fs::write(
        root.join("index.ts"),
        format!("{imports}type Missing = Variants<'Missing'>; const v: Variants<'V'> = 'wrong'; styles.missing;"),
    )
    .unwrap();
    let check = |config: &str| {
        Command::new(env!("CARGO_BIN_EXE_xcss"))
            .current_dir(&root)
            .args(["check", "--project", config])
            .output()
            .unwrap()
    };
    let output = check("tsconfig.json");
    assert!(!output.status.success());
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("index.ts:2:"), "{message}");
    assert!(
        message.contains("XC-001 Unknown XCSS class — Class 'missing' can't be found in 'a.xcss'."),
        "{message}"
    );
    assert!(
        message.contains(
            "XC-002 Unknown XCSS variant group — 'Missing' is not a variant group in 'a.xcss'."
        ),
        "{message}"
    );
    assert!(
        message.contains("XC-003 Invalid XCSS variant value — Value is not valid for variant group 'V' in 'a.xcss'."),
        "{message}"
    );
    assert_eq!(message.lines().count(), 4, "{message}");
    assert!(!root.join("a.css").exists());
    let dependency = root.join("node_modules/example");
    fs::create_dir_all(dependency.join("dist")).unwrap();
    fs::write(
        dependency.join("package.json"),
        r#"{"types":"dist/index.d.ts"}"#,
    )
    .unwrap();
    fs::write(
        dependency.join("dist/index.d.ts"),
        "export type Variant = 'one';",
    )
    .unwrap();
    fs::write(root.join("index.ts"), format!("{imports}import type {{ Variant }} from 'example'; const v: Variants<'V'> = 'one' as Variant; styles.one;")).unwrap();
    let output = check("tsconfig.json");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn stylesheet_checks_survive_typescript_setup_failures() {
    let root = sandbox();
    fs::write(root.join("index.ts"), "").unwrap();
    fs::write(root.join("bad.ts"), [0xff]).unwrap();
    fs::write(root.join("valid.json"), r#"{"files":["index.ts"]}"#).unwrap();
    fs::write(root.join("malformed.json"), "{ invalid:").unwrap();
    let check = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_xcss"))
            .current_dir(&root)
            .arg("check")
            .args(args)
            .output()
            .unwrap()
    };

    for (stylesheet, valid) in [(".a {}", true), ("bogus;", false)] {
        fs::write(root.join("a.xcss"), stylesheet).unwrap();
        for (config, reason) in [(None, None), (Some("valid.json"), Some("bad.ts"))] {
            let output = match config {
                Some(config) => check(&["--project", config]),
                None => check(&[]),
            };
            let message = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.success(), valid, "{message}");
            assert_eq!(
                message.contains("XC-105 Invalid XCSS statement"),
                !valid,
                "{message}"
            );
            let warnings: Vec<_> = message
                .lines()
                .filter(|line| line.starts_with("Warning:"))
                .collect();
            if let Some(reason) = reason {
                assert_eq!(warnings.len(), 1, "{message}");
                assert!(
                    warnings[0]
                        .starts_with("Warning: Couldn't analyze XCSS TypeScript integration: ")
                );
                assert!(warnings[0].contains(reason), "{message}");
            } else {
                assert!(warnings.is_empty(), "{message}");
            }
        }
    }
    fs::write(root.join("bad.ts"), "").unwrap();
    for (stylesheet, valid) in [(".a {}", true), ("bogus;", false)] {
        fs::write(root.join("a.xcss"), stylesheet).unwrap();
        for (config, warning) in [("malformed.json", true), ("valid.json", false)] {
            let output = check(&["--project", config]);
            let message = String::from_utf8_lossy(&output.stderr);
            if warning {
                assert!(!output.status.success(), "{message}");
                assert!(message.contains("malformed.json"), "{message}");
                continue;
            }
            assert_eq!(output.status.success(), valid, "{message}");
            assert_eq!(
                message.contains("XC-105 Invalid XCSS statement"),
                !valid,
                "{message}"
            );
            assert_eq!(
                message
                    .lines()
                    .filter(|line| line.starts_with("Warning:"))
                    .count(),
                usize::from(warning),
                "{message}"
            );
            if warning {
                assert!(message.contains("Property value"), "{message}");
            }
        }
    }
}

fn sandbox() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/cli-tests/{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::canonicalize(path).unwrap()
}
fn run(root: &Path, input: &str, output: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xcss"))
        .args([
            "compile",
            root.join(input).to_str().unwrap(),
            "--output",
            root.join(output).to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}
#[test]
fn writes_scoped_css_exports_and_maps() {
    let root = sandbox();
    fs::write(root.join("a.xcss"), ".a { width: --size * 2; }").unwrap();
    let output = run(&root, "a.xcss", "out.css");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let css = fs::read_to_string(root.join("out.css")).unwrap();
    let exports: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("out.exports.json")).unwrap()).unwrap();
    assert!(css.contains(exports["a"].as_str().unwrap()));
    assert!(css.contains("calc(var(--size) * 2)"));
    assert!(root.join("out.css.map").exists());
}
#[test]
fn writes_one_shared_utility_stylesheet_with_imports_and_maps() {
    let root = sandbox();
    fs::write(
        root.join("util.xcss"),
        "@util trim($n: integer = 3) { max-height: $n * 1lh; background: url('./icon.svg'); }",
    )
    .unwrap();
    fs::write(root.join("a.xcss"), ".a { @:trim; } span { @:trim(4); }").unwrap();
    fs::write(root.join("b.xcss"), ".b { @:trim(3); }").unwrap();
    let output = run(&root, "a.xcss", "dist/out.css");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let shared = fs::read_to_string(root.join("dist/out.utils.css")).unwrap();
    assert_eq!(shared.matches("._util_trim_3").count(), 1);
    assert!(shared.contains("../icon.svg"));
    assert!(root.join("dist/out.utils.css.map").exists());
    let caller = fs::read_to_string(root.join("dist/out.css")).unwrap();
    assert!(caller.contains("@import \"./out.utils.css\";"));
    assert!(caller.contains("_util_inj_"));
}
#[test]
fn compile_errors_preserve_all_previous_artifacts() {
    let root = sandbox();
    fs::write(root.join("a.xcss"), ".a { @:missing; }").unwrap();
    for name in ["out.css", "out.css.map", "out.exports.json"] {
        fs::write(root.join(name), "previous").unwrap();
    }
    assert!(!run(&root, "a.xcss", "out.css").status.success());
    for name in ["out.css", "out.css.map", "out.exports.json"] {
        assert_eq!(fs::read_to_string(root.join(name)).unwrap(), "previous");
    }
}
#[test]
fn rejects_sass_and_double_scoped_output() {
    let root = sandbox();
    fs::write(root.join("a.xcss"), "$factor: 2;").unwrap();
    assert!(!run(&root, "a.xcss", "out.css").status.success());
    assert!(!run(&root, "a.xcss", "out.module.css").status.success());
}

#[test]
fn output_can_be_outside_project_root_without_changing_source_identity() {
    let root = sandbox();
    fs::write(root.join("a.xcss"), ".a { background: url('./icon.svg'); }").unwrap();
    let filename = format!(
        "../outside-{}.css",
        root.file_name().unwrap().to_string_lossy()
    );
    let result = run(&root, "a.xcss", &filename);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let css = fs::read_to_string(root.join(filename)).unwrap();
    assert!(
        css.contains(&format!(
            "./{}/icon.svg",
            root.file_name().unwrap().to_string_lossy()
        )),
        "{css}"
    );
}

#[test]
fn tsconfig_aliases_resolve_imports_and_definition_site_assets() {
    let root = sandbox();
    fs::create_dir_all(root.join("src/deep")).unwrap();
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(root.join("assets/icon.svg"), "svg").unwrap();
    fs::write(
        root.join("aliases.json"),
        r#"{"compilerOptions":{"paths":{"@styles/*":["src/*"],"@assets/*":["assets/*"]}}}"#,
    )
    .unwrap();
    fs::write(root.join("src/recipe.xcss"), r#"$image = url("@assets/icon.svg?q=1#part"); @injection icon { background: $image; } @util icon-util { mask: url(@assets/icon.svg); }"#).unwrap();
    fs::write(
        root.join("src/deep/main.xcss"),
        r#"@import { icon, icon-util } from "@styles/recipe.xcss"; .a { @:icon-util; @:icon; }"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_xcss"))
        .current_dir(&root)
        .args([
            "compile",
            "src/deep/main.xcss",
            "-o",
            "out/main.css",
            "--project",
            "aliases.json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let css = fs::read_to_string(root.join("out/main.css")).unwrap();
    assert!(css.contains("../assets/icon.svg?q=1#part"), "{css}");
    let css = fs::read_to_string(root.join("out/main.utils.css")).unwrap();
    assert!(css.contains("../assets/icon.svg"), "{css}");
    fs::write(
        root.join("tsconfig.json"),
        fs::read(root.join("aliases.json")).unwrap(),
    )
    .unwrap();
    let checked = Command::new(env!("CARGO_BIN_EXE_xcss"))
        .current_dir(&root)
        .arg("check")
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    fs::remove_file(root.join("assets/icon.svg")).unwrap();
    let checked = Command::new(env!("CARGO_BIN_EXE_xcss"))
        .current_dir(&root)
        .arg("check")
        .output()
        .unwrap();
    assert!(!checked.status.success());
    assert!(
        String::from_utf8_lossy(&checked.stderr).contains("Cannot resolve tsconfig paths alias")
    );
}

#[test]
fn cli_requires_explicit_aliases_and_valid_selected_config() {
    let root = sandbox();
    fs::write(
        root.join("recipe.xcss"),
        "@injection recipe { color: red; }",
    )
    .unwrap();
    fs::write(
        root.join("main.xcss"),
        r#"@import { recipe } from "@/recipe.xcss"; .a { @:recipe; }"#,
    )
    .unwrap();
    let invoke = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_xcss"))
            .current_dir(&root)
            .args(["compile", "main.xcss", "-o", "out/main.css"])
            .args(extra)
            .output()
            .unwrap()
    };
    assert!(!invoke(&[]).status.success());
    fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"paths":{"@/*":["./*"]}}}"#,
    )
    .unwrap();
    assert!(invoke(&[]).status.success());
    let result = invoke(&["--project", "missing.json"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("missing.json"));
}
