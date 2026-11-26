use super::*;

/// Resolve expression types in dependency order, independently of file ordering.
pub(super) fn analyze_types(db: &mut Database, parsed: &[Arc<ParseResult>]) {
    let mut values = BTreeMap::<SymbolId, (Vec<Value>, Option<String>)>::new();
    for parse in parsed {
        for node in parse.tree.descendants() {
            if let Some(f) = parse.function(node)
                && let Some(id) = db.local(
                    &parse.source.id,
                    &decode_identifier(f.signature.name),
                    SymbolKind::Function,
                )
            {
                values.insert(
                    id,
                    (parse.values(node), f.return_annotation.map(str::to_owned)),
                );
            }
            if let Some(c) = parse.constant(node)
                && let Some(s) = db
                    .symbols
                    .iter()
                    .find(|s| s.location.file == parse.source.id && s.location.range == c.range)
            {
                values.insert(s.id, (vec![c.initial], c.annotation.map(str::to_owned)));
            }
            if let Some(i) = parse.injection(node) {
                for p in i.parameters {
                    if let Some(default) = p.default {
                        let start =
                            default.as_ptr() as usize - parse.source.text().as_ptr() as usize;
                        if let Some(s) = db.symbols.iter().find(|s| {
                            s.location.file == parse.source.id && s.location.range == p.range
                        }) {
                            values.insert(
                                s.id,
                                (
                                    vec![parse.value(TextRange::new(start, start + default.len()))],
                                    p.annotation.map(str::to_owned),
                                ),
                            );
                        }
                    }
                }
            }
            if let Some(v) = parse.variable(node)
                && let Some(initial) = v.initial.filter(|v| v.contains("@:"))
            {
                let start = initial.as_ptr() as usize - parse.source.text().as_ptr() as usize;
                if let Some(s) = db
                    .symbols
                    .iter()
                    .find(|s| s.location.file == parse.source.id && s.location.range == v.range)
                {
                    values.insert(
                        s.id,
                        (
                            vec![parse.value(TextRange::new(start, start + initial.len()))],
                            v.annotation.map(str::to_owned),
                        ),
                    );
                }
            }
        }
    }
    fn visit(
        db: &mut Database,
        values: &BTreeMap<SymbolId, (Vec<Value>, Option<String>)>,
        id: SymbolId,
        active: &mut BTreeSet<SymbolId>,
        done: &mut BTreeSet<SymbolId>,
    ) {
        if done.contains(&id) {
            return;
        }
        if !active.insert(id) {
            let location = db.symbols[id.0].location.clone();
            db.error(
                "binding.function-cycle",
                &location.file,
                location.range,
                "Cyclic function, constant, or default expression",
            );
            return;
        }
        if let Some((roots, annotation)) = values.get(&id) {
            let symbol = db.symbols[id.0].clone();
            let dependencies: Vec<_> = db
                .references
                .iter()
                .filter(|r| {
                    r.location.file == symbol.location.file
                        && roots.iter().any(|v| {
                            v.range.start <= r.location.range.start
                                && v.range.end >= r.location.range.end
                        })
                })
                .filter_map(|r| r.target)
                .collect();
            for dependency in dependencies {
                visit(db, values, dependency, active, done);
            }
            let actual = expression_type(db, &symbol.location.file, &roots[0]);
            let expected = annotation
                .as_ref()
                .map(|a| {
                    ValueType::union(
                        types::without_comments(a)
                            .split('|')
                            .map(str::trim)
                            .map(ValueType::named),
                    )
                })
                .unwrap_or_else(|| actual.clone());
            if symbol.kind == SymbolKind::Function {
                let expected = if annotation.is_some() {
                    types::checked_parameter_type(
                        db,
                        &symbol.location.file,
                        roots[0].range,
                        annotation.as_deref(),
                        None,
                    )
                } else {
                    expected.clone()
                };
                if !expected.accepts(&actual)
                    && !types::numeric_zero(
                        &expected,
                        db.files[&symbol.location.file].slice(roots[0].range),
                    )
                {
                    db.error(
                        "type.return-mismatch",
                        &symbol.location.file,
                        roots[0].range,
                        format!("Expected return type {expected}, found {actual}"),
                    );
                }
                constants::validate_arithmetic(db, &symbol.location.file, &roots[0], false);
                db.symbols[id.0].value_type = expected;
            } else {
                if symbol.kind == SymbolKind::Variable {
                    if annotation.is_none() && actual == ValueType::Unknown {
                        db.error(
                            "type.cannot-infer",
                            &symbol.location.file,
                            roots[0].range,
                            "Specify a type when the initial value cannot be inferred",
                        );
                    } else if !expected.accepts(&actual) {
                        db.error(
                            "type.mismatch",
                            &symbol.location.file,
                            roots[0].range,
                            format!("Expected {expected}, found {actual}"),
                        );
                    }
                }
                db.symbols[id.0].value_type = expected.clone();
                for signature in db.signatures.values_mut() {
                    for parameter in &mut signature.parameters {
                        if parameter.symbol == id {
                            parameter.value_type = expected.clone();
                        }
                    }
                }
            }
        }
        active.remove(&id);
        done.insert(id);
    }
    let mut done = BTreeSet::new();
    for id in values.keys() {
        visit(db, &values, *id, &mut BTreeSet::new(), &mut done);
    }
}
