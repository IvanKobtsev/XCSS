//! In-memory, snapshot-based semantic analysis for XCSS.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use xcss_source::{Diagnostic, FileId, Location, SourceFile, TextRange};
use xcss_syntax::ast::{Value, ValueKind, decode_identifier};
use xcss_syntax::{Node, NodeKind, ParseResult, TokenKind};

mod collect;
mod constants;
mod facts;
mod host;
mod registration;
mod types;
mod variants;
use collect::collect;
use constants::analyze_constants;
pub use facts::*;
pub use host::{AnalysisHost, AnalysisSnapshot, UpdateError};
pub use registration::registration_initial_is_independent;
use types::checked_type;
pub use types::infer_type;
pub use variants::ModuleInterface;

#[derive(Default)]
struct Database {
    generation: u64,
    files: BTreeMap<FileId, Arc<ParseResult>>,
    diagnostics: Vec<Diagnostic>,
    symbols: Vec<Symbol>,
    scopes: Vec<Scope>,
    references: Vec<Reference>,
    signatures: BTreeMap<SymbolId, InjectionSignature>,
    expressions: Vec<Expression>,
    dependencies: Vec<Dependency>,
    compositions: Vec<ResolvedComposition>,
    imports: Vec<ImportBinding>,
    constants: BTreeMap<SymbolId, Value>,
    interfaces: BTreeMap<FileId, ModuleInterface>,
    resolutions: BTreeMap<(FileId, String), Option<FileId>>,
}
#[derive(Clone)]
struct Import {
    file: FileId,
    range: TextRange,
    namespace: Option<String>,
    namespace_range: Option<TextRange>,
    names: Vec<(String, TextRange)>,
    specifier: String,
}
#[derive(Clone)]
struct Call {
    file: FileId,
    node: Node,
    owner: Option<SymbolId>,
}
#[derive(Clone)]
struct Composition {
    file: FileId,
    node: Node,
    owners: Vec<SymbolId>,
}
#[derive(Default)]
struct Collected {
    imports: Vec<Import>,
    calls: Vec<Call>,
    compositions: Vec<Composition>,
    scopes: BTreeMap<(FileId, usize), ScopeId>,
}

impl Database {
    fn callable(&self, file: &FileId, name: &str) -> Option<SymbolId> {
        self.local(file, name, SymbolKind::Util)
            .or_else(|| self.local(file, name, SymbolKind::Injection))
    }

    fn error(
        &mut self,
        code: &'static str,
        file: &FileId,
        range: TextRange,
        message: impl Into<String>,
    ) {
        self.diagnostics
            .push(Diagnostic::error(code, file, range, message));
    }
    fn scope(&mut self, file: &FileId, range: TextRange, parent: Option<ScopeId>) -> ScopeId {
        let id = ScopeId(self.scopes.len());
        self.scopes.push(Scope {
            id,
            parent,
            location: Location {
                file: file.clone(),
                range,
            },
            symbols: Vec::new(),
        });
        id
    }
    fn declare(
        &mut self,
        location: Location,
        name: String,
        kind: SymbolKind,
        scope: ScopeId,
        value_type: ValueType,
        global: bool,
    ) -> SymbolId {
        let file = &location.file;
        let range = location.range;
        if !matches!(kind, SymbolKind::Class | SymbolKind::Keyframe)
            && let Some(old) = self.scopes[scope.0]
                .symbols
                .iter()
                .map(|id| &self.symbols[id.0])
                .find(|s| {
                    s.name == name
                        && (s.kind == kind
                            || matches!(
                                (s.kind, kind),
                                (SymbolKind::Injection, SymbolKind::Util)
                                    | (SymbolKind::Util, SymbolKind::Injection)
                            ))
                })
        {
            let related = old.location.clone();
            self.error(
                "binding.duplicate",
                file,
                range,
                format!("Duplicate declaration `{name}`"),
            );
            self.diagnostics.last_mut().unwrap().related.push(related);
        }
        let id = SymbolId(self.symbols.len());
        self.symbols.push(Symbol {
            id,
            name,
            kind,
            location: Location {
                file: file.clone(),
                range,
            },
            scope,
            value_type,
            global,
        });
        self.scopes[scope.0].symbols.push(id);
        id
    }
    fn local(&self, file: &FileId, name: &str, kind: SymbolKind) -> Option<SymbolId> {
        self.symbols
            .iter()
            .find(|s| &s.location.file == file && s.kind == kind && s.name == name && !s.global)
            .map(|s| s.id)
    }
    fn lexical_at(&self, mut scope: ScopeId, name: &str, offset: usize) -> Option<SymbolId> {
        loop {
            for id in &self.scopes[scope.0].symbols {
                let symbol = &self.symbols[id.0];
                if symbol.name != name {
                    continue;
                }
                if symbol.kind == SymbolKind::Parameter {
                    return Some(*id);
                }
                if symbol.kind == SymbolKind::Constant
                    && self.constants.get(id).is_some_and(|v| v.range.end < offset)
                {
                    return Some(*id);
                }
            }
            scope = self.scopes[scope.0].parent?;
        }
    }
}

