use std::collections::{BTreeMap, BTreeSet};

use tsrs_ast::{self as ast, Kind, Node, SourceFile, SymbolFlags};
use tsrs_checker::{Checker, ContextFlags, LiteralValue, TypeFlags};
use tsrs_compiler::{Context, Program};
use tsrs_core::P;
use tsrs_scanner::get_token_pos_of_node;
use xcss_analysis::AnalysisSnapshot;
use xcss_source::{Diagnostic, FileId, TextRange};

use crate::Project;

pub(crate) const VARIANT_GROUP_MARKER: &str = "__xcssVariantGroup__";

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Issue {
    start: usize,
    end: usize,
    code: i32,
}

impl Issue {
    pub(crate) fn is_value_error(self) -> bool {
        matches!(self.code, 2322 | 2345 | 2375 | 2379 | 2412 | 2820)
    }

    fn matches(self, range: TextRange) -> bool {
        self.start == range.start && self.end <= range.end
    }
}

pub(crate) type Issues = BTreeMap<FileId, BTreeSet<Issue>>;

pub(crate) fn semantic_issues(program: &'static Program, project: &Project) -> Issues {
    let mut issues = Issues::new();
    for diagnostic in program.get_semantic_diagnostics(&Context::default(), None) {
        let code = diagnostic.code();
        let (Some(file), Some(start), Some(end)) =
            (diagnostic.file(), diagnostic.pos(), diagnostic.end())
        else {
            continue;
        };
        let issue = Issue {
            start: start as usize,
            end: end as usize,
            code,
        };
        if !matches!(code, 2339 | 2344 | 2551 | 7053) && !issue.is_value_error() {
            continue;
        }
        if let Some(id) = project_file(project, file) {
            issues.entry(id).or_default().insert(issue);
        }
    }
    issues
}

pub(crate) fn missing_modules(program: &Program, project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for &file in program.source_files() {
        let Some(id) = project_file(project, file) else {
            continue;
        };
        for &specifier in file.imports() {
            if specifier.text().ends_with(".xcss")
                && program
                    .get_resolved_module_from_module_specifier(file, specifier)
                    .is_none_or(|module| !module.is_resolved())
            {
                diagnostics.push(Diagnostic::error(
                    "XC-004",
                    &id,
                    node_range(file, specifier),
                    format!("Can't resolve XCSS module '{}'.", specifier.text()),
                ));
            }
        }
    }
    diagnostics
}

pub(crate) fn contract_errors(
    program: &Program,
    snapshot: &AnalysisSnapshot,
    issues: &Issues,
) -> Vec<Diagnostic> {
    locate_issues(
        program,
        issues,
        |checker, file, id, node, issue| match issue.code {
            2339 | 2551 | 7053 => {
                let (receiver, key) = member_access(node)?;
                let range = node_range(file, key);
                if !issue.matches(range) && !issue.matches(node_range(file, node)) {
                    return None;
                }
                if let Some(error) = provided_member_error(checker, snapshot, file, id, node) {
                    return Some(error);
                }
                if node.kind() == Kind::ElementAccessExpression && !ast::is_string_literal_like(key)
                {
                    return None;
                }
                let name = key.text();
                let module = styles_module(checker, snapshot, receiver)?;
                if snapshot.module_interface(&module)?.classes.contains(name) {
                    return None;
                }
                Some(Diagnostic::error(
                    "XC-001",
                    id,
                    range,
                    format!("Class '{name}' can't be found in '{}'.", module.0),
                ))
            }
            2344 if node.kind() == Kind::TypeReference => {
                let reference = node.as_type_reference_node();
                let &[argument] = reference.type_arguments()?.nodes() else {
                    return None;
                };
                let range = node_range(file, argument);
                if !issue.matches(range) {
                    return None;
                }
                let module = variants_module(checker, snapshot, reference.type_name())?;
                let name =
                    string_type(argument).unwrap_or_else(|| &file.text()[range.start..range.end]);
                Some(Diagnostic::error(
                    "XC-002",
                    id,
                    range,
                    format!("'{name}' is not a variant group in '{}'.", module.0),
                ))
            }
            _ => None,
        },
    )
}

fn member_access(node: P<Node>) -> Option<(P<Node>, P<Node>)> {
    match node.kind() {
        Kind::PropertyAccessExpression => {
            let access = node.as_property_access_expression();
            Some((access.expression(), access.name()))
        }
        Kind::ElementAccessExpression => {
            let access = node.as_element_access_expression();
            Some((access.expression(), access.argument_expression()))
        }
        _ => None,
    }
}

