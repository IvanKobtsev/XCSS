use super::*;

pub(super) fn collect(
    db: &mut Database,
    state: &mut Collected,
    parse: &ParseResult,
    node: &Node,
    scope: ScopeId,
    owner: Option<SymbolId>,
    classes: &[SymbolId],
) {
    let file = &parse.source.id;
    let mut child_scope = scope;
    let mut child_owner = owner;
    let mut child_classes = classes.to_vec();
    state.scopes.insert((file.clone(), node.range.start), scope);
    match node.kind {
        NodeKind::Vars => {
            if db.scopes[scope.0].parent.is_some() {
                db.error(
                    "syntax.vars-placement",
                    file,
                    node.header,
                    "@vars must be top-level",
                );
            }
        }
        NodeKind::Variable => {
            let Some(variable) = parse.variable(node) else {
                return;
            };
            let decoded = decode_identifier(variable.name);
            let name = decoded.as_str();
            let range = variable.range;
            if !name.starts_with("--") {
                db.error(
                    "syntax.variable",
                    file,
                    range,
                    "Expected custom property name",
                );
            }
            let value_type = if variable.annotation.is_none() && variable.initial.is_none() {
                ValueType::Unknown
            } else {
                checked_type(db, file, range, variable.annotation, variable.initial)
            };
            if value_type != ValueType::Unknown
                && let Some(initial) = variable.initial
                && (!registration_initial_is_independent(initial)
                    || !xcss_syntax::valid_css_value(initial))
            {
                db.error(
                    "type.property-initial",
                    file,
                    range,
                    "Typed @property initial values must be valid and computationally independent",
                );
            }
            db.declare(
                Location {
                    file: file.clone(),
                    range,
                },
                name.into(),
                SymbolKind::Variable,
                scope,
                value_type,
                false,
            );
        }
        NodeKind::Injection => {
            let Some(injection) = parse.injection(node) else {
                return;
            };
            let decoded = decode_identifier(injection.name);
            let name = decoded.as_str();
            if db.scopes[scope.0].parent.is_some() {
                db.error(
                    "syntax.injection-placement",
                    file,
                    node.header,
                    "Injection definitions must be top-level",
                );
            }
            let id = db.declare(
                Location {
                    file: file.clone(),
                    range: injection.name_range,
                },
                name.into(),
                SymbolKind::Injection,
                scope,
                ValueType::Unknown,
                false,
            );
            child_owner = Some(id);
            child_scope = db.scope(file, node.range, Some(scope));
            let mut parameters = Vec::new();
            let mut optional_seen = false;
            for parameter in &injection.parameters {
                let decoded = decode_identifier(parameter.name);
                let pname = decoded.as_str();
                let range = parameter.range;
                if !pname.starts_with('$') {
                    db.error(
                        "syntax.parameter",
                        file,
                        range,
                        "Injection parameters must start with $",
                    );
                }
                let initial = parameter.default;
                let value_type = checked_type(db, file, range, parameter.annotation, initial);
                if optional_seen && initial.is_none() {
                    db.error(
                        "type.parameter-order",
                        file,
                        range,
                        "Required parameter follows a defaulted parameter",
                    );
                }
                optional_seen |= initial.is_some();
                let symbol = db.declare(
                    Location {
                        file: file.clone(),
                        range,
                    },
                    pname.into(),
                    SymbolKind::Parameter,
                    child_scope,
                    value_type.clone(),
                    false,
                );
                parameters.push(Parameter {
                    symbol,
                    value_type,
                    default: initial.map(str::to_owned),
                });
            }
            let accepts_content = node.descendants().any(|n| n.kind == NodeKind::Content);
            db.signatures.insert(
                id,
                InjectionSignature {
                    symbol: id,
                    parameters,
                    accepts_content,
                },
            );
        }
        NodeKind::Import => {
            if let Some(import) = parse.injection_import(node) {
                let specifier = import.specifier.to_owned();
                let namespace = import.namespace.map(decode_identifier);
                let namespace_range = import.namespace.map(|name| {
                    let start = name.as_ptr() as usize - parse.source.text().as_ptr() as usize;
                    TextRange::new(start, start + name.len())
                });
                let names = import
                    .names
                    .into_iter()
                    .map(|(name, range)| (decode_identifier(name), range))
                    .collect();
                state.imports.push(Import {
                    file: file.clone(),
                    range: node.header,
                    namespace,
                    namespace_range,
                    names,
                    specifier,
                });
            }
        }
        NodeKind::Call => {
            state.calls.push(Call {
                file: file.clone(),
                node: node.clone(),
                scope,
                owner,
            });
            if node.block.is_some() {
                child_scope = db.scope(file, node.range, Some(scope));
            }
        }
        NodeKind::AtRule if node.block.is_some() => {
            child_scope = db.scope(file, node.range, Some(scope));
            if let Some(keyframes) = parse.keyframes(node) {
                let name = decode_identifier(keyframes.name);
                if let Some(existing) = db
                    .symbols
                    .iter()
                    .find(|s| {
                        s.kind == SymbolKind::Keyframe
                            && s.location.file == *file
                            && s.name == name
                            && s.global == keyframes.global
                    })
                    .map(|s| s.id)
                {
                    db.references.push(Reference {
                        role: ReferenceRole::Declaration,
                        name,
                        location: Location {
                            file: file.clone(),
                            range: keyframes.name_range,
                        },
                        target: Some(existing),
                    });
                } else {
                    db.declare(
                        Location {
                            file: file.clone(),
                            range: keyframes.name_range,
                        },
                        decode_identifier(keyframes.name),
                        SymbolKind::Keyframe,
                        scope,
                        ValueType::Unknown,
                        keyframes.global,
                    );
                }
            }
        }
        NodeKind::Content if owner.is_none() => db.error(
            "binding.content-scope",
            file,
            node.header,
            "@content is only valid inside an injection",
        ),
        NodeKind::Rule => {
            child_scope = db.scope(file, node.range, Some(scope));
            child_classes.clear();
            for (name, range, global) in selector_classes(parse, node) {
                let existing = db
                    .symbols
                    .iter()
                    .find(|s| {
                        s.kind == SymbolKind::Class
                            && s.location.file == *file
                            && s.name == name
                            && s.global == global
                    })
                    .map(|s| s.id);
                let id = if let Some(id) = existing {
                    db.references.push(Reference {
                        role: ReferenceRole::Declaration,
                        name,
                        location: Location {
                            file: file.clone(),
                            range,
                        },
                        target: Some(id),
                    });
                    id
                } else {
                    db.declare(
                        Location {
                            file: file.clone(),
                            range,
                        },
                        name,
                        SymbolKind::Class,
                        scope,
                        ValueType::Unknown,
                        global,
                    )
                };
                if !global && !child_classes.contains(&id) {
                    child_classes.push(id);
                }
            }
            if child_classes.is_empty() {
                child_classes.extend_from_slice(classes);
            }
        }
        NodeKind::Declaration => {
            if let Some(declaration) = parse.declaration(node) {
                let normalized_property = declaration.property.to_ascii_lowercase();
                let property = normalized_property.as_str();
                let value = parse.slice(declaration.value.range);
                if property.trim() == "composes" {
                    state.compositions.push(Composition {
                        file: file.clone(),
                        node: node.clone(),
                        owners: classes.to_vec(),
                    });
                }
                let slash = !matches!(
                    property.trim(),
                    "font"
                        | "font-family"
                        | "border-radius"
                        | "background"
                        | "background-position"
                        | "background-size"
                        | "grid-area"
                        | "grid-row"
                        | "grid-column"
                );
                let arithmetic = declaration
                    .value
                    .descendants()
                    .iter()
                    .any(|v| match v.kind {
                        ValueKind::Operator('/') => slash,
                        ValueKind::Operator(_) => true,
                        _ => false,
                    });
                let transition_group = property.trim() == "transition"
                    && declaration
                        .value
                        .descendants()
                        .iter()
                        .any(|v| matches!(v.kind, ValueKind::Group(_)));
                let kind = if property.trim() == "content" && value.trim() == "preserve" {
                    ExpressionKind::ContentDirective
                } else if transition_group {
                    ExpressionKind::TransitionGroup
                } else if arithmetic {
                    ExpressionKind::Arithmetic
                } else {
                    ExpressionKind::Value
                };
                if property.trim() == "transition" {
                    validate_transition(db, parse, &declaration.value);
                }
                db.expressions.push(Expression {
                    arithmetic: if arithmetic {
                        declaration.value.arithmetic()
                    } else {
                        None
                    },
                    location: Location {
                        file: file.clone(),
                        range: declaration.value.range,
                    },
                    value_type: infer_type(value),
                    kind,
                    syntax: declaration.value,
                });
                if kind == ExpressionKind::Arithmetic && ambiguous_arithmetic(value) {
                    db.error(
                        "type.ambiguous-arithmetic",
                        file,
                        node.header,
                        "Group arithmetic when mixed with whitespace-separated values",
                    );
                }
            }
        }
        _ => {}
    }
    state
        .scopes
        .insert((file.clone(), node.range.start), child_scope);
    for child in &node.children {
        collect(
            db,
            state,
            parse,
            child,
            child_scope,
            child_owner,
            &child_classes,
        );
    }
}
