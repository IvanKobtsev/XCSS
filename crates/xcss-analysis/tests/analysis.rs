use xcss_analysis::{AnalysisHost, SymbolKind, UpdateError, ValueType};
use xcss_source::FileId;

fn id(name: &str) -> FileId {
    FileId(name.into())
}
#[test]
fn raw_contents_have_no_semantics() {
    let mut host = AnalysisHost::new();
    host.set_file(id("a.xcss"),1,"@raw { @import { missing } from './absent.xcss'; @vars { --raw: length = nonsense; } $x = $unknown; .raw { @:missing; composes: absent; } @keyframes raw {} } .normal { color: red; }").unwrap();
    let snapshot = host.snapshot();
    assert!(
        snapshot.diagnostics().is_empty(),
        "{:?}",
        snapshot.diagnostics()
    );
    assert!(snapshot.dependencies().is_empty());
    assert!(snapshot.references().iter().all(|r| r.name == "normal"));
    assert_eq!(snapshot.symbols().len(), 1);
    assert_eq!(snapshot.symbols()[0].name, "normal");
}
#[test]
fn utilities_validate_context_order_types_and_transitive_bodies() {
    for (source, code) in [
        ("@util u {} @:u;", "syntax.util-selector"),
        (
            "@util u {} .a { color: red; @:u; width: 1px; }",
            "syntax.util-order",
        ),
        (
            "@util u { .local { color: red; } }",
            "syntax.util-local-class",
        ),
        (
            "@injection bad { .local {} } @util u { @:bad; }",
            "syntax.util-local-class",
        ),
        ("@util u { @content; }", "syntax.util-content"),
        ("@util u {} .a { @:u {} }", "syntax.util-content"),
        (
            "@injection i {} .a { @:i as injection; }",
            "syntax.util-modifier",
        ),
        (
            "@util u($n: integer) {} .a { @:u(red); }",
            "type.argument-mismatch",
        ),
        ("@util u { @:u; } .a { @:u; }", "binding.injection-cycle"),
    ] {
        let mut h = host(&[("a.xcss", source)]);
        let snapshot = h.snapshot();
        assert!(
            snapshot.diagnostics().iter().any(|d| d.code == code),
            "{source}: {:?}",
            snapshot.diagnostics()
        );
    }
    let mut h = host(&[(
        "a.xcss",
        "@util u($n: integer = 3) { &:hover { color: red; } :global(.icon) { width: $n * 1px; } } .a { @:u; color: red; @:u(4); }",
    )]);
    let s = h.snapshot();
    assert!(s.diagnostics().is_empty(), "{:?}", s.diagnostics());
    assert!(s.symbols().iter().any(|s| s.kind == SymbolKind::Util));
}
#[test]
fn constants_infer_through_references_and_dimensional_arithmetic() {
    let mut h = host(&[(
        "a.xcss",
        "@vars { --height: length = 4px; } $base = 56px; $height = $base + --height / 2; $gradient = linear-gradient(to right, #000 --start, #fff (--end + --offset)); .a { $local = $height; height: $local; }",
    )]);
    let s = h.snapshot();
    assert!(s.diagnostics().is_empty(), "{:#?}", s.diagnostics());
    for (name, ty) in [
        ("$base", "length"),
        ("$height", "length"),
        ("$gradient", "image"),
        ("$local", "length"),
    ] {
        let symbol = s.symbols().iter().find(|s| s.name == name).unwrap();
        assert_eq!(symbol.kind, SymbolKind::Constant);
        assert_eq!(symbol.value_type, ValueType::named(ty));
        assert!(s.constant_initializer(symbol.id).is_some());
    }
}

