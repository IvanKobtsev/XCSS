use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
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
fn writes_composition_dependencies_and_rebases_assets() {
    let root = sandbox();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/a.xcss"),
        ".a { composes: b from \"./b.xcss\"; background: url('./image.svg'); }",
    )
    .unwrap();
    fs::write(root.join("src/b.xcss"), ".b { color: red; }").unwrap();
    let result = run(&root, "src/a.xcss", "dist/out.css");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let css = fs::read_to_string(root.join("dist/out.css")).unwrap();
    assert!(css.contains("@import"));
    assert!(css.contains("../src/image.svg"));
    assert!(root.join("dist/.xcss/src/b.css").exists());
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
