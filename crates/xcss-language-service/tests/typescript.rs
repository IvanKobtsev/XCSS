use xcss_language_service::LanguageService;
use xcss_source::FileId;

#[test]
fn revisions_snapshots_unsaved_edits_and_utf16() {
    let mut service = LanguageService::new();
    let xcss = FileId("a.xcss".into());
    let ts = FileId("index.ts".into());
    service
        .set_file(xcss.clone(), 1, "@variants V: .one; .one {}".into())
        .unwrap();
    service
        .set_typescript_file(
            FileId("tsconfig.json".into()),
            1,
            r#"{"compilerOptions":{"strict":true},"files":["index.ts"]}"#.into(),
        )
        .unwrap();
    service.set_typescript_file(ts.clone(), 1, "import styles, { type Variants } from './a.xcss'; /* 😀 */ styles.absent; const v: Variants<'V'> = 'two';".into()).unwrap();
    service.set_typescript_project(Some("tsconfig.json".into()));
    let old = service.snapshot();
    let diagnostics = old.diagnostics(&ts);
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    let missing = diagnostics
        .iter()
        .find(|d| d.diagnostic.message.contains("absent"))
        .unwrap();
    assert_eq!(missing.location.range.start.character, 66);
    service
        .set_file(
            xcss.clone(),
            2,
            "@variants V: .two; .two {} .absent {}".into(),
        )
        .unwrap();
    let next = service.snapshot();
    assert!(
        next.diagnostics(&ts).is_empty(),
        "{:?}",
        next.diagnostics(&ts)
    );
    assert!(next.revision() > old.revision());
    assert_eq!(old.diagnostics(&ts).len(), 2);
    service.remove_typescript_file(&ts, 2).unwrap();
    assert!(
        service
            .set_typescript_file(ts.clone(), 2, String::new())
            .is_err()
    );
    service
        .set_typescript_file(ts.clone(), 3, "export {};".into())
        .unwrap();
    assert!(service.snapshot().diagnostics(&ts).is_empty());
    service.remove_file(&xcss, 3).unwrap();
    service
        .set_typescript_file(ts.clone(), 4, "import s from './a.xcss'; s.one;".into())
        .unwrap();
    assert!(!service.snapshot().diagnostics(&ts).is_empty());
}

#[test]
fn tsconfig_and_resolution_updates_invalidate_the_checker() {
    let mut service = LanguageService::new();
    let config = FileId("tsconfig.json".into());
    let ts = FileId("index.ts".into());
    service
        .set_file(FileId("a.xcss".into()), 1, ".one {}".into())
        .unwrap();
    service
        .set_file(FileId("b.xcss".into()), 1, ".two {}".into())
        .unwrap();
    service.set_typescript_project(Some(config.0.clone()));
    service
        .set_typescript_file(ts.clone(), 1, "import s from 'styles.xcss'; s.one;".into())
        .unwrap();
    service
        .set_typescript_file(
            config.clone(),
            1,
            r#"{"compilerOptions":{"paths":{"styles.xcss":["./a.xcss"]}},"files":["index.ts"]}"#
                .into(),
        )
        .unwrap();
    let old = service.snapshot();
    assert!(
        old.diagnostics(&ts).is_empty(),
        "{:?}",
        old.diagnostics(&ts)
    );
    service
        .set_typescript_file(
            config,
            2,
            r#"{"compilerOptions":{"paths":{"styles.xcss":["./b.xcss"]}},"files":["index.ts"]}"#
                .into(),
        )
        .unwrap();
    assert_eq!(service.snapshot().diagnostics(&ts).len(), 1);
    assert!(old.diagnostics(&ts).is_empty());
    service.set_typescript_project(None);
    assert!(service.snapshot().diagnostics(&ts).is_empty());
}
