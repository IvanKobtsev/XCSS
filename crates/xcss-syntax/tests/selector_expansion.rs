use xcss_syntax::expand_selectors;

fn expand(parent: &str, child: &str) -> String {
    expand_selectors(Some(parent), child, &mut 100_000)
        .unwrap()
        .text
}

#[test]
fn lists_and_repeated_parents_expand_independently() {
    assert_eq!(
        expand(".a, #b", "&.c, > .d"),
        ".a.c, #b.c, .a > .d, #b > .d"
    );
    assert_eq!(
        expand(".a, .b", "& + &"),
        ".a + .a, .a + .b, .b + .a, .b + .b"
    );
    assert_eq!(
        expand(".outer .inner", "[dir=rtl] &.active"),
        "[dir=rtl] .outer .inner.active"
    );
}

#[test]
fn selector_functions_keep_their_argument_lists() {
    assert_eq!(expand(".a, .b", ":is(of &)"), ":is(of .a, of .b)");
    assert_eq!(expand(".a, .b", ":not([of] &)"), ":not([of] .a, [of] .b)");
    assert_eq!(expand(".a, #b", ":not(&)"), ":not(.a, #b)");
    assert_eq!(
        expand(".a, #b", "&:is(.x, :where(&))"),
        ".a:is(.x, :where(.a, #b)), #b:is(.x, :where(.a, #b))"
    );
    assert_eq!(
        expand(".a, .b", ":nth-child(2n of &.x)"),
        ":nth-child(2n of .a.x, .b.x)"
    );
    assert_eq!(expand(".a, .b", ":has(> &)"), ":has(> .a, > .b)");
}

#[test]
fn strings_attributes_comments_and_escapes_are_not_nesting() {
    assert_eq!(
        expand(".a", "[data-value='&,'] .escaped\\&"),
        ".a [data-value='&,'] .escaped\\&"
    );
    assert_eq!(expand(".a", "& /* & */ > .b"), ".a /* & */ > .b");
}

#[test]
fn expansion_has_a_budget_and_tracks_both_sources() {
    assert!(
        expand_selectors(Some(".a"), "&--modifier", &mut 100)
            .unwrap_err()
            .contains("suffix")
    );
    assert!(expand_selectors(Some(".a, .b"), "& & & &", &mut 10).is_err());
    let result = expand_selectors(Some(".parent"), "&.child", &mut 100).unwrap();
    assert_eq!(result.text, ".parent.child");
    assert!(result.spans.iter().any(|s| s.parent));
    assert!(result.spans.iter().any(|s| !s.parent));
    for span in &result.spans {
        let source = if span.parent { ".parent" } else { "&.child" };
        assert_eq!(
            &result.text[span.generated.start..span.generated.end],
            &source[span.source.start..span.source.end]
        );
    }
}
