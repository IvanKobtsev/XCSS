use std::collections::BTreeMap;

use tsrs_ast::{self as ast, Kind, Node, SourceFile, Symbol, SymbolFlags};
use tsrs_checker::{Checker, LiteralValue, Type, TypeFlags};
use tsrs_compiler::{Context, Program};
use tsrs_core::P;
use xcss_analysis::{
    AnalysisSnapshot, DIMENSION_UNITS, ProvidedVariable, VALUE_KINDS, ValueType, infer_type,
};
use xcss_source::{Diagnostic, FileId, TextRange};

use crate::Project;
use crate::diagnostics::{node_range, project_file, provided_member_error};

pub(crate) const CONTRACT_PATH: &str = "/node_modules/.xcss-internal/provided-values.d.ts";
pub(crate) fn declaration() -> String {
    let mut text = String::new();
    for &kind in VALUE_KINDS {
        let expected = ValueType::named(kind);
        let brands = VALUE_KINDS
            .iter()
            .filter(|name| expected.accepts(&ValueType::named(name)))
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(" | ");
        let mut types = vec![format!(
            "(string & {{ readonly __xcssCssValue__: {brands} }})"
        )];
        for &(dimension, units) in DIMENSION_UNITS {
            if expected.accepts(&ValueType::named(dimension)) {
                types.extend(units.iter().map(|unit| format!("`${{number}}{unit}`")));
            }
        }
        if kind == "number" {
            types.extend(["number".into(), "`${number}`".into()]);
        }
        text.push_str(&format!(
            "export type X_{} = {};\n",
            kind.replace('-', "_"),
            types.join(" | ")
        ));
    }
    text
}

