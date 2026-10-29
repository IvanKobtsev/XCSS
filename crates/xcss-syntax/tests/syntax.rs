use std::sync::Arc;
use xcss_source::{FileId, SourceFile};
use xcss_syntax::{NodeKind, ast::ValueKind, parse};

fn parsed(text: &str) -> xcss_syntax::ParseResult {
    parse(Arc::new(SourceFile::new(
        FileId("test.xcss".into()),
        1,
        text,
    )))
}

#[test]
fn retained_language_fixture_has_owned_views() {
    let p = parsed(include_str!("fixtures/language.xcss"));
    assert!(p.diagnostics.is_empty(), "{:#?}", p.diagnostics);
    assert_eq!(
        p.tree.descendants().filter_map(|n| p.variable(n)).count(),
        4
    );
    assert_eq!(
        p.tree
            .descendants()
            .filter_map(|n| p.injection_import(n))
            .count(),
        1
    );
    let injection = p.tree.descendants().find_map(|n| p.injection(n)).unwrap();
    assert_eq!(injection.parameters[0].name, "$thickness");
    assert_eq!(injection.parameters[0].default, Some("1px"));
    assert!(
        p.tree
            .descendants()
            .filter_map(|n| p.declaration(n))
            .any(|d| d
                .value
                .descendants()
                .iter()
                .any(|v| matches!(v.kind, ValueKind::Group(_))))
    );
}

#[test]
fn rejects_sass_without_rejecting_injection_parameters() {
    for text in [
        "$factor: 2;",
        "@mixin foo {}",
        "@include foo;",
        "@use 'foo';",
        "@forward 'foo';",
        "@if true {}",
        "@each $x in 1 {}",
        ".a { color: #{$x}; }",
    ] {
        let p = parsed(text);
        assert!(!p.diagnostics.is_empty(), "Accepted {text}");
    }
    assert!(
        parsed("@injection foo($size: length) { width: $size; @content; }")
            .diagnostics
            .is_empty()
    );
}

#[test]
fn recovers_unfinished_calls_and_declarations() {
    let p = parsed(".a { color: red\n width: 2px; @:foo(\n height: 3px; } .b { color: blue }");
    for property in ["width", "height", "color"] {
        assert!(
            p.tree
                .descendants()
                .filter_map(|n| p.declaration(n))
                .any(|d| d.property == property),
            "Lost {property}"
        );
    }
    assert!(p.tree.descendants().any(|n| n.missing.contains(&')')));
    assert!(
        p.tree
            .descendants()
            .any(|n| n.kind == NodeKind::Rule && n.header_text(&p.source).trim() == ".b")
    );
}

#[test]
fn css_values_escapes_and_crlf_keep_ranges() {
    let p = parsed("/*😀*/\r\n.caf\\e9 { --data: { key: value; }; opacity: .5; width: 1e2px; }");
    assert!(p.diagnostics.is_empty(), "{:?}", p.diagnostics);
    assert_eq!(
        p.tokens
            .iter()
            .map(|t| p.slice(t.range))
            .collect::<String>(),
        p.source.text()
    );
    for node in p.tree.descendants() {
        assert!(p.source.text().is_char_boundary(node.range.start));
        assert!(p.source.text().is_char_boundary(node.range.end));
    }
    assert_eq!(xcss_syntax::ast::decode_identifier("caf\\e9"), "café");
}

#[test]
fn malformed_input_and_typed_queries_never_panic() {
    let alphabet = [
        "@", ":", ";", "{", "}", "(", ")", "[", "]", "$", "--x", "foo", "\n", "'", "\\", "😀", ",",
        "=", "/*", "#",
    ];
    let mut state = 42u64;
    for _ in 0..1500 {
        let mut text = String::new();
        for _ in 0..32 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push_str(alphabet[(state >> 32) as usize % alphabet.len()]);
        }
        let p = parsed(&text);
        for n in p.tree.descendants() {
            p.declaration(n);
            p.injection(n);
            p.variable(n);
            p.call(n);
            p.injection_import(n);
        }
    }
}
