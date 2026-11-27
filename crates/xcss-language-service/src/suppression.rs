use super::*;

/// Codes suppressed on a zero-based physical source line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Suppression {
    pub line: usize,
    pub codes: Vec<String>,
}

pub(super) fn suppresses(suppressions: &[Suppression], line: usize, code: &str) -> bool {
    suppressions
        .iter()
        .any(|s| s.line == line && s.codes.iter().any(|c| c == code))
}

impl ServiceSnapshot {
    pub(super) fn suppressions(&self, file: &FileId) -> Vec<Suppression> {
        let Some(parse) = self.0.syntax(file) else {
            return vec![];
        };
        let text = parse.source.text();
        parse
            .tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Comment)
            .filter_map(|token| {
                let comment = parse.slice(token.range).strip_prefix("//")?.trim_start();
                let list = comment.strip_prefix("@xcss-ignore")?;
                if !list.starts_with(char::is_whitespace) {
                    return None;
                }
                let prefix = &text[..token.range.start];
                if !prefix.rsplit('\n').next()?.trim().is_empty() {
                    return None;
                }
                let mut codes: Vec<_> = list
                    .split(',')
                    .map(|code| code.trim().to_ascii_uppercase())
                    .filter(|code| {
                        code.len() == 6
                            && code.starts_with("XC-")
                            && code.as_bytes()[3..].iter().all(u8::is_ascii_digit)
                    })
                    .collect();
                codes.sort();
                codes.dedup();
                if codes.is_empty() {
                    return None;
                }
                Some(Suppression {
                    line: parse.source.byte_to_utf16(token.range.start)?.line + 1,
                    codes,
                })
            })
            .collect()
    }
}
