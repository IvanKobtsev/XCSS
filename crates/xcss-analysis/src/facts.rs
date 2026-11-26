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
    Function,
    Util,
    Parameter,
    Constant,
    Class,
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
    Union(Vec<ValueType>),
}
impl std::fmt::Display for ValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => f.write_str("unknown"),
            Self::Named(name) => f.write_str(name),
            Self::Union(members) => {
                for (index, member) in members.iter().enumerate() {
                    if index != 0 {
                        f.write_str(" | ")?;
                    }
                    write!(f, "{member}")?;
                }
                Ok(())
            }
        }
    }
}
impl ValueType {
    /// Flatten and deduplicate alternatives in source order. Unknown propagates
    /// so an invalid or unresolved branch cannot masquerade as a known type.
    pub fn union(members: impl IntoIterator<Item = Self>) -> Self {
        fn insert(member: ValueType, unique: &mut Vec<ValueType>) -> bool {
            match member {
                ValueType::Unknown => false,
                ValueType::Union(members) => {
                    members.into_iter().all(|member| insert(member, unique))
                }
                member => {
                    if !unique.contains(&member) {
                        unique.push(member);
                    }
                    true
                }
            }
        }
        let mut unique = Vec::new();
        for member in members {
            if !insert(member, &mut unique) {
                return Self::Unknown;
            }
        }
        match unique.len() {
            0 => Self::Unknown,
            1 => unique.pop().unwrap(),
            _ => Self::Union(unique),
        }
    }
    pub fn named(name: &str) -> Self {
        Self::Named(name.into())
    }
    pub fn accepts(&self, actual: &Self) -> bool {
        if let Self::Union(members) = actual {
            return members.iter().all(|member| self.accepts(member));
        }
        if let Self::Union(members) = self {
            return members.iter().any(|member| member.accepts(actual));
        }
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
pub struct FunctionSignature {
    pub symbol: SymbolId,
    pub parameters: Vec<Parameter>,
    pub return_type: ValueType,
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
    Injection,
    Function,
}
#[derive(Clone, Debug)]
pub struct Dependency {
    pub from: FileId,
    pub to: FileId,
    pub kind: DependencyKind,
}