pub(crate) fn provided_member_error(
    checker: &mut Checker,
    snapshot: &AnalysisSnapshot,
    file: P<SourceFile>,
    id: &FileId,
    node: P<Node>,
) -> Option<Diagnostic> {
    let (receiver, key) = member_access(node)?;
    let name = if node.kind() == Kind::PropertyAccessExpression || ast::is_string_literal_like(key)
    {
        key.text()
    } else {
        let ty = checker.get_type_at_location(key);
        if !ty.flags().intersects(TypeFlags::StringLiteral) {
            return None;
        }
        let LiteralValue::String(name) = ty.as_literal_type().value()? else {
            return None;
        };
        name
    };
    let module = dictionary_module(checker, snapshot, receiver, "cssVars")?;
    (!snapshot
        .module_interface(&module)?
        .provided_variables
        .contains_key(name))
    .then(|| {
        Diagnostic::error(
            "XC-006",
            id,
            node_range(file, key),
            format!(
                "Provided variable '{name}' can't be found in '{}'.",
                module.0
            ),
        )
    })
}

pub(crate) fn variant_errors(
    program: &Program,
    snapshot: &AnalysisSnapshot,
    precise: Issues,
    relaxed: &Issues,
) -> Vec<Diagnostic> {
    let candidates = precise
        .into_iter()
        .filter_map(|(file, mut issues)| {
            issues.retain(|issue| issue.is_value_error());
            (!issues.is_empty()).then_some((file, issues))
        })
        .collect();
    locate_issues(program, &candidates, |checker, file, id, node, issue| {
        let (expected, value) = match node.kind() {
            Kind::JsxAttribute => {
                let attribute = node.as_jsx_attribute();
                let value = attribute.initializer().unwrap_or_else(|| attribute.name());
                if !issue.matches(node_range(file, attribute.name()))
                    && !issue.matches(node_range(file, value))
                    && !issue.matches(node_range(file, node))
                {
                    return None;
                }
                (
                    checker.get_contextual_type_for_jsx_attribute_exported(node)?,
                    value,
                )
            }
            Kind::VariableDeclaration if issue.matches(node_range(file, node)) => {
                let value = node.as_variable_declaration().initializer()?;
                (
                    checker.get_contextual_type(value, ContextFlags::None)?,
                    value,
                )
            }
            Kind::PropertyAssignment if issue.matches(node_range(file, node)) => {
                let value = node.as_property_assignment().initializer();
                (
                    checker.get_contextual_type(value, ContextFlags::None)?,
                    value,
                )
            }
            _ if issue.matches(node_range(file, node)) && ast::is_expression_node(node) => {
                (checker.get_contextual_type(node, ContextFlags::None)?, node)
            }
            _ => return None,
        };
        let groups = contextual_groups(checker, snapshot, expected);
        let labels = groups
            .into_iter()
            .map(|(module, group)| format!("'{group}' in '{}'", module.0))
            .collect::<Vec<_>>();
        let message = match labels.as_slice() {
            [] => return None,
            [group] => format!("Value is not valid for variant group {group}."),
            _ => format!(
                "Value is not valid for any of these variant groups: {}.",
                labels.join(", ")
            ),
        };
        let value = if value.kind() == Kind::JsxExpression {
            value.as_jsx_expression().expression()?
        } else {
            value
        };
        // Relaxing member names removes string mismatches. A non-string value can
        // still fail in the relaxed program, so confirm that mismatch directly.
        if relaxed
            .get(id)
            .is_some_and(|issues| issues.contains(&issue))
            && !(node.kind() == Kind::JsxAttribute
                && node.as_jsx_attribute().initializer().is_none())
        {
            let actual = checker.get_type_at_location(value);
            if checker.is_type_assignable_to(actual, expected) {
                return None;
            }
        }
        Some(Diagnostic::error(
            "XC-003",
            id,
            node_range(file, value),
            message,
        ))
    })
}

fn locate_issues(
    program: &Program,
    issues: &Issues,
    mut classify: impl FnMut(&mut Checker, P<SourceFile>, &FileId, P<Node>, Issue) -> Option<Diagnostic>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let context = Context::default();
    for &file in program.source_files() {
        let id = file_id(file);
        let Some(issues) = issues.get(&id) else {
            continue;
        };
        let mut checker = program.get_type_checker_for_file_exclusive(&context, file);
        for &issue in issues {
            if let Some(diagnostic) = find_node(file.as_node(), issue, &mut |node| {
                classify(&mut checker, file, &id, node, issue)
            }) {
                diagnostics.push(diagnostic);
            }
        }
    }
    diagnostics
}

