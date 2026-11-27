//! Presentation-neutral message content and its plain-text rendering.
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiagnosticType {
    Unknown,
    Named(String),
    Union(Vec<DiagnosticType>),
}

impl fmt::Display for DiagnosticType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => f.write_str("unknown"),
            Self::Named(name) => f.write_str(name),
            Self::Union(members) => {
                for (index, member) in members.iter().enumerate() {
                    if index > 0 {
                        f.write_str(" | ")?;
                    }
                    write!(f, "{member}")?;
                }
                Ok(())
            }
        }
    }
}

/// Only used by the plain-text renderer; editor parts omit this metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeQuote {
    None,
    Single,
    Backtick,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiagnosticPart {
    Text(String),
    Type(DiagnosticType),
    Code { text: String, quote: CodeQuote },
    Number(usize),
}

impl DiagnosticPart {
    pub fn code(text: impl AsRef<str>) -> Self {
        Self::Code {
            text: text.as_ref().into(),
            quote: CodeQuote::None,
        }
    }
    pub fn single_quote(text: impl AsRef<str>) -> Self {
        Self::Code {
            text: text.as_ref().into(),
            quote: CodeQuote::Single,
        }
    }
    pub fn backtick(text: impl AsRef<str>) -> Self {
        Self::Code {
            text: text.as_ref().into(),
            quote: CodeQuote::Backtick,
        }
    }
    pub fn value_type(value: impl Into<DiagnosticType>) -> Self {
        Self::Type(value.into())
    }
}

impl From<&str> for DiagnosticPart {
    fn from(text: &str) -> Self {
        Self::Text(text.into())
    }
}
impl From<String> for DiagnosticPart {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl fmt::Display for DiagnosticPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => f.write_str(text),
            Self::Type(value) => write!(f, "{value}"),
            Self::Number(value) => write!(f, "{value}"),
            Self::Code { text, quote } => {
                let wrapper = match quote {
                    CodeQuote::None => "",
                    CodeQuote::Single => "'",
                    CodeQuote::Backtick => "`",
                };
                write!(f, "{wrapper}{text}{wrapper}")
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticMessage {
    pub parts: Vec<DiagnosticPart>,
}

impl From<&str> for DiagnosticMessage {
    fn from(text: &str) -> Self {
        Self {
            parts: vec![text.into()],
        }
    }
}
impl From<String> for DiagnosticMessage {
    fn from(text: String) -> Self {
        Self {
            parts: vec![text.into()],
        }
    }
}
impl From<&String> for DiagnosticMessage {
    fn from(text: &String) -> Self {
        Self::from(text.as_str())
    }
}
impl fmt::Display for DiagnosticMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for part in &self.parts {
            write!(f, "{part}")?;
        }
        Ok(())
    }
}

/// Build the message once, keeping semantic inserts separate from prose.
#[macro_export]
macro_rules! diagnostic_message {
    ($($part:expr),* $(,)?) => {
        $crate::DiagnosticMessage { parts: vec![$($crate::DiagnosticPart::from($part)),*] }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plain_text_uses_parts_and_preserves_quotes_and_types() {
        let message = crate::diagnostic_message![
            "Expected ",
            DiagnosticPart::Type(DiagnosticType::Union(vec![
                DiagnosticType::Named("length".into()),
                DiagnosticType::Unknown,
            ])),
            " for ",
            DiagnosticPart::backtick("$value"),
            " in ",
            DiagnosticPart::single_quote("theme.xcss"),
            ", got ",
            DiagnosticPart::Number(2),
        ];
        assert_eq!(
            message.to_string(),
            "Expected length | unknown for `$value` in 'theme.xcss', got 2"
        );
        assert_eq!(
            DiagnosticMessage::from("Plain prose").to_string(),
            "Plain prose"
        );
    }
}
