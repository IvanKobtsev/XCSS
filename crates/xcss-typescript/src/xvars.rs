use std::collections::{BTreeMap, BTreeSet};

use tsrs_ast::{self as ast, Kind, Node, NodeFlags, SourceFile, Symbol, SymbolFlags};
use tsrs_checker::{Checker, LiteralValue, Type, TypeFlags};
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
}

#[derive(Default)]
pub struct ClassSlotPlan {
    pub sites: BTreeMap<FileId, Vec<ClassSite>>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct ClassSite {
    pub insert: usize,
    pub slots: Vec<BTreeSet<ClassOrigin>>,
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
) -> ClassSlotPlan {
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
        if project_file(project, file).is_some() && file.file_name().ends_with("sx") {
            collect_component_calls(file.as_node(), &mut component_calls);
        }
    }
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
            snapshot,
            sites: sites.entry(id.clone()).or_insert_with(Vec::new),
            component_calls: &component_calls,
        };
        // TS may suppress semantic diagnostics for JavaScript files with checkJs off.
        check.visit(&mut checker, file.as_node(), javascript.then_some(snapshot));
    }
    ClassSlotPlan { sites, diagnostics }
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
        if let Some((tag, attributes)) = jsx_parts(node)
            && intrinsic(tag)
        {
            let attributes = attributes.as_jsx_attributes().properties().nodes();
            self.attributes(checker, attributes, "xVars");
            let before = self.sites.len();
            self.attributes(checker, attributes, "xClass");
            if self.sites.len() > before {
                let insert = self.range(node).end
                    - if node.kind() == Kind::JsxSelfClosingElement {
                        2
                    } else {
                        1
                    };
                let mut candidates = self.sites.split_off(before).into_iter();
                let mut site = candidates.next().unwrap();
                site.insert = insert;
                for alternative in candidates {
                    if site.slots.len() != alternative.slots.len() {
                        self.invalid_class(self.range(node));
                        break;
                    }
                    for (slot, alternatives) in site.slots.iter_mut().zip(alternative.slots) {
                        slot.extend(alternatives);
                    }
                }
                for slot in &site.slots {
                    let mut labels = BTreeSet::new();
                    if slot
                        .iter()
                        .filter_map(|origin| origin.variant_label.as_ref())
                        .any(|label| !labels.insert(label))
                    {
                        self.diagnostics.push(Diagnostic::error("XC-009", self.id, self.range(node),
                            "An xClass variant label refers to more than one stylesheet in this slot."));
                    }
                }
                self.sites.push(site);
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
                    } else {
                        self.invalid_class(self.range(node));
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
                        let expression = prop
                            .declarations()
                            .first()
                            .copied()
                            .filter(|decl| decl.kind() == Kind::PropertyAssignment)
                            .map(|decl| decl.as_property_assignment().initializer())
                            .filter(|&value| {
                                name != "xClass"
                                    || readonly_tuple(ty)
                                    || ast::get_source_file_of_node(value) == Some(self.file)
                                        && self.range(value) == range
                            });
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
            let slots = self.tuple(checker, ty, expression, 0).or_else(|| {
                let origins = match expression {
                    Some(expression) => self.expression_origins(checker, expression, 0),
                    None => self.origins(checker, ty),
                }?;
                Some(vec![origins])
            });
            if let Some(slots) = slots {
                self.sites.push(ClassSite { insert: 0, slots });
            } else {
                self.invalid_class(range);
            }
        }
    }

    fn tuple(
        &self,
        checker: &mut Checker,
        ty: P<Type>,
        expression: Option<P<Node>>,
        depth: usize,
    ) -> Option<Vec<BTreeSet<ClassOrigin>>> {
        if depth > 64 {
            return None;
        }
        if let Some(expression) = expression {
            let expression = ast::skip_parentheses(expression);
            match expression.kind() {
                Kind::ArrayLiteralExpression => {
                    return expression
                        .as_array_literal_expression()
                        .elements()
                        .nodes()
                        .iter()
                        .map(|&element| {
                            if element.kind() == Kind::SpreadElement {
                                return None;
                            }
                            if element.kind() == Kind::OmittedExpression {
                                return Some(BTreeSet::new());
                            }
                            self.expression_origins(checker, element, 0)
                        })
                        .collect();
                }
                Kind::AsExpression => {
                    return self.tuple(
                        checker,
                        ty,
                        Some(expression.as_as_expression().expression()),
                        depth + 1,
                    );
                }
                Kind::SatisfiesExpression => {
                    return self.tuple(
                        checker,
                        ty,
                        Some(expression.as_satisfies_expression().expression()),
                        depth + 1,
                    );
                }
                Kind::Identifier => {
                    if !alternatives(&ty)
                        .iter()
                        .all(|&candidate| readonly_tuple(candidate))
                    {
                        return None;
                    }
                    if let Some(mut symbol) = checker.get_symbol_at_location_exported(expression) {
                        if symbol.flags().intersects(SymbolFlags::Alias) {
                            symbol = checker.get_aliased_symbol(symbol);
                        }
                        if let Some(decl) = symbol.value_declaration()
                            && const_variable(decl)
                            && let Some(initializer) = decl.as_variable_declaration().initializer()
                        {
                            return self.tuple(checker, ty, Some(initializer), depth + 1);
                        }
                    }
                }
                Kind::ConditionalExpression => {
                    let conditional = expression.as_conditional_expression();
                    let first = conditional.when_true();
                    let second = conditional.when_false();
                    let first_type = checker.get_type_at_location(first);
                    let second_type = checker.get_type_at_location(second);
                    let mut first = self.tuple(checker, first_type, Some(first), depth + 1)?;
                    let second = self.tuple(checker, second_type, Some(second), depth + 1)?;
                    if first.len() != second.len() {
                        return None;
                    }
                    for (slot, other) in first.iter_mut().zip(second) {
                        slot.extend(other);
                    }
                    return Some(first);
                }
                _ => {}
            }
        }
        let ty = constraint(checker, ty);
        let mut slots: Option<Vec<BTreeSet<ClassOrigin>>> = None;
        for &candidate in alternatives(&ty) {
            if candidate
                .flags()
                .intersects(TypeFlags::Nullable | TypeFlags::Never)
            {
                continue;
            }
            if !readonly_tuple(candidate) {
                return None;
            }
            let length = checker.get_property_of_type(candidate, "length")?;
            if !checker
                .get_type_of_symbol_exported(length)
                .flags()
                .intersects(TypeFlags::NumberLiteral)
            {
                return None;
            }
            let origins: Vec<_> = checker
                .get_type_arguments(candidate)
                .iter()
                .map(|&element| self.origins(checker, element))
                .collect::<Option<_>>()?;
            if let Some(slots) = &mut slots {
                if slots.len() != origins.len() {
                    return None;
                }
                for (slot, alternatives) in slots.iter_mut().zip(origins) {
                    slot.extend(alternatives);
                }
            } else {
                slots = Some(origins);
            }
        }
        slots
    }

    fn expression_origins(
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
        let ty = checker.get_type_at_location(expression);
        if let Some(origins) = self.origins(checker, ty) {
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
            let (_, attributes) = jsx_parts(call)?;
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
            "xClass requires a traceable XCSS class, variant, or fixed tuple of slots.",
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

fn readonly_tuple(ty: P<Type>) -> bool {
    ty.is_tuple_type()
        && ty
            .target()
            .is_some_and(|target| target.as_tuple_type().is_readonly())
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
