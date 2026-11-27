use xcss_language_service::LanguageService;
use xcss_source::FileId;

#[test]
fn unknown_at_rules_use_name_ranges_and_preserve_class_queries() {
    let mut service = LanguageService::new();
    let file = FileId("a.xcss".into());
    service.set_file(file.clone(), 1,
        "/* 😀 */ @medai screen;\n.empty {}\n@injection paint { @suports (display: grid) {} }\n@function f(): number { @retun 1; }\n.a { @VARs {} @-webkit-keyframe foo {} }".into()).unwrap();
    let snapshot = service.snapshot();
    let warnings = snapshot.warnings(&file);
    let unknown: Vec<_> = warnings.iter().filter(|w| w.code == "XC-505").collect();
    assert_eq!(unknown.len(), 5, "{warnings:?}");
    assert_eq!(unknown[0].message, "Unknown at-rule '@medai'.");
    assert_eq!(unknown[0].category, "unknown-at-rule");
    assert_eq!(unknown[0].range.start.line, 0);
    assert_eq!(unknown[0].range.start.character, 9);
    assert_eq!(unknown[0].range.end.character, 15);
    assert!(warnings.windows(2).all(|w| {
        (w[0].range.start.line, w[0].range.start.character)
            <= (w[1].range.start.line, w[1].range.start.character)
    }));
    assert!(
        snapshot
            .class_warnings(&file)
            .iter()
            .all(|w| w.code != "XC-505")
    );
    assert!(
        snapshot
            .class_warnings(&file)
            .iter()
            .any(|w| w.code == "XC-503")
    );
}

#[test]
fn css_names_directives_and_opaque_regions_are_not_unknown() {
    let mut service = LanguageService::new();
    let file = FileId("a.xcss".into());
    service
        .set_file(
            file.clone(),
            1,
            r#"
@MEDIA screen {} @supports(display: grid) {} @container card {} @layer a;
@scope {} @starting-style {} @keyframes spin {} @-webkit-keyframes spin {}
@font-face {} @font-palette-values --p {} @counter-style foo {} @property --x {}
@page { @top-left {} @bottom-right-corner {} @left-middle {} }
@font-feature-values Font { @swash {} @styleset {} @character-variant {} @historical-forms {} }
@view-transition {} @position-try --x {} @custom-media --small (width < 1px);
@charset "utf-8"; @namespace svg "x"; @document url(x) {} @-moz-document url(x) {}
@m\65 dia screen {} @\73 upports(display: grid) {}
@vars { --x: number = 1; @provided --y: number = 2; }
@injection paint { @content; } @util .util { color: red; }
@function value(): number => 1; @variants V: .a; @:paint;
@raw { @bogus; // @xcss-ignore xc-505
  @other {} }
/* @comment; */ // @line;
.a { content: "@string;"; background: url("@url"); }
@mixin foo {} @include foo; @return 1;
@; @-; @?;
"#
            .into(),
        )
        .unwrap();
    let snapshot = service.snapshot();
    assert!(
        snapshot.warnings(&file).iter().all(|w| w.code != "XC-505"),
        "{:?}",
        snapshot.warnings(&file)
    );
    assert!(
        snapshot
            .diagnostics(&file)
            .iter()
            .any(|d| d.legacy_code == Some("syntax.sass"))
    );
}

#[test]
fn unknown_at_rule_suppression_is_selective_physical_and_revisioned() {
    for newline in ["\n", "\r\n"] {
        let mut service = LanguageService::new();
        let file = FileId("a.xcss".into());
        let source = "// @xcss-ignore xc-505\n@one; @two; .empty {}\n// @xcss-ignore xc-505\n\n@three;\n@four\n// @xcss-ignore xc-505\nfoo;\n@five;".replace('\n', newline);
        service.set_file(file.clone(), 1, source.clone()).unwrap();
        let old = service.snapshot();
        let warnings = old.warnings(&file);
        assert_eq!(
            warnings.iter().map(|w| w.code).collect::<Vec<_>>(),
            ["XC-503", "XC-505", "XC-505", "XC-505"]
        );
        assert_eq!(warnings[1].message, "Unknown at-rule '@three'.");
        assert_eq!(warnings[2].range.start.line, 5);
        let other = FileId("b.xcss".into());
        service.set_file(other.clone(), 1, "@one;".into()).unwrap();
        assert_eq!(service.snapshot().warnings(&other).len(), 1);
        service
            .set_file(
                file.clone(),
                2,
                source.replace("@xcss-ignore", "ordinary comment"),
            )
            .unwrap();
        assert_eq!(service.snapshot().warnings(&file).len(), 6);
        assert_eq!(old.warnings(&file).len(), 4);
        service
            .set_file(file.clone(), 3, "@media screen {}".into())
            .unwrap();
        assert!(service.snapshot().warnings(&file).is_empty());
        assert!(
            service
                .snapshot()
                .warnings(&FileId("missing.xcss".into()))
                .is_empty()
        );
    }
}

#[test]
fn non_standalone_suppressions_do_not_hide_unknown_rules() {
    let mut service = LanguageService::new();
    let file = FileId("a.xcss".into());
    service.set_file(file.clone(), 1, "@known-typo; // @xcss-ignore xc-505\n@second;\n/* @xcss-ignore xc-505 */\n@third;\n@raw { // @xcss-ignore xc-505\n@raw-typo; }\n@fourth;".into()).unwrap();
    assert_eq!(service.snapshot().warnings(&file).len(), 4);
}

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
