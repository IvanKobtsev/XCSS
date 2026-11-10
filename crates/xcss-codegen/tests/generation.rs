use xcss_analysis::AnalysisHost;
use xcss_codegen::{GenerateOptions, GeneratedProject, generate};
use xcss_source::FileId;
fn id(name: &str) -> FileId {
    FileId(name.into())
}

#[test]
fn provided_variable_exports_match_scoped_css() {
    let result = compile(
        &[(
            "a.xcss",
            "
        @vars {
            @provided --provided-width = 0px;
            @provided { local --other-provided-var: angle; }
            --private = 1px;
        }
        .a { width: --provided-width; rotate: --other-provided-var; margin: --private; }
    ",
        )],
        "a.xcss",
        &[],
    )
    .unwrap();
    let module = &result.modules[0];
    assert_eq!(module.css_vars.len(), 2);
    let width = &module.css_vars["providedWidth"];
    let angle = &module.css_vars["otherProvidedVar"];
    assert_eq!(width, &module.custom_properties["--provided-width"]);
    assert_eq!(angle, &module.custom_properties["--other-provided-var"]);
    for property in [width, angle] {
        assert!(module.css.contains(&format!("@property {property}")));
        assert!(module.css.contains(&format!("var({property})")));
    }
    assert!(module.css.contains("inherits: false"));
    assert!(!module.css.contains("@provided"));
    assert!(!module.css_vars.contains_key("private"));
}
#[test]
fn variants_are_erased_and_static_exports_match_generation() {
    let mut host = AnalysisHost::new();
    host.set_file(id("a.xcss"), 1, "@variants Button: .primary; @injection used() { &.primary { color: red; } @content; } @injection ignored() { @content; } .button { @:used() { &.secondary { color: blue; } } @:ignored() { &.third { color: green; } } } :global(.outside) {}" ).unwrap();
    let snapshot = host.snapshot();
    let result = generate(&snapshot, &id("a.xcss"), &GenerateOptions::default()).unwrap();
    let module = &result.modules[0];
    assert!(!module.css.contains("@variants"));
    assert_eq!(
        module
            .classes
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        snapshot.module_interface(&id("a.xcss")).unwrap().classes
    );
}

