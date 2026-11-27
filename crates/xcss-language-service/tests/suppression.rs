use xcss_language_service::LanguageService;
use xcss_source::FileId;

fn setup(source: &str) -> (LanguageService, FileId) {
    let mut service = LanguageService::new();
    let file = FileId("a.xcss".into());
    service.set_file(file.clone(), 1, source.into()).unwrap();
    (service, file)
}

#[test]
fn listed_codes_are_case_insensitive_selective_and_revisioned() {
    let source = "@injection f($p: number, $q: number) {}\n.a {\n  // @xcss-ignore xc-303, Xc-302, xc-303, XC-999, bad\n  @:f(red); @:f(blue); @:missing();\n  @:f(red);\n}";
    let (mut service, file) = setup(source);
    let snapshot = service.snapshot();
    let diagnostics = snapshot.diagnostics(&file);
    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "XC-212" && d.location.range.start.line == 3)
    );
    assert!(
        diagnostics
            .iter()
            .filter(|d| d.code != "XC-212")
            .all(|d| d.location.range.start.line == 4)
    );
    let plain = snapshot.plain_diagnostics(&file);
    assert_eq!(plain.len(), diagnostics.len());
    for (plain, editor) in plain.iter().zip(&diagnostics) {
        assert_eq!(plain.diagnostic.code, editor.code);
        assert_eq!(plain.location, editor.location);
    }
    let suppressions = snapshot.css_declarations(&file).suppressions;
    assert_eq!(suppressions.len(), 1);
    assert_eq!(suppressions[0].line, 3);
    assert_eq!(suppressions[0].codes, ["XC-302", "XC-303", "XC-999"]);
    service
        .set_file(
            file.clone(),
            2,
            source.replace("@xcss-ignore", "ordinary comment"),
        )
        .unwrap();
    assert_eq!(service.snapshot().diagnostics(&file).len(), 7);
    assert_eq!(snapshot.diagnostics(&file).len(), 3);
}

#[test]
fn directives_target_only_the_next_physical_start_line() {
    for newline in ["\n", "\r\n"] {
        for (between, suppressed) in [("", true), ("\n", false), ("// comment\n", false)] {
            let source = format!("/* 😀 */\n.a {{\n  // @xcss-ignore xc-212\n{between}  @:missing();\n  @:other();\n}}").replace('\n', newline);
            let (mut service, file) = setup(&source);
            let diagnostics = service.snapshot().diagnostics(&file);
            assert_eq!(
                diagnostics.iter().filter(|d| d.code == "XC-212").count(),
                if suppressed { 1 } else { 2 },
                "{source}: {diagnostics:?}"
            );
        }
    }
    // Argument diagnostics cover the call: match its start, not a later argument line.
    let (mut service, file) =
        setup("@injection f($p: number) {}\n.a {\n// @xcss-ignore xc-303\n@:f(\nred\n);\n}");
    assert!(
        service
            .snapshot()
            .diagnostics(&file)
            .iter()
            .all(|d| d.code != "XC-303")
    );
    let (mut service, file) =
        setup("@injection f($p: number) {}\n.a {\n@:f(\n// @xcss-ignore xc-303\nred\n);\n}");
    assert!(
        service
            .snapshot()
            .diagnostics(&file)
            .iter()
            .any(|d| d.code == "XC-303")
    );
}

#[test]
fn only_standalone_line_comment_tokens_activate_suppression() {
    for prefix in [
        "// @xcss-ignore",
        "// @xcss-ignore *",
        "// @xcss-ignore xc-21, xc-2120",
        "// @xcss-ignore-extra xc-212",
        "/* @xcss-ignore xc-212 */",
        "/*\n// @xcss-ignore xc-212\n*/",
        ".b {} // @xcss-ignore xc-212",
        ".b { content: '// @xcss-ignore xc-212'; }",
        ".b { background: url(//cdn.example.com/@xcss-ignore); }",
        "@raw {\n// @xcss-ignore xc-212\n}",
    ] {
        let (mut service, file) = setup(&format!("{prefix}\n.a {{ @:missing(); }}"));
        let snapshot = service.snapshot();
        assert!(
            snapshot.css_declarations(&file).suppressions.is_empty(),
            "{prefix}"
        );
        assert!(
            snapshot
                .diagnostics(&file)
                .iter()
                .any(|d| d.code == "XC-212"),
            "{prefix}"
        );
    }
    let (mut service, file) = setup("// @xcss-ignore xc-212");
    assert!(service.snapshot().diagnostics(&file).is_empty());
}

#[test]
fn syntax_errors_can_be_suppressed_without_changing_parser_recovery() {
    let (mut service, file) = setup("// @xcss-ignore xc-105\nbogus;\nbogus;");
    let diagnostics = service.snapshot().diagnostics(&file);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "XC-105");
    assert_eq!(diagnostics[0].location.range.start.line, 2);
}

#[test]
fn suppression_does_not_cross_files_or_apply_to_typescript() {
    let (mut service, file) = setup("// @xcss-ignore xc-212, xc-001\n.a { @:missing(); }");
    let other = FileId("other.xcss".into());
    service
        .set_file(other.clone(), 1, "\n.b { @:missing(); }".into())
        .unwrap();
    service
        .set_typescript_file(
            FileId("tsconfig.json".into()),
            1,
            r#"{"files":["app.ts"]}"#.into(),
        )
        .unwrap();
    let app = FileId("app.ts".into());
    service
        .set_typescript_file(
            app.clone(),
            1,
            "import { styles } from './a.xcss';\n// @xcss-ignore xc-001\nstyles.missing;".into(),
        )
        .unwrap();
    service.set_typescript_project(Some("tsconfig.json".into()));
    let snapshot = service.snapshot();
    assert!(snapshot.diagnostics(&file).is_empty());
    assert!(
        snapshot
            .diagnostics(&other)
            .iter()
            .any(|d| d.code == "XC-212")
    );
    assert!(
        snapshot
            .diagnostics(&app)
            .iter()
            .any(|d| d.code == "XC-001")
    );
}

#[test]
fn class_warning_codes_are_filtered_independently() {
    let (mut service, file) = setup(
        "// @xcss-ignore xc-503\n.empty {}\n// @xcss-ignore xc-504\n.unused {}\n// @xcss-ignore xc-503, xc-504\n.both {}",
    );
    service
        .set_typescript_file(
            FileId("tsconfig.json".into()),
            1,
            r#"{"files":["app.ts"]}"#.into(),
        )
        .unwrap();
    service
        .set_typescript_file(FileId("app.ts".into()), 1, "export {};".into())
        .unwrap();
    service.set_typescript_project(Some("tsconfig.json".into()));
    let warnings = service.snapshot().class_warnings(&file);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings
            .iter()
            .any(|w| w.code == "XC-504" && w.range.start.line == 1 && w.category == "unused-class")
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.code == "XC-503" && w.range.start.line == 3 && w.category == "empty-class")
    );
}
