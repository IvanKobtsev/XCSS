use std::collections::{BTreeMap, BTreeSet};

use tsrs_ast::{self as ast, Kind, Node, NodeFlags, SourceFile, Symbol, SymbolFlags};
use tsrs_checker::{Checker, LiteralValue, SignatureKind, Type, TypeFlags};
use tsrs_compiler::{Context, Program};
use tsrs_core::P;
use xcss_analysis::{
    AnalysisSnapshot, DIMENSION_UNITS, ProvidedVariable, VALUE_KINDS, ValueType, infer_type,
};
use xcss_source::{Diagnostic, FileId, TextRange};

use crate::Project;
use crate::diagnostics::{
    contextual_groups, node_range, project_file, provided_member_error, string_type, styles_module,
    stylesheet,
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ClassOrigin {
    pub module: FileId,
    pub class: String,
    pub variant_label: Option<String>,
    pub overridden: bool,
}

#[derive(Default)]
pub struct ClassUsagePlan {
    pub sites: BTreeMap<FileId, Vec<ClassSite>>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct ClassSite {
    pub props: bool,
    pub range: TextRange,
    pub argument: Option<TextRange>,
    pub level: Option<String>,
    pub origins: BTreeSet<ClassOrigin>,
}

pub(crate) const CONTRACT_PATH: &str = "/node_modules/.xcss-internal/jsx-props.d.ts";
pub(crate) fn declaration(value_contracts: bool) -> String {
    let mut text = String::from(crate::CLASS_NAME);
    if !value_contracts {
        return text;
    }
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
) -> ClassUsagePlan {
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
    let mut sites = BTreeMap::new();
    let mut component_calls = Vec::new();
    for &file in program.source_files() {
        if project_file(project, file).is_some() {
            collect_component_calls(file.as_node(), &mut component_calls);
        }
    }
    let context = Context::default();
    for &file in program.source_files() {
        let name = file.file_name();
        let javascript = [".js", ".jsx", ".mjs", ".cjs"]
            .iter()
            .any(|ext| name.ends_with(ext));
        if ![".ts", ".tsx", ".mts", ".cts"]
            .iter()
            .any(|ext| name.ends_with(ext))
            && !javascript
        {
            continue;
        }
        if [".d.ts", ".d.mts", ".d.cts"]
            .iter()
            .any(|ext| name.ends_with(ext))
            || name.contains("/node_modules/")
        {
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
            snapshot,
            sites: sites.entry(id.clone()).or_insert_with(Vec::new),
            component_calls: &component_calls,
        };
        // TS may suppress semantic diagnostics for JavaScript files with checkJs off.
        check.visit(&mut checker, file.as_node(), javascript.then_some(snapshot));
    }
    ClassUsagePlan { sites, diagnostics }
}

struct Check<'a> {
    file: P<SourceFile>,
    id: &'a FileId,
    variables: &'a BTreeMap<&'a str, &'a ProvidedVariable>,
    contracts: &'a BTreeMap<String, P<Node>>,
    diagnostics: &'a mut Vec<Diagnostic>,
    snapshot: &'a AnalysisSnapshot,
    sites: &'a mut Vec<ClassSite>,
    component_calls: &'a [P<Node>],
}

fn jsx_parts(node: P<Node>) -> Option<(P<Node>, P<Node>)> {
    match node.kind() {
        Kind::JsxOpeningElement => {
            let jsx = node.as_jsx_opening_element();
            Some((jsx.tag_name(), jsx.attributes()))
        }
        Kind::JsxSelfClosingElement => {
            let jsx = node.as_jsx_self_closing_element();
            Some((jsx.tag_name(), jsx.attributes()))
        }
        _ => None,
    }
}

fn intrinsic(tag: P<Node>) -> bool {
    tag.kind() == Kind::Identifier
        && (tag.text().starts_with(|c: char| c.is_ascii_lowercase()) || tag.text().contains('-'))
}

fn collect_component_calls(node: P<Node>, calls: &mut Vec<P<Node>>) {
    if node.kind() == Kind::CallExpression {
        calls.push(node);
    }
    if let Some((tag, _)) = jsx_parts(node)
        && !intrinsic(tag)
    {
        calls.push(node);
    }
    node.for_each_child(&mut |child| {
        collect_component_calls(child, calls);
        false
    });
}

impl Check<'_> {
    fn is_override(&self, checker: &mut Checker, expression: P<Node>) -> bool {
        self.override_identity(checker, expression, 0)
    }

    fn override_identity(&self, checker: &mut Checker, expression: P<Node>, depth: usize) -> bool {
        if depth > 32 {
            return false;
        }
        let expression = ast::skip_parentheses(expression);
        let ty = checker.get_type_at_location(expression);
        if let Some(marker) = checker.get_property_of_type(ty, "__xcssOverrideHelper__") {
            let marker = checker.get_type_of_symbol_exported(marker);
            if marker.flags().intersects(TypeFlags::BooleanLiteral)
                && matches!(
                    marker.as_literal_type().value(),
                    Some(LiteralValue::Boolean(true))
                )
                && !checker
                    .get_signatures_of_type_exported(ty, SignatureKind::Call)
                    .is_empty()
            {
                return true;
            }
        }
        let inner = match expression.kind() {
            Kind::AsExpression => Some(expression.as_as_expression().expression()),
            Kind::SatisfiesExpression => Some(expression.as_satisfies_expression().expression()),
            Kind::NonNullExpression => Some(expression.as_non_null_expression().expression()),
            Kind::Identifier => checker
                .get_symbol_at_location_exported(expression)
                .and_then(|mut symbol| {
                    if symbol.flags().intersects(SymbolFlags::Alias) {
                        symbol = checker.get_aliased_symbol(symbol);
                    }
                    symbol
                        .value_declaration()
                        .filter(|&d| const_variable(d))
                        .and_then(|d| d.as_variable_declaration().initializer())
                }),
            _ => None,
        };
        inner.is_some_and(|inner| self.override_identity(checker, inner, depth + 1))
    }

    fn override_site(&mut self, checker: &mut Checker, node: P<Node>) {
        let call = node.as_call_expression();
        if !self.static_helper_callee(checker, call.expression(), 0) {
            self.diagnostics.push(Diagnostic::error("XC-013", self.id, self.range(call.expression()), "override() requires a direct binding or module namespace access; dynamic callee evaluation is not supported."));
            return;
        }
        let args = call.arguments().nodes();
        let level = if args.len() == 1 {
            Some("1".into())
        } else if args.len() == 2 {
            let value = ast::skip_parentheses(args[1]);
            if value.kind() != Kind::NumericLiteral {
                None
            } else {
                value
                    .text()
                    .replace('_', "")
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite() && *n > 0.0 && n.fract() == 0.0)
                    .map(|n| format!("{n:.0}"))
            }
        } else {
            None
        };
        let Some(level) = level else {
            self.diagnostics.push(Diagnostic::error(
                "XC-010",
                self.id,
                args.get(1).map_or(self.range(node), |&n| self.range(n)),
                "override() specificity must be a positive integer literal.",
            ));
            return;
        };
        let Some(&input) = args.first() else {
            return;
        };
        let input_type = checker.get_type_at_location(input);
        if alternatives(&input_type).iter().any(|t| {
            t.flags()
                .intersects(TypeFlags::Nullable | TypeFlags::BooleanLike)
        }) {
            self.diagnostics.push(Diagnostic::error(
                "XC-011",
                self.id,
                self.range(input),
                "override() requires a non-null XCSS ClassName value.",
            ));
            return;
        }
        let origins = self.expression_origins(checker, input, 0);
        if origins.as_ref().is_some_and(|s| s.is_empty()) {
            return;
        }
        let Some(origins) =
            origins.filter(|s| !s.is_empty() && s.iter().all(|o| o.variant_label.is_none()))
        else {
            self.diagnostics.push(Diagnostic::error(
                "XC-011",
                self.id,
                self.range(input),
                "override() requires a traceable XCSS ClassName value.",
            ));
            return;
        };
        if origins.iter().any(|o| o.overridden) {
            self.diagnostics.push(Diagnostic::error(
                "XC-012",
                self.id,
                self.range(input),
                "override() cannot be applied to an already-overridden value.",
            ));
            return;
        }
        self.sites.push(ClassSite {
            props: false,
            range: self.range(node),
            argument: Some(self.range(input)),
            level: Some(level),
            origins,
        });
    }

    fn static_helper_callee(
        &self,
        checker: &mut Checker,
        expression: P<Node>,
        depth: usize,
    ) -> bool {
        if depth > 32 {
            return false;
        }
        let expression = ast::skip_parentheses(expression);
        match expression.kind() {
            Kind::Identifier => true,
            Kind::AsExpression => self.static_helper_callee(
                checker,
                expression.as_as_expression().expression(),
                depth + 1,
            ),
            Kind::SatisfiesExpression => self.static_helper_callee(
                checker,
                expression.as_satisfies_expression().expression(),
                depth + 1,
            ),
            Kind::NonNullExpression => self.static_helper_callee(
                checker,
                expression.as_non_null_expression().expression(),
                depth + 1,
            ),
            Kind::PropertyAccessExpression => self.module_namespace(
                checker,
                expression.as_property_access_expression().expression(),
                depth + 1,
            ),
            Kind::ElementAccessExpression => {
                let access = expression.as_element_access_expression();
                ast::is_string_literal_like(access.argument_expression())
                    && self.module_namespace(checker, access.expression(), depth + 1)
            }
            _ => false,
        }
    }

    fn module_namespace(&self, checker: &mut Checker, expression: P<Node>, depth: usize) -> bool {
        if depth > 32 || expression.kind() != Kind::Identifier {
            return false;
        }
        let Some(symbol) = checker.get_symbol_at_location_exported(expression) else {
            return false;
        };
        if symbol
            .declarations()
            .iter()
            .any(|d| d.kind() == Kind::NamespaceImport)
        {
            return true;
        }
        symbol
            .value_declaration()
            .filter(|&d| const_variable(d))
            .and_then(|d| d.as_variable_declaration().initializer())
            .is_some_and(|v| self.module_namespace(checker, v, depth + 1))
    }

    fn variant_site(&mut self, range: TextRange, origins: BTreeSet<ClassOrigin>) {
        let mut origins: BTreeSet<_> = origins
            .into_iter()
            .filter(|o| o.variant_label.is_some())
            .collect();
        if origins.is_empty() {
            return;
        }
        let existing = self
            .sites
            .iter()
            .position(|s| s.level.is_none() && s.range == range);
        if let Some(index) = existing {
            origins.extend(self.sites[index].origins.iter().cloned());
        }
        let mut labels = BTreeSet::new();
        if origins
            .iter()
            .any(|o| !labels.insert(o.variant_label.clone()))
        {
            self.invalid_class(range);
            return;
        }
        if let Some(index) = existing {
            self.sites[index].origins = origins;
        } else {
            self.sites.push(ClassSite {
                props: false,
                range,
                argument: None,
                level: None,
                origins,
            });
        }
    }

    fn variant_value(&mut self, checker: &mut Checker, expression: P<Node>, depth: usize) {
        if depth > 64 {
            return;
        }
        let expression = ast::skip_parentheses(expression);
        let ty = checker.get_type_at_location(expression);
        if let Some(origins) = self
            .origins(checker, ty)
            .filter(|s| s.iter().any(|o| o.variant_label.is_some()))
        {
            self.variant_site(self.range(expression), origins);
            return;
        }
        if ty
            .flags()
            .intersects(TypeFlags::Object | TypeFlags::Intersection)
        {
            let number = checker.get_number_type();
            if let Some(element) = checker.get_index_type_of_type(ty, number)
                && let Some(origins) = self
                    .origins(checker, element)
                    .filter(|s| s.iter().any(|o| o.variant_label.is_some()))
            {
                self.variant_site(self.range(expression), origins);
                return;
            }
        }
        match expression.kind() {
            Kind::ArrayLiteralExpression => {
                for &child in expression.as_array_literal_expression().elements().nodes() {
                    self.variant_value(checker, child, depth + 1);
                }
            }
            Kind::ObjectLiteralExpression => {
                for &property in expression
                    .as_object_literal_expression()
                    .properties()
                    .nodes()
                {
                    if property.kind() == Kind::PropertyAssignment {
                        let key = property.as_property_assignment().name();
                        if key.kind() == Kind::ComputedPropertyName {
                            self.variant_value(
                                checker,
                                key.as_computed_property_name().expression(),
                                depth + 1,
                            );
                        }
                    }
                }
            }
            Kind::AsExpression => self.variant_value(
                checker,
                expression.as_as_expression().expression(),
                depth + 1,
            ),
            Kind::SatisfiesExpression => self.variant_value(
                checker,
                expression.as_satisfies_expression().expression(),
                depth + 1,
            ),
            Kind::ConditionalExpression => {
                let c = expression.as_conditional_expression();
                self.variant_value(checker, c.when_true(), depth + 1);
                self.variant_value(checker, c.when_false(), depth + 1);
            }
            Kind::BinaryExpression => {
                let b = expression.as_binary_expression();
                self.variant_value(checker, b.right(), depth + 1);
                if b.operator_token().kind() != Kind::AmpersandAmpersandToken {
                    self.variant_value(checker, b.left(), depth + 1);
                }
            }
            _ => {}
        }
    }

    fn variant_types(
        &self,
        checker: &mut Checker,
        ty: P<Type>,
        depth: usize,
    ) -> BTreeSet<ClassOrigin> {
        if depth > 32 {
            return BTreeSet::new();
        }
        if let Some(origins) = self.origins(checker, ty) {
            return origins
                .into_iter()
                .filter(|o| o.variant_label.is_some())
                .collect();
        }
        let mut origins = BTreeSet::new();
        for &alternative in alternatives(&ty) {
            if alternative
                .flags()
                .intersects(TypeFlags::Object | TypeFlags::Intersection)
            {
                let number = checker.get_number_type();
                if let Some(element) = checker.get_index_type_of_type(alternative, number) {
                    origins.extend(self.variant_types(checker, element, depth + 1));
                }
            }
        }
        origins
    }

    fn parameter_origins(
        &self,
        checker: &mut Checker,
        parameter: P<Node>,
        depth: usize,
    ) -> Option<BTreeSet<ClassOrigin>> {
        if depth > 64 {
            return None;
        }
        let function = parameter.parent()?;
        let index = function.parameters().iter().position(|&p| p == parameter)?;
        let mut origins = BTreeSet::new();
        let mut found = false;
        for &call in self
            .component_calls
            .iter()
            .filter(|c| c.kind() == Kind::CallExpression)
        {
            if checker.get_resolved_signature_exported(call).declaration() == Some(function) {
                found = true;
                origins.extend(self.trace_expression_origins(
                    checker,
                    *call.arguments().get(index)?,
                    depth + 1,
                )?);
            }
        }
        found.then_some(origins)
    }

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
        if javascript.is_some() {
            let access = match node.kind() {
                Kind::PropertyAccessExpression => {
                    let a = node.as_property_access_expression();
                    Some((a.expression(), a.name()))
                }
                Kind::ElementAccessExpression => {
                    let a = node.as_element_access_expression();
                    ast::is_string_literal_like(a.argument_expression())
                        .then_some((a.expression(), a.argument_expression()))
                }
                _ => None,
            };
            if let Some((receiver, key)) = access
                && let Some(module) = styles_module(checker, self.snapshot, receiver)
                && self
                    .snapshot
                    .module_interface(&module)
                    .is_some_and(|m| !m.classes.contains(key.text()))
            {
                self.diagnostics.push(Diagnostic::error(
                    "XC-001",
                    self.id,
                    self.range(key),
                    format!("Class '{}' can't be found in '{}'.", key.text(), module.0),
                ));
            }
        }
        if let Some((tag, attributes)) = jsx_parts(node)
            && intrinsic(tag)
        {
            let attributes = attributes.as_jsx_attributes().properties().nodes();
            self.attributes(checker, attributes, "xVars");
            self.attributes(checker, attributes, "xClass");
        }
        if node.kind() == Kind::CallExpression
            && self.is_override(checker, node.as_call_expression().expression())
        {
            self.override_site(checker, node);
        }
        if matches!(
            node.kind(),
            Kind::Identifier | Kind::PropertyAccessExpression
        ) && self.is_override(checker, node)
        {
            let mut value = node;
            let mut parent = node.parent();
            while parent.is_some_and(|p| {
                matches!(
                    p.kind(),
                    Kind::ParenthesizedExpression
                        | Kind::AsExpression
                        | Kind::SatisfiesExpression
                        | Kind::NonNullExpression
                )
            }) {
                value = parent.unwrap();
                parent = value.parent();
            }
            if let Some(parent) = parent {
                let allowed = match parent.kind() {
                    Kind::CallExpression => parent.as_call_expression().expression() == value,
                    Kind::VariableDeclaration => true,
                    Kind::ImportSpecifier
                    | Kind::ExportSpecifier
                    | Kind::ImportClause
                    | Kind::NamespaceImport => true,
                    Kind::TypeQuery => true,
                    Kind::PropertyAccessExpression => {
                        parent.as_property_access_expression().name() == node
                    }
                    _ => false,
                };
                if !allowed {
                    self.diagnostics.push(Diagnostic::error("XC-013", self.id, self.range(node), "override() must be called directly or through a statically identifiable alias."));
                }
            }
        }

        node.for_each_child(&mut |child| {
            self.visit(checker, child, javascript);
            false
        });
    }

    fn attributes(&mut self, checker: &mut Checker, attributes: &[P<Node>], name: &str) {
        // Later props override earlier ones. A spread with an optional
        // property can still leave the preceding value in place.
        for &node in attributes.iter().rev() {
            match node.kind() {
                Kind::JsxAttribute if node.as_jsx_attribute().name().text() == name => {
                    let attribute = node.as_jsx_attribute();
                    if let Some(expression) = attribute
                        .initializer()
                        .filter(|init| init.kind() == Kind::JsxExpression)
                        .and_then(|init| init.as_jsx_expression().expression())
                    {
                        let ty = checker.get_type_at_location(expression);
                        self.value(checker, name, ty, self.range(expression), Some(expression));
                    } else if name == "xVars" {
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
                        let Some(prop) = checker.get_property_of_type(alternative, name) else {
                            required = false;
                            continue;
                        };
                        required &= !prop.flags().intersects(SymbolFlags::Optional);
                        let range = self.range(expression);
                        let (ty, range) = self.property_value(checker, prop, range);
                        if name == "xClass" {
                            let origins = self.variant_types(checker, ty, 0);
                            let range = self.range(expression);
                            self.variant_site(range, origins);
                            if let Some(site) = self
                                .sites
                                .iter_mut()
                                .find(|s| s.range == range && s.level.is_none())
                            {
                                site.props = true;
                            }
                            continue;
                        }
                        let expression = prop
                            .declarations()
                            .first()
                            .copied()
                            .filter(|decl| decl.kind() == Kind::PropertyAssignment)
                            .map(|decl| decl.as_property_assignment().initializer());
                        self.value(checker, name, ty, range, expression);
                    }
                    if required {
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    fn value(
        &mut self,
        checker: &mut Checker,
        name: &str,
        ty: P<Type>,
        range: TextRange,
        expression: Option<P<Node>>,
    ) {
        if name == "xVars" {
            self.map(checker, ty, range, expression, 0);
        } else {
            if let Some(expression) = expression {
                self.variant_value(checker, expression, 0);
            } else if let Some(origins) = self.origins(checker, ty) {
                self.variant_site(range, origins);
            }
        }
    }

    fn expression_origins(
        &self,
        checker: &mut Checker,
        expression: P<Node>,
        depth: usize,
    ) -> Option<BTreeSet<ClassOrigin>> {
        self.trace_expression_origins(checker, expression, depth)
            .or_else(|| {
                let ty = checker.get_type_at_location(expression);
                self.origins(checker, ty)
            })
    }

    fn trace_expression_origins(
        &self,
        checker: &mut Checker,
        expression: P<Node>,
        depth: usize,
    ) -> Option<BTreeSet<ClassOrigin>> {
        if depth > 64 {
            return None;
        }
        let expression = ast::skip_parentheses(expression);
        let access = match expression.kind() {
            Kind::PropertyAccessExpression => {
                let access = expression.as_property_access_expression();
                Some((access.expression(), access.name()))
            }
            Kind::ElementAccessExpression => {
                let access = expression.as_element_access_expression();
                Some((access.expression(), access.argument_expression()))
            }
            _ => None,
        };
        if let Some((receiver, key)) = access
            && (expression.kind() == Kind::PropertyAccessExpression
                || ast::is_string_literal_like(key))
            && let Some(module) = styles_module(checker, self.snapshot, receiver)
            && !self
                .snapshot
                .module_interface(&module)?
                .classes
                .contains(key.text())
        {
            // The member diagnostic owns this failure; retain the slot without
            // inventing a class origin for an export that does not exist.
            return Some(BTreeSet::new());
        }
        if expression.kind() == Kind::AsExpression {
            return self.expression_origins(
                checker,
                expression.as_as_expression().expression(),
                depth + 1,
            );
        }
        if expression.kind() == Kind::CallExpression {
            let call = expression.as_call_expression();
            if self.is_override(checker, call.expression()) {
                let origins = self.expression_origins(
                    checker,
                    *call.arguments().nodes().first()?,
                    depth + 1,
                )?;
                return Some(
                    origins
                        .into_iter()
                        .map(|mut o| {
                            o.overridden = true;
                            o
                        })
                        .collect(),
                );
            }
            let declaration = checker
                .get_resolved_signature_exported(expression)
                .declaration()?;
            let body = declaration.body()?;
            let mut returns = Vec::new();
            if body.kind() == Kind::Block {
                collect_returns(body, &mut returns);
            } else {
                returns.push(body);
            }
            let mut origins = BTreeSet::new();
            for value in returns {
                origins.extend(self.expression_origins(checker, value, depth + 1)?);
            }
            return (!origins.is_empty()).then_some(origins);
        }
        let ty = checker.get_type_at_location(expression);
        let trace_first = matches!(
            expression.kind(),
            Kind::Identifier | Kind::ConditionalExpression | Kind::BinaryExpression
        ) || expression.kind() == Kind::PropertyAccessExpression
            && access.is_some_and(|(receiver, _)| {
                component_parameter(checker, receiver, depth + 1).is_some()
            });
        if !trace_first && let Some(origins) = self.origins(checker, ty) {
            return Some(origins);
        }
        match expression.kind() {
            Kind::NonNullExpression => self.expression_origins(
                checker,
                expression.as_non_null_expression().expression(),
                depth + 1,
            ),
            Kind::ConditionalExpression => {
                let conditional = expression.as_conditional_expression();
                let mut origins =
                    self.expression_origins(checker, conditional.when_true(), depth + 1)?;
                origins.extend(self.expression_origins(
                    checker,
                    conditional.when_false(),
                    depth + 1,
                )?);
                Some(origins)
            }
            Kind::BinaryExpression => {
                let binary = expression.as_binary_expression();
                match binary.operator_token().kind() {
                    Kind::AmpersandAmpersandToken => {
                        let left = checker.get_type_at_location(binary.left());
                        if !left
                            .flags()
                            .intersects(TypeFlags::BooleanLike | TypeFlags::Nullable)
                        {
                            return None;
                        }
                        self.expression_origins(checker, binary.right(), depth + 1)
                    }
                    Kind::BarBarToken | Kind::QuestionQuestionToken => {
                        let mut origins =
                            self.expression_origins(checker, binary.left(), depth + 1)?;
                        origins.extend(self.expression_origins(
                            checker,
                            binary.right(),
                            depth + 1,
                        )?);
                        Some(origins)
                    }
                    _ => None,
                }
            }
            Kind::Identifier => {
                let mut symbol = checker.get_symbol_at_location_exported(expression)?;
                if symbol.flags().intersects(SymbolFlags::Alias) {
                    symbol = checker.get_aliased_symbol(symbol);
                }
                let declaration = symbol.value_declaration()?;
                match declaration.kind() {
                    Kind::VariableDeclaration if const_variable(declaration) => {
                        let value = declaration.as_variable_declaration().initializer()?;
                        self.expression_origins(checker, value, depth + 1)
                    }
                    Kind::Parameter => self.parameter_origins(checker, declaration, depth + 1),
                    Kind::BindingElement => {
                        let binding = declaration.as_binding_element();
                        let owner = declaration.parent()?.parent()?;
                        let parameter = match owner.kind() {
                            Kind::Parameter => owner,
                            Kind::VariableDeclaration if const_variable(owner) => {
                                component_parameter(
                                    checker,
                                    owner.as_variable_declaration().initializer()?,
                                    depth + 1,
                                )?
                            }
                            _ => return None,
                        };
                        let name = binding.property_name().or_else(|| binding.name())?;
                        let mut origins = self.component_prop(
                            checker,
                            parameter.parent()?,
                            name.text(),
                            depth + 1,
                        )?;
                        if let Some(default) = binding.initializer() {
                            origins.extend(self.expression_origins(checker, default, depth + 1)?);
                        }
                        Some(origins)
                    }
                    _ => None,
                }
            }
            Kind::PropertyAccessExpression => {
                let access = expression.as_property_access_expression();
                let declaration = component_parameter(checker, access.expression(), depth + 1)?;
                self.component_prop(
                    checker,
                    declaration.parent()?,
                    access.name().text(),
                    depth + 1,
                )
            }
            _ => None,
        }
    }

    fn component_prop(
        &self,
        checker: &mut Checker,
        component: P<Node>,
        name: &str,
        depth: usize,
    ) -> Option<BTreeSet<ClassOrigin>> {
        if depth > 64 {
            return None;
        }
        let mut origins = BTreeSet::new();
        let mut found = false;
        for &call in self.component_calls {
            if checker.get_resolved_signature_exported(call).declaration() != Some(component) {
                continue;
            }
            found = true;
            if call.kind() == Kind::CallExpression {
                let value = *call.arguments().first()?;
                let ty = checker.get_type_at_location(value);
                let property = checker.get_property_of_type(ty, name)?;
                if let Some(declaration) = property
                    .value_declaration()
                    .filter(|d| d.kind() == Kind::PropertyAssignment)
                {
                    origins.extend(self.trace_expression_origins(
                        checker,
                        declaration.as_property_assignment().initializer(),
                        depth + 1,
                    )?);
                } else {
                    let parameter = component_parameter(checker, value, depth + 1)?;
                    origins.extend(self.component_prop(
                        checker,
                        parameter.parent()?,
                        name,
                        depth + 1,
                    )?);
                }
                continue;
            }
            let Some((_, attributes)) = jsx_parts(call) else {
                continue;
            };
            for &node in attributes
                .as_jsx_attributes()
                .properties()
                .nodes()
                .iter()
                .rev()
            {
                match node.kind() {
                    Kind::JsxAttribute if node.as_jsx_attribute().name().text() == name => {
                        if let Some(value) = node.as_jsx_attribute().initializer() {
                            let value = value.as_jsx_expression().expression()?;
                            origins.extend(self.expression_origins(checker, value, depth + 1)?);
                        }
                        break;
                    }
                    Kind::JsxSpreadAttribute => {
                        let value = node.as_jsx_spread_attribute().expression();
                        let ty = checker.get_type_at_location(value);
                        if let Some(property) = checker.get_property_of_type(ty, name) {
                            let ty = checker.get_type_of_symbol_exported(property);
                            let resolved = if let Some(origins) = self.origins(checker, ty) {
                                origins
                            } else if let Some(declaration) = property.value_declaration()
                                && declaration.kind() == Kind::PropertyAssignment
                            {
                                self.expression_origins(
                                    checker,
                                    declaration.as_property_assignment().initializer(),
                                    depth + 1,
                                )?
                            } else {
                                let parameter = component_parameter(checker, value, depth + 1)?;
                                self.component_prop(checker, parameter.parent()?, name, depth + 1)?
                            };
                            origins.extend(resolved);
                            if !property.flags().intersects(SymbolFlags::Optional) {
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        found.then_some(origins)
    }

    fn origins(&self, checker: &mut Checker, ty: P<Type>) -> Option<BTreeSet<ClassOrigin>> {
        let ty = constraint(checker, ty);
        let mut result = BTreeSet::new();
        for &ty in alternatives(&ty) {
            if ty
                .flags()
                .intersects(TypeFlags::Nullable | TypeFlags::Never | TypeFlags::BooleanLike)
            {
                continue;
            }
            let groups = contextual_groups(checker, self.snapshot, ty);
            if !groups.is_empty() {
                for (module, group) in groups {
                    for class in self
                        .snapshot
                        .module_interface(&module)?
                        .variants
                        .get(group)?
                    {
                        result.insert(ClassOrigin {
                            module: module.clone(),
                            class: class.clone(),
                            variant_label: Some(class.clone()),
                            overridden: false,
                        });
                    }
                }
                continue;
            }
            let marker = checker.get_property_of_type(ty, "__xcssClassExport__")?;
            let mut found = false;
            for &decl in marker.declarations() {
                if decl.kind() == Kind::PropertySignature
                    && let Some(module) = stylesheet(self.snapshot, decl)
                    && let Some(class) = decl
                        .as_property_signature_declaration()
                        .type_()
                        .and_then(string_type)
                {
                    found = true;
                    result.insert(ClassOrigin {
                        module,
                        class: class.into(),
                        variant_label: None,
                        overridden: false,
                    });
                }
            }
            if !found {
                return None;
            }
        }
        Some(result)
    }

    fn map(
        &mut self,
        checker: &mut Checker,
        ty: P<Type>,
        range: TextRange,
        expression: Option<P<Node>>,
        depth: usize,
    ) {
        if depth < 64
            && let Some(expression) = expression
        {
            let expression = ast::skip_parentheses(expression);
            let inner = match expression.kind() {
                Kind::AsExpression => Some(expression.as_as_expression().expression()),
                Kind::SatisfiesExpression => {
                    Some(expression.as_satisfies_expression().expression())
                }
                Kind::NonNullExpression => Some(expression.as_non_null_expression().expression()),
                _ => None,
            };
            if let Some(inner) = inner {
                self.map(checker, ty, range, Some(inner), depth + 1);
                return;
            }
            if expression.kind() == Kind::Identifier
                && let Some(symbol) = checker.get_symbol_at_location_exported(expression)
                && let Some(decl) = symbol.value_declaration()
                && const_variable(decl)
                && let Some(value) = decl.as_variable_declaration().initializer()
            {
                self.map(checker, ty, range, Some(value), depth + 1);
                return;
            }
            if expression.kind() == Kind::ObjectLiteralExpression {
                let properties = expression
                    .as_object_literal_expression()
                    .properties()
                    .nodes();
                let mut missing = false;
                let mut other_dynamic = false;
                for &property in properties {
                    if property.kind() == Kind::PropertyAssignment {
                        let name = property.as_property_assignment().name();
                        if name.kind() == Kind::ComputedPropertyName {
                            let key = name.as_computed_property_name().expression();
                            if provided_member_error(
                                checker,
                                self.snapshot,
                                self.file,
                                self.id,
                                key,
                            )
                            .is_some()
                            {
                                missing = true;
                            } else {
                                let key_type = checker.get_type_at_location(key);
                                other_dynamic |= !key_type.flags().intersects(
                                    TypeFlags::StringLiteral | TypeFlags::NumberLiteral,
                                );
                            }
                        }
                    } else {
                        other_dynamic = true;
                    }
                }
                if missing && !other_dynamic {
                    self.map_properties(checker, ty, range, true);
                    return;
                }
            }
        }
        self.map_properties(checker, ty, range, false);
    }

    fn map_properties(
        &mut self,
        checker: &mut Checker,
        ty: P<Type>,
        range: TextRange,
        missing_key: bool,
    ) {
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
            if !missing_key && !checker.get_index_infos_of_type_exported(ty).is_empty() {
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

    fn invalid_class(&mut self, range: TextRange) {
        self.diagnostics.push(Diagnostic::error(
            "XC-009",
            self.id,
            range,
            "An xClass variant label refers to more than one stylesheet in this expression.",
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

fn const_variable(node: P<Node>) -> bool {
    node.kind() == Kind::VariableDeclaration
        && node
            .parent()
            .is_some_and(|parent| parent.flags().intersects(NodeFlags::Const))
}

fn component_parameter(
    checker: &mut Checker,
    expression: P<Node>,
    depth: usize,
) -> Option<P<Node>> {
    if depth > 64 {
        return None;
    }
    let expression = ast::skip_parentheses(expression);
    let declaration = checker
        .get_symbol_at_location_exported(expression)?
        .value_declaration()?;
    match declaration.kind() {
        Kind::Parameter => Some(declaration),
        Kind::VariableDeclaration if const_variable(declaration) => component_parameter(
            checker,
            declaration.as_variable_declaration().initializer()?,
            depth + 1,
        ),
        _ => None,
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

fn collect_returns(node: P<Node>, out: &mut Vec<P<Node>>) {
    if node.kind() == Kind::ReturnStatement {
        if let Some(value) = node.as_return_statement().expression() {
            out.push(value);
        }
        return;
    }
    node.for_each_child(&mut |child| {
        if !matches!(
            child.kind(),
            Kind::ArrowFunction | Kind::FunctionExpression | Kind::FunctionDeclaration
        ) {
            collect_returns(child, out);
        }
        false
    });
}
