use xcss_language_service::{HoverSymbolKind, HoverType, LanguageService};
use xcss_source::{FileId, SourceFile};

#[test]
fn hover_sections_distinguish_empty_signatures_unknown_types_and_absent_values() {
    let file = FileId("hover.xcss".into());
    let source = "/* 😀 */ @vars { --theme: color; } @injection wrap { @content; } @injection pair($first: length, $second: color = red) {} @function size(): length => 2px; .a { @:wrap { color: --theme; } @:pair(1px); width: @:size(); }";
    let mut host = LanguageService::new();
    host.set_file(file.clone(), 1, source.into()).unwrap();
    let snapshot = host.snapshot();
    assert!(
        snapshot.diagnostics(&file).is_empty(),
        "{:?}",
        snapshot.diagnostics(&file)
    );
    let source_file = SourceFile::new(file.clone(), 1, source);
    let hover = |needle: &str| {
        snapshot
            .hover(
                &file,
                source_file
                    .byte_to_utf16(source.find(needle).unwrap())
                    .unwrap(),
            )
            .unwrap()
    };

    let wrap = hover("wrap {");
    assert_eq!(wrap.kind, HoverSymbolKind::Injection);
    assert_eq!(wrap.value_type, HoverType::Unknown);
    let signature = wrap.signature.unwrap();
    assert!(signature.parameters.is_empty());
    assert!(signature.accepts_content);
    assert!(wrap.variable.is_none());
    assert!(wrap.constant_value.is_none());
    assert_eq!(
        wrap.range.start.character,
        source_file
            .byte_to_utf16(source.find("wrap").unwrap())
            .unwrap()
            .character
    );

    let pair = hover("pair(1px)").signature.unwrap();
    assert_eq!(
        pair.parameters
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["$first", "$second"]
    );
    assert!(pair.parameters[0].default_value.is_none());
    assert_eq!(pair.parameters[1].default_value.as_deref(), Some("red"));
    assert!(!pair.accepts_content);

    let size = hover("size()");
    assert_eq!(size.kind, HoverSymbolKind::Function);
    assert_eq!(size.value_type, HoverType::Named("length".into()));
    assert!(size.signature.unwrap().parameters.is_empty());

    let theme = hover("--theme;");
    assert_eq!(theme.name, "--theme");
    assert_eq!(theme.value_type, HoverType::Named("color".into()));
    let registration = theme.variable.unwrap();
    assert!(registration.inherits);
    assert!(registration.initial_value.is_none());
    assert!(theme.signature.is_none());
}

#[test]
fn constant_initializers_are_source_not_evaluated_values() {
    let file = FileId("hover.xcss".into());
    let source = "$size =  2px + 3px ; .a { width: $size; }";
    let mut host = LanguageService::new();
    host.set_file(file.clone(), 1, source.into()).unwrap();
    let position = SourceFile::new(file.clone(), 1, source)
        .byte_to_utf16(source.rfind("$size").unwrap())
        .unwrap();
    let hover = host.snapshot().hover(&file, position).unwrap();
    assert_eq!(hover.constant_value.as_deref(), Some("2px + 3px"));
    assert_eq!(hover.value_type, HoverType::Named("length".into()));
}
