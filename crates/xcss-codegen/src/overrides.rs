use super::*;
use parcel_sourcemap::{OriginalLocation, SourceMap};
use xcss_source::SourceFile;

pub fn rewrite(
    file: &FileId,
    css: String,
    json: String,
    markers: &xcss_syntax::OverrideMarkers,
) -> Result<(String, String), Vec<Diagnostic>> {
    if markers.is_empty() {
        return Ok((css, json));
    }
    use lightningcss::{stylesheet::PrinterOptions, traits::ToCss, values::ident::CustomIdent};
    let escaped = markers
        .iter()
        .map(|(class, levels)| {
            (
                class.clone(),
                levels
                    .iter()
                    .map(|(level, marker)| {
                        (
                            level.clone(),
                            CustomIdent(marker.as_str().into())
                                .to_css_string(PrinterOptions::default())
                                .unwrap(),
                        )
                    })
                    .collect(),
            )
        })
        .collect();
    let parsed = xcss_syntax::parse(std::sync::Arc::new(SourceFile::new(
        file.clone(),
        0,
        css.clone(),
    )));
    let mut edits = Vec::new();
    let mut budget = 100_000;
    for node in parsed
        .tree
        .descendants()
        .filter(|n| n.kind == NodeKind::Rule)
    {
        let text = parsed.slice(node.header);
        let rewritten =
            xcss_syntax::override_selectors(text, &escaped, &mut budget).map_err(|message| {
                vec![Diagnostic::error(
                    "generate.expansion-limit",
                    file,
                    node.header,
                    message,
                )]
            })?;
        if rewritten.text != text {
            edits.push((node.header, rewritten));
        }
    }
    if edits.is_empty() {
        return Ok((css, json));
    }
    let mut out = String::new();
    let mut spans = Vec::new();
    let mut cursor = 0;
    for (range, replacement) in edits {
        let start = out.len();
        out.push_str(&css[cursor..range.start]);
        spans.push((
            TextRange::new(start, out.len()),
            TextRange::new(cursor, range.start),
        ));
        let start = out.len();
        out.push_str(&replacement.text);
        spans.extend(replacement.spans.into_iter().map(|(generated, source)| {
            (
                TextRange::new(start + generated.start, start + generated.end),
                TextRange::new(range.start + source.start, range.start + source.end),
            )
        }));
        cursor = range.end;
    }
    let start = out.len();
    out.push_str(&css[cursor..]);
    spans.push((
        TextRange::new(start, out.len()),
        TextRange::new(cursor, css.len()),
    ));
    let mut original = SourceMap::from_json("/", &json).unwrap();
    let mut map = SourceMap::new("/");
    for (i, source) in original.get_sources().iter().enumerate() {
        map.add_source(source);
        if let Ok(content) = original.get_source_content(i as u32) {
            let _ = map.set_source_content(i, content);
        }
    }
    let generated = SourceFile::new(file.clone(), 0, out.clone());
    for (range, source) in spans {
        for (offset, _) in out[range.start..range.end].char_indices() {
            let source_offset = if range.end - range.start == source.end - source.start {
                source.start + offset
            } else {
                source.start
            };
            let Some(position) = parsed.source.byte_to_utf16(source_offset) else {
                continue;
            };
            let Some(mapping) = original
                .find_closest_mapping(position.line as u32, position.character as u32)
                .and_then(|m| m.original)
            else {
                continue;
            };
            let position = generated.byte_to_utf16(range.start + offset).unwrap();
            map.add_mapping(
                position.line as u32,
                position.character as u32,
                Some(OriginalLocation {
                    name: None,
                    ..mapping
                }),
            );
        }
    }
    Ok((out, map.to_json(None).unwrap()))
}
