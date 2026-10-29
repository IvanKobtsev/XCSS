use super::*;
#[derive(Clone, Debug, Default)]
pub(super) struct Mapped {
    pub text: String,
    pub spans: Vec<(TextRange, Location)>,
}
impl Mapped {
    pub fn origin_at(&self, offset: usize) -> Option<Location> {
        let (range, origin) = self.spans.iter().find(|(r, _)| r.contains(offset))?;
        let mut origin = origin.clone();
        if range.end - range.start == origin.range.end - origin.range.start {
            origin.range.start += offset - range.start;
            origin.range.end = origin.range.start;
        }
        Some(origin)
    }
    /// The backend maps rules; add declaration/token mappings from the lowered IR.
    /// Matching stays within a declaration, so substituted arguments retain caller origins.
    pub fn map_printed(
        &self,
        snapshot: &AnalysisSnapshot,
        css: &str,
        map: &mut parcel_sourcemap::SourceMap,
    ) {
        let input = xcss_syntax::parse(std::sync::Arc::new(xcss_source::SourceFile::new(
            FileId("lowered".into()),
            0,
            self.text.clone(),
        )));
        let output = xcss_syntax::parse(std::sync::Arc::new(xcss_source::SourceFile::new(
            FileId("printed".into()),
            0,
            css.to_owned(),
        )));
        let mut declarations: BTreeMap<String, Vec<xcss_syntax::ast::Declaration<'_>>> =
            BTreeMap::new();
        for node in input.tree.descendants() {
            if let Some(d) = input.declaration(node) {
                declarations
                    .entry(d.property.to_ascii_lowercase())
                    .or_default()
                    .push(d);
            }
        }
        let mut indexes = BTreeMap::<String, usize>::new();
        let mut add = |generated: usize, original: usize| {
            let Some(origin) = self.origin_at(original) else {
                return;
            };
            let Some(source) = snapshot.syntax(&origin.file) else {
                return;
            };
            let Some(position) = source.source.byte_to_utf16(origin.range.start) else {
                return;
            };
            let generated = output.source.byte_to_utf16(generated).unwrap();
            let index = map.add_source(&origin.file.0);
            let _ = map.set_source_content(index as usize, source.source.text());
            map.add_mapping(
                generated.line as u32,
                generated.character as u32,
                Some(parcel_sourcemap::OriginalLocation {
                    original_line: position.line as u32,
                    original_column: position.character as u32,
                    source: index,
                    name: None,
                }),
            );
        };
        for node in output.tree.descendants() {
            let Some(declaration) = output.declaration(node) else {
                continue;
            };
            let name = declaration.property.to_ascii_lowercase();
            let index = indexes.entry(name.clone()).or_default();
            let Some(original) = declarations.get(&name).and_then(|d| d.get(*index)) else {
                continue;
            };
            *index += 1;
            add(
                declaration.property_range.start,
                original.property_range.start,
            );
            let tokens: Vec<_> = input
                .tokens
                .iter()
                .filter(|t| {
                    original.value.range.contains(t.range.start)
                        && !matches!(
                            t.kind,
                            xcss_syntax::TokenKind::Whitespace | xcss_syntax::TokenKind::Comment
                        )
                })
                .collect();
            let mut cursor = 0;
            for token in output.tokens.iter().filter(|t| {
                declaration.value.range.contains(t.range.start)
                    && !matches!(
                        t.kind,
                        xcss_syntax::TokenKind::Whitespace | xcss_syntax::TokenKind::Comment
                    )
            }) {
                let text = output.token_text(token).trim_matches(['\'', '"']);
                let found = tokens
                    .iter()
                    .enumerate()
                    .skip(cursor)
                    .find(|(_, t)| input.token_text(t).trim_matches(['\'', '"']) == text);
                let original_offset = if let Some((index, token)) = found {
                    cursor = index + 1;
                    token.range.start
                } else {
                    tokens
                        .get(cursor)
                        .or_else(|| tokens.first())
                        .map(|t| t.range.start)
                        .unwrap_or(original.property_range.start)
                };
                add(token.range.start, original_offset);
            }
        }
    }
    pub fn push(&mut self, text: &str, origin: &Location) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        self.spans
            .push((TextRange::new(start, self.text.len()), origin.clone()));
    }
    pub fn append(&mut self, other: &Self) {
        let offset = self.text.len();
        self.text.push_str(&other.text);
        self.spans.extend(other.spans.iter().map(|(range, origin)| {
            (
                TextRange::new(range.start + offset, range.end + offset),
                origin.clone(),
            )
        }));
    }
    pub fn source_map(&self, snapshot: &AnalysisSnapshot) -> parcel_sourcemap::SourceMap {
        use parcel_sourcemap::{OriginalLocation, SourceMap};
        let generated =
            xcss_source::SourceFile::new(FileId("generated".into()), 0, self.text.clone());
        let mut map = SourceMap::new("/");
        for (range, origin) in &self.spans {
            let Some(parse) = snapshot.syntax(&origin.file) else {
                continue;
            };
            let index = map.add_source(&origin.file.0);
            let _ = map.set_source_content(index as usize, parse.source.text());
            let exact = range.end - range.start == origin.range.end - origin.range.start;
            for (offset, _) in self.text[range.start..range.end].char_indices() {
                let position = generated.byte_to_utf16(range.start + offset).unwrap();
                let source_offset = if exact {
                    origin.range.start + offset
                } else {
                    origin.range.start
                };
                if let Some(original) = parse.source.byte_to_utf16(source_offset) {
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
        }
        map
    }
    pub fn error(
        &self,
        snapshot: &AnalysisSnapshot,
        file: &FileId,
        code: &'static str,
        message: String,
        position: Option<(u32, u32)>,
    ) -> Diagnostic {
        let source = xcss_source::SourceFile::new(file.clone(), 0, self.text.clone());
        let offset = position
            .and_then(|(line, character)| {
                source.utf16_to_byte(xcss_source::Position {
                    line: line as usize,
                    character: character as usize,
                })
            })
            .unwrap_or(0);
        let origin = self
            .spans
            .iter()
            .find(|(r, _)| r.contains(offset))
            .or_else(|| self.spans.iter().rev().find(|(r, _)| r.start <= offset))
            .map(|(_, l)| l.clone())
            .unwrap_or(Location {
                file: file.clone(),
                range: TextRange::default(),
            });
        let _ = snapshot;
        Diagnostic::error(code, &origin.file, origin.range, message)
    }
}