#[test]
fn constants_enforce_visibility_immutability_and_types() {
    for source in [
        "$x = 1px; $x = 1px;",
        "$x = 1px; .a { $x = 2px; }",
        "$x = $x;",
        "$x = $later; $later = 1px;",
        ".a { $x = 1px; } .b { width: $x; }",
        ".a { $x = 1px; } .a { width: $x; }",
        "$x: color = 1px;",
        "$x = var(--unknown);",
        "$x = 1px; @injection f($x: length) {}",
        "@injection f($x: length) { $x = 1px; }",
        "$x = 1px; @injection f($p: color) {} .a { @:f($x); }",
        "$x = 1px; .$x { color: red; }",
        "$x = 1px; @keyframes $x { to { opacity: 1; } }",
        "$x = 1px; @import $x;",
        "$x = 1px; .a { background: url($x); }",
        "$x: length = red + 1px;",
        "$x: color = red + blue;",
        "$x: length = 1px * 2px;",
    ] {
        let mut h = host(&[("a.xcss", source)]);
        assert!(
            !h.snapshot().diagnostics().is_empty(),
            "Expected error: {source}"
        );
    }
    let mut h = host(&[(
        "a.xcss",
        "$x: length = var(--unknown); .a { $local = 2px; &:hover { width: $local; } } .b { $local = 3px; width: $local; }",
    )]);
    assert!(
        h.snapshot().diagnostics().is_empty(),
        "{:#?}",
        h.snapshot().diagnostics()
    );
}
#[test]
fn constants_are_block_scoped_and_file_local() {
    let mut h = host(&[
        (
            "a.xcss",
            "@media (width > 1px) { $size = 2px; .a { width: $size; } } .b { width: $size; }",
        ),
        ("b.xcss", "$size = 3px; .c { width: $size; }"),
    ]);
    let snapshot = h.snapshot();
    assert_eq!(
        snapshot
            .diagnostics()
            .iter()
            .filter(|d| d.code == "binding.parameter-scope")
            .count(),
        1
    );
    assert!(
        snapshot
            .diagnostics()
            .iter()
            .all(|d| d.location.file == id("a.xcss"))
    );
}
#[test]
fn constant_dependent_defaults_are_inferred_and_checked() {
    let mut h = host(&[(
        "a.xcss",
        "$size = 3px; @injection f($p = $size) { $local = $p * 2; width: $local; } .a { @:f(); }",
    )]);
    let snapshot = h.snapshot();
    assert!(
        snapshot.diagnostics().is_empty(),
        "{:#?}",
        snapshot.diagnostics()
    );
    for name in ["$p", "$local"] {
        assert_eq!(
            snapshot
                .symbols()
                .iter()
                .find(|s| s.name == name)
                .unwrap()
                .value_type,
            ValueType::named("length")
        );
    }
    let mut h = host(&[("a.xcss", "$size = 3px; @injection f($p: color = $size) {}")]);
    assert!(
        h.snapshot()
            .diagnostics()
            .iter()
            .any(|d| d.code == "type.mismatch")
    );
}
fn host(files: &[(&str, &str)]) -> AnalysisHost {
    let mut host = AnalysisHost::new();
    for (name, text) in files {
        host.set_file(id(name), 1, *text).unwrap();
    }
    host
}
#[test]
fn resolves_injections_parameters_vars_and_classes() {
    let mut h = host(&[
        (
            "lib.xcss",
            "@injection tone($color: color = red) { color: $color; @content; }",
        ),
        (
            "a.xcss",
            "@import { tone } from \"lib.xcss\"; @vars { --size = 2px; } .a { @:tone(blue) { width: --size; } }",
        ),
    ]);
    h.set_resolution(id("a.xcss"), "lib.xcss", Some(id("lib.xcss")));
    let s = h.snapshot();
    assert!(s.diagnostics().is_empty(), "{:#?}", s.diagnostics());
    let injection = s
        .symbols()
        .iter()
        .find(|s| s.kind == SymbolKind::Injection)
        .unwrap();
    assert_eq!(s.references_to(injection.id).count(), 2);
    assert_eq!(s.signature(injection.id).unwrap().parameters.len(), 1);
    let variable = s.symbols().iter().find(|s| s.name == "--size").unwrap();
    assert_eq!(variable.value_type, ValueType::named("length"));
    assert_eq!(s.references_to(variable.id).count(), 1);
    let reference = s.references_to(variable.id).next().unwrap();
    assert_eq!(
        s.symbol_at(&id("a.xcss"), reference.location.range.start)
            .unwrap()
            .id,
        variable.id
    );
}
#[test]
fn edits_global_index_and_preserves_snapshots() {
    let mut h = host(&[("a", "@injection shared {}"), ("b", ".b { @:shared; }")]);
    let old = h.snapshot();
    assert!(old.diagnostics().is_empty());
    let parsed = old.syntax(&id("b")).unwrap() as *const _;
    h.set_file(id("c"), 1, "@injection shared {}").unwrap();
    let conflict = h.snapshot();
    assert!(
        conflict
            .diagnostics()
            .iter()
            .any(|d| d.code == "binding.ambiguous-injection")
    );
    assert_eq!(parsed, conflict.syntax(&id("b")).unwrap() as *const _);
    h.remove_file(&id("c"), 2).unwrap();
    assert!(h.snapshot().diagnostics().is_empty());
    assert!(old.diagnostics().is_empty());
    assert_eq!(h.set_file(id("c"), 1, ""), Err(UpdateError::StaleRevision));
}
#[test]
fn resolution_precedence_and_exact_names() {
    let mut h = host(&[
        ("a", "@injection same {} @injection foo-bar {}"),
        (
            "b",
            "@injection same {} @injection foo_bar {} .b { @:same; @:foo_bar; @:foo-bar; }",
        ),
        (
            "c",
            "@import lib from \"a\"; @import { same } from \"a\"; .c { @:lib.same; @:same; }",
        ),
    ]);
    h.set_resolution(id("c"), "a", Some(id("a")));
    assert!(
        h.snapshot().diagnostics().is_empty(),
        "{:#?}",
        h.snapshot().diagnostics()
    );
}
#[test]
fn diagnostics_and_cycles() {
    let mut h = host(&[(
        "a",
        "@injection a($size: length) { @:b; width: $size; } @injection b { @:a(2px); } .a { @:a(red); color: $outside; @:missing; }",
    )]);
    let s = h.snapshot();
    for code in [
        "binding.injection-cycle",
        "type.argument-mismatch",
        "binding.parameter-scope",
        "binding.unknown-injection",
    ] {
        assert!(
            s.diagnostics().iter().any(|d| d.code == code),
            "Missing {code}: {:#?}",
            s.diagnostics()
        );
    }
}
#[test]
fn css_modules_compositions_and_global_classes() {
    let mut h = host(&[
        (
            "a",
            ".base { color: red; } :global(.external) { color: red; }",
        ),
        (
            "b",
            ".button { composes: base from \"a\"; } .one { composes: two; } .two { composes: one; }",
        ),
    ]);
    h.set_resolution(id("b"), "a", Some(id("a")));
    let s = h.snapshot();
    assert!(s.symbols().iter().any(|s| s.name == "external" && s.global));
    assert_eq!(
        s.diagnostics()
            .iter()
            .filter(|d| d.code == "binding.composition-cycle")
            .count(),
        2
    );
    assert!(
        !s.diagnostics()
            .iter()
            .any(|d| d.code == "binding.missing-export")
    );
}
#[test]
fn missing_import_does_not_cascade() {
    let mut h = host(&[("a", "@import { tone } from \"missing\"; .a { @:tone; }")]);
    let s = h.snapshot();
    assert_eq!(s.diagnostics().len(), 1, "{:#?}", s.diagnostics());
}
#[test]
fn dynamic_values_do_not_cause_type_errors() {
    let mut h = host(&[(
        "a",
        "@vars { --size: length; } @injection size($x: length) { width: $x; } .a { @:size(calc(1px + 2%)); width: var(--external); }",
    )]);
    assert!(
        h.snapshot().diagnostics().is_empty(),
        "{:#?}",
        h.snapshot().diagnostics()
    );
}

