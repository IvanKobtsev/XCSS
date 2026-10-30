use xcss_syntax::{Specificity, analyze_selectors};

fn s(ids: u32, classes: u32, types: u32) -> Specificity {
    Specificity {
        ids,
        classes,
        types,
    }
}
fn check(selector: &str, expected: Specificity) {
    let result = analyze_selectors(selector, 0, None);
    assert_eq!(result.len(), 1, "{selector}: {result:?}");
    assert_eq!(
        result[0].specificity,
        Some(expected),
        "{selector}: {result:?}"
    );
}
#[test]
fn mdn_specificity_examples() {
    for (selector, expected) in [
        ("*", s(0, 0, 0)),
        ("h1", s(0, 0, 1)),
        ("#myApp [id=\"myElement\"]", s(1, 1, 0)),
        (
            "div.button#main[data-kind='hello']:hover::after",
            s(1, 3, 2),
        ),
        (":where(#defaultTheme) a", s(0, 0, 1)),
        (":is(p, #fakeId)", s(1, 0, 0)),
        ("h2:has(~ h2)", s(0, 0, 2)),
        ("div:not(.inner, #fakeId) p", s(1, 0, 2)),
        ("a:not(#fakeId#fakeId#fakeID)", s(3, 0, 1)),
        (":nth-child(even of li, .item)", s(0, 2, 0)),
        (":nth-last-child(2n of #main, .item)", s(1, 1, 0)),
        (":nth-of-type(3n)", s(0, 1, 0)),
        (":host(.active)", s(0, 2, 0)),
        (":host-context(#theme)", s(1, 1, 0)),
        ("::slotted(.item)", s(0, 1, 1)),
        ("::part(label):hover", s(0, 1, 1)),
        ("p:before", s(0, 0, 2)),
        ("svg|a > *|* + |b", s(0, 0, 2)),
        (":local(.button):global(#root)", s(1, 1, 0)),
        (".caf\\e9", s(0, 1, 0)),
        (".café", s(0, 1, 0)),
        ("a\\|b", s(0, 0, 1)),
        (":local .a :global #b", s(1, 1, 0)),
        (":scope .item", s(0, 2, 0)),
    ] {
        check(selector, expected);
    }
}
#[test]
fn nesting_and_lexicographic_comparison() {
    assert!(s(1, 0, 0) > s(0, 100, 100));
    for (selector, expected) in [
        ("& > .child", s(1, 1, 0)),
        (".child", s(1, 1, 0)),
        ("&&", s(2, 0, 0)),
        (":where(&) .child", s(0, 1, 0)),
    ] {
        let result = analyze_selectors(selector, 0, Some(Some(s(1, 0, 0))));
        assert_eq!(
            result[0].specificity,
            Some(expected),
            "{selector}: {result:?}"
        );
    }
}
#[test]
fn classifications_preserve_ranges_and_nested_arguments() {
    let text = "svg|a.foo#bar[data-name='hello']:not(.disabled)::after > * + &";
    let result = analyze_selectors(text, 12, Some(Some(s(0, 1, 0))));
    assert!(result[0].specificity.is_some(), "{result:?}");
    let parts: Vec<_> = result[0]
        .parts
        .iter()
        .map(|p| {
            (
                p.kind.token_type(),
                &text[p.range.start - 12..p.range.end - 12],
            )
        })
        .collect();
    assert!(parts.contains(&("class", "disabled")));
    assert!(parts.contains(&("selectorAttribute", "data-name")));
    for kind in [
        "class",
        "selectorId",
        "selectorTag",
        "selectorAttribute",
        "pseudoClass",
        "pseudoElement",
        "selectorUniversal",
        "selectorNesting",
        "selectorCombinator",
        "selectorNamespace",
    ] {
        assert!(parts.iter().any(|p| p.0 == kind), "{kind}: {parts:?}");
    }
    for pair in result[0].parts.windows(2) {
        assert!(pair[0].range.end <= pair[1].range.start);
    }
}
#[test]
fn malformed_and_unknown_are_not_exact_scores() {
    for text in [".a[", ":not(", ":unknown-function(.a)", "&"] {
        let result = analyze_selectors(text, 0, None);
        assert!(
            result
                .iter()
                .all(|s| s.specificity.is_none() && s.reason.is_some()),
            "{text}: {result:?}"
        );
    }
    let result = analyze_selectors(".valid, .broken[", 0, None);
    assert_eq!(result[0].specificity, Some(s(0, 1, 0)));
    assert!(result[1].specificity.is_none());
    let result = analyze_selectors("a\\|b", 0, None);
    assert_eq!(result[0].parts.len(), 1);
    assert_eq!(result[0].parts[0].kind.token_type(), "selectorTag");
}
