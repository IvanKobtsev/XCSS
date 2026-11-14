use xcss_format::{EndOfLine, FormatOptions, format};

fn formatted(source: &str, options: &FormatOptions) -> String {
    let result = format(source, options);
    assert!(
        result.diagnostics.is_empty(),
        "{source}\n{:?}",
        result.diagnostics
    );
    let text = result.text.expect("formatted source");
    assert_eq!(
        Some(&text),
        format(&text, options).text.as_ref(),
        "not idempotent: {source}"
    );
    text
}
#[test]
fn css_layout() {
    assert_eq!(
        formatted(
            ".a,.b{color:red;margin:.5px 0;content:'hi'}",
            &FormatOptions::default()
        ),
        ".a,\n.b {\n  color: red;\n  margin: 0.5px 0;\n  content: \"hi\";\n}\n"
    );
}
#[test]
fn variant_groups() {
    assert_eq!(
        formatted(
            "@variants Button:.primary,.secondary;",
            &FormatOptions::default()
        ),
        "@variants Button: .primary, .secondary;\n"
    );
    let comments = formatted(
        "@variants /* group */ Button: .primary, /* member */ .secondary;",
        &FormatOptions::default(),
    );
    assert!(comments.contains("/* group */"));
    assert!(comments.contains("/* member */"));
}
#[test]
fn provided_variable_forms_preserve_comments() {
    let output = formatted(
        "@vars{@provided /* inline */ --gap=1px; @provided{/* group */ local --turn:angle=0deg;}}",
        &FormatOptions::default(),
    );
    assert!(
        output.contains("@provided /* inline */ --gap = 1px;"),
        "{output}"
    );
    assert!(output.contains("@provided {"), "{output}");
    assert!(output.contains("/* group */"), "{output}");
    assert!(output.contains("local --turn: angle = 0deg;"), "{output}");
}

#[test]
fn xcss_constructs() {
    let input = "@vars{--size:length=1px;local --color:color;} $gap:length=2px; @injection paint($color:color=red,$width:length=1px){color:$color;width:$width;} @util spaced($n:length=1px){margin:$n;} .a{@:paint(red,2px);width:--size * 2;@content;}";
    let text = formatted(input, &FormatOptions::default());
    assert!(text.contains("--size: length = 1px;"), "{text}");
    assert!(text.contains("$gap: length = 2px;"), "{text}");
    assert!(text.contains("@:paint(red, 2px);"), "{text}");
    assert!(text.contains("width: --size * 2;"), "{text}");
}
#[test]
fn fixture_language() {
    formatted(
        include_str!("fixtures/language.xcss"),
        &FormatOptions::default(),
    );
}
#[test]
fn raw_and_ignore_are_verbatim() {
    let raw = "{\r\n .raw{  --x:  1;}\r\n}";
    let input = format!(".a{{@raw {raw}}}\n/* prettier-ignore */\n.b   {{color:   red;}} ");
    let output = formatted(&input, &FormatOptions::default());
    assert!(output.contains(raw), "{output}");
    assert!(output.contains(".b   {color:   red;}"));
}
#[test]
fn comments_and_blank_lines() {
    let source = "/* 😀 lead */\n.a { /* before */ color:red; /* after */\n\n/* multiline\n exact */ width:1px; }\n/* end */";
    let output = formatted(source, &FormatOptions::default());
    assert!(output.contains("/* multiline\n exact */"));
    assert!(output.contains("/* after */\n\n"), "{output}");
}
#[test]
fn invalid_syntax_is_not_repaired() {
    for source in [".a{", ".a{color:\"oops}", "/*oops", ".a{@:paint("] {
        let result = format(source, &FormatOptions::default());
        assert!(result.text.is_none(), "{source}");
        assert!(!result.diagnostics.is_empty());
    }
}
#[test]
fn semantic_errors_are_allowed() {
    formatted(".a{color:$missing; @:missing;}", &FormatOptions::default());
}
#[test]
fn tabs_crlf_and_width() {
    let options = FormatOptions {
        use_tabs: true,
        end_of_line: EndOfLine::Crlf,
        print_width: 30,
        ..Default::default()
    };
    let text = formatted(
        "@injection paint($first:color=red,$second:length=1px){color:$first;}",
        &options,
    );
    assert!(text.contains("\r\n\t$first: color = red,"), "{text:?}");
    assert!(!text.replace("\r\n", "").contains('\n'));
}
#[test]
fn standard_edge_cases() {
    for input in [
        "@keyframes spin{0%,100%{opacity:0}50%{opacity:1}}",
        ".café\\:x[data-a='hello']:not(.b){background:url(https://example.com/a.png);content:'😀'}",
        ".a{--tokens: 10px  20px; width:calc(100% - 1px); &:hover{color:red}}",
        "@media (width > 10px){.a{display:flex}}",
        "@future strange( a  b ) { .opaque{ x: y; } }",
        "@import { paint } from './lib'; .a { @:paint { @content; } }",
    ] {
        formatted(input, &FormatOptions::default());
    }
}
#[test]
fn empty_files() {
    assert_eq!(formatted("", &FormatOptions::default()), "");
    assert_eq!(formatted(" \n\t", &FormatOptions::default()), "");
    assert_eq!(
        formatted(".a{}\n\n\n", &FormatOptions::default()),
        ".a {\n}\n"
    );
}
#[test]
fn invalid_options() {
    assert!(
        format(
            ".a{}",
            &FormatOptions {
                tab_width: 0,
                ..Default::default()
            }
        )
        .text
        .is_none()
    );
}

#[test]
fn line_comments_are_preserved_and_force_line_breaks() {
    formatted(
        "@vars { --gap = 0 // zero\n; } @injection f($n: number = 0 // default\n) {} .a { @:f(0 // arg\n); }",
        &FormatOptions::default(),
    );
    for eol in ["\n", "\r\n", "\r"] {
        let source = format!(
            "// lead 😀{eol}.a // selector{eol}{{color: // value{eol}red; // trailing{eol}width:1px;}} // eof"
        );
        let output = formatted(&source, &FormatOptions::default());
        assert!(output.contains("// value\n"), "{output}");
        assert!(output.contains("// selector\n"), "{output}");
        assert!(output.contains("// trailing\n"), "{output}");
    }
    let output = formatted(
        "// prettier-ignore\n.a   {color:   red;}\n",
        &FormatOptions::default(),
    );
    assert!(output.contains(".a   {color:   red;}"), "{output}");
}
