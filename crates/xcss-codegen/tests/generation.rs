use xcss_analysis::AnalysisHost;
use xcss_codegen::{GenerateOptions, GeneratedProject, generate};
use xcss_source::FileId;
fn id(name: &str) -> FileId {
    FileId(name.into())
}
fn compile(
    files: &[(&str, &str)],
    entry: &str,
    resolutions: &[(&str, &str, &str)],
) -> Result<GeneratedProject, Vec<xcss_source::Diagnostic>> {
    let mut host = AnalysisHost::new();
    for (file, text) in files {
        host.set_file(id(file), 1, *text).unwrap();
    }
    for (file, specifier, target) in resolutions {
        host.set_resolution(id(file), *specifier, Some(id(target)));
    }
    generate(&host.snapshot(), &id(entry), &GenerateOptions::default())
}
#[test]
fn scoped_css_and_existing_value_extensions() {
    let result=compile(&[("a.xcss",".button { color: --accent; padding: --spacing * 2; margin: (10px + 2px) 20px; transition: (display, opacity) .5s ease; &:hover { width: calc(100% - --gap); } }")],"a.xcss",&[]).unwrap();
    let module = &result.modules[0];
    let css = &module.css;
    assert!(module.classes.contains_key("button"));
    assert!(css.contains(&module.classes["button"]));
    for text in [
        "var(--accent)",
        "calc(var(--spacing) * 2)",
        "12px 20px",
        "allow-discrete",
        "&:hover",
    ] {
        assert!(css.contains(text), "Missing {text}: {css}");
    }
    assert!(!css.contains("(display, opacity)"));
}
#[test]
fn registrations_inference_defaults_and_wildcards() {
    let result=compile(&[("a.xcss","@vars { --n = 2px; --color: color; local --anything; --flex: flex; } .a { --n: 3px; color: --color; }")],"a.xcss",&[]).unwrap();
    let module = &result.modules[0];
    assert_eq!(module.custom_properties.len(), 4);
    for text in [
        "<length>",
        "#0000",
        "inherits: false",
        "syntax: \"*\"",
        "0fr",
    ] {
        assert!(module.css.contains(text), "{text}: {}", module.css);
    }
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "generate.registration-wildcard")
    );
}
#[test]
fn injection_substitution_content_scope_and_assets() {
    let result=compile(&[("lib/mixins.xcss","@vars { --ink: color = red; } @injection outline($size: length = 1px) { border-width: $size; color: --ink; .icon { background: url('./icon.svg'); @content; @content; } } .unrelated { color: blue; }"),("src/a.xcss",".a { @:outline(2px) { width: --gap; } }")],"src/a.xcss",&[]).unwrap();
    let module = &result.modules[0];
    assert!(module.classes.contains_key("icon"));
    assert!(!module.css.contains("unrelated"));
    assert!(module.css.contains("2px"));
    assert!(module.css.contains("../lib/icon.svg"));
    assert_eq!(module.css.matches("var(--gap)").count(), 2);
    assert!(!module.css.contains("@injection"));
    assert!(!module.css.contains("$size"));
    let map: serde_json::Value = serde_json::from_str(&module.source_map).unwrap();
    assert!(
        map["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().contains("mixins.xcss"))
    );
}
#[test]
fn composition_exports_and_dependencies() {
    let result = compile(
        &[
            ("a.xcss", ".a { composes: b from \"./b.xcss\"; }"),
            ("b.xcss", ".b { composes: global-name from global; }"),
        ],
        "a.xcss",
        &[("a.xcss", "./b.xcss", "b.xcss")],
    )
    .unwrap();
    assert_eq!(result.modules.len(), 2);
    let a = result
        .modules
        .iter()
        .find(|m| m.module_id == id("a.xcss"))
        .unwrap();
    let b = result
        .modules
        .iter()
        .find(|m| m.module_id == id("b.xcss"))
        .unwrap();
    assert!(a.classes["a"].contains(&b.classes["b"]));
    assert!(!a.css.contains("composes"));
    assert_eq!(a.stylesheet_dependencies, vec![id("b.xcss")]);
}
#[test]
fn keyframes_and_global_switches() {
    let result=compile(&[("a.xcss","@keyframes spin { from { opacity: 0; } to { opacity: 1; } } .a :global .external { animation: spin 1s; } :global(.outside) { color: red; }")],"a.xcss",&[]).unwrap();
    let module = &result.modules[0];
    assert!(module.classes.contains_key("a"));
    assert!(!module.classes.contains_key("external"));
    assert!(module.css.contains(".external"));
    assert!(!module.css.contains(":global"));
    assert!(!module.css.contains("@keyframes spin"));
}
#[test]
fn pseudo_content_after_expansion() {
    let result=compile(&[("a.xcss","@injection content { content: 'provided'; } .a::before { @:content; } .b::after { width: 1px; } .c::before { content: preserve; }")],"a.xcss",&[]).unwrap();
    let css = &result.modules[0].css;
    assert!(css.contains("provided"));
    assert!(!css.contains("preserve"));
    assert_eq!(css.matches("content:").count(), 2);
}
#[test]
fn failures_and_unrelated_errors() {
    assert!(
        compile(
            &[("a.xcss", "@vars { --x: length = 1em; } .a {}")],
            "a.xcss",
            &[]
        )
        .unwrap_err()
        .iter()
        .any(|d| matches!(
            d.code,
            "generate.property-initial" | "type.property-initial"
        ))
    );
    assert!(
        compile(
            &[
                ("a.xcss", ".a {}"),
                ("broken.xcss", ".broken { @:missing; }")
            ],
            "a.xcss",
            &[]
        )
        .is_ok()
    );
    assert!(
        compile(
            &[("a.xcss", "@injection a { @:a; } .a { @:a; }")],
            "a.xcss",
            &[]
        )
        .is_err()
    );
}
#[test]
fn stable_names_and_defaults() {
    let a = compile(
        &[(
            "a.xcss",
            "@injection f($x: length = 2px) { width: $x; } .a { @:f; }",
        )],
        "a.xcss",
        &[],
    )
    .unwrap();
    let b = compile(
        &[("a.xcss", "/* edit */ .a { width: 3px; }")],
        "a.xcss",
        &[],
    )
    .unwrap();
    assert_eq!(a.modules[0].classes, b.modules[0].classes);
    assert!(a.modules[0].css.contains("2px"));
}

#[test]
fn maps_injection_declarations_arguments_and_content_to_their_origins() {
    let library = "@injection f($size: length) { width: $size; @content; }";
    let caller = ".a { @:f(9px) { height: 7px; } }";
    let result = compile(&[("lib.xcss", library), ("a.xcss", caller)], "a.xcss", &[]).unwrap();
    let module = &result.modules[0];
    let mut map = parcel_sourcemap::SourceMap::from_json("/", &module.source_map).unwrap();
    let generated = xcss_source::SourceFile::new(id("css"), 0, module.css.clone());
    for (text, file, original) in [
        ("width", "lib.xcss", library.find("width").unwrap()),
        ("9px", "a.xcss", caller.find("9px").unwrap()),
        ("height", "a.xcss", caller.find("height").unwrap()),
    ] {
        let position = generated
            .byte_to_utf16(module.css.find(text).unwrap())
            .unwrap();
        let mapping = map
            .find_closest_mapping(position.line as u32, position.character as u32)
            .unwrap()
            .original
            .unwrap();
        assert_eq!(map.get_source(mapping.source).unwrap(), file);
        assert_eq!(mapping.original_column as usize, original, "{text}");
    }
}

#[test]
fn supports_global_keyframes_without_scoping_their_references() {
    let result=compile(&[("a.xcss","@keyframes :global(spin) { from { opacity: 0; } to { opacity: 1; } } .a { animation: spin 1s; }")],"a.xcss",&[]).unwrap();
    let css = &result.modules[0].css;
    assert!(css.contains("@keyframes spin"), "{css}");
    assert!(css.contains("1s spin"), "{css}");
}

#[test]
fn defaults_cover_all_supported_types_and_invalid_values_fail() {
    for kind in [
        "number",
        "integer",
        "percentage",
        "length",
        "length-percentage",
        "angle",
        "time",
        "resolution",
        "color",
        "image",
        "url",
        "transform-function",
        "transform-list",
        "frequency",
        "flex",
    ] {
        let source = format!("@vars {{ local --x: {kind}; }} .a {{ width: --x; }}");
        assert!(
            compile(&[("a.xcss", &source)], "a.xcss", &[]).is_ok(),
            "{kind}"
        );
    }
    for source in [
        "@vars { --x = var(--external); }",
        "@vars { --x: length = var(--external); }",
        "@vars { --x: color = currentColor; }",
        "@vars { --x: length = 1rem; }",
    ] {
        assert!(
            compile(&[("a.xcss", source)], "a.xcss", &[]).is_err(),
            "{source}"
        );
    }
}

#[test]
fn expansion_depth_is_bounded() {
    let mut source = String::new();
    for index in 0..130 {
        source.push_str(&format!("@injection f{index} {{ @:f{}; }}\n", index + 1));
    }
    source.push_str("@injection f130 { color: red; } .a { @:f0; }");
    let errors = compile(&[("a.xcss", &source)], "a.xcss", &[]).unwrap_err();
    assert!(errors.iter().any(|d| d.code == "generate.expansion-limit"));
}

#[test]
fn arithmetic_respects_slash_separators_and_content_directive_comments() {
    let result=compile(&[("a.xcss",".a { background-size: --size * 2; border-radius: --radius + 2px; font: 12px / 1.5 serif; color: rgb(1 2 3 / .5); width: min(100% - --gap, --size * 2); } .a::before { content: preserve; /* supplied elsewhere */ }")],"a.xcss",&[]).unwrap();
    let css = &result.modules[0].css;
    assert!(css.contains("calc(var(--size) * 2)"));
    assert!(css.contains("calc(var(--radius) + 2px)"));
    assert!(!css.contains("content:"));
    assert!(!css.contains("calc(12px"));
    let comments = compile(
        &[(
            "a.xcss",
            ".a::before { content: preserve /* supplied elsewhere */; }",
        )],
        "a.xcss",
        &[],
    )
    .unwrap();
    assert!(!comments.modules[0].css.contains("content:"));
}

#[test]
fn annotated_independent_math_initial_values_are_valid() {
    assert!(
        compile(
            &[(
                "a.xcss",
                "@vars { --size: length = calc(1px + 2px); } .a { width: --size; }"
            )],
            "a.xcss",
            &[]
        )
        .is_ok()
    );
}

#[test]
fn composition_bundle_hoists_native_imports_and_rebases_their_paths() {
    let mut host = AnalysisHost::new();
    host.set_file(
        id("src/a.xcss"),
        1,
        "@import './base.css'; .a { composes: b from './lib/b.xcss'; }",
    )
    .unwrap();
    host.set_file(
        id("src/lib/b.xcss"),
        1,
        "@import './lib.css'; .b { background: url('./icon.svg'); }",
    )
    .unwrap();
    host.set_resolution(id("src/a.xcss"), "./lib/b.xcss", Some(id("src/lib/b.xcss")));
    let result = generate(
        &host.snapshot(),
        &id("src/a.xcss"),
        &GenerateOptions {
            bundle: true,
            ..GenerateOptions::default()
        },
    )
    .unwrap();
    let module = &result.modules[0];
    let parsed = xcss_syntax::parse(std::sync::Arc::new(xcss_source::SourceFile::new(
        id("css"),
        0,
        module.css.clone(),
    )));
    assert_eq!(parsed.tree.children[0].kind, xcss_syntax::NodeKind::Import);
    assert_eq!(parsed.tree.children[1].kind, xcss_syntax::NodeKind::Import);
    assert!(module.css.contains("./lib/lib.css"));
    assert!(module.css.contains("./lib/icon.svg"));
    assert_eq!(module.native_imports.len(), 2);
}

#[test]
fn statement_expansion_is_bounded_and_never_returns_partial_artifacts() {
    let mut source = String::new();
    for index in 0..17 {
        source.push_str(&format!(
            "@injection f{index} {{ @:f{}; @:f{}; }}\n",
            index + 1,
            index + 1
        ));
    }
    source.push_str("@injection f17 { color: red; } .a { @:f0; }");
    assert!(
        compile(&[("a.xcss", &source)], "a.xcss", &[])
            .unwrap_err()
            .iter()
            .any(|d| d.code == "generate.expansion-limit")
    );
}

#[test]
fn golden_css_and_exports() {
    let result = compile(
        &[("basic.xcss", include_str!("fixtures/basic.xcss"))],
        "basic.xcss",
        &[],
    )
    .unwrap();
    assert_eq!(
        result.modules[0].css,
        include_str!("fixtures/basic.css").replace("\r\n", "\n")
    );
    let expected: std::collections::BTreeMap<String, String> =
        serde_json::from_str(include_str!("fixtures/basic.exports.json")).unwrap();
    assert_eq!(result.modules[0].classes, expected);
    assert!(
        result.modules[0]
            .css
            .contains("/*! XCSS fixture license */")
    );
}

#[test]
fn minified_output_keeps_exports_and_original_maps() {
    let mut host = AnalysisHost::new();
    host.set_file(id("a.xcss"),1,"@keyframes :global(spin) { from { opacity: 0; } } .a { animation: spin 1s; width: --size * 2; }").unwrap();
    let snapshot = host.snapshot();
    let readable = generate(&snapshot, &id("a.xcss"), &GenerateOptions::default()).unwrap();
    let minified = generate(
        &snapshot,
        &id("a.xcss"),
        &GenerateOptions {
            minify: true,
            ..GenerateOptions::default()
        },
    )
    .unwrap();
    assert_eq!(readable.modules[0].classes, minified.modules[0].classes);
    assert!(!minified.modules[0].css.contains("__xcss_global"));
    let map: serde_json::Value = serde_json::from_str(&minified.modules[0].source_map).unwrap();
    assert!(!map["mappings"].as_str().unwrap().is_empty());
}