fn analyze(
    generation: u64,
    files: BTreeMap<FileId, Arc<ParseResult>>,
    resolutions: &BTreeMap<(FileId, String), Option<FileId>>,
) -> Database {
    let mut db = Database {
        generation,
        files,
        resolutions: resolutions.clone(),
        ..Database::default()
    };
    let mut collected = Collected::default();
    let parsed: Vec<_> = db.files.values().cloned().collect();
    for parse in &parsed {
        db.diagnostics.extend(parse.diagnostics.iter().cloned());
        let scope = db.scope(&parse.source.id, parse.tree.range, None);
        collect(
            &mut db,
            &mut collected,
            parse,
            &parse.tree,
            scope,
            None,
            &[],
        );
    }
    let mut named = BTreeMap::<(FileId, String), Option<SymbolId>>::new();
    let mut namespaces = BTreeMap::<(FileId, String), Option<FileId>>::new();
    for import in &collected.imports {
        let target = resolutions
            .get(&(import.file.clone(), import.specifier.clone()))
            .and_then(Clone::clone)
            .filter(|id| db.files.contains_key(id));
        if target.is_none() {
            db.error(
                "binding.unresolved-import",
                &import.file,
                import.range,
                format!("Cannot resolve `{}`", import.specifier),
            );
        }
        if let Some(target) = &target {
            db.dependencies.push(Dependency {
                from: import.file.clone(),
                to: target.clone(),
                kind: DependencyKind::Import,
            });
        }
        if let Some(namespace) = &import.namespace
            && namespaces
                .insert((import.file.clone(), namespace.clone()), target.clone())
                .is_some()
        {
            db.error(
                "binding.import-conflict",
                &import.file,
                import.range,
                "Duplicate namespace import",
            );
        }
        if let (Some(namespace), Some(range)) = (&import.namespace, import.namespace_range) {
            let scope = db
                .scopes
                .iter()
                .find(|s| s.location.file == import.file && s.parent.is_none())
                .unwrap()
                .id;
            let symbol = db.declare(
                Location {
                    file: import.file.clone(),
                    range,
                },
                namespace.clone(),
                SymbolKind::Namespace,
                scope,
                ValueType::Unknown,
                false,
            );
            db.imports.push(ImportBinding {
                location: Location {
                    file: import.file.clone(),
                    range,
                },
                specifier: import.specifier.clone(),
                target_file: target.clone(),
                symbol: Some(symbol),
            });
        }
        for (name, range) in &import.names {
            let symbol = target.as_ref().and_then(|f| db.callable(f, name));
            db.imports.push(ImportBinding {
                location: Location {
                    file: import.file.clone(),
                    range: *range,
                },
                specifier: import.specifier.clone(),
                target_file: target.clone(),
                symbol,
            });
            if target.is_some() && symbol.is_none() {
                db.error(
                    "binding.missing-export",
                    &import.file,
                    *range,
                    format!("Injection `{name}` is not exported"),
                );
            }
            if named
                .insert((import.file.clone(), name.clone()), symbol)
                .is_some()
            {
                db.error(
                    "binding.import-conflict",
                    &import.file,
                    *range,
                    "Conflicting named import",
                );
            }
            db.references.push(Reference {
                role: ReferenceRole::Import,
                name: name.clone(),
                location: Location {
                    file: import.file.clone(),
                    range: *range,
                },
                target: symbol,
            });
        }
    }
    for parse in &parsed {
        analyze_references(&mut db, &collected, parse);
    }
    analyze_constants(&mut db, &parsed);
    let mut edges = Vec::new();
    for call in &collected.calls {
        let parsed = db.files[&call.file].clone();
        let Some(syntax) = parsed.call(&call.node) else {
            continue;
        };
        let decoded = decode_identifier(syntax.name);
        let name = decoded.as_str();
        let range = syntax.name_range;
        if let Some((ns, _)) = name.split_once('.') {
            let ns_range = TextRange::new(
                range.start,
                range.start + syntax.name.find('.').unwrap_or(ns.len()),
            );
            let target = db.local(&call.file, ns, SymbolKind::Namespace);
            db.references.push(Reference {
                role: ReferenceRole::Read,
                name: ns.into(),
                location: Location {
                    file: call.file.clone(),
                    range: ns_range,
                },
                target,
            });
        }
        let mut suppressed = false;
        let target = if let Some((ns, member)) = name.split_once('.') {
            match namespaces.get(&(call.file.clone(), ns.into())) {
                Some(Some(file)) => db.callable(file, member),
                Some(None) => {
                    suppressed = true;
                    None
                }
                None => None,
            }
        } else if let Some(local) = db.callable(&call.file, name) {
            Some(local)
        } else if let Some(imported) = named.get(&(call.file.clone(), name.into())) {
            suppressed = imported.is_none();
            *imported
        } else {
            let candidates: Vec<_> = db
                .symbols
                .iter()
                .filter(|s| {
                    matches!(s.kind, SymbolKind::Injection | SymbolKind::Util) && s.name == name
                })
                .map(|s| s.id)
                .collect();
            if candidates.len() > 1 {
                db.error(
                    "binding.ambiguous-injection",
                    &call.file,
                    range,
                    format!("Ambiguous injection `{name}`"),
                );
                suppressed = true;
                None
            } else {
                candidates.first().copied()
            }
        };
        db.references.push(Reference {
            role: ReferenceRole::Read,
            name: name.into(),
            location: Location {
                file: call.file.clone(),
                range,
            },
            target,
        });
        if let Some(target) = target {
            db.dependencies.push(Dependency {
                from: call.file.clone(),
                to: db.symbols[target.0].location.file.clone(),
                kind: DependencyKind::Injection,
            });
            if let Some(owner) = call.owner {
                edges.push((owner, target));
            }
            validate_call(&mut db, call, target);
        } else if !suppressed {
            db.error(
                "binding.unknown-injection",
                &call.file,
                range,
                format!("Unknown injection `{name}`"),
            );
        }
    }
    check_cycles(&mut db, &edges, "binding.injection-cycle");
    validate_utils(&mut db);
    analyze_compositions(&mut db, &collected.compositions, resolutions);
    variants::analyze(&mut db);
    for i in 0..db.expressions.len() {
        let expression = &db.expressions[i];
        let value_type = expression_type(&db, &expression.location.file, &expression.syntax);
        db.expressions[i].value_type = value_type;
    }
    db
}

