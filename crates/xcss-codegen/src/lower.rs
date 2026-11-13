use super::*;
#[derive(Clone, Default)]
pub(super) struct Frame {
    pub parameters: BTreeMap<SymbolId, Mapped>,
    pub content: Option<Box<Content>>,
}
#[derive(Clone)]
pub(super) struct Content {
    file: FileId,
    nodes: Vec<Node>,
    frame: Frame,
}
#[derive(Clone)]
struct Statement {
    kind: NodeKind,
    header: Mapped,
    children: Option<Vec<Statement>>,
    origin: Location,
    preserve_content: bool,
}
pub(super) struct Lowerer<'a> {
    pub snapshot: &'a AnalysisSnapshot,
    pub entry: &'a FileId,
    pub output_identity: &'a str,
    pub source_root: Option<&'a str>,
    pub asset_resolutions: Option<&'a AssetResolutions>,
    pub sources: BTreeSet<FileId>,
    pub properties: BTreeMap<String, String>,
    pub diagnostics: Vec<Diagnostic>,
    pub registrations: BTreeSet<SymbolId>,
    pub globals: BTreeMap<String, String>,
    pub native_imports: BTreeSet<String>,
    pub constant_depth: usize,
    pub constant_expansions: usize,
    pub utilities: BTreeMap<String, Mapped>,
    pub utility_order: Vec<String>,
    pub utility_classes: BTreeMap<String, Vec<String>>,
    pub utility_output_identity: &'a str,
    selector: Option<String>,
    conditions: Vec<(Mapped, Location)>,
    injected_utils: Mapped,
    util_depth: usize,
    count: usize,
}
impl<'a> Lowerer<'a> {
    pub fn new(
        snapshot: &'a AnalysisSnapshot,
        entry: &'a FileId,
        output_identity: &'a str,
        source_root: Option<&'a str>,
    ) -> Self {
        Self {
            snapshot,
            entry,
            output_identity,
            source_root,
            asset_resolutions: None,
            sources: BTreeSet::new(),
            properties: BTreeMap::new(),
            diagnostics: Vec::new(),
            registrations: BTreeSet::new(),
            globals: BTreeMap::new(),
            native_imports: BTreeSet::new(),
            constant_depth: 0,
            constant_expansions: 0,
            utilities: BTreeMap::new(),
            utility_order: Vec::new(),
            utility_classes: BTreeMap::new(),
            utility_output_identity: output_identity,
            selector: None,
            conditions: Vec::new(),
            injected_utils: Mapped::default(),
            util_depth: 0,
            count: 0,
        }
    }
    pub fn asset_url(
        &self,
        file: &FileId,
        url: &str,
        range: TextRange,
    ) -> Result<String, Vec<Diagnostic>> {
        let decoded = xcss_syntax::ast::decode_identifier(url);
        if let Some(resolution) = self
            .asset_resolutions
            .and_then(|m| m.get(file))
            .and_then(|m| m.get(&decoded))
        {
            let target = resolution.as_ref().map_err(|error| {
                vec![Diagnostic::error(
                    "generate.asset-alias",
                    file,
                    range,
                    error,
                )]
            })?;
            let root = self.source_root.unwrap_or("");
            let source = FileId(if root.is_empty() {
                "__xcss_assets__.xcss".into()
            } else {
                format!("{root}/__xcss_assets__.xcss")
            });
            return Ok(values::rebase(&source, self.output_identity, target));
        }
        let source = self
            .source_root
            .map(|root| FileId(format!("{root}/{}", file.0)))
            .unwrap_or_else(|| file.clone());
        Ok(values::rebase(&source, self.output_identity, url))
    }
    pub fn module(
        &mut self,
        positions: Option<&BTreeMap<String, BTreeSet<usize>>>,
    ) -> Result<Mapped, Vec<Diagnostic>> {
        let parse = self.snapshot.syntax(self.entry).unwrap();
        let statements = self.nodes(self.entry, &parse.tree.children, &Frame::default(), 0)?;
        let mut out = Mapped::default();
        // Registrations are gathered while lowering, including references captured by injections.
        let registrations = self.registrations.clone();
        let mut registrations_text = Mapped::default();
        for id in registrations {
            registrations_text.append(&registration::emit(self, id)?);
        }
        for source in &self.sources {
            if let Some(parse) = self.snapshot.syntax(source) {
                for token in parse.tokens.iter().filter(|t| {
                    t.kind == xcss_syntax::TokenKind::Comment
                        && parse.slice(t.range).starts_with("/*!")
                }) {
                    out.push(
                        parse.slice(token.range),
                        &Location {
                            file: source.clone(),
                            range: token.range,
                        },
                    );
                    out.push(
                        "\n",
                        &Location {
                            file: source.clone(),
                            range: token.range,
                        },
                    );
                }
            }
        }
        // Native @imports must precede registrations and ordinary rules.
        for statement in statements.iter().filter(|s| {
            s.kind == NodeKind::Import || s.header.text.trim_start().starts_with("@charset ")
        }) {
            self.print(statement, &mut out);
        }
        out.append(&registrations_text);
        for statement in statements.iter().filter(|s| {
            s.kind != NodeKind::Import && !s.header.text.trim_start().starts_with("@charset ")
        }) {
            self.print(statement, &mut out);
            if let Some(positions) = positions
                && statement.kind == NodeKind::Rule
                && statement.origin.file == *self.entry
                && let Some(node) = parse.tree.children.iter().find(|node| {
                    node.kind == NodeKind::Rule && node.header == statement.origin.range
                })
            {
                self.print_positional(parse, node, statement, positions, &mut out);
            }
        }
        out.append(&self.injected_utils);
        Ok(out)
    }