pub(crate) fn check(
    program: &Program,
    project: &Project,
    snapshot: &AnalysisSnapshot,
) -> Vec<Diagnostic> {
    let variables = snapshot
        .files()
        .filter_map(|file| snapshot.module_interface(file))
        .flat_map(|interface| interface.provided_variables.values())
        .map(|variable| (variable.scoped_name.as_str(), variable))
        .collect();
    let contracts = program
        .get_source_file(CONTRACT_PATH)
        .map(|file| {
            file.statements()
                .nodes()
                .iter()
                .filter_map(|node| {
                    let alias = node.as_type_alias_declaration();
                    Some((
                        alias.name().text().strip_prefix("X_")?.replace('_', "-"),
                        alias.type_()?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut diagnostics = Vec::new();
    let context = Context::default();
    for &file in program.source_files() {
        let name = file.file_name();
        let javascript = [".js", ".jsx", ".mjs", ".cjs"]
            .iter()
            .any(|ext| name.ends_with(ext));
        if !name.ends_with(".tsx") && !name.ends_with(".jsx") && !javascript {
            continue;
        }
        let Some(id) = project_file(project, file) else {
            continue;
        };
        let mut checker = program.get_type_checker_for_file_exclusive(&context, file);
        let mut check = Check {
            file,
            id: &id,
            variables: &variables,
            contracts: &contracts,
            diagnostics: &mut diagnostics,
        };
        // TS may suppress semantic diagnostics for JavaScript files with checkJs off.
        check.visit(&mut checker, file.as_node(), javascript.then_some(snapshot));
    }
    diagnostics
}

struct Check<'a> {
    file: P<SourceFile>,
    id: &'a FileId,
    variables: &'a BTreeMap<&'a str, &'a ProvidedVariable>,
    contracts: &'a BTreeMap<String, P<Node>>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl Check<'_> {
    fn visit(
        &mut self,
        checker: &mut Checker,
        node: P<Node>,
        javascript: Option<&AnalysisSnapshot>,
    ) {
        if let Some(snapshot) = javascript
            && let Some(error) = provided_member_error(checker, snapshot, self.file, self.id, node)
        {
            self.diagnostics.push(error);
        }
        let jsx = match node.kind() {
            Kind::JsxOpeningElement => {
                let jsx = node.as_jsx_opening_element();
                Some((jsx.tag_name(), jsx.attributes()))
            }
            Kind::JsxSelfClosingElement => {
                let jsx = node.as_jsx_self_closing_element();
                Some((jsx.tag_name(), jsx.attributes()))
            }
            _ => None,
        };
        if let Some((tag, attributes)) = jsx
            && tag.kind() == Kind::Identifier
            && (tag.text().starts_with(|c: char| c.is_ascii_lowercase())
                || tag.text().contains('-'))
        {
            self.attributes(checker, attributes.as_jsx_attributes().properties().nodes());
        }
        node.for_each_child(&mut |child| {
            self.visit(checker, child, javascript);
            false
        });
    }

    fn attributes(&mut self, checker: &mut Checker, attributes: &[P<Node>]) {
        // Later props override earlier ones. A spread with an optional xVars
        // property can still leave the preceding value in place.
        for &node in attributes.iter().rev() {
            match node.kind() {
                Kind::JsxAttribute if node.as_jsx_attribute().name().text() == "xVars" => {
                    let attribute = node.as_jsx_attribute();
                    if let Some(expression) = attribute
                        .initializer()
                        .filter(|init| init.kind() == Kind::JsxExpression)
                        .and_then(|init| init.as_jsx_expression().expression())
                    {
                        let ty = checker.get_type_at_location(expression);
                        self.map(checker, ty, self.range(expression));
                    } else {
                        self.invalid_map(self.range(node));
                    }
                    break;
                }
                Kind::JsxSpreadAttribute => {
                    let expression = node.as_jsx_spread_attribute().expression();
                    let ty = checker.get_type_at_location(expression);
                    let ty = constraint(checker, ty);
                    let mut required = true;
                    for &alternative in alternatives(&ty) {
                        let Some(prop) = checker.get_property_of_type(alternative, "xVars") else {
                            required = false;
                            continue;
                        };
                        required &= !prop.flags().intersects(SymbolFlags::Optional);
                        let range = self.range(expression);
                        let (ty, range) = self.property_value(checker, prop, range);
                        self.map(checker, ty, range);
                    }
                    if required {
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    fn map(&mut self, checker: &mut Checker, ty: P<Type>, range: TextRange) {
        let ty = constraint(checker, ty);
        for &ty in alternatives(&ty) {
            if ty
                .flags()
                .intersects(TypeFlags::Nullable | TypeFlags::Never)
            {
                continue;
            }
            if !ty
                .flags()
                .intersects(TypeFlags::Object | TypeFlags::Intersection)
            {
                self.invalid_map(range);
                continue;
            }
            if !checker.get_index_infos_of_type_exported(ty).is_empty() {
                self.invalid_map(range);
            }
            for &property in checker.get_properties_of_type_exported(ty) {
                let key = property.name();
                let Some(variable) = self.variables.get(key) else {
                    self.diagnostics.push(Diagnostic::error(
                        "XC-007",
                        self.id,
                        range,
                        format!("'{key}' is not an exported XCSS provided variable."),
                    ));
                    continue;
                };
                let ValueType::Named(kind) = &variable.value_type else {
                    continue;
                };
                let (actual, value_range) = self.property_value(checker, property, range);
                let expected = checker.get_type_from_type_node(self.contracts[kind]);
                if !accepts(checker, &variable.value_type, expected, actual) {
                    self.diagnostics.push(Diagnostic::error(
                        "XC-008",
                        self.id,
                        value_range,
                        format!("Value for '{key}' must be a CSS {kind}."),
                    ));
                }
            }
        }
    }

    fn property_value(
        &self,
        checker: &mut Checker,
        symbol: P<Symbol>,
        range: TextRange,
    ) -> (P<Type>, TextRange) {
        // Fresh object literals widen their property values. Check the actual
        // initializer when it is part of this JSX expression, and the checked
        // property type for aliased maps and spreads from elsewhere.
        if let &[decl] = symbol.declarations()
            && decl.kind() == Kind::PropertyAssignment
            && ast::get_source_file_of_node(decl) == Some(self.file)
        {
            let value = decl.as_property_assignment().initializer();
            let value_range = self.range(value);
            if value_range.start >= range.start && value_range.end <= range.end {
                return (checker.get_type_at_location(value), value_range);
            }
        }
        (checker.get_type_of_symbol_exported(symbol), range)
    }

    fn range(&self, node: P<Node>) -> TextRange {
        node_range(self.file, node)
    }

    fn invalid_map(&mut self, range: TextRange) {
        self.diagnostics.push(Diagnostic::error(
            "XC-007",
            self.id,
            range,
            "xVars requires a map with statically known provided variable keys.",
        ));
    }
}

fn constraint(checker: &mut Checker, ty: P<Type>) -> P<Type> {
    if ty.is_type_parameter() {
        checker
            .get_base_constraint_of_type_exported(ty)
            .unwrap_or(ty)
    } else {
        ty
    }
}

fn alternatives(ty: &P<Type>) -> &[P<Type>] {
    if ty.is_union() {
        ty.as_union_type().types()
    } else {
        std::slice::from_ref(ty)
    }
}

fn accepts(checker: &mut Checker, kind: &ValueType, expected: P<Type>, actual: P<Type>) -> bool {
    let actual = constraint(checker, actual);
    alternatives(&actual).iter().all(|&ty| {
        if ty.flags().intersects(TypeFlags::Nullable) {
            return ty.flags().intersects(TypeFlags::Undefined);
        }
        if ty.flags().intersects(TypeFlags::StringOrNumberLiteral)
            && let Some(literal) = ty.as_literal_type().value()
        {
            let value = match literal {
                LiteralValue::String(value) => value.to_owned(),
                LiteralValue::Number(value) => value.to_string(),
                _ => unreachable!(),
            };
            if ["inherit", "initial", "unset", "revert", "revert-layer"]
                .iter()
                .any(|keyword| value.trim().eq_ignore_ascii_case(keyword))
            {
                return true;
            }
            let inferred = infer_type(&value);
            return inferred != ValueType::Unknown && kind.accepts(&inferred)
                || value.trim() == "0" && matches!(kind, ValueType::Named(name) if name == "number" || name == "integer");
        }
        checker.is_type_assignable_to(ty, expected)
    })
}