fn validate_utils(db: &mut Database) {
    fn target(db: &Database, file: &FileId, node: &Node) -> Option<SymbolId> {
        let call = db.files[file].call(node)?;
        db.references
            .iter()
            .find(|r| r.location.file == *file && r.location.range == call.name_range)
            .and_then(|r| r.target)
    }
    fn body(db: &mut Database, file: &FileId, util: &Node, seen: &mut BTreeSet<SymbolId>) {
        // Walk the call graph iteratively: semantic validation must remain safe even
        // when generation will subsequently reject an over-deep expansion.
        let owner = Location {
            file: file.clone(),
            range: util.header,
        };
        let mut pending: Vec<_> = util
            .children
            .iter()
            .cloned()
            .map(|node| (file.clone(), node))
            .collect();
        while let Some((file, node)) = pending.pop() {
            let file = &file;
            let node = &node;
            if node.kind == NodeKind::Content
                || (node.kind == NodeKind::Call && node.block.is_some())
            {
                db.error(
                    "syntax.util-content",
                    &owner.file,
                    owner.range,
                    "Utils cannot contain or supply content",
                );
                db.diagnostics.last_mut().unwrap().related.push(Location {
                    file: file.clone(),
                    range: node.header,
                });
            }
            if node.kind == NodeKind::Rule
                && xcss_syntax::selector_classes(&db.files[file], node)
                    .iter()
                    .any(|(_, _, global)| !global)
            {
                db.error(
                    "syntax.util-local-class",
                    &owner.file,
                    owner.range,
                    "Class selectors inside utils must be explicitly :global",
                );
                db.diagnostics.last_mut().unwrap().related.push(Location {
                    file: file.clone(),
                    range: node.header,
                });
            }
            if node.kind == NodeKind::Call
                && let Some(id) = target(db, file, node)
                && seen.insert(id)
            {
                let symbol = &db.symbols[id.0];
                let source = symbol.location.file.clone();
                let range = symbol.location.range;
                let definition = db.files[&source]
                    .tree
                    .descendants()
                    .find(|n| {
                        db.files[&source]
                            .injection(n)
                            .is_some_and(|i| i.name_range == range)
                    })
                    .cloned();
                if let Some(definition) = definition {
                    pending.extend(
                        definition
                            .children
                            .into_iter()
                            .map(|node| (source.clone(), node)),
                    );
                }
            }
            pending.extend(
                node.children
                    .iter()
                    .cloned()
                    .map(|node| (file.clone(), node)),
            );
        }
    }
    fn walk(db: &mut Database, file: &FileId, node: &Node, selector: bool, callable: bool) {
        let is_definition = matches!(node.kind, NodeKind::Util | NodeKind::Injection);
        let selector = if is_definition {
            false
        } else {
            selector || node.kind == NodeKind::Rule
        };
        if node.kind == NodeKind::Util {
            body(db, file, node, &mut BTreeSet::new());
        }
        if node.kind == NodeKind::Call
            && let Some(id) = target(db, file, node)
        {
            let util = db.symbols[id.0].kind == SymbolKind::Util;
            let syntax = db.files[file].call(node).unwrap();
            if syntax.as_injection && !util {
                db.error(
                    "syntax.util-modifier",
                    file,
                    node.header,
                    "as injection is only valid on util calls",
                );
            }
            if util {
                if !selector && !callable {
                    db.error(
                        "syntax.util-selector",
                        file,
                        node.header,
                        "A util call requires a selector",
                    );
                }
                if node.block.is_some() {
                    db.error(
                        "syntax.util-content",
                        file,
                        node.header,
                        "Utils do not accept content blocks",
                    );
                }
            }
        }
        for (index, child) in node.children.iter().enumerate() {
            if child.kind == NodeKind::Call
                && target(db, file, child)
                    .is_some_and(|id| db.symbols[id.0].kind == SymbolKind::Util)
                && node.children[..index]
                    .iter()
                    .any(|n| n.kind == NodeKind::Declaration)
                && node.children[index + 1..]
                    .iter()
                    .any(|n| n.kind == NodeKind::Declaration)
            {
                db.error(
                    "syntax.util-order",
                    file,
                    child.header,
                    "Util calls must precede or follow all properties",
                );
            }
            walk(db, file, child, selector, callable || is_definition);
        }
    }
    let files: Vec<_> = db
        .files
        .iter()
        .map(|(id, p)| (id.clone(), p.clone()))
        .collect();
    for (file, parse) in files {
        walk(db, &file, &parse.tree, false, false);
    }
}