fn find_node(
    node: P<Node>,
    issue: Issue,
    classify: &mut impl FnMut(P<Node>) -> Option<Diagnostic>,
) -> Option<Diagnostic> {
    let loc = node.loc();
    let range = TextRange::new(loc.source_pos()? as usize, loc.source_end()? as usize);
    if !range.contains(issue.start) || issue.end > range.end {
        return None;
    }
    let mut found = None;
    node.for_each_child(&mut |child| {
        found = find_node(child, issue, classify);
        found.is_some()
    });
    found.or_else(|| classify(node))
}

pub(crate) fn node_range(file: P<SourceFile>, node: P<Node>) -> TextRange {
    TextRange::new(
        get_token_pos_of_node(node, file, false) as usize,
        node.loc().source_end().unwrap_or(0) as usize,
    )
}

fn file_id(file: P<SourceFile>) -> FileId {
    FileId(file.file_name().trim_start_matches('/').to_owned())
}

pub(crate) fn project_file(project: &Project, file: P<SourceFile>) -> Option<FileId> {
    let id = file_id(file);
    (!id.0.ends_with(".d.xcss.ts") && project.files.contains_key(&id)).then_some(id)
}

pub(crate) fn stylesheet(snapshot: &AnalysisSnapshot, declaration: P<Node>) -> Option<FileId> {
    let file = ast::get_source_file_of_node(declaration)?;
    let stem = file
        .file_name()
        .trim_start_matches('/')
        .strip_suffix(".d.xcss.ts")?;
    let stem = stem
        .split_once("__xcss_project_root__/")
        .map_or(stem, |(_, path)| path);
    let id = FileId(format!("{stem}.xcss"));
    snapshot.module_interface(&id).map(|_| id)
}

pub(crate) fn string_type(node: P<Node>) -> Option<&'static str> {
    if node.kind() != Kind::LiteralType {
        return None;
    }
    let literal = node.as_literal_type_node().literal();
    ast::is_string_literal_like(literal).then(|| literal.text())
}

pub(crate) fn styles_module(
    checker: &mut Checker,
    snapshot: &AnalysisSnapshot,
    receiver: P<Node>,
) -> Option<FileId> {
    dictionary_module(checker, snapshot, receiver, "styles")
}

fn dictionary_module(
    checker: &mut Checker,
    snapshot: &AnalysisSnapshot,
    receiver: P<Node>,
    name: &str,
) -> Option<FileId> {
    let ty = checker.get_type_at_location(receiver);
    let ty = checker.get_non_nullable_type(ty);
    let declaration = ty.symbol()?.declarations().iter().copied().find(|&decl| {
        decl.kind() == Kind::TypeLiteral
            && decl.parent().is_some_and(|parent| {
                parent.kind() == Kind::VariableDeclaration
                    && parent.as_variable_declaration().name().text() == name
            })
    })?;
    stylesheet(snapshot, declaration)
}

pub(crate) fn variants_module(
    checker: &mut Checker,
    snapshot: &AnalysisSnapshot,
    type_name: P<Node>,
) -> Option<FileId> {
    let mut symbol = checker.get_symbol_at_location_exported(type_name)?;
    if symbol.flags().intersects(SymbolFlags::Alias) {
        symbol = checker.get_aliased_symbol(symbol);
    }
    let declaration = symbol.declarations().iter().copied().find(|&decl| {
        decl.kind() == Kind::TypeAliasDeclaration
            && decl.as_type_alias_declaration().name().text() == "Variants"
    })?;
    stylesheet(snapshot, declaration)
}

pub(crate) fn contextual_groups(
    checker: &mut Checker,
    snapshot: &AnalysisSnapshot,
    expected: P<tsrs_checker::Type>,
) -> BTreeSet<(FileId, &'static str)> {
    let non_nullable = checker.get_non_nullable_type(expected);
    // The marker declaration identifies the XCSS module even when another
    // stylesheet declares the same group and member names.
    let mut groups = BTreeSet::new();
    let alternatives = if non_nullable.is_union() {
        non_nullable.as_union_type().types()
    } else {
        std::slice::from_ref(&non_nullable)
    };
    for &alternative in alternatives {
        let Some(marker) = checker.get_property_of_type(alternative, VARIANT_GROUP_MARKER) else {
            continue;
        };
        for &decl in marker.declarations() {
            if decl.kind() == Kind::PropertySignature
                && let Some(module) = stylesheet(snapshot, decl)
                && let Some(group) = decl
                    .as_property_signature_declaration()
                    .type_()
                    .and_then(string_type)
            {
                groups.insert((module, group));
            }
        }
    }
    groups
}
