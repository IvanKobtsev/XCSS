use crate::DiagnosticType;

/// Ordered message content. Hosts control code and type presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiagnosticPart {
    Text(String),
    Type(DiagnosticType),
    Code(String),
    Number(usize),
}

impl From<xcss_source::DiagnosticPart> for DiagnosticPart {
    fn from(part: xcss_source::DiagnosticPart) -> Self {
        match part {
            xcss_source::DiagnosticPart::Text(text) => Self::Text(text),
            xcss_source::DiagnosticPart::Type(value) => Self::Type(value),
            xcss_source::DiagnosticPart::Code { text, .. } => Self::Code(text),
            xcss_source::DiagnosticPart::Number(value) => Self::Number(value),
        }
    }
}