#[test]
fn expressions_and_transition_property_positions() {
    let mut h = host(&[(
        "a",
        "@vars { --theme: color; --duration = 1s; --width = 2px; } .a { width: --width; transition: (color, --theme) --duration ease; transition-property: --theme; margin: 10px + 2px 20px; }",
    )]);
    let s = h.snapshot();
    assert!(
        s.diagnostics()
            .iter()
            .any(|d| d.code == "type.ambiguous-arithmetic")
    );
    let theme = s.symbols().iter().find(|s| s.name == "--theme").unwrap();
    assert_eq!(s.references_to(theme.id).count(), 0);
    let duration = s.symbols().iter().find(|s| s.name == "--duration").unwrap();
    assert_eq!(s.references_to(duration.id).count(), 1);
    assert!(
        s.expressions()
            .iter()
            .any(|e| e.value_type == ValueType::named("length"))
    );
}

#[test]
fn malformed_projects_still_offer_symbols() {
    let mut h = host(&[(
        "a",
        "@injection tone($x: length) { width: $x; } .broken { @:tone(\n height: 1px; } .later { color: red; }",
    )]);
    let s = h.snapshot();
    assert!(!s.diagnostics().is_empty());
    assert!(s.symbols().iter().any(|s| s.name == "later"));
    assert!(s.symbols().iter().any(|s| s.name == "tone"));
}