#[test]
fn repeated_injection_in_caller_content_keeps_its_class_exports() {
    let mut host = AnalysisHost::new();
    host.set_file(id("a.xcss"), 1, "@variants V: .inner; @injection wrap() { @content; } .outer { @:wrap() { @:wrap() { &.inner { color: red; } } } }").unwrap();
    let snapshot = host.snapshot();
    assert!(
        snapshot.diagnostics().is_empty(),
        "{:?}",
        snapshot.diagnostics()
    );
    let result = generate(&snapshot, &id("a.xcss"), &GenerateOptions::default()).unwrap();
    assert!(result.modules[0].classes.contains_key("inner"));
}
#[test]
fn raw_css_bypasses_all_transformations_and_bundling_import_hoisting() {
    let body = "\r\n/* ordinary 😀 */ .untouched#id::before { x: $missing + --unknown; content: '}' }\r\n@import './missing.xcss'; @raw { @include literal; }\r\n";
    let mut host = AnalysisHost::new();
    host.set_file(
        id("a.xcss"),
        1,
        format!(".before {{ color: red; }} @raw {{{body}}} .after {{ color: blue; }}"),
    )
    .unwrap();
    let snapshot = host.snapshot();
    assert!(
        snapshot.diagnostics().is_empty(),
        "{:?}",
        snapshot.diagnostics()
    );
    assert!(snapshot.dependencies().is_empty());
    for minify in [false, true] {
        for bundle in [false, true] {
            let result = generate(
                &snapshot,
                &id("a.xcss"),
                &GenerateOptions {
                    minify,
                    bundle,
                    ..Default::default()
                },
            )
            .unwrap();
            let module = &result.modules[0];
            assert!(module.css.contains(body), "{}", module.css);
            assert_eq!(module.classes.len(), 2);
            assert!(module.native_imports.is_empty());
            assert!(!module.css.contains("--xcss-raw-"));
            assert!(
                module.css.find(&module.classes["before"]).unwrap()
                    < module.css.find(body).unwrap()
            );
            assert!(
                module.css.find(body).unwrap() < module.css.find(&module.classes["after"]).unwrap()
            );
        }
    }
}
#[test]
fn raw_expands_in_injections_and_shared_utilities() {
    let body = " /* untouched */ color: $n; --name: --literal; ";
    let mut host = AnalysisHost::new();
    host.set_file(id("a.xcss"),1,format!("@injection i($n: number) {{ @raw {{{body}}} }} @util u {{ @raw {{{body}}} }} .a {{ @:u; @:i(1); }} .b {{ @:i(2); }}")).unwrap();
    let snapshot = host.snapshot();
    for minify in [false, true] {
        for bundle in [false, true] {
            let result = generate(
                &snapshot,
                &id("a.xcss"),
                &GenerateOptions {
                    minify,
                    bundle,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                result.modules[0].css.matches(body).count(),
                if bundle { 3 } else { 2 }
            );
            if let Some(shared) = result.utilities {
                assert!(shared.css.contains(body));
                let merged = xcss_codegen::merge_utilities(vec![shared.clone(), shared])
                    .unwrap()
                    .unwrap();
                assert_eq!(merged.css.matches(body).count(), 1);
            }
        }
    }
}
#[test]
fn raw_source_maps_preserve_unicode_and_following_declarations() {
    let library = "@injection i { @raw {\r\n/* 😀 */ color: future(é);\r\n} }";
    let caller = "@raw {} .a { @:i; width: 123px; } .b { @:i; height: 234px; }";
    let mut host = AnalysisHost::new();
    host.set_file(id("lib.xcss"), 1, library).unwrap();
    host.set_file(id("a.xcss"), 1, caller).unwrap();
    let snapshot = host.snapshot();
    for minify in [false, true] {
        for bundle in [false, true] {
            let result = generate(
                &snapshot,
                &id("a.xcss"),
                &GenerateOptions {
                    minify,
                    bundle,
                    ..Default::default()
                },
            )
            .unwrap();
            let module = &result.modules[0];
            let mut map = parcel_sourcemap::SourceMap::from_json("/", &module.source_map).unwrap();
            let generated = xcss_source::SourceFile::new(id("css"), 0, module.css.clone());
            for (text, file, source) in [
                ("future", "lib.xcss", library),
                ("width", "a.xcss", caller),
                ("height", "a.xcss", caller),
            ] {
                for (offset, _) in module.css.match_indices(text) {
                    let position = generated.byte_to_utf16(offset).unwrap();
                    let mapping = map
                        .find_closest_mapping(position.line as u32, position.character as u32)
                        .unwrap()
                        .original
                        .unwrap();
                    let expected = xcss_source::SourceFile::new(id(file), 0, source)
                        .byte_to_utf16(source.find(text).unwrap())
                        .unwrap();
                    assert_eq!(map.get_source(mapping.source).unwrap(), file);
                    assert_eq!(
                        (mapping.original_line, mapping.original_column),
                        (expected.line as u32, expected.character as u32),
                        "{text}"
                    );
                }
            }
        }
    }
}
#[test]
fn raw_nested_conditions_keep_literal_names_urls_and_comments() {
    let body = "\n/*! once */\ncontent: preserve; color: $constant; width: --size; background: url('../original.svg');\n";
    let mut host = AnalysisHost::new();
    host.set_file(id("src/a.xcss"),1,format!("$constant = red; @vars {{ --size: length = 1px; }} @media (width > 1px) {{ .a::before {{ @supports (display: grid) {{ @raw {{{body}}} }} }} }}")).unwrap();
    let result = generate(
        &host.snapshot(),
        &id("src/a.xcss"),
        &GenerateOptions {
            minify: true,
            output_paths: std::collections::BTreeMap::from([(
                id("src/a.xcss"),
                "dist/out.css".into(),
            )]),
            ..Default::default()
        },
    )
    .unwrap();
    let css = &result.modules[0].css;
    assert!(css.contains(body), "{css}");
    assert_eq!(css.matches("/*! once */").count(), 1);
    assert!(css.contains("@media") && css.contains("@supports"));
}
#[test]
fn raw_blocks_in_vars_are_emitted_and_placeholder_collisions_are_avoided() {
    // This literal unknown at-rule has the exact name initially chosen for the
    // raw block at bytes 0..17. It must not be mistaken for the private marker.
    let source = "@raw {color: red;} @--xcss-raw-612e78637373-6-17; @vars { @raw { @property --literal { syntax: '*'; inherits: false; } } }";
    let project = compile(&[("a.xcss", source)], "a.xcss", &[]).unwrap();
    let css = &project.modules[0].css;
    assert!(css.contains("color: red;"));
    assert!(css.contains("@--xcss-raw-612e78637373-6-17;"), "{css}");
    assert!(
        css.contains(" @property --literal { syntax: '*'; inherits: false; } "),
        "{css}"
    );
}
#[test]
fn utilities_compose_defaults_and_keep_injected_rules_local() {
    let result = compile(&[("a.xcss", "@util truncate($lines: integer = 3) { display: -webkit-box; max-height: $lines * 1lh; } .a, :local(.b) { @:truncate(2); color: white; } .c { @:truncate; } .d { @:truncate(3); } span[data-truncate] { @:truncate(3); color: black; } .forced:hover { @:truncate(4) as injection; }")], "a.xcss", &[]).unwrap();
    let module = &result.modules[0];
    let shared = result.utilities.as_ref().unwrap();
    assert_eq!(shared.rules.len(), 2, "{}", shared.css);
    assert!(shared.css.contains("._util_truncate_2"), "{}", shared.css);
    assert!(shared.css.contains("@layer utils"));
    assert!(module.classes["a"].contains("_util_truncate_2"));
    assert!(module.classes["b"].contains("_util_truncate_2"));
    assert!(module.classes["c"].contains("_util_truncate_3"));
    assert!(!module.classes.keys().any(|key| key.starts_with("_util_")));
    assert!(!module.classes["forced"].contains("_util_"));
    assert!(module.css.contains("_util_inj_"), "{}", module.css);
    assert!(module.css.contains("span[data-truncate]"));
    assert!(module.css.contains("4lh"), "{}", module.css);
}
#[test]
fn utilities_share_across_modules_and_preserve_conditions_assets_and_maps() {
    let mut host = AnalysisHost::new();
    for (file, source) in [
        (
            "lib/util.xcss",
            "$default = 3; @util trim($n: integer = $default) { max-height: $n * 1lh; background: url('./icon.svg'); &:hover { color: red; } :global(.icon) { display: block; } }",
        ),
        (
            "src/a.xcss",
            ".a { composes: b from './b.xcss'; @:trim(3); } @media (width > 10px) { @layer components { .c { @:trim; } } } .parent { &:hover { @:trim(4); } }",
        ),
        ("src/b.xcss", ".b { @:trim; }"),
    ] {
        host.set_file(id(file), 1, source).unwrap();
    }
    host.set_resolution(id("src/a.xcss"), "./b.xcss", Some(id("src/b.xcss")));
    let snapshot = host.snapshot();
    let options = GenerateOptions {
        utilities_output_path: Some("dist/shared/utils.css".into()),
        ..Default::default()
    };
    let project = generate(&snapshot, &id("src/a.xcss"), &options).unwrap();
    let shared = project.utilities.as_ref().unwrap();
    assert_eq!(shared.rules.len(), 2, "{}", shared.css);
    assert!(shared.css.contains("../../lib/icon.svg"), "{}", shared.css);
    assert!(shared.css.contains("@media"));
    assert!(!shared.css.contains("components"));
    assert!(shared.css.contains(".icon"));
    assert!(shared.source_map.contains("lib/util.xcss"));
    let caller = project
        .modules
        .iter()
        .find(|m| m.module_id == id("src/a.xcss"))
        .unwrap();
    assert!(caller.css.contains(":hover"), "{}", caller.css);
    assert!(caller.css.contains("_util_inj_"));
    assert_eq!(caller.classes["a"].matches("_util_trim_3").count(), 1);
    assert!(!caller.classes["parent"].contains("_util_"));
    let bundled = generate(
        &snapshot,
        &id("src/a.xcss"),
        &GenerateOptions {
            bundle: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(bundled.utilities.is_none());
    assert_eq!(bundled.modules[0].css.matches("._util_trim_3 {").count(), 2); // unconditional + media
}

#[test]
fn utility_selectors_and_nested_calls_choose_the_right_emission_mode() {
    let result = compile(&[("a.xcss", "@injection plain { border-width: 1px; } @util inner { @:plain; color: red; } @util outer { @:inner; &:hover { opacity: .5; } } .a.b, .c:hover { @:outer; } .parent .descendant { @:outer; } :global(.g), .mixed { @:outer; } .composed { @:outer; color: blue; @:outer; }")], "a.xcss", &[]).unwrap();
    let shared = result.utilities.unwrap();
    assert_eq!(shared.rules.len(), 1);
    assert!(shared.css.contains("_util_outer"));
    assert!(!shared.css.contains("_util_inner"));
    let m = &result.modules[0];
    assert!(m.classes["composed"].contains("_util_outer"));
    for name in ["a", "b", "c", "parent", "descendant", "mixed"] {
        assert!(!m.classes[name].contains("_util_"));
    }
    assert_eq!(m.css.matches("_util_inj_").count(), 3);
}

#[test]
fn imported_utilities_names_and_rule_order_are_stable() {
    let files = [
        (
            "left.xcss",
            "@util same($n: integer = 2) { width: $n * 1px; }",
        ),
        (
            "right.xcss",
            "@util same($n: integer = 2) { height: $n * 1px; }",
        ),
        (
            "a.xcss",
            "@import l from './left.xcss'; @import r from './right.xcss'; .a { @:r.same; @:l.same; }",
        ),
    ];
    let resolutions = [
        ("a.xcss", "./left.xcss", "left.xcss"),
        ("a.xcss", "./right.xcss", "right.xcss"),
    ];
    let result = compile(&files, "a.xcss", &resolutions).unwrap();
    let shared = result.utilities.unwrap();
    assert_eq!(shared.rules.len(), 2);
    assert_eq!(result.modules[0].classes["a"].split_whitespace().count(), 3);
    assert!(shared.css.find("height:").unwrap() < shared.css.find("width:").unwrap());
    let merged = xcss_codegen::merge_utilities(vec![shared.clone(), shared.clone()])
        .unwrap()
        .unwrap();
    assert_eq!(merged.rules.len(), 2);
    assert!(merged.css.find("height:").unwrap() < merged.css.find("width:").unwrap());
    let minified = {
        let mut host = AnalysisHost::new();
        host.set_file(
            id("a.xcss"),
            1,
            "@util u($n: integer = 2) { width: $n * 1px; } .a { @:u; }",
        )
        .unwrap();
        generate(
            &host.snapshot(),
            &id("a.xcss"),
            &GenerateOptions {
                minify: true,
                ..Default::default()
            },
        )
        .unwrap()
    };
    assert!(minified.utilities.unwrap().css.contains("@layer utils"));
}

#[test]
fn utility_layer_preludes_global_keyframes_and_bundle_assets_keep_their_meaning() {
    let mut host = AnalysisHost::new();
    host.set_file(id("lib/util.xcss"), 1, "@keyframes :global(spin) { to { opacity: 1; } } @util u { animation: spin 1s; background: url('./icon.svg'); }").unwrap();
    host.set_file(id("src/a.xcss"), 1, "@layer components, utils; .a { @:u; }")
        .unwrap();
    let snapshot = host.snapshot();
    let project = generate(&snapshot, &id("src/a.xcss"), &GenerateOptions::default()).unwrap();
    let shared = project.utilities.unwrap();
    assert!(
        shared.css.starts_with("@layer components, utils;"),
        "{}",
        shared.css
    );
    assert!(shared.css.contains("animation: 1s spin"), "{}", shared.css);
    assert!(!shared.css.contains("__xcss_global_"));
    let bundled = generate(
        &snapshot,
        &id("src/a.xcss"),
        &GenerateOptions {
            bundle: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        bundled.modules[0].css.contains("../lib/icon.svg"),
        "{}",
        bundled.modules[0].css
    );
}

#[test]
fn invalid_unrelated_utils_do_not_poison_ordinary_injection_consumers() {
    let project = compile(
        &[
            (
                "injection.xcss",
                "@injection ordinary { .child { color: red; } }",
            ),
            ("invalid-util.xcss", "@util invalid { @:ordinary; }"),
            ("a.xcss", ".a { @:ordinary; }"),
        ],
        "a.xcss",
        &[],
    )
    .unwrap();
    assert!(project.modules[0].classes.contains_key("child"));
    assert!(project.utilities.is_none());
}
#[test]
fn constant_values_gradients_math_and_conditions() {
    let result = compile(&[("a.xcss", "$base = 56px; $height: length = $base + --my-height / 2; $gradient = linear-gradient(to right, #000 --start-value, #fff (--end-value + --offset)); @media (width > $base) { .a { height: $height; background: $gradient; width: $height * 2; } }")], "a.xcss", &[]).unwrap();
    let css = &result.modules[0].css;
    for expected in [
        "56px",
        "var(--my-height)",
        "var(--start-value)",
        "var(--end-value)",
        "var(--offset)",
        "linear-gradient",
        "@media",
    ] {
        assert!(css.contains(expected), "Missing {expected}: {css}");
    }
    assert!(!css.contains('$'), "{css}");
    assert!(!css.contains("@property"), "{css}");
    assert!(result.modules[0].source_map.contains("a.xcss"));
}

#[test]
fn constants_capture_definition_values_and_expand_per_call() {
    let result = compile(&[
        ("lib/paint.xcss", "$asset = url('./icon.svg'); $default = 3px; @injection paint($size: length = $default) { $double = $size * 2; background-image: $asset; width: $double; @content; }"),
        ("a.xcss", "@import { paint } from './lib/paint.xcss'; $caller = 7px; .a { @:paint(5px) { height: $caller; } } .b { @:paint(); }")
    ], "a.xcss", &[("a.xcss", "./lib/paint.xcss", "lib/paint.xcss")]).unwrap();
    let css = &result.modules[0].css;
    for expected in ["10px", "6px", "7px", "lib/icon.svg"] {
        assert!(css.contains(expected), "Missing {expected}: {css}");
    }
    assert!(!css.contains('$'));
}
#[test]
fn constant_substitution_preserves_precedence_and_conditions() {
    let result = compile(&[("a.xcss", "$sum: length = 10px + --offset; $min = 56px; @supports (width: $min) { @container layout (width > $min) { .a { width: $sum * 2; } } }")], "a.xcss", &[]).unwrap();
    let css = &result.modules[0].css;
    assert!(css.contains("(10px + var(--offset)) * 2"), "{css}");
    assert!(css.contains("@supports (width: 56px)"), "{css}");
    assert!(css.contains("_layout (width > 56px)"), "{css}");
}

#[test]
fn constant_expansion_depth_is_bounded() {
    let mut source = "$c0 = 1px;".to_string();
    for i in 1..140 {
        source.push_str(&format!("$c{i} = $c{};", i - 1));
    }
    source.push_str(".a { width: $c139; }");
    let errors = compile(&[("a.xcss", &source)], "a.xcss", &[]).unwrap_err();
    assert!(errors.iter().any(|e| e.code == "generate.expansion-limit"));
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

#[test]
fn zero_registration_and_silent_line_comments() {
    let mut host = AnalysisHost::new();
    host.set_file(id("a.xcss"), 1, "// leading\n@vars { --gap = 0 // zero 😀\n; } @injection f($n: integer = 0) { z-index: $n; } .a // selector\n{ @:f(0 // argument\n); margin: --gap; color: // color\nred; width: calc(1px + // inside\n2px); background: url(//cdn.example.com/a); content: \"//literal\"; }").unwrap();
    let snapshot = host.snapshot();
    assert!(
        snapshot.diagnostics().is_empty(),
        "{:?}",
        snapshot.diagnostics()
    );
    for minify in [false, true] {
        let result = generate(
            &snapshot,
            &id("a.xcss"),
            &GenerateOptions {
                minify,
                ..Default::default()
            },
        )
        .unwrap();
        let css = &result.modules[0].css;
        assert!(css.contains("<length>"), "{css}");
        assert!(css.contains("//literal"), "{css}");
        assert!(css.contains("//cdn.example.com/a"), "{css}");
        for comment in [
            "// leading",
            "// zero",
            "// selector",
            "// argument",
            "// color",
            "// inside",
        ] {
            assert!(!css.contains(comment), "{css}");
        }
    }
}
