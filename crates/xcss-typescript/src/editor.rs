use std::collections::{BTreeMap, BTreeSet};

use tsrs_ast::{self as ast, Kind, Node, SourceFile};
use tsrs_checker::{Checker, ContextFlags};
use tsrs_compiler::{Context, Program};
use tsrs_core::{LanguageVariant, P};
use xcss_analysis::AnalysisSnapshot;
use xcss_source::{FileId, Location, TextRange};

use crate::{
    Project,
    diagnostics::{contextual_groups, node_range, project_file, styles_module, variants_module},
    with_program,
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Symbol {
    Class(FileId, String),
    Group(FileId, String),
    Member(FileId, String, String),
}

pub struct Occurrence {
    pub location: Location,
    pub symbols: Vec<Symbol>,
}

pub struct Completion {
    pub label: String,
    pub kind: &'static str,
    pub range: TextRange,
    pub new_text: String,
}

#[derive(Default)]
pub struct EditorIndex {
    pub occurrences: Vec<Occurrence>,
    sites: BTreeMap<FileId, Vec<Site>>,
}

struct Site {
    range: TextRange,
    scope: Scope,
    insertion: Insertion,
}

enum Scope {
    Classes(FileId),
    Groups(FileId),
    Members(BTreeSet<(FileId, String)>),
}

#[derive(Clone, Copy)]
enum Insertion {
    Identifier,
    Quoted,
    String(char, bool),
}

impl Insertion {
    fn text(self, name: &str) -> Option<String> {
        match self {
            Self::Identifier => tsrs_scanner::is_identifier_text(name, LanguageVariant::Standard)
                .then(|| name.to_owned()),
            Self::Quoted => serde_json::to_string(name).ok(),
            Self::String(quote, true) => {
                let escaped = name.replace('&', "&amp;").replace('<', "&lt;");
                Some(if quote == '"' {
                    escaped.replace('"', "&quot;")
                } else {
                    escaped.replace('\'', "&apos;")
                })
            }
            Self::String(quote, false) => {
                let quoted = serde_json::to_string(name).ok()?;
                let body = &quoted[1..quoted.len() - 1];
                Some(match quote {
                    '\'' => body.replace('\'', "\\'"),
                    '\u{60}' => body.replace('\u{60}', "\\\u{60}").replace("${", "\\${"),
                    _ => body.to_owned(),
                })
            }
        }
    }
}

impl Scope {
    fn resolve(&self, snapshot: &AnalysisSnapshot, name: &str) -> Vec<Symbol> {
        match self {
            Self::Classes(file)
                if snapshot
                    .module_interface(file)
                    .is_some_and(|i| i.classes.contains(name)) =>
            {
                vec![Symbol::Class(file.clone(), name.into())]
            }
            Self::Groups(file)
                if snapshot
                    .module_interface(file)
                    .is_some_and(|i| i.variants.contains_key(name)) =>
            {
                vec![Symbol::Group(file.clone(), name.into())]
            }
            Self::Members(groups) => groups
                .iter()
                .filter_map(|(file, group)| {
                    let interface = snapshot.module_interface(file)?;
                    (interface.classes.contains(name)
                        && interface
                            .variants
                            .get(group)?
                            .iter()
                            .any(|member| member == name))
                    .then(|| Symbol::Member(file.clone(), group.clone(), name.into()))
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn candidates(&self, snapshot: &AnalysisSnapshot) -> BTreeSet<Symbol> {
        match self {
            Self::Classes(file) => snapshot
                .module_interface(file)
                .into_iter()
                .flat_map(|interface| &interface.classes)
                .map(|name| Symbol::Class(file.clone(), name.clone()))
                .collect(),
            Self::Groups(file) => snapshot
                .module_interface(file)
                .into_iter()
                .flat_map(|interface| interface.variants.keys())
                .map(|name| Symbol::Group(file.clone(), name.clone()))
                .collect(),
            Self::Members(groups) => groups
                .iter()
                .flat_map(|(file, group)| {
                    snapshot
                        .module_interface(file)
                        .and_then(|interface| interface.variants.get(group))
                        .into_iter()
                        .flatten()
                        .filter(|name| {
                            snapshot
                                .module_interface(file)
                                .is_some_and(|i| i.classes.contains(*name))
                        })
                        .map(|name| Symbol::Member(file.clone(), group.clone(), name.clone()))
                })
                .collect(),
        }
    }
}

impl Symbol {
    fn label(&self) -> &str {
        match self {
            Self::Class(_, name) | Self::Group(_, name) | Self::Member(_, _, name) => name,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Class(..) => "class",
            Self::Group(..) => "variantGroup",
            Self::Member(..) => "variantMember",
        }
    }
}

impl EditorIndex {
    pub fn completion(
        &self,
        snapshot: &AnalysisSnapshot,
        file: &FileId,
        offset: usize,
    ) -> Vec<Completion> {
        let Some(site) = self
            .sites
            .get(file)
            .into_iter()
            .flatten()
            .filter(|site| site.range.start <= offset && offset <= site.range.end)
            .min_by_key(|site| site.range.end - site.range.start)
        else {
            return Vec::new();
        };
        let candidates: BTreeMap<_, _> = site
            .scope
            .candidates(snapshot)
            .into_iter()
            .filter_map(|symbol| {
                let label = symbol.label();
                let new_text = site.insertion.text(label)?;
                Some((label.to_owned(), (symbol.kind(), new_text)))
            })
            .collect();
        candidates
            .into_iter()
            .map(|(label, (kind, new_text))| Completion {
                label,
                kind,
                range: site.range,
                new_text,
            })
            .collect()
    }
}

pub fn editor_index(project: &Project, snapshot: &AnalysisSnapshot) -> Result<EditorIndex, String> {
    with_program(project, snapshot, false, |program| {
        collect(program, project, snapshot)
    })
}

fn collect(program: &Program, project: &Project, snapshot: &AnalysisSnapshot) -> EditorIndex {
    let mut index = EditorIndex::default();
    let context = Context::default();
    for &file in program.source_files() {
        let Some(id) = project_file(project, file) else {
            continue;
        };
        if id.0.split('/').any(|part| part == "node_modules")
            || ![".ts", ".tsx", ".mts", ".cts"]
                .iter()
                .any(|ext| id.0.ends_with(ext))
        {
            continue;
        }
        let mut checker = program.get_type_checker_for_file_exclusive(&context, file);
        let mut collector = Collector {
            snapshot,
            id: &id,
            file,
            checker: &mut checker,
            index: &mut index,
        };
        collector.visit(file.as_node());
    }
    index
}

struct Collector<'a> {
    snapshot: &'a AnalysisSnapshot,
    id: &'a FileId,
    file: P<SourceFile>,
    checker: &'a mut Checker,
    index: &'a mut EditorIndex,
}

impl Collector<'_> {
    fn visit(&mut self, node: P<Node>) {
        match node.kind() {
            Kind::PropertyAccessExpression => {
                let access = node.as_property_access_expression();
                if let Some(module) =
                    styles_module(self.checker, self.snapshot, access.expression())
                {
                    let name = access.name();
                    let range = node_range(self.file, name);
                    self.site(
                        range,
                        Scope::Classes(module),
                        Insertion::Identifier,
                        Some(name.text()),
                    );
                }
            }
            Kind::ElementAccessExpression => {
                let access = node.as_element_access_expression();
                if let Some(module) =
                    styles_module(self.checker, self.snapshot, access.expression())
                {
                    let key = access.argument_expression();
                    if ast::is_string_literal_like(key) {
                        self.string(key, Scope::Classes(module), false);
                    } else {
                        let range = node_range(self.file, key);
                        if range.start == range.end {
                            self.site(range, Scope::Classes(module), Insertion::Quoted, None);
                        }
                    }
                }
            }
            Kind::TypeReference => {
                let reference = node.as_type_reference_node();
                if let Some(module) =
                    variants_module(self.checker, self.snapshot, reference.type_name())
                {
                    if let Some(args) = reference.type_arguments() {
                        for &argument in args.nodes() {
                            self.group_argument(argument, &module);
                        }
                        if args.nodes().is_empty() {
                            self.empty_group_argument(node, reference.type_name(), module);
                        }
                    } else {
                        self.empty_group_argument(node, reference.type_name(), module);
                    }
                }
            }
            Kind::StringLiteral | Kind::NoSubstitutionTemplateLiteral => {
                self.contextual_string(node);
            }
            _ => {}
        }
        node.for_each_child(&mut |child| {
            self.visit(child);
            false
        });
    }

    fn group_argument(&mut self, node: P<Node>, module: &FileId) {
        if node.kind() == Kind::UnionType {
            for &part in node.as_union_type_node().types().nodes() {
                self.group_argument(part, module);
            }
        } else if node.kind() == Kind::LiteralType {
            let literal = node.as_literal_type_node().literal();
            if ast::is_string_literal_like(literal) {
                self.string(literal, Scope::Groups(module.clone()), false);
            }
        }
    }

    fn empty_group_argument(&mut self, node: P<Node>, name: P<Node>, module: FileId) {
        let start = node_range(self.file, name).end;
        let end = node_range(self.file, node).end;
        let source = self.file.text();
        let Some(tail) = source.get(start..) else {
            return;
        };
        let trimmed = tail.trim_start_matches([' ', '\t']);
        let Some(after) = trimmed.strip_prefix('<') else {
            return;
        };
        let open_end = source.len() - after.len();
        let whitespace_end = source.len() - after.trim_start_matches([' ', '\t']).len();
        if open_end <= end || start == end {
            self.site(
                TextRange::new(open_end, whitespace_end),
                Scope::Groups(module),
                Insertion::Quoted,
                None,
            );
        }
    }

    fn contextual_string(&mut self, node: P<Node>) {
        let parent = node.parent();
        if parent.is_some_and(|p| {
            p.kind() == Kind::LiteralType
                || p.kind() == Kind::ElementAccessExpression
                    && p.as_element_access_expression().argument_expression() == node
        }) {
            return;
        }
        let jsx = parent.filter(|p| p.kind() == Kind::JsxAttribute);
        let expected = if let Some(attribute) = jsx {
            self.checker
                .get_contextual_type_for_jsx_attribute_exported(attribute)
        } else {
            self.checker.get_contextual_type(node, ContextFlags::None)
        };
        let Some(expected) = expected else { return };
        let groups: BTreeSet<_> = contextual_groups(self.checker, self.snapshot, expected)
            .into_iter()
            .map(|(module, group)| (module, group.to_owned()))
            .collect();
        if !groups.is_empty() {
            self.string(node, Scope::Members(groups), jsx.is_some());
        }
    }

    fn string(&mut self, node: P<Node>, scope: Scope, jsx: bool) {
        let mut range = node_range(self.file, node);
        let Some(raw) = self.file.text().get(range.start..range.end) else {
            return;
        };
        let Some(quote @ ('"' | '\'' | '\u{60}')) = raw.chars().next() else {
            return;
        };
        range.start += 1;
        if raw.len() > 1 && raw.ends_with(quote) {
            let backslashes = raw.as_bytes()[..raw.len() - 1]
                .iter()
                .rev()
                .take_while(|&&b| b == b'\\')
                .count();
            if jsx || backslashes % 2 == 0 {
                range.end -= 1;
            }
        }
        self.site(
            range,
            scope,
            Insertion::String(quote, jsx),
            Some(node.text()),
        );
    }

    fn site(&mut self, range: TextRange, scope: Scope, insertion: Insertion, name: Option<&str>) {
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            let symbols = scope.resolve(self.snapshot, name);
            if !symbols.is_empty() {
                self.index.occurrences.push(Occurrence {
                    location: Location {
                        file: self.id.clone(),
                        range,
                    },
                    symbols,
                });
            }
        }
        self.index
            .sites
            .entry(self.id.clone())
            .or_default()
            .push(Site {
                range,
                scope,
                insertion,
            });
    }
}
