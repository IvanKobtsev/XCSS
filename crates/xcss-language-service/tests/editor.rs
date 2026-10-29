use xcss_language_service::{LanguageService, TOKEN_TYPES};
use xcss_source::{FileId, Position, SourceFile};

fn at(source: &str, needle: &str) -> Position {
    SourceFile::new(FileId("test".into()), 0, source)
        .byte_to_utf16(source.find(needle).unwrap())
        .unwrap()
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
            .any(|d| d.diagnostic.code == "binding.unknown-injection")
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
