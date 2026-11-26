use xcss_language_service::{LanguageService, TOKEN_TYPES};
use xcss_source::{FileId, Position, SourceFile};

fn at(source: &str, needle: &str) -> Position {
    SourceFile::new(FileId("test".into()), 0, source)
        .byte_to_utf16(source.find(needle).unwrap())
        .unwrap()
}
#[test]
fn parameter_unions_have_readable_hover_and_separate_type_tokens() {
    for keyword in ["injection", "util"] {
        let source = format!(
            "@{keyword} fade($p: percentage /* member */ | number = 50%) {{ opacity: $p; }} .a {{ @:fade(red); }}"
        );
        let file = FileId("a.xcss".into());
        let mut host = LanguageService::new();
        host.set_file(file.clone(), 1, source.clone()).unwrap();
        let snapshot = host.snapshot();
        assert!(
            snapshot
                .hover(&file, at(&source, "fade(red)"))
                .unwrap()
                .text
                .contains("$p: percentage | number = 50%")
        );
        assert!(
            snapshot
                .hover(&file, at(&source, "$p;"))
                .unwrap()
                .text
                .contains("percentage | number")
        );
        for name in ["percentage", "number"] {
            let position = at(&source, name);
            assert!(
                snapshot
                    .semantic_tokens(&file)
                    .iter()
                    .any(|token| TOKEN_TYPES[token.token_type] == "type"
                        && token.line == position.line
                        && token.character == position.character
                        && token.length == name.len())
            );
        }
        assert!(
            snapshot
                .diagnostics(&file)
                .iter()
                .any(|d| d.diagnostic.message.contains("percentage | number"))
        );
        let declarations = snapshot.css_declarations(&file);
        assert!(!declarations.declarations[0].checkable);
    }
}
#[test]
fn raw_contents_are_excluded_from_editor_queries() {
    let source = "@raw { .raw { unknown: $missing; } @:missing; } .normal { color: red; }";
    let file = FileId("a.xcss".into());
    let mut host = LanguageService::new();
    host.set_file(file.clone(), 1, source.into()).unwrap();
    let snapshot = host.snapshot();
    assert!(snapshot.diagnostics(&file).is_empty());
    let position = at(source, "$missing");
    assert!(snapshot.hover(&file, position).is_none());
    assert!(snapshot.definition(&file, position).is_empty());
    assert!(snapshot.references(&file, position, true).is_empty());
    assert!(
        snapshot
            .semantic_tokens(&file)
            .iter()
            .all(|t| t.character >= source.find(".normal").unwrap())
    );
    assert_eq!(snapshot.css_declarations(&file).declarations.len(), 1);
}
#[test]
fn utilities_have_signatures_hover_navigation_and_tokens() {
    let mut host = LanguageService::new();
    let file = FileId("a.xcss".into());
    let source =
        "@util trim($n: integer = 3) { max-height: $n * 1lh; } .a { @:trim(2) as injection; }";
    host.set_file(file.clone(), 1, source.into()).unwrap();
    let snapshot = host.snapshot();
    let position = at(source, "trim(2)");
    let hover = snapshot.hover(&file, position).unwrap();
    assert!(
        hover.text.contains("@util trim($n: integer = 3)"),
        "{}",
        hover.text
    );
    assert_eq!(snapshot.definition(&file, position).len(), 1);
    assert_eq!(snapshot.references(&file, position, true).len(), 2);
    assert_eq!(
        snapshot
            .semantic_tokens(&file)
            .iter()
            .filter(|t| TOKEN_TYPES[t.token_type] == "function")
            .count(),
        2
    );
}
#[test]
fn constants_have_types_navigation_tokens_and_revisioned_values() {
    let mut host = LanguageService::new();
    let file = FileId("a.xcss".into());
    let source = "$size: length = 56px; .a { width: $size; }";
    host.set_file(file.clone(), 1, source.into()).unwrap();
    let old = host.snapshot();
    let position = at(source, "$size;");
    let hover = old.hover(&file, position).unwrap();
    assert!(
        hover.text.contains("Constant $size: length"),
        "{}",
        hover.text
    );
    assert!(hover.text.contains("56px"));
    assert_eq!(old.definition(&file, position).len(), 1);
    assert_eq!(old.references(&file, position, true).len(), 2);
    let tokens = old.semantic_tokens(&file);
    assert_eq!(
        tokens
            .iter()
            .filter(|t| TOKEN_TYPES[t.token_type] == "variable")
            .count(),
        2
    );
    assert!(tokens.iter().any(|t| TOKEN_TYPES[t.token_type] == "type"));
    let declarations = old.css_declarations(&file);
    assert!(declarations.declarations[0].checkable);
    assert!(
        declarations.declarations[0]
            .parts
            .iter()
            .any(|p| p.value_type.as_deref() == Some("length"))
    );
    host.set_file(file.clone(), 2, source.replace("56px", "red"))
        .unwrap();
    assert!(!host.snapshot().diagnostics(&file).is_empty());
    assert!(old.diagnostics(&file).is_empty());
}
#[test]
fn multifile_queries_and_immutable_results() {
    let mut host = LanguageService::new();
    let lib = FileId("file:///lib.xcss".into());
    let file = FileId("file:///app.xcss".into());
    let source = "/* 😀 */\r\n@import lib from './lib'; .a { @:lib.paint(); color: --global; }";
    host.set_file(
        lib.clone(),
        1,
        "@injection paint($color: color = red) { color: $color; }".into(),
    )
    .unwrap();
    host.set_file(file.clone(), 1, source.into()).unwrap();
    host.set_resolution(file.clone(), "./lib".into(), Some(lib.clone()));
    let old = host.snapshot();
    assert!(old.diagnostics(&file).is_empty());
    assert_eq!(old.definition(&file, at(source, "paint"))[0].file, lib);
    assert!(
        old.hover(&file, at(source, "paint"))
            .unwrap()
            .text
            .contains("$color: color = red")
    );
    assert_eq!(old.references(&file, at(source, "paint"), false).len(), 1);
    assert_eq!(old.references(&file, at(source, "paint"), true).len(), 2);
    assert!(old.definition(&file, at(source, "--global")).is_empty());
    let tokens = old.semantic_tokens(&file);
    assert!(
        tokens
            .iter()
            .any(|t| TOKEN_TYPES[t.token_type] == "namespace" && !t.declaration)
    );
    for pair in tokens.windows(2) {
        assert!(
            (pair[0].line, pair[0].character + pair[0].length) <= (pair[1].line, pair[1].character)
        );
    }
    host.remove_file(&lib, 2).unwrap();
    assert!(!host.snapshot().diagnostics(&file).is_empty());
    assert!(old.diagnostics(&file).is_empty());
    assert_eq!(old.definition(&file, at(source, "paint")).len(), 1);
}
#[test]
fn classifications_recovery_escapes_and_utf16() {
    let mut host = LanguageService::new();
    let file = FileId("a".into());
    let text = "/* 😀 */ @vars { local --size: length = 1px; } @injection paint($p: color = red) { color: $p; } @keyframes spin { to { opacity: 1; } } .caf\\e9 #id { width: --size; animation: spin 1s; @:missing(); content: 'paint'; } .later { color: red; }";
    host.set_file(file.clone(), 1, text.into()).unwrap();
    let snapshot = host.snapshot();
    let tokens = snapshot.semantic_tokens(&file);
    for ty in [
        "function",
        "parameter",
        "variable",
        "type",
        "class",
        "enumMember",
        "property",
    ] {
        assert!(
            tokens.iter().any(|t| TOKEN_TYPES[t.token_type] == ty),
            "missing {ty}"
        );
    }
    assert!(
        snapshot
            .diagnostics(&file)
            .iter()
            .any(|d| d.diagnostic.code == "XC-212"
                && d.label == "Unknown injection"
                && d.legacy_code == Some("binding.unknown-injection"))
    );
    assert!(
        snapshot
            .hover(&file, at(text, "--size;"))
            .unwrap()
            .text
            .contains("inherits: false")
    );
    assert!(
        snapshot
            .definition(
                &file,
                Position {
                    line: 0,
                    character: 4
                }
            )
            .is_empty()
    );
    assert!(host.set_file(file.clone(), 1, "".into()).is_err());
    host.set_file(
        file.clone(),
        2,
        ".a { @:missing(\ncolor: red; } .later { color: blue; }".into(),
    )
    .unwrap();
    assert!(!host.snapshot().semantic_tokens(&file).is_empty());
}