    fn print_positional(
        &self,
        parse: &ParseResult,
        node: &Node,
        statement: &Statement,
        positions: &BTreeMap<String, BTreeSet<usize>>,
        out: &mut Mapped,
    ) {
        let children: Vec<_> = statement
            .children
            .iter()
            .flatten()
            .filter(|child| child.kind == NodeKind::Declaration)
            .cloned()
            .collect();
        if children.is_empty() {
            return;
        }
        let mut selectors = BTreeSet::new();
        for (selector, class, name) in xcss_syntax::subject_class_selectors(parse, node) {
            let Some(indices) = positions.get(&name) else {
                continue;
            };
            for &index in indices {
                let alias = positional_class(self.entry, &name, index);
                let left =
                    without_line_comments(parse.slice(TextRange::new(selector.start, class.end)));
                let right =
                    without_line_comments(parse.slice(TextRange::new(class.end, selector.end)));
                let boost = if index == 0 {
                    String::new()
                } else {
                    format!(":is(*, {})", "#_".repeat(index))
                };
                selectors.insert(normalize_selector(&format!(
                    "{left}:where(:global(.{alias})){boost}{right}"
                )));
            }
        }
        if !selectors.is_empty() {
            let mut header = Mapped::default();
            header.push(
                &selectors.into_iter().collect::<Vec<_>>().join(", "),
                &statement.origin,
            );
            self.print(
                &Statement {
                    header,
                    children: Some(children),
                    ..statement.clone()
                },
                out,
            );
        }
    }
    fn nodes(
        &mut self,
        file: &FileId,
        nodes: &[Node],
        frame: &Frame,
        depth: usize,
    ) -> Result<Vec<Statement>, Vec<Diagnostic>> {
        self.sources.insert(file.clone());
        let parse = self.snapshot.syntax(file).unwrap();
        let mut out = Vec::new();
        for node in nodes {
            let origin = Location {
                file: file.clone(),
                range: node.header,
            };
            self.count += 1;
            if depth >= 128 || self.count > 100_000 {
                return Err(vec![Diagnostic::error(
                    "generate.expansion-limit",
                    file,
                    node.header,
                    "Injection expansion exceeds the depth or statement limit",
                )]);
            }
            match node.kind {
                NodeKind::Raw => {
                    let range = node.raw_body().unwrap();
                    let body = parse.slice(range).to_owned();
                    // Exact definition identities avoid collisions across merged artifacts.
                    let identity: String = file.0.bytes().map(|b| format!("{b:02x}")).collect();
                    let mut marker =
                        format!("@--xcss-raw-{identity}-{}-{}", range.start, range.end);
                    while self.snapshot.files().any(|f| {
                        let source = self.snapshot.syntax(f).unwrap();
                        source.source.text().contains(&marker)
                            || source.tokens.iter().any(|t| {
                                t.kind == xcss_syntax::TokenKind::Word
                                    && xcss_syntax::ast::decode_identifier(source.token_text(t))
                                        == marker[1..]
                            })
                    }) {
                        marker.push('x');
                    }
                    let location = Location {
                        file: file.clone(),
                        range,
                    };
                    let mut header = Mapped::default();
                    header.push(&marker, &location);
                    header.raw.insert(marker, (body, location));
                    out.push(Statement {
                        kind: NodeKind::Raw,
                        header,
                        children: None,
                        origin,
                        preserve_content: false,
                    });
                }
                NodeKind::Vars => {
                    for variable in node.children.iter().flat_map(|child| {
                        if child.kind == NodeKind::Provided {
                            child.children.as_slice()
                        } else {
                            std::slice::from_ref(child)
                        }
                    }) {
                        if variable.kind == NodeKind::Raw {
                            out.extend(self.nodes(
                                file,
                                std::slice::from_ref(variable),
                                frame,
                                depth,
                            )?);
                            continue;
                        }
                        if let Some(syntax) = parse.variable(variable)
                            && let Some(symbol) = self.snapshot.symbol_at(file, syntax.range.start)
                        {
                            self.registrations.insert(symbol.id);
                        }
                    }
                }
                NodeKind::Injection | NodeKind::Util | NodeKind::Constant | NodeKind::Variants => {}
                NodeKind::Import if parse.injection_import(node).is_some() => {}
                NodeKind::Call => {
                    let Some(target) = self.snapshot.resolved_call(file, node) else {
                        return Err(vec![Diagnostic::error(
                            "generate.unresolved-call",
                            file,
                            node.header,
                            "Injection call has no analyzed target",
                        )]);
                    };
                    let (definition, body) = self.snapshot.injection_definition(target).unwrap();
                    let def_file = definition.source.id.clone();
                    let syntax = parse.call(node).unwrap();
                    let signature = self.snapshot.signature(target).unwrap();
                    let definition_syntax = definition.injection(body).unwrap();
                    let call_output = self.output_identity;
                    if body.kind == NodeKind::Util
                        && self.util_depth == 0
                        && !syntax.as_injection
                        && self
                            .selector
                            .as_ref()
                            .is_some_and(|s| !composition_classes(s).is_empty())
                    {
                        self.output_identity = self.utility_output_identity;
                    }
                    let mut call_frame = Frame::default();
                    for (index, parameter) in signature.parameters.iter().enumerate() {
                        let value = if let Some(argument) = syntax.arguments.get(index) {
                            values::render(
                                self,
                                parse,
                                argument,
                                frame,
                                values::Context::default(),
                            )?
                        } else {
                            let parameter_syntax = &definition_syntax.parameters[index];
                            let Some(default) = parameter_syntax.default else {
                                return Err(vec![Diagnostic::error(
                                    "generate.argument",
                                    file,
                                    node.header,
                                    "Missing required injection argument",
                                )]);
                            };
                            let start = parameter_syntax.range.end
                                + definition.source.text()
                                    [parameter_syntax.range.end..body.header.end]
                                    .find('=')
                                    .unwrap()
                                + 1;
                            let end = start
                                + definition.source.text()[start..body.header.end]
                                    .find(default)
                                    .unwrap_or(0)
                                + default.len();
                            let range = TextRange::new(end - default.len(), end);
                            values::render(
                                self,
                                definition,
                                &definition.value(range),
                                &call_frame,
                                values::Context::default(),
                            )?
                        };
                        call_frame.parameters.insert(parameter.symbol, value);
                    }
                    if node.block.is_some() {
                        call_frame.content = Some(Box::new(Content {
                            file: file.clone(),
                            nodes: node.children.clone(),
                            frame: frame.clone(),
                        }));
                    }
                    let is_util = body.kind == NodeKind::Util;
                    if is_util && self.util_depth == 0 {
                        let Some(selector) = self.selector.clone() else {
                            return Err(vec![Diagnostic::error(
                                "generate.util-selector",
                                file,
                                node.header,
                                "A util call requires a selector",
                            )]);
                        };
                        let owners = if syntax.as_injection {
                            Vec::new()
                        } else {
                            composition_classes(&selector)
                        };
                        let arguments: Vec<_> = signature
                            .parameters
                            .iter()
                            .map(|p| canonical(&call_frame.parameters[&p.symbol].text))
                            .collect();
                        let symbol = self.snapshot.symbol(target).unwrap();
                        let identity = format!("{}:{}:{:?}", def_file.0, symbol.name, arguments);
                        let mut label = std::iter::once(readable(&symbol.name))
                            .chain(arguments.iter().map(|v| readable(v)))
                            .collect::<Vec<_>>()
                            .join("_");
                        let duplicate_name = self
                            .snapshot
                            .symbols()
                            .iter()
                            .filter(|s| {
                                matches!(s.kind, SymbolKind::Util | SymbolKind::Injection)
                                    && s.name == symbol.name
                            })
                            .count()
                            > 1;
                        if duplicate_name
                            || !symbol
                                .name
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '-')
                            || arguments
                                .iter()
                                .any(|v| !v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
                            || self.snapshot.symbols().iter().any(|s| {
                                s.kind == SymbolKind::Class
                                    && s.global
                                    && s.name == format!("_util_{label}")
                            })
                        {
                            label.push_str(&format!("_{:016x}", stable_hash(&identity)));
                        }
                        let class = format!("_util_{label}");
                        let old_output = call_output;
                        if !owners.is_empty() {
                            self.output_identity = self.utility_output_identity;
                        }
                        self.util_depth += 1;
                        let expanded =
                            self.nodes(&def_file, &body.children, &call_frame, depth + 1);
                        self.util_depth -= 1;
                        self.output_identity = old_output;
                        let expanded = expanded?;
                        let mut css = Mapped::default();
                        css.push("@layer utils {\n", &origin);
                        for (condition, location) in &self.conditions {
                            css.append(condition);
                            css.push(" {\n", location);
                        }
                        let header = if owners.is_empty() {
                            format!("_util_inj_\\[{label}\\], {selector}")
                        } else {
                            format!(":global(.{class})")
                        };
                        css.push(&header, &origin);
                        css.push(" {\n", &origin);
                        for statement in &expanded {
                            self.print(statement, &mut css);
                        }
                        css.push("}\n", &origin);
                        for (_, location) in self.conditions.iter().rev() {
                            css.push("}\n", location);
                        }
                        css.push("}\n", &origin);
                        if owners.is_empty() {
                            self.injected_utils.append(&css);
                        } else {
                            for owner in owners {
                                self.utility_classes
                                    .entry(owner)
                                    .or_default()
                                    .push(class.clone());
                            }
                            let key = format!(
                                "{identity}:{:?}",
                                self.conditions
                                    .iter()
                                    .map(|(m, _)| &m.text)
                                    .collect::<Vec<_>>()
                            );
                            if !self.utilities.contains_key(&key) {
                                self.utility_order.push(key.clone());
                                self.utilities.insert(key, css);
                            }
                        }
                    } else {
                        out.extend(self.nodes(
                            &def_file,
                            &body.children,
                            &call_frame,
                            depth + 1,
                        )?);
                        self.output_identity = call_output;
                    }
                }
                NodeKind::Content => {
                    if let Some(content) = &frame.content {
                        out.extend(self.nodes(
                            &content.file,
                            &content.nodes,
                            &content.frame,
                            depth + 1,
                        )?);
                    }
                }
                NodeKind::Declaration => {
                    let Some(declaration) = parse.declaration(node) else {
                        continue;
                    };
                    let mut header = Mapped::default();
                    let property = values::property_name(
                        self,
                        file,
                        declaration.property,
                        declaration.property_range,
                    );
                    header.push(
                        &property,
                        &Location {
                            file: file.clone(),
                            range: declaration.property_range,
                        },
                    );
                    header.push(": ", &origin);
                    let value = if declaration.property.eq_ignore_ascii_case("transition") {
                        values::transition(self, parse, &declaration.value, frame)?
                    } else {
                        values::render(
                            self,
                            parse,
                            &declaration.value,
                            frame,
                            values::Context {
                                name: declaration
                                    .property
                                    .eq_ignore_ascii_case("transition-property"),
                                arithmetic: !matches!(
                                    declaration.property.to_ascii_lowercase().as_str(),
                                    "font"
                                        | "font-family"
                                        | "border-radius"
                                        | "background"
                                        | "background-position"
                                        | "background-size"
                                        | "grid-area"
                                        | "grid-row"
                                        | "grid-column"
                                ),
                                math: false,
                            },
                        )?
                    };
                    header.append(&value);
                    out.push(Statement {
                        kind: node.kind,
                        header,
                        children: None,
                        origin,
                        preserve_content:declaration.property.eq_ignore_ascii_case("content") && matches!(&declaration.value.kind,xcss_syntax::ast::ValueKind::Sequence(parts) if parts.len()==1 && parse.slice(parts[0].range)=="preserve"),
                    });
                }
                _ => {
                    let mut header = Mapped::default();
                    let cleaned_header = without_line_comments(parse.slice(node.header));
                    let raw = cleaned_header.trim();
                    if node.kind == NodeKind::Rule {
                        header.push(&normalize_selector(raw), &origin);
                    } else if let Some(keyframes) = parse.keyframes(node).filter(|k| k.scope_switch)
                    {
                        if keyframes.global {
                            let decoded = xcss_syntax::ast::decode_identifier(keyframes.name);
                            let placeholder = global_keyframe(&decoded);
                            self.globals.insert(placeholder.clone(), decoded);
                            header.push(&format!("@{} {placeholder}", keyframes.keyword), &origin);
                        } else {
                            header.push(
                                &format!("@{} {}", keyframes.keyword, keyframes.literal),
                                &origin,
                            );
                        }
                    } else if node.kind == NodeKind::Import
                        && native_import_string(parse, node).is_some()
                    {
                        let token = native_import_string(parse, node).unwrap();
                        header.push(
                            &parse.source.text()[node.header.start..token.range.start],
                            &origin,
                        );
                        let url = self.asset_url(
                            file,
                            parse.token_text(token).trim_matches(['\'', '"']),
                            token.range,
                        )?;
                        header.push(
                            &serde_json::to_string(&url).unwrap(),
                            &Location {
                                file: file.clone(),
                                range: token.range,
                            },
                        );
                        header.push(
                            &parse.source.text()[token.range.end..node.header.end],
                            &origin,
                        );
                    } else {
                        header = values::render(
                            self,
                            parse,
                            &parse.value(node.header),
                            frame,
                            values::Context {
                                name: true,
                                arithmetic: false,
                                math: false,
                            },
                        )?;
                    }
                    let old_selector = self.selector.clone();
                    let old_conditions = self.conditions.len();
                    if self.util_depth == 0 {
                        if node.kind == NodeKind::Rule {
                            self.selector =
                                Some(resolve_selector(old_selector.as_deref(), &header.text));
                        } else if node.kind == NodeKind::AtRule
                            && node.block.is_some()
                            && !header.text.trim_start().starts_with("@layer")
                        {
                            self.conditions.push((header.clone(), origin.clone()));
                        }
                    }
                    let children = if node.block.is_some() {
                        Some(self.nodes(file, &node.children, frame, depth)?)
                    } else {
                        None
                    };
                    self.selector = old_selector;
                    self.conditions.truncate(old_conditions);
                    if node.kind == NodeKind::Import {
                        let parsed =
                            xcss_syntax::parse(std::sync::Arc::new(xcss_source::SourceFile::new(
                                file.clone(),
                                0,
                                format!("{};", header.text),
                            )));
                        if let Some(import) = parsed
                            .tokens
                            .iter()
                            .filter(|t| {
                                !matches!(
                                    t.kind,
                                    xcss_syntax::TokenKind::Whitespace
                                        | xcss_syntax::TokenKind::Comment
                                )
                            })
                            .nth(2)
                        {
                            if import.kind == xcss_syntax::TokenKind::String {
                                self.native_imports.insert(
                                    parsed.token_text(import).trim_matches(['\'', '"']).into(),
                                );
                            } else if parsed.token_text(import).eq_ignore_ascii_case("url")
                                && let Some(start) = header.text.find('(')
                                && let Some(end) = header.text[start + 1..].find(')')
                            {
                                self.native_imports.insert(
                                    header.text[start + 1..start + 1 + end]
                                        .trim()
                                        .trim_matches(['\'', '"'])
                                        .into(),
                                );
                            }
                        }
                    }
                    out.push(Statement {
                        kind: node.kind,
                        header,
                        children,
                        origin,
                        preserve_content: false,
                    });
                }
            }
        }
        Ok(out)
    }
    fn print(&self, statement: &Statement, out: &mut Mapped) {
        out.append(&statement.header);
        if let Some(children) = &statement.children {
            out.push(" {\n", &statement.origin);
            let eligible = statement.kind == NodeKind::Rule
                && xcss_syntax::selector_needs_content(&statement.header.text);
            let has_content = children.iter().any(|s| {
                s.kind == NodeKind::Declaration
                    && s.header
                        .text
                        .split_once(':')
                        .is_some_and(|(p, _)| p.trim().eq_ignore_ascii_case("content"))
            });
            if eligible && !has_content {
                out.push("content: '';\n", &statement.origin);
            }
            for child in children {
                if child.preserve_content {
                    continue;
                }
                self.print(child, out);
            }
            out.push("}\n", &statement.origin);
        } else {
            out.push(";\n", &statement.origin);
        }
    }
}
pub(super) fn global_keyframe(name: &str) -> String {
    format!(
        "__xcss_global_{}",
        scoped_property(&FileId("keyframes".into()), name).replace('-', "_")
    )
}
fn native_import_string<'a>(parse: &'a ParseResult, node: &Node) -> Option<&'a xcss_syntax::Token> {
    parse
        .tokens
        .iter()
        .filter(|t| {
            node.header.contains(t.range.start)
                && !matches!(
                    t.kind,
                    xcss_syntax::TokenKind::Whitespace | xcss_syntax::TokenKind::Comment
                )
        })
        .nth(2)
        .filter(|t| t.kind == xcss_syntax::TokenKind::String)
}

