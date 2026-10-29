use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SymbolId(pub usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScopeId(pub usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolKind {
    Namespace,
    Keyframe,
    Variable,
    Injection,
    Parameter,
    Class,
}

#[derive(Clone, Debug)]
pub struct ResolvedComposition {
    pub owner: SymbolId,
    pub members: Vec<CompositionMember>,
}
#[derive(Clone, Debug)]
pub struct CompositionMember {
    pub name: String,
    pub target: Option<SymbolId>,
    pub location: Location,
}
#[derive(Clone, Debug)]
pub struct VariableRegistration {
    pub symbol: SymbolId,
    pub location: Location,
    pub value_type: ValueType,
    pub initial: Option<String>,
    pub inherits: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueType {
    Unknown,
    Named(String),
}
impl ValueType {
    pub fn named(name: &str) -> Self {
        Self::Named(name.into())
    }
    pub fn accepts(&self, actual: &Self) -> bool {
        self == &Self::Unknown
            || actual == &Self::Unknown
            || self == actual
            || matches!((self,actual), (Self::Named(a),Self::Named(b)) if a == "number" && b == "integer" || a == "length-percentage" && matches!(b.as_str(), "length" | "percentage") || a == "image" && b == "url" || a == "transform-list" && b == "transform-function")
    }
}
#[derive(Clone, Debug)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub location: Location,
    pub scope: ScopeId,
    pub value_type: ValueType,
    pub global: bool,
}
#[derive(Clone, Debug)]
pub struct Scope {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub location: Location,
    pub symbols: Vec<SymbolId>,
}
#[derive(Clone, Debug)]
pub struct Reference {
    pub role: ReferenceRole,
    pub name: String,
    pub location: Location,
    pub target: Option<SymbolId>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceRole {
    Read,
    Declaration,
    Import,
}
#[derive(Clone, Debug)]
pub struct ImportBinding {
    pub location: Location,
    pub specifier: String,
    pub target_file: Option<FileId>,
    pub symbol: Option<SymbolId>,
}
#[derive(Clone, Debug)]
pub struct Parameter {
    pub symbol: SymbolId,
    pub value_type: ValueType,
    pub default: Option<String>,
}
#[derive(Clone, Debug)]
pub struct InjectionSignature {
    pub symbol: SymbolId,
    pub parameters: Vec<Parameter>,
    pub accepts_content: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpressionKind {
    Value,
    Arithmetic,
    TransitionGroup,
    ContentDirective,
}
#[derive(Clone, Debug)]
pub struct Expression {
    pub arithmetic: Option<xcss_syntax::ast::ArithmeticExpression>,
    pub location: Location,
    pub value_type: ValueType,
    pub kind: ExpressionKind,
    pub syntax: Value,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencyKind {
    Import,
    Composition,
    Injection,
}
#[derive(Clone, Debug)]
pub struct Dependency {
    pub from: FileId,
    pub to: FileId,
    pub kind: DependencyKind,
}
