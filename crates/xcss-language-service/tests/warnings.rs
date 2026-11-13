use xcss_language_service::LanguageService;
use xcss_source::FileId;

#[test]
fn classes_count_only_typescript_and_variant_memberships() {
    let mut service = LanguageService::new();
    let file = FileId("a.xcss".into());
    service.set_file(file.clone(), 1, ".used { color: red; } .variant {} .unused {} .unused {} .parent { .child { color: red; } } @variants V: .variant;".into()).unwrap();
    let before = service.snapshot();
    assert!(
        before
            .class_warnings(&file)
            .iter()
            .all(|w| !w.message.contains("unused in"))
    );
    service
        .set_typescript_file(
            FileId("tsconfig.json".into()),
            1,
            r#"{"files":["app.ts"]}"#.into(),
        )
        .unwrap();
    service
        .set_typescript_file(
            FileId("app.ts".into()),
            1,
            "import { styles } from './a.xcss'; styles.used;".into(),
        )
        .unwrap();
    service.set_typescript_project(Some("tsconfig.json".into()));
    let warnings = service.snapshot().class_warnings(&file);
    let unused: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("unused in"))
        .map(|w| w.message.as_str())
        .collect();
    assert_eq!(unused.len(), 3, "{warnings:?}");
    assert!(unused.iter().any(|w| w.contains("'unused'")));
    assert!(unused.iter().any(|w| w.contains("'parent'")));
    assert!(unused.iter().any(|w| w.contains("'child'")));
    assert_eq!(
        warnings
            .iter()
            .filter(|w| w.message.contains("empty declaration"))
            .count(),
        3
    );
    service
        .set_typescript_file(FileId("tsconfig.json".into()), 2, "{ invalid:".into())
        .unwrap();
    assert!(
        service
            .snapshot()
            .class_warnings(&file)
            .iter()
            .all(|w| !w.message.contains("unused in"))
    );
}

#[test]
fn nested_rules_injections_and_comments() {
    let mut service = LanguageService::new();
    let file = FileId("a.xcss".into());
    service.set_file(file.clone(), 1, "@injection paint { color: red; } .empty { /* comment */ } .injected { @:paint; } .nested { &:hover { color: red; } } :global(.external) {}".into()).unwrap();
    let warnings = service.snapshot().class_warnings(&file);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].message.contains("'empty'"));
}
