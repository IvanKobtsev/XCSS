use xcss_language_service::{LanguageService, Specificity, TOKEN_TYPES};
use xcss_source::{FileId, SourceFile};

#[test]
fn selectors_are_revisioned_and_use_original_utf16_ranges() {
    let mut host = LanguageService::new();
    let file = FileId("file:///selectors.xcss".into());
    let text = "/* 😀 */\r\n#parent, .a { @media (width > 0px) { & > a.foo[data-x='hello']:not(.off)::after { color: red; } } .child {} } @keyframes spin { from {} 50% {} to {} }";
    host.set_file(file.clone(), 1, text.into()).unwrap();
    let old = host.snapshot();
    let result = old.selectors(&file);
    assert_eq!(result.selectors.len(), 4, "{result:?}");
    assert_eq!(
        result.selectors[2].specificity,
        Some(Specificity {
            ids: 1,
            classes: 3,
            types: 2
        })
    );
    assert_eq!(
        result.selectors[3].specificity,
        Some(Specificity {
            ids: 1,
            classes: 1,
            types: 0
        })
    );
    let source = SourceFile::new(file.clone(), 1, text);
    for part in result.selectors.iter().flat_map(|s| &s.parts) {
        assert!(source.utf16_to_byte(part.range.start).is_some());
        assert!(source.utf16_to_byte(part.range.end).is_some());
    }
    let tokens = old.semantic_tokens(&file);
    for expected in [
        "selectorId",
        "selectorTag",
        "class",
        "selectorAttribute",
        "pseudoClass",
        "pseudoElement",
        "selectorNesting",
        "selectorCombinator",
    ] {
        assert!(
            tokens.iter().any(|t| TOKEN_TYPES[t.token_type] == expected),
            "missing {expected}: {tokens:?}"
        );
    }
    for pair in tokens.windows(2) {
        assert!(
            pair[0].line < pair[1].line
                || pair[0].line == pair[1].line
                    && pair[0].character + pair[0].length <= pair[1].character
        );
    }
    host.set_file(file.clone(), 2, ".replacement {}".into())
        .unwrap();
    let new = host.snapshot().selectors(&file);
    assert!(new.revision > result.revision);
    assert_eq!(old.selectors(&file).revision, result.revision);
    assert_eq!(new.selectors.len(), 1);
}

#[test]
fn invalid_parent_and_injection_context_are_explicitly_unavailable() {
    let mut host = LanguageService::new();
    let file = FileId("file:///invalid.xcss".into());
    host.set_file(
        file.clone(),
        1,
        "@injection paint() { & .child {} } .valid, .bad[ {}".into(),
    )
    .unwrap();
    let snapshot = host.snapshot();
    let result = snapshot.selectors(&file);
    assert!(
        result
            .selectors
            .iter()
            .any(|s| s.specificity.is_none() && s.reason.is_some()),
        "{result:?}"
    );
}
