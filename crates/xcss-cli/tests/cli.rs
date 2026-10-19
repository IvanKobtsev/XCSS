use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static SANDBOX_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn sandbox() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = SANDBOX_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/xcss-cli-tests")
        .join(format!("{}-{nonce}-{sequence}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn compiles_a_file() {
    let directory = sandbox();
    let input = directory.join("button.cross.css");
    let output = directory.join("button.module.css");
    fs::write(&input, ".a { color: --brand; }").unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_xcss"))
        .args([
            "compile",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    assert!(status.success());
    assert_eq!(
        fs::read_to_string(output).unwrap(),
        ".a { color: var(--brand); }"
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn parse_failure_does_not_replace_existing_output() {
    let directory = sandbox();
    let input = directory.join("broken.cross.scss");
    let output = directory.join("broken.module.scss");
    fs::write(&input, ".a { color: \"unterminated; }").unwrap();
    fs::write(&output, "previous output").unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_xcss"))
        .args([
            "compile",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert_eq!(fs::read_to_string(output).unwrap(), "previous output");
    assert!(String::from_utf8_lossy(&result.stderr).contains(":1:"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn rejects_mismatched_extensions_and_missing_inputs() {
    let directory = sandbox();
    let missing = directory.join("missing.cross.css");
    let wrong_output = directory.join("missing.module.scss");
    let rejected = Command::new(env!("CARGO_BIN_EXE_xcss"))
        .args([
            "compile",
            missing.to_str().unwrap(),
            "--output",
            wrong_output.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(!rejected.success());

    let valid_output = directory.join("missing.module.css");
    let missing_result = Command::new(env!("CARGO_BIN_EXE_xcss"))
        .args([
            "compile",
            missing.to_str().unwrap(),
            "--output",
            valid_output.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(!missing_result.success());
    assert!(!valid_output.exists());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn compiles_arithmetic_and_preserves_output_on_ambiguity() {
    let directory = sandbox();
    let input = directory.join("math.cross.css");
    let output = directory.join("math.module.css");
    fs::write(&input, ".a { width: 100% - --sidebar; }").unwrap();

    let success = Command::new(env!("CARGO_BIN_EXE_xcss"))
        .args([
            "compile",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(success.success());
    assert_eq!(
        fs::read_to_string(&output).unwrap(),
        ".a { width: calc(100% - var(--sidebar)); }"
    );

    fs::write(&input, ".a { margin: 10px + 2px 20px; }").unwrap();
    let failure = Command::new(env!("CARGO_BIN_EXE_xcss"))
        .args([
            "compile",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!failure.status.success());
    assert!(
        String::from_utf8_lossy(&failure.stderr).contains("wrap the expression in parentheses")
    );
    assert_eq!(
        fs::read_to_string(&output).unwrap(),
        ".a { width: calc(100% - var(--sidebar)); }"
    );
    fs::remove_dir_all(directory).unwrap();
}
