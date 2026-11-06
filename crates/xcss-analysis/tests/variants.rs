use xcss_analysis::AnalysisHost;
use xcss_source::FileId;

#[test]
fn groups_are_local_forward_lists_of_exported_classes() {
    let mut host = AnalysisHost::new();
    let id = FileId("a.xcss".into());
    host.set_file(id.clone(), 1, "@variants Button: .primary, .secondary; .Button { &.primary {} &.secondary {} &.other {} }").unwrap();
    let snapshot = host.snapshot();
    assert!(
        snapshot.diagnostics().is_empty(),
        "{:?}",
        snapshot.diagnostics()
    );
    let interface = snapshot.module_interface(&id).unwrap();
    assert_eq!(interface.variants["Button"], ["primary", "secondary"]);
    assert!(interface.classes.contains("other"));
    host.set_file(id.clone(), 2, "@variants Button: .missing, .missing; @variants Button: .global; :global(.global) {} .a { @variants Nested: .a; }").unwrap();
    let next = host.snapshot();
    for code in [
        "variants.unknown-class",
        "variants.duplicate-member",
        "variants.duplicate-group",
        "variants.placement",
    ] {
        assert!(
            next.diagnostics().iter().any(|d| d.code == code),
            "{code}: {:?}",
            next.diagnostics()
        );
    }
    assert_eq!(
        snapshot.module_interface(&id).unwrap().variants["Button"],
        ["primary", "secondary"]
    );
}

#[test]
fn malformed_groups_recover_and_called_injections_export_classes() {
    let mut host = AnalysisHost::new();
    let id = FileId("a.xcss".into());
    host.set_file(id.clone(), 1, "@variants Empty:; @variants Broken: .a,; .fine {} @injection unused() { .hidden {} } @injection used() { .visible {} } @:used();").unwrap();
    let s = host.snapshot();
    assert!(s.diagnostics().len() >= 2);
    let interface = s.module_interface(&id).unwrap();
    assert!(interface.classes.contains("fine"));
    assert!(interface.classes.contains("visible"));
    assert!(!interface.classes.contains("hidden"));
}

#[test]
fn group_and_member_identifiers_follow_css_grammar() {
    let mut host = AnalysisHost::new();
    host.set_file(
        FileId("a.xcss".into()),
        1,
        "@variants 123: .fine; @variants Fine: . fine; .fine {}",
    )
    .unwrap();
    let snapshot = host.snapshot();
    assert_eq!(
        snapshot
            .diagnostics()
            .iter()
            .filter(|d| d.code == "syntax.invalid-construct")
            .count(),
        2
    );
}
