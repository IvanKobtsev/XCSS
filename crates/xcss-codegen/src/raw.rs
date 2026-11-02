//! Opaque fragments are restored after printing. Map extensions retain their
//! boundaries for later artifact bundling without changing public Rust types.
use super::*;
use parcel_sourcemap::{OriginalLocation, SourceMap};
use xcss_source::{Position, SourceFile};

const RANGES: &str = "x_xcss_raw";

pub(super) fn ranges(json: &str) -> Vec<TextRange> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| v.get(RANGES).cloned())
        .and_then(|v| serde_json::from_value::<Vec<(usize, usize)>>(v).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|(a, b)| TextRange::new(a, b))
        .collect()
}
pub(super) fn tag(json: String, ranges: &[TextRange]) -> String {
    if ranges.is_empty() {
        return json;
    }
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value[RANGES] = serde_json::json!(ranges.iter().map(|r| (r.start, r.end)).collect::<Vec<_>>());
    value.to_string()
}
pub(super) fn masked(css: &str, ranges: &[TextRange]) -> String {
    let mut bytes = css.as_bytes().to_vec();
    for range in ranges {
        for byte in &mut bytes[range.start..range.end] {
            if !matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(bytes).unwrap()
}

pub(super) fn restore(
    snapshot: &AnalysisSnapshot,
    css: String,
    json: String,
    input: &Mapped,
) -> (String, String) {
    if input.raw.is_empty() {
        return (css, json);
    }
    let parsed = xcss_syntax::parse(std::sync::Arc::new(SourceFile::new(
        FileId("raw-placeholders".into()),
        0,
        css.clone(),
    )));
    let mut replacements = Vec::new();
    for node in parsed.tree.descendants() {
        if let Some((body, origin)) = input.raw.get(node.header_text(&parsed.source).trim()) {
            replacements.push((node.range, body, origin));
        }
    }
    replacements.sort_by_key(|(r, _, _)| r.start);
    let mut out = String::new();
    let mut cursor = 0;
    let mut restored = Vec::new();
    for (range, body, _) in &replacements {
        out.push_str(&css[cursor..range.start]);
        let start = out.len();
        out.push_str(body);
        restored.push(TextRange::new(start, out.len()));
        cursor = range.end;
    }
    out.push_str(&css[cursor..]);
    let original = SourceMap::from_json("/", &json).unwrap();
    let mut map = SourceMap::new("/");
    for (i, name) in original.get_sources().iter().enumerate() {
        map.add_source(name);
        if let Ok(content) = original.get_source_content(i as u32) {
            let _ = map.set_source_content(i, content);
        }
    }
    for name in original.get_names() {
        map.add_name(name);
    }
    let generated = SourceFile::new(FileId("restored".into()), 0, out.clone());
    for mapping in original.get_mappings() {
        let Some(offset) = parsed.source.utf16_to_byte(Position {
            line: mapping.generated_line as usize,
            character: mapping.generated_column as usize,
        }) else {
            continue;
        };
        if replacements.iter().any(|(r, _, _)| r.contains(offset)) {
            continue;
        }
        let delta: isize = replacements
            .iter()
            .filter(|(r, _, _)| r.end <= offset)
            .map(|(r, b, _)| b.len() as isize - (r.end - r.start) as isize)
            .sum();
        let position = generated
            .byte_to_utf16(offset.checked_add_signed(delta).unwrap())
            .unwrap();
        map.add_mapping(
            position.line as u32,
            position.character as u32,
            mapping.original,
        );
    }
    for ((_, body, origin), range) in replacements.iter().zip(&restored) {
        let source = snapshot.syntax(&origin.file).unwrap();
        let index = map.add_source(&origin.file.0);
        let _ = map.set_source_content(index as usize, source.source.text());
        for (offset, _) in body.char_indices() {
            let position = generated.byte_to_utf16(range.start + offset).unwrap();
            let original = source
                .source
                .byte_to_utf16(origin.range.start + offset)
                .unwrap();
            map.add_mapping(
                position.line as u32,
                position.character as u32,
                Some(OriginalLocation {
                    original_line: original.line as u32,
                    original_column: original.character as u32,
                    source: index,
                    name: None,
                }),
            );
        }
    }
    (out, tag(map.to_json(None).unwrap(), &restored))
}