#[test]
fn functions_have_return_signatures_navigation_and_value_types() {
    let file = FileId("a.xcss".into());
    let source = "@function size($n: length = 2px): length => $n; .a { width: @:size(3px); }";
    let mut host = LanguageService::new();
    host.set_file(file.clone(), 1, source.into()).unwrap();
    let snapshot = host.snapshot();
    assert!(
        snapshot.diagnostics(&file).is_empty(),
        "{:?}",
        snapshot.diagnostics(&file)
    );
    let position = at(source, "size(3px)");
    let hover = snapshot.hover(&file, position).unwrap();
    assert!(
        hover
            .text
            .contains("@function size($n: length = 2px): length"),
        "{}",
        hover.text
    );
    assert_eq!(snapshot.definition(&file, position).len(), 1);
    assert_eq!(snapshot.references(&file, position, true).len(), 2);
    assert!(
        snapshot
            .semantic_tokens(&file)
            .iter()
            .any(|t| t.character == position.character && TOKEN_TYPES[t.token_type] == "function")
    );
    let declarations = snapshot.css_declarations(&file);
    assert!(declarations.declarations[0].checkable);
    assert!(
        declarations.declarations[0]
            .parts
            .iter()
            .any(|p| p.value_type.as_deref() == Some("length"))
    );
    assert!(
        snapshot
            .css_values(&file)
            .values
            .iter()
            .any(|v| v.text == "$n")
    );
}

#[test]
fn function_completions_and_revisions_follow_resolution() {
    let file = FileId("a.xcss".into());
    let source = "@import lib from './lib.xcss'; .a { width: @:lib.si; }";
    let mut host = LanguageService::new();
    host.set_file(
        FileId("lib.xcss".into()),
        1,
        "@function size(): length => 2px;".into(),
    )
    .unwrap();
    host.set_file(file.clone(), 1, source.into()).unwrap();
    host.set_resolution(
        file.clone(),
        "./lib.xcss".into(),
        Some(FileId("lib.xcss".into())),
    );
    let snapshot = host.snapshot();
    let position = at(source, "; }");
    assert!(
        snapshot
            .completion(&file, position)
            .iter()
            .any(|item| item.label == "lib.size")
    );
    host.set_file(
        FileId("lib.xcss".into()),
        2,
        "@function scale(): length => 3px;".into(),
    )
    .unwrap();
    assert!(
        snapshot
            .completion(&file, position)
            .iter()
            .any(|item| item.label == "lib.size")
    );
    assert!(host.snapshot().completion(&file, position).is_empty());
}