fn parsed_text(text: &str) -> ParseResult {
    xcss_syntax::parse(std::sync::Arc::new(xcss_source::SourceFile::new(
        FileId("utility".into()),
        0,
        text.to_owned(),
    )))
}
fn canonical(text: &str) -> String {
    let parse = parsed_text(text);
    parse
        .tokens
        .iter()
        .filter(|t| {
            !matches!(
                t.kind,
                xcss_syntax::TokenKind::Whitespace | xcss_syntax::TokenKind::Comment
            )
        })
        .map(|t| parse.token_text(t))
        .collect::<Vec<_>>()
        .join(" ")
}
fn readable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
fn stable_hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, b| {
        (hash ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
fn composition_classes(selector: &str) -> Vec<String> {
    let members = xcss_syntax::analyze_selectors(selector, 0, None);
    let mut classes = Vec::new();
    for member in members {
        let mut text = selector[member.range.start..member.range.end].trim();
        if let Some(local) = text
            .strip_prefix(":local(")
            .and_then(|s| s.strip_suffix(')'))
        {
            text = local.trim();
        }
        let parse = parsed_text(text);
        let tokens: Vec<_> = parse
            .tokens
            .iter()
            .filter(|t| {
                !matches!(
                    t.kind,
                    xcss_syntax::TokenKind::Whitespace | xcss_syntax::TokenKind::Comment
                )
            })
            .collect();
        if tokens.len() != 2
            || parse.token_text(tokens[0]) != "."
            || tokens[1].kind != xcss_syntax::TokenKind::Word
        {
            return Vec::new();
        }
        classes.push(xcss_syntax::ast::decode_identifier(
            parse.token_text(tokens[1]),
        ));
    }
    classes
}
fn resolve_selector(parent: Option<&str>, child: &str) -> String {
    let Some(parent) = parent else {
        return child.to_owned();
    };
    let parent = format!(":is({parent})");
    xcss_syntax::analyze_selectors(child, 0, Some(None))
        .iter()
        .map(|member| {
            let text = &child[member.range.start..member.range.end];
            let parse = parsed_text(text);
            let nesting: Vec<_> = parse
                .tokens
                .iter()
                .filter(|t| {
                    t.kind == xcss_syntax::TokenKind::Punctuation && parse.token_text(t) == "&"
                })
                .collect();
            if nesting.is_empty() {
                format!("{parent} {}", text.trim())
            } else {
                let mut resolved = text.to_owned();
                for token in nesting.into_iter().rev() {
                    resolved.replace_range(token.range.start..token.range.end, &parent);
                }
                resolved
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}
fn normalize_selector(raw: &str) -> String {
    // Scope switches only occur outside strings and attribute selectors; parse token ranges.
    let parse = xcss_syntax::parse(std::sync::Arc::new(xcss_source::SourceFile::new(
        FileId("selector".into()),
        0,
        format!("{raw} {{}}"),
    )));
    let range = TextRange::new(0, raw.len());
    let tokens: Vec<_> = parse
        .tokens
        .iter()
        .filter(|t| {
            t.range.end <= range.end
                && !matches!(
                    t.kind,
                    xcss_syntax::TokenKind::Whitespace | xcss_syntax::TokenKind::Comment
                )
        })
        .collect();
    let mut out = String::new();
    let mut cursor = 0;
    let mut depth = 0usize;
    let mut open = false;
    for (i, token) in tokens.iter().enumerate() {
        let text = parse.token_text(token);
        if text == ":"
            && depth == 0
            && tokens
                .get(i + 1)
                .is_some_and(|t| matches!(parse.token_text(t), "global" | "local"))
            && tokens.get(i + 2).is_none_or(|t| parse.token_text(t) != "(")
        {
            if token.range.start < cursor {
                continue;
            }
            out.push_str(&raw[cursor..token.range.start]);
            if open {
                out.push(')');
                out.push(' ');
            }
            out.push(':');
            out.push_str(parse.token_text(tokens[i + 1]));
            out.push('(');
            cursor = tokens[i + 1].range.end;
            while cursor < raw.len() && raw[cursor..].starts_with(char::is_whitespace) {
                cursor += raw[cursor..].chars().next().unwrap().len_utf8();
            }
            open = true;
        } else if text == "," && depth == 0 && open {
            out.push_str(raw[cursor..token.range.start].trim_end());
            out.push(')');
            cursor = token.range.start;
            open = false;
        }
        match text {
            "(" | "[" => depth += 1,
            ")" | "]" => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    out.push_str(raw[cursor..].trim_end());
    if open {
        out.push(')');
    }
    out
}