#[test]
fn imports_and_file_replacements_drop_stale_targets() {
    let mut h = host(&[
        ("a", "@injection foo {}"),
        ("b", "@import ns from \"a\"; .b { @:ns.foo; }"),
    ]);
    h.set_resolution(id("b"), "a", Some(id("a")));
    let old = h.snapshot();
    assert!(old.diagnostics().is_empty());
    h.set_file(id("a"), 2, "@injection bar {}").unwrap();
    assert!(
        h.snapshot()
            .diagnostics()
            .iter()
            .any(|d| d.code == "binding.unknown-injection")
    );
    h.set_resolution(id("b"), "a", None);
    assert_eq!(h.snapshot().diagnostics().len(), 1);
    assert!(old.diagnostics().is_empty());
    assert_eq!(h.set_file(id("a"), 2, ""), Err(UpdateError::StaleRevision));
}

#[test]
fn declaration_signature_and_import_diagnostics() {
    let mut h = host(&[
        (
            "a",
            "@vars { --x: mystery; --x = 2px; --bad: length = red; } @injection f($x: length = 1px, $y: time) {} @import { missing } from \"b\"; @import { missing } from \"b\"; .a { @:f; @:f(2px, 1s) {} @content; }",
        ),
        ("b", "@injection other {}"),
    ]);
    h.set_resolution(id("a"), "b", Some(id("b")));
    let s = h.snapshot();
    for code in [
        "type.unknown-type",
        "binding.duplicate",
        "type.mismatch",
        "type.parameter-order",
        "binding.missing-export",
        "binding.import-conflict",
        "type.argument-count",
        "type.unexpected-content",
        "binding.content-scope",
    ] {
        assert!(
            s.diagnostics().iter().any(|d| d.code == code),
            "Missing {code}: {:#?}",
            s.diagnostics()
        );
    }
    assert!(
        s.diagnostics()
            .iter()
            .find(|d| d.code == "binding.duplicate")
            .unwrap()
            .related
            .len()
            == 1
    );
}

#[test]
fn escaped_classes_and_composition_reference_ranges() {
    let mut h = host(&[(
        "a",
        ".composes {} .caf\\e9 {} .target { composes: composes caf\\e9; }",
    )]);
    let s = h.snapshot();
    assert!(s.diagnostics().is_empty(), "{:?}", s.diagnostics());
    let symbol = s.symbols().iter().find(|s| s.name == "composes").unwrap();
    let reference = s.references_to(symbol.id).next().unwrap();
    let source = s.syntax(&id("a")).unwrap();
    assert_eq!(source.slice(reference.location.range), "composes");
    assert_eq!(
        reference.location.range.start,
        source.source.text().rfind("composes caf").unwrap()
    );
    assert!(s.symbols().iter().any(|s| s.name == "café"));
}

#[test]
fn literal_types_comments_and_scoped_assignments() {
    let mut h = host(&[(
        "a",
        "@vars { --color /*name*/: color /*type*/ = lime /*value*/; --size = 1e2px; } @injection f($x /*name*/: length /*type*/ = 1px) { width: $x; } .a { --color: blue; @:f(--color); }",
    )]);
    let s = h.snapshot();
    assert_eq!(
        s.diagnostics().iter().map(|d| d.code).collect::<Vec<_>>(),
        vec!["type.argument-mismatch"]
    );
    let color = s.symbols().iter().find(|s| s.name == "--color").unwrap();
    assert_eq!(color.value_type, ValueType::named("color"));
    assert_eq!(s.references_to(color.id).count(), 2);
    assert_eq!(
        xcss_analysis::infer_type("1e2px"),
        ValueType::named("length")
    );
}

#[test]
fn invalid_updates_are_atomic_and_snapshots_are_cached() {
    let mut h = host(&[("a", ".a {}")]);
    let first = h.snapshot();
    let pointer = first.symbols().as_ptr();
    assert_eq!(pointer, h.snapshot().symbols().as_ptr());
    assert_eq!(
        h.set_file(id("a"), 0, "broken"),
        Err(UpdateError::StaleRevision)
    );
    assert_eq!(h.set_file(id(""), 1, ""), Err(UpdateError::InvalidModuleId));
    assert_eq!(pointer, h.snapshot().symbols().as_ptr());
    assert_eq!(first.syntax(&id("a")).unwrap().source.text(), ".a {}");
}
