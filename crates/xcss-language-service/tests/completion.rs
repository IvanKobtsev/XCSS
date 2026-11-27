use xcss_language_service::LanguageService;
use xcss_source::{FileId, SourceFile};

fn complete(source: &str, libraries: &[(&str, &str)]) -> Vec<(String, String)> {
    let offset = source.find('|').unwrap();
    let source = source.replacen('|', "", 1);
    let file = FileId("main.xcss".into());
    let position = SourceFile::new(file.clone(), 1, source.clone())
        .byte_to_utf16(offset)
        .unwrap();
    let mut service = LanguageService::new();
    service.set_file(file.clone(), 1, source).unwrap();
    for (name, text) in libraries {
        let target = FileId((*name).into());
        service.set_file(target.clone(), 1, (*text).into()).unwrap();
        service.set_resolution(file.clone(), format!("./{name}"), Some(target));
    }
    service
        .snapshot()
        .completion(&file, position)
        .into_iter()
        .map(|c| (c.label, c.kind.into()))
        .collect()
}

const DEFINITIONS: &str = "@util equalGridColumns() { display: grid; } @injection paint() { color: red; } @function size(): length => 1px;";

#[test]
fn statement_candidates_are_complete_and_kind_specific() {
    for call in [".a { @:Grid|; }", ".a { @:|", "@:|", ".a { @:Gr|id(); }"] {
        let items = complete(&format!("{DEFINITIONS} {call}"), &[]);
        assert_eq!(
            items,
            vec![
                ("equalGridColumns".into(), "util".into()),
                ("paint".into(), "injection".into())
            ],
            "{call}"
        );
    }
}

#[test]
fn value_candidates_ignore_prefix_and_exclude_statement_callables() {
    for call in [
        ".a { width: @:Other|; }",
        ".a { width: @:|",
        ".a { width: calc(@:|); }",
    ] {
        assert_eq!(
            complete(&format!("{DEFINITIONS} {call}"), &[]),
            vec![("size".into(), "function".into())],
            "{call}"
        );
    }
}

#[test]
fn opaque_contexts_have_no_candidates() {
    for call in [
        "/* @:| */",
        ".a { content: '@:|'; }",
        ".a { background: url(@:|); }",
        "@raw { @:| }",
        ".a { @:size(|); }",
    ] {
        assert!(
            complete(&format!("{DEFINITIONS} {call}"), &[]).is_empty(),
            "{call}"
        );
    }
}

#[test]
fn imported_and_global_candidates_follow_resolution() {
    let libraries = [("lib.xcss", DEFINITIONS)];
    let items = complete(
        "@import lib from './lib.xcss'; .a { @:lib.Gr|; }",
        &libraries,
    );
    assert!(items.contains(&("lib.equalGridColumns".into(), "util".into())));
    assert!(items.contains(&("lib.paint".into(), "injection".into())));
    assert!(!items.iter().any(|(_, kind)| kind == "function"));
    let items = complete(
        "@import { equalGridColumns } from './lib.xcss'; .a { @:|; }",
        &libraries,
    );
    assert!(items.contains(&("equalGridColumns".into(), "util".into())));
    let items = complete(
        ".a { @:|; }",
        &[("a.xcss", "@util same() {}"), ("b.xcss", "@util same() {}")],
    );
    assert!(items.is_empty());
    let items = complete(
        "@util same() {} .a { @:|; }",
        &[("a.xcss", "@util same() {}")],
    );
    assert_eq!(items, vec![("same".into(), "util".into())]);
}

#[test]
fn replacement_includes_suffix_and_preserves_punctuation() {
    let source = format!("{DEFINITIONS} .a {{ @:Gr|id(2); }}");
    let offset = source.find('|').unwrap();
    let text = source.replace('|', "");
    let file = FileId("main.xcss".into());
    let source = SourceFile::new(file.clone(), 1, text.clone());
    let mut service = LanguageService::new();
    service.set_file(file.clone(), 1, text).unwrap();
    let snapshot = service.snapshot();
    let items = snapshot.completion(&file, source.byte_to_utf16(offset).unwrap());
    let item = items
        .iter()
        .find(|c| c.label == "equalGridColumns")
        .unwrap();
    assert_eq!(item.range.start, source.byte_to_utf16(offset - 2).unwrap());
    assert_eq!(item.range.end, source.byte_to_utf16(offset + 2).unwrap());
    assert_eq!(item.new_text, "equalGridColumns");
}
