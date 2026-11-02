use super::*;

pub(super) fn analyze_constants(db: &mut Database, parsed: &[Arc<ParseResult>]) {
    for parse in parsed {
        let file = &parse.source.id;
        for node in parse.tree.descendants() {
            if let Some(injection) = parse.injection(node) {
                for parameter in injection.parameters {
                    let Some(default) = parameter.default.filter(|v| v.contains('$')) else {
                        continue;
                    };
                    let start = default.as_ptr() as usize - parse.source.text().as_ptr() as usize;
                    let value = parse.value(TextRange::new(start, start + default.len()));
                    let actual = expression_type(db, file, &value);
                    let expected = parameter
                        .annotation
                        .map(|a| checked_type(db, file, parameter.range, Some(a), None))
                        .unwrap_or_else(|| actual.clone());
                    if parameter.annotation.is_none() && actual == ValueType::Unknown {
                        db.error(
                            "type.cannot-infer",
                            file,
                            parameter.range,
                            "Cannot infer parameter default type",
                        );
                    } else if !expected.accepts(&actual) {
                        db.error(
                            "type.mismatch",
                            file,
                            value.range,
                            format!("Expected {expected:?}, found {actual:?}"),
                        );
                    }
                    if let Some(id) = db
                        .symbols
                        .iter()
                        .find(|s| s.location.file == *file && s.location.range == parameter.range)
                        .map(|s| s.id)
                    {
                        db.symbols[id.0].value_type = expected.clone();
                        for signature in db.signatures.values_mut() {
                            for p in &mut signature.parameters {
                                if p.symbol == id {
                                    p.value_type = expected.clone();
                                }
                            }
                        }
                    }
                }
            }
            let Some(constant) = parse.constant(node) else {
                continue;
            };
            let Some(id) = db
                .symbols
                .iter()
                .find(|s| s.location.file == *file && s.location.range == constant.range)
                .map(|s| s.id)
            else {
                continue;
            };
            let symbol = db.symbols[id.0].clone();
            if let Some(old) = db.lexical_at(symbol.scope, &symbol.name, constant.range.start) {
                db.error(
                    "binding.constant-redeclaration",
                    file,
                    constant.range,
                    format!("Cannot redeclare or shadow `{}`", symbol.name),
                );
                db.diagnostics
                    .last_mut()
                    .unwrap()
                    .related
                    .push(db.symbols[old.0].location.clone());
            }
            let actual = expression_type(db, file, &constant.initial);
            validate_constant_arithmetic(db, file, &constant.initial);
            let expected = if let Some(annotation) = constant.annotation {
                checked_type(db, file, constant.range, Some(annotation), None)
            } else {
                actual.clone()
            };
            if constant.annotation.is_none() && actual == ValueType::Unknown {
                db.error(
                    "type.cannot-infer",
                    file,
                    constant.range,
                    "Specify a type when the constant value cannot be inferred",
                );
            } else if !expected.accepts(&actual) {
                db.error(
                    "type.mismatch",
                    file,
                    constant.initial.range,
                    format!("Expected {expected:?}, found {actual:?}"),
                );
            }
            db.symbols[id.0].value_type = expected;
        }
        let parameters: Vec<_> = db
            .symbols
            .iter()
            .filter(|s| s.location.file == *file && s.kind == SymbolKind::Parameter)
            .cloned()
            .collect();
        for parameter in parameters {
            if let Some(parent) = db.scopes[parameter.scope.0].parent
                && db
                    .lexical_at(parent, &parameter.name, parameter.location.range.start)
                    .is_some()
            {
                db.error(
                    "binding.constant-redeclaration",
                    file,
                    parameter.location.range,
                    format!("Cannot shadow `{}`", parameter.name),
                );
            }
        }
    }
}
fn validate_constant_arithmetic(db: &mut Database, file: &FileId, value: &Value) {
    use xcss_syntax::ast::{ArithmeticExpression, ArithmeticKind};
    fn known_operands(db: &Database, file: &FileId, tree: &ArithmeticExpression) -> bool {
        match &tree.kind {
            ArithmeticKind::Operand(value) => {
                expression_type(db, file, value) != ValueType::Unknown
            }
            ArithmeticKind::Unary { operand, .. } => known_operands(db, file, operand),
            ArithmeticKind::Binary { left, right, .. } => {
                known_operands(db, file, left) && known_operands(db, file, right)
            }
        }
    }
    for part in value.descendants() {
        if matches!(part.kind, ValueKind::Sequence(_) | ValueKind::Group(_))
            && let Some(tree) = part.arithmetic()
            && matches!(
                tree.kind,
                ArithmeticKind::Binary { .. } | ArithmeticKind::Unary { .. }
            )
            && known_operands(db, file, &tree)
            && expression_type(db, file, part) == ValueType::Unknown
        {
            db.error(
                "type.arithmetic",
                file,
                part.range,
                "Incompatible types in constant arithmetic",
            );
            break;
        }
    }
}
