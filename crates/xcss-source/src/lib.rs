//! Source identities, immutable revisions and original-source locations.
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct FileId(pub String);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}
impl TextRange {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    pub fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Information,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Location {
    pub file: FileId,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub location: Location,
    pub related: Vec<Location>,
}
impl Diagnostic {
    pub fn error(
        code: &'static str,
        file: &FileId,
        range: TextRange,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            location: Location {
                file: file.clone(),
                range,
            },
            related: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    pub line: usize,
    pub character: usize,
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub id: FileId,
    pub revision: u64,
    text: Arc<str>,
    lines: Arc<[usize]>,
}
impl SourceFile {
    pub fn new(id: FileId, revision: u64, text: impl Into<Arc<str>>) -> Self {
        let text = text.into();
        let lines = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect::<Vec<_>>()
            .into();
        Self {
            id,
            revision,
            text,
            lines,
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn byte_to_utf16(&self, offset: usize) -> Option<Position> {
        if offset > self.text.len() || !self.text.is_char_boundary(offset) {
            return None;
        }
        let line = self.lines.partition_point(|&start| start <= offset) - 1;
        Some(Position {
            line,
            character: self.text[self.lines[line]..offset].encode_utf16().count(),
        })
    }
    pub fn utf16_to_byte(&self, position: Position) -> Option<usize> {
        let start = *self.lines.get(position.line)?;
        let end = self
            .lines
            .get(position.line + 1)
            .copied()
            .unwrap_or(self.text.len());
        let mut units = 0;
        for (i, c) in self.text[start..end].char_indices() {
            if units == position.character {
                return Some(start + i);
            }
            if c == '\n' {
                return None;
            }
            units += c.len_utf16();
        }
        (units == position.character).then_some(end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_positions() {
        let file = SourceFile::new(FileId("a".into()), 1, "é😀\r\nx");
        for i in (0..=file.text().len()).filter(|&i| file.text().is_char_boundary(i)) {
            assert_eq!(file.utf16_to_byte(file.byte_to_utf16(i).unwrap()), Some(i));
        }
        assert_eq!(
            file.utf16_to_byte(Position {
                line: 0,
                character: 2
            }),
            None
        );
    }
}
