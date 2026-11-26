use xcss_analysis::{SymbolKind, ValueType};
use xcss_source::{FileId, Position};

use crate::{EditorLocation, Range, ServiceSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HoverSymbolKind {
    Namespace,
    Keyframe,
    Variable,
    Injection,
    Function,
    Util,
    Parameter,
    Constant,
    Class,
}

impl From<SymbolKind> for HoverSymbolKind {
    fn from(kind: SymbolKind) -> Self {
        match kind {
            SymbolKind::Namespace => Self::Namespace,
            SymbolKind::Keyframe => Self::Keyframe,
            SymbolKind::Variable => Self::Variable,
            SymbolKind::Injection => Self::Injection,
            SymbolKind::Function => Self::Function,
            SymbolKind::Util => Self::Util,
            SymbolKind::Parameter => Self::Parameter,
            SymbolKind::Constant => Self::Constant,
            SymbolKind::Class => Self::Class,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HoverType {
    Unknown,
    Named(String),
    Union(Vec<HoverType>),
}

impl From<&ValueType> for HoverType {
    fn from(value: &ValueType) -> Self {
        match value {
            ValueType::Unknown => Self::Unknown,
            ValueType::Named(name) => Self::Named(name.clone()),
            ValueType::Union(members) => Self::Union(members.iter().map(Self::from).collect()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HoverParameter {
    pub name: String,
    pub value_type: HoverType,
    /// XCSS source, without presentation labels or evaluation.
    pub default_value: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HoverSignature {
    pub parameters: Vec<HoverParameter>,
    pub accepts_content: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HoverVariable {
    pub inherits: bool,
    /// XCSS source, without presentation labels or evaluation.
    pub initial_value: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hover {
    /// The hovered occurrence, in the queried file.
    pub range: Range,
    pub kind: HoverSymbolKind,
    pub name: String,
    pub declaration: EditorLocation,
    /// The analyzed symbol type; for functions, this is the return type.
    pub value_type: HoverType,
    pub signature: Option<HoverSignature>,
    pub variable: Option<HoverVariable>,
    /// The constant initializer's source, trimmed but not evaluated.
    pub constant_value: Option<String>,
}

impl ServiceSnapshot {
    pub fn hover(&self, file: &FileId, position: Position) -> Option<Hover> {
        let offset = self.offset(file, position)?;
        let symbol = self.0.symbol_at(file, offset)?;
        let occurrence = self
            .0
            .references()
            .iter()
            .filter(|r| r.location.file == *file && r.location.range.contains(offset))
            .min_by_key(|r| r.location.range.end - r.location.range.start)
            .map(|r| &r.location)
            .unwrap_or(&symbol.location);
        let signature = self.0.signature(symbol.id).map(|signature| HoverSignature {
            parameters: signature
                .parameters
                .iter()
                .map(|parameter| HoverParameter {
                    name: self.0.symbol(parameter.symbol).unwrap().name.clone(),
                    value_type: HoverType::from(&parameter.value_type),
                    default_value: parameter.default.clone(),
                })
                .collect(),
            accepts_content: signature.accepts_content,
        });
        let variable = self
            .0
            .variable_registration(symbol.id)
            .map(|variable| HoverVariable {
                inherits: variable.inherits,
                initial_value: variable.initial,
            });
        let constant_value = self
            .0
            .constant_initializer(symbol.id)
            .map(|(parse, initial)| parse.slice(initial.range).trim().to_owned());
        Some(Hover {
            range: self.location(occurrence)?.range,
            kind: symbol.kind.into(),
            name: symbol.name.clone(),
            declaration: self.location(&symbol.location)?,
            value_type: HoverType::from(&symbol.value_type),
            signature,
            variable,
            constant_value,
        })
    }
}
