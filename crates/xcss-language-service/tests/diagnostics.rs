use xcss_language_service::{DiagnosticPart as Part, DiagnosticType as Type, LanguageService};
use xcss_source::FileId;

#[test]
fn editor_parts_preserve_types_and_strip_cli_quotes() {
    let mut service = LanguageService::new();
    let file = FileId("a.xcss".into());
    service
        .set_file(
            file.clone(),
            1,
            "/* 😀 */ @injection f($p: percentage | number) {} .a { @:f(red); @:missing(); }"
                .into(),
        )
        .unwrap();
    let snapshot = service.snapshot();
    let diagnostics = snapshot.diagnostics(&file);
    let mismatch = diagnostics.iter().find(|d| d.code == "XC-303").unwrap();
    assert_eq!(
        mismatch.message_parts,
        vec![
            Part::Text("Argument of type ".into()),
            Part::Type(Type::Named("color".into())),
            Part::Text(" is not assignable to parameter of type ".into()),
            Part::Type(Type::Union(vec![
                Type::Named("percentage".into()),
                Type::Named("number".into())
            ])),
        ]
    );
    let unknown = diagnostics.iter().find(|d| d.code == "XC-212").unwrap();
    assert!(
        unknown
            .message_parts
            .contains(&Part::Code("missing".into()))
    );
    let plain = snapshot.plain_diagnostics(&file);
    assert_eq!(
        plain
            .iter()
            .find(|d| d.diagnostic.code == "XC-212")
            .unwrap()
            .diagnostic
            .message
            .to_string(),
        "Unknown callable `missing`"
    );
    for (editor, plain) in diagnostics.iter().zip(&plain) {
        assert_eq!(editor.code, plain.diagnostic.code);
        assert_eq!(editor.location, plain.location);
        assert_eq!(editor.related, plain.related);
        assert_eq!(editor.severity, plain.diagnostic.severity);
    }
    service.set_file(file.clone(), 2, ".a {}".into()).unwrap();
    assert!(service.snapshot().diagnostics(&file).is_empty());
    assert_eq!(snapshot.diagnostics(&file).len(), diagnostics.len());
}

#[test]
fn count_messages_cover_exact_ranges_and_optional_parameters() {
    for keyword in ["injection", "util", "function"] {
        for (parameters, arguments, expected) in [
            ("", "red", Some("Expected 0 arguments, but got 1")),
            ("$p: color", "", Some("Expected 1 arguments, but got 0")),
            (
                "$p: color",
                "red, blue",
                Some("Expected 1 arguments, but got 2"),
            ),
            (
                "$p: color, $q: color = red",
                "",
                Some("Expected 1-2 arguments, but got 0"),
            ),
            (
                "$p: color, $q: color = red",
                "red, blue, red",
                Some("Expected 1-2 arguments, but got 3"),
            ),
            (
                "$p: color = red",
                "red, blue",
                Some("Expected 0-1 arguments, but got 2"),
            ),
            ("", "", None),
            ("$p: color, $q: color = red", "red", None),
            ("$p: color, $q: color = red", "red, blue", None),
            ("$p: color = red", "", None),
        ] {
            let mut service = LanguageService::new();
            let file = FileId("a.xcss".into());
            let source = if keyword == "function" {
                format!("@function f({parameters}) => red; .a {{ color: @:f({arguments}); }}")
            } else {
                format!("@{keyword} f({parameters}) {{}} .a {{ @:f({arguments}); }}")
            };
            service.set_file(file.clone(), 1, source.clone()).unwrap();
            let snapshot = service.snapshot();
            let plain = snapshot.plain_diagnostics(&file);
            let count = plain.iter().find(|d| d.diagnostic.code == "XC-302");
            assert_eq!(
                count.map(|d| d.diagnostic.message.to_string()).as_deref(),
                expected,
                "{source}: {plain:?}"
            );
            if expected.is_some() {
                let parts = snapshot
                    .diagnostics(&file)
                    .into_iter()
                    .find(|d| d.code == "XC-302")
                    .unwrap()
                    .message_parts;
                assert!(parts.contains(&Part::Number(if arguments.is_empty() {
                    0
                } else {
                    arguments.split(',').count()
                })));
            } else {
                assert!(plain.is_empty(), "{source}: {plain:?}");
            }
        }
    }
}