fn ambiguous_arithmetic(value: &str) -> bool {
    if value.contains('(') {
        return false;
    }
    let words: Vec<_> = value.split_whitespace().collect();
    words.len() > 3
        && words.windows(2).any(|w| {
            !matches!(w[0], "+" | "-" | "*" | "/") && !matches!(w[1], "+" | "-" | "*" | "/")
        })
}
fn expression_type(db: &Database, file: &FileId, value: &Value) -> ValueType {
    match &value.kind {
        ValueKind::Parameter(_) | ValueKind::CustomProperty(_) => db
            .references
            .iter()
            .find(|r| r.location.file == *file && r.location.range == value.range)
            .and_then(|r| r.target)
            .map(|id| db.symbols[id.0].value_type.clone())
            .unwrap_or(ValueType::Unknown),
        ValueKind::Group(values) | ValueKind::Sequence(values) => {
            let direct = infer_type(db.files[file].slice(value.range));
            if direct != ValueType::Unknown {
                return direct;
            }
            if values.len() == 1 {
                return expression_type(db, file, &values[0]);
            }
            fn arithmetic_type(
                db: &Database,
                file: &FileId,
                tree: &xcss_syntax::ast::ArithmeticExpression,
            ) -> ValueType {
                use xcss_syntax::ast::ArithmeticKind;
                match &tree.kind {
                    ArithmeticKind::Operand(v) => {
                        if types::numeric_zero(
                            &ValueType::named("integer"),
                            db.files[file].slice(v.range),
                        ) {
                            ValueType::named("integer")
                        } else {
                            expression_type(db, file, v)
                        }
                    }
                    ArithmeticKind::Unary { operand, .. } => arithmetic_type(db, file, operand),
                    ArithmeticKind::Binary {
                        operator,
                        left,
                        right,
                    } => {
                        let a = arithmetic_type(db, file, left);
                        let b = arithmetic_type(db, file, right);
                        let numeric = |t: &ValueType| matches!(t, ValueType::Named(n) if n == "number" || n == "integer");
                        let dimensional = |t: &ValueType| matches!(t, ValueType::Named(n) if matches!(n.as_str(), "length" | "length-percentage" | "percentage" | "angle" | "time" | "frequency" | "resolution" | "flex"));
                        let length_percentage = |t: &ValueType| matches!(t, ValueType::Named(n) if matches!(n.as_str(), "length" | "percentage" | "length-percentage"));
                        if a == ValueType::Unknown || b == ValueType::Unknown {
                            return ValueType::Unknown;
                        }
                        match operator {
                            '+' | '-' if a == b && (numeric(&a) || dimensional(&a)) => a,
                            '+' | '-' if numeric(&a) && numeric(&b) => ValueType::named("number"),
                            '+' | '-' if length_percentage(&a) && length_percentage(&b) => {
                                ValueType::named("length-percentage")
                            }
                            '*' if numeric(&b) && (numeric(&a) || dimensional(&a)) => a,
                            '*' if numeric(&a) && dimensional(&b) => b,
                            '/' if numeric(&b) && (numeric(&a) || dimensional(&a)) => {
                                if numeric(&a) {
                                    ValueType::named("number")
                                } else {
                                    a
                                }
                            }
                            _ => ValueType::Unknown,
                        }
                    }
                }
            }
            value
                .arithmetic()
                .map(|tree| arithmetic_type(db, file, &tree))
                .unwrap_or(ValueType::Unknown)
        }
        ValueKind::Function { name, .. } => {
            let lower = name.to_ascii_lowercase();
            if lower.ends_with("gradient") {
                ValueType::named("image")
            } else if matches!(
                lower.as_str(),
                "rgb"
                    | "rgba"
                    | "hsl"
                    | "hsla"
                    | "hwb"
                    | "lab"
                    | "lch"
                    | "oklab"
                    | "oklch"
                    | "color"
                    | "color-mix"
            ) {
                ValueType::named("color")
            } else {
                infer_type(db.files[file].slice(value.range))
            }
        }
        _ => infer_type(db.files[file].slice(value.range)),
    }
}
fn validate_transition(db: &mut Database, parse: &ParseResult, value: &Value) {
    for range in parse.split_ranges(value.range, ",") {
        let item = parse.value(range);
        let ValueKind::Sequence(parts) = item.kind else {
            continue;
        };
        for (index, part) in parts.iter().enumerate() {
            if let ValueKind::Group(properties) = &part.kind {
                if index != 0
                    || properties.is_empty()
                    || properties.iter().any(|p| {
                        !matches!(p.kind, ValueKind::Literal | ValueKind::CustomProperty(_))
                            || parse
                                .slice(p.range)
                                .contains(|c: char| c.is_whitespace() || c == '(')
                    })
                {
                    db.error("type.transition-group",&parse.source.id,part.range,"Transition groups must contain property names in the property-first position");
                }
                let text = parse.slice(part.range);
                if text.contains(",,") || text.trim_end_matches(')').trim_end().ends_with(',') {
                    db.error(
                        "type.transition-group",
                        &parse.source.id,
                        part.range,
                        "Expected a property name between commas",
                    );
                }
            }
        }
        if parts
            .first()
            .is_some_and(|v| matches!(v.kind, ValueKind::CustomProperty(_)))
            && parts
                .get(1)
                .is_some_and(|v| matches!(v.kind, ValueKind::CustomProperty(_)))
        {
            db.error(
                "type.ambiguous-transition",
                &parse.source.id,
                range,
                "Group the transition property explicitly to distinguish it from timing values",
            );
        }
    }
}
fn is_transition_property_name(parse: &ParseResult, range: TextRange) -> bool {
    for node in parse
        .tree
        .descendants()
        .filter(|n| n.kind == NodeKind::Declaration && n.header.contains(range.start))
    {
        let Some(declaration) = parse.declaration(node) else {
            continue;
        };
        if declaration
            .property
            .eq_ignore_ascii_case("transition-property")
        {
            return true;
        }
        if !declaration.property.eq_ignore_ascii_case("transition") {
            continue;
        }
        for item_range in parse.split_ranges(declaration.value.range, ",") {
            let item = parse.value(item_range);
            let ValueKind::Sequence(parts) = item.kind else {
                continue;
            };
            if let Some(first) = parts.first() {
                if let ValueKind::Group(_) = first.kind
                    && first.range.contains(range.start)
                {
                    return true;
                }
                // A bare custom name followed by a literal time is a property.
                if matches!(first.kind, ValueKind::CustomProperty(_))
                    && parts.get(1).is_some_and(|v| {
                        infer_type(parse.slice(v.range)) == ValueType::named("time")
                    })
                    && first.range == range
                {
                    return true;
                }
            }
        }
    }
    false
}
fn selector_classes(parse: &ParseResult, node: &Node) -> Vec<(String, TextRange, bool)> {
    xcss_syntax::selector_classes(parse, node)
}
fn validate_call(db: &mut Database, call: &Call, target: SymbolId) {
    if !call.node.missing.is_empty() {
        return;
    }
    let signature = db.signatures[&target].clone();
    let parse = db.files[&call.file].clone();
    let Some(syntax) = parse.call(&call.node) else {
        return;
    };
    let args = syntax.arguments;
    let required = signature
        .parameters
        .iter()
        .filter(|p| p.default.is_none())
        .count();
    if args.len() < required || args.len() > signature.parameters.len() {
        db.error(
            "type.argument-count",
            &call.file,
            call.node.header,
            format!(
                "Expected {required}..{} arguments, got {}",
                signature.parameters.len(),
                args.len()
            ),
        );
    }
    for (value, param) in args.iter().zip(&signature.parameters) {
        let actual = expression_type(db, &call.file, value);
        if !param.value_type.accepts(&actual)
            && !types::numeric_zero(&param.value_type, parse.slice(value.range))
        {
            db.error(
                "type.argument-mismatch",
                &call.file,
                call.node.header,
                format!("Argument does not match {:?}", param.value_type),
            );
        }
    }
    if call.node.block.is_some() && !signature.accepts_content {
        db.error(
            "type.unexpected-content",
            &call.file,
            call.node.header,
            "Injection has no content slot",
        );
    }
}
fn analyze_references(db: &mut Database, state: &Collected, parse: &ParseResult) {
    let file = &parse.source.id;
    for node in parse.tree.descendants() {
        let Some(declaration) = parse.declaration(node) else {
            continue;
        };
        if !matches!(
            declaration.property.to_ascii_lowercase().as_str(),
            "animation" | "animation-name" | "-webkit-animation" | "-webkit-animation-name"
        ) {
            continue;
        }
        let ValueKind::Sequence(parts) = declaration.value.kind else {
            continue;
        };
        for value in parts
            .iter()
            .filter(|v| matches!(v.kind, ValueKind::Literal))
        {
            let name = decode_identifier(parse.slice(value.range).trim_matches(['\'', '"']));
            let target = db
                .symbols
                .iter()
                .find(|s| {
                    s.kind == SymbolKind::Keyframe && s.location.file == *file && s.name == name
                })
                .map(|s| s.id);
            if let Some(target) = target {
                db.references.push(Reference {
                    role: ReferenceRole::Read,
                    name,
                    location: Location {
                        file: file.clone(),
                        range: value.range,
                    },
                    target: Some(target),
                });
            }
        }
    }
    let tokens: Vec<_> = parse
        .tokens
        .iter()
        .filter(|t| {
            !matches!(
                t.kind,
                TokenKind::Comment | TokenKind::Whitespace | TokenKind::String
            )
        })
        .collect();
    for (i, token) in tokens.iter().enumerate() {
        let raw = &parse.source.text()[token.range.start..token.range.end];
        let (name, range) = if raw == "$" {
            if let Some(next) = tokens.get(i + 1) {
                (
                    format!(
                        "${}",
                        decode_identifier(&parse.source.text()[next.range.start..next.range.end])
                    ),
                    TextRange::new(token.range.start, next.range.end),
                )
            } else {
                continue;
            }
        } else if raw.starts_with("--") {
            (decode_identifier(raw), token.range)
        } else {
            continue;
        };
        if db
            .symbols
            .iter()
            .any(|s| s.location.file == *file && s.location.range == range)
        {
            continue;
        }
        if is_transition_property_name(parse, range) {
            continue;
        }
        if name.starts_with('$') && !reference_value_position(parse, range) {
            db.error("binding.constant-position", file, range,
                "Constants and parameters can only be referenced in values and supported at-rule conditions");
        }
        // Scoped custom-property assignments bind to the same module symbol.
        // Ordinary CSS custom-property declarations remain global.
        if !name.starts_with('$')
            && tokens
                .get(i + 1)
                .is_some_and(|t| &parse.source.text()[t.range.start..t.range.end] == ":")
            && db.local(file, &name, SymbolKind::Variable).is_none()
        {
            continue;
        }
        let scope = state
            .scopes
            .iter()
            .filter(|((f, start), scope)| {
                f == file
                    && *start <= range.start
                    && db.scopes[scope.0].location.range.end >= range.end
            })
            .max_by_key(|((_, start), _)| *start)
            .map(|(_, scope)| *scope);
        let target = if name.starts_with('$') {
            scope.and_then(|scope| db.lexical_at(scope, &name, range.start))
        } else {
            db.local(file, &name, SymbolKind::Variable)
        };
        if name.starts_with('$') && target.is_none() {
            db.error(
                "binding.parameter-scope",
                file,
                range,
                format!("Unknown or not yet declared constant or injection parameter `{name}`"),
            );
        }
        db.references.push(Reference {
            role: ReferenceRole::Read,
            name,
            location: Location {
                file: file.clone(),
                range,
            },
            target,
        });
    }
}
fn reference_value_position(parse: &ParseResult, range: TextRange) -> bool {
    let contains = |r: TextRange| r.start <= range.start && r.end >= range.end;
    for node in parse.tree.descendants().filter(|n| contains(n.header)) {
        let value = if let Some(c) = parse.constant(node) {
            Some(c.initial)
        } else {
            parse.declaration(node).map(|d| d.value)
        };
        if let Some(value) = value.filter(|v| contains(v.range)) {
            return !value.descendants().iter().any(|v| {
                matches!(&v.kind,
                ValueKind::Function { name, .. } if name.eq_ignore_ascii_case("url"))
                    && contains(v.range)
            });
        }
        if parse
            .call(node)
            .is_some_and(|c| c.arguments.iter().any(|v| contains(v.range)))
        {
            return true;
        }
        if parse.injection(node).is_some_and(|i| {
            i.parameters.iter().any(|p| {
                p.default.is_some_and(|v| {
                    let start = v.as_ptr() as usize - parse.source.text().as_ptr() as usize;
                    contains(TextRange::new(start, start + v.len()))
                })
            })
        }) {
            return true;
        }
        if node.kind == NodeKind::AtRule {
            let keyword = parse
                .slice(node.header)
                .split(|c: char| c.is_whitespace() || c == '(')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(keyword.as_str(), "@media" | "@supports" | "@container") {
                return true;
            }
        }
    }
    false
}
fn analyze_compositions(
    db: &mut Database,
    compositions: &[Composition],
    resolutions: &BTreeMap<(FileId, String), Option<FileId>>,
) {
    let mut edges = Vec::new();
    let mut seen = BTreeMap::<(SymbolId, String), SymbolId>::new();
    for composition in compositions {
        let parse = db.files[&composition.file].clone();
        let Some(syntax) = parse.composition(&composition.node) else {
            continue;
        };
        if composition.owners.len() != 1 {
            db.error(
                "binding.composition-owner",
                &composition.file,
                composition.node.header,
                "Composition requires a single local class",
            );
            continue;
        }
        if syntax.source == xcss_syntax::ast::CompositionSource::Global {
            let mut members = Vec::new();
            for (name, range) in syntax.names {
                members.push(CompositionMember {
                    name: decode_identifier(name),
                    target: None,
                    location: Location {
                        file: composition.file.clone(),
                        range,
                    },
                });
                db.references.push(Reference {
                    role: ReferenceRole::Read,
                    name: decode_identifier(name),
                    location: Location {
                        file: composition.file.clone(),
                        range,
                    },
                    target: None,
                });
            }
            db.compositions.push(ResolvedComposition {
                owner: composition.owners[0],
                members,
            });
            continue;
        }
        let target_file = if let xcss_syntax::ast::CompositionSource::Module(from) = syntax.source {
            resolutions
                .get(&(composition.file.clone(), from.into()))
                .and_then(Clone::clone)
                .filter(|f| db.files.contains_key(f))
        } else {
            Some(composition.file.clone())
        };
        let Some(target_file) = target_file else {
            db.error(
                "binding.unresolved-import",
                &composition.file,
                composition.node.header,
                "Cannot resolve composition source",
            );
            continue;
        };
        db.dependencies.push(Dependency {
            from: composition.file.clone(),
            to: target_file.clone(),
            kind: DependencyKind::Composition,
        });
        let mut members = Vec::new();
        for (raw_name, range) in syntax.names {
            let decoded = decode_identifier(raw_name);
            let name = decoded.as_str();
            let target = db.local(&target_file, name, SymbolKind::Class);
            members.push(CompositionMember {
                name: name.into(),
                target,
                location: Location {
                    file: composition.file.clone(),
                    range,
                },
            });
            db.references.push(Reference {
                role: ReferenceRole::Read,
                name: name.into(),
                location: Location {
                    file: composition.file.clone(),
                    range,
                },
                target,
            });
            if let Some(target) = target {
                let owner = composition.owners[0];
                if seen
                    .insert((owner, name.into()), target)
                    .is_some_and(|old| old != target)
                {
                    db.error(
                        "binding.composition-conflict",
                        &composition.file,
                        range,
                        "Conflicting composed class",
                    );
                }
                edges.push((owner, target));
            } else {
                db.error(
                    "binding.missing-export",
                    &composition.file,
                    range,
                    format!("Class `{name}` is not exported"),
                );
            }
        }
        db.compositions.push(ResolvedComposition {
            owner: composition.owners[0],
            members,
        });
    }
    check_cycles(db, &edges, "binding.composition-cycle");
}
fn check_cycles(db: &mut Database, edges: &[(SymbolId, SymbolId)], code: &'static str) {
    for symbol in edges.iter().map(|(from, _)| *from).collect::<BTreeSet<_>>() {
        let mut stack: Vec<_> = edges
            .iter()
            .filter(|(from, _)| *from == symbol)
            .map(|(_, to)| *to)
            .collect();
        let mut seen = BTreeSet::new();
        while let Some(next) = stack.pop() {
            if next == symbol {
                let location = db.symbols[symbol.0].location.clone();
                db.error(
                    code,
                    &location.file,
                    location.range,
                    "Recursive dependency cycle",
                );
                break;
            }
            if seen.insert(next) {
                stack.extend(
                    edges
                        .iter()
                        .filter(|(from, _)| *from == next)
                        .map(|(_, to)| *to),
                );
            }
        }
    }
}
