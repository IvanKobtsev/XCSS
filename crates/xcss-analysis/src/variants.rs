use super::*;

/// The static contract of an XCSS module, independent of CSS generation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModuleInterface {
    pub classes: BTreeSet<String>,
    pub variants: BTreeMap<String, Vec<String>>,
    pub class_locations: BTreeMap<String, Vec<Location>>,
    pub variant_locations: BTreeMap<String, VariantLocations>,
    pub provided_variables: BTreeMap<String, ProvidedVariable>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProvidedVariable {
    pub scoped_name: String,
    pub value_type: ValueType,
    pub location: Location,
}

fn provided_variables(
    db: &mut Database,
    parse: &ParseResult,
) -> BTreeMap<String, ProvidedVariable> {
    let file = &parse.source.id;
    let mut exports = BTreeMap::new();
    let vars = parse
        .tree
        .children
        .iter()
        .filter(|node| node.kind == NodeKind::Vars);
    for node in vars.flat_map(|node| &node.children) {
        let grouped = node.kind == NodeKind::Provided;
        let members = if grouped {
            &node.children
        } else {
            std::slice::from_ref(node)
        };
        for variable in members.iter().filter_map(|member| parse.variable(member)) {
            if !grouped && !variable.provided {
                continue;
            }
            let decoded = decode_identifier(variable.name);
            let Some(name) = decoded.strip_prefix("--") else {
                continue;
            };
            let Some(id) = db.local(file, &decoded, SymbolKind::Variable) else {
                continue;
            };
            let symbol = &db.symbols[id.0];
            if symbol.location.range != variable.range {
                continue;
            }
            if variable.annotation.is_none() && variable.initial.is_none() {
                db.error(
                    "type.provided-untyped",
                    file,
                    variable.range,
                    "A provided variable requires a type or an inferable initial value",
                );
                continue;
            }
            if symbol.value_type == ValueType::Unknown {
                continue;
            }
            let mut parts = name.split('-').filter(|part| !part.is_empty());
            let mut key = parts.next().unwrap_or_default().to_owned();
            for part in parts {
                let mut chars = part.chars();
                key.extend(chars.next().unwrap().to_uppercase());
                key.push_str(chars.as_str());
            }
            if let Some(previous) = exports.get(&key) {
                let ProvidedVariable { location, .. } = previous;
                let related = location.clone();
                db.error(
                    "vars.duplicate-export",
                    file,
                    variable.range,
                    format!("Provided variables produce the same export key `{key}`"),
                );
                db.diagnostics.last_mut().unwrap().related.push(related);
                continue;
            }
            exports.insert(
                key,
                ProvidedVariable {
                    scoped_name: xcss_source::scoped_property(file, &decoded),
                    value_type: symbol.value_type.clone(),
                    location: symbol.location.clone(),
                },
            );
        }
    }
    exports
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VariantLocations {
    pub group: Location,
    pub members: BTreeMap<String, Location>,
}

#[derive(Clone)]
struct Content {
    file: FileId,
    nodes: Vec<Node>,
    parent: Option<Box<Content>>,
    stack: Vec<SymbolId>,
}

fn classes(
    db: &Database,
    file: &FileId,
    nodes: &[Node],
    stack: &mut Vec<SymbolId>,
    out: &mut BTreeMap<String, Vec<Location>>,
    content: Option<&Content>,
    count: &mut usize,
) {
    let parse = &db.files[file];
    for node in nodes {
        *count += 1;
        if *count > 100_000 {
            return;
        }
        match node.kind {
            NodeKind::Injection | NodeKind::Util | NodeKind::Raw => continue,
            NodeKind::Rule => {
                for (name, range, global) in xcss_syntax::selector_classes(parse, node) {
                    if global {
                        continue;
                    }
                    let locations = out.entry(name).or_default();
                    let location = Location {
                        file: file.clone(),
                        range,
                    };
                    if !locations.contains(&location) {
                        locations.push(location);
                    }
                }
            }
            NodeKind::Call => {
                if let Some(call) = parse.call(node)
                    && let Some(target) = db
                        .references
                        .iter()
                        .find(|r| r.location.file == *file && r.location.range == call.name_range)
                        .and_then(|r| r.target)
                    && !stack.contains(&target)
                    && stack.len() < 128
                {
                    let symbol = &db.symbols[target.0];
                    let source = &db.files[&symbol.location.file];
                    if let Some(definition) = source.tree.descendants().find(|n| {
                        source
                            .injection(n)
                            .is_some_and(|i| i.name_range == symbol.location.range)
                    }) {
                        let supplied = node.block.map(|_| Content {
                            file: file.clone(),
                            nodes: node.children.clone(),
                            parent: content.cloned().map(Box::new),
                            stack: stack.clone(),
                        });
                        stack.push(target);
                        classes(
                            db,
                            &symbol.location.file,
                            &definition.children,
                            stack,
                            out,
                            supplied.as_ref(),
                            count,
                        );
                        stack.pop();
                    }
                }
                continue;
            }
            NodeKind::Content => {
                if let Some(content) = content {
                    classes(
                        db,
                        &content.file,
                        &content.nodes,
                        &mut content.stack.clone(),
                        out,
                        content.parent.as_deref(),
                        count,
                    );
                }
                continue;
            }
            _ => {}
        }
        classes(db, file, &node.children, stack, out, content, count);
    }
}

pub(super) fn analyze(db: &mut Database) {
    let files: Vec<_> = db.files.keys().cloned().collect();
    for file in files {
        let parse = db.files[&file].clone();
        let mut interface = ModuleInterface::default();
        classes(
            db,
            &file,
            &parse.tree.children,
            &mut Vec::new(),
            &mut interface.class_locations,
            None,
            &mut 0,
        );
        interface.classes = interface.class_locations.keys().cloned().collect();
        interface.provided_variables = provided_variables(db, &parse);
        let top: BTreeSet<_> = parse.tree.children.iter().map(|n| n.range.start).collect();
        for node in parse
            .tree
            .descendants()
            .filter(|n| n.kind == NodeKind::Variants)
        {
            if !top.contains(&node.range.start) {
                db.error(
                    "variants.placement",
                    &file,
                    node.header,
                    "@variants must be top-level",
                );
                continue;
            }
            let Some(group) = parse.variants(node) else {
                continue;
            };
            let name = decode_identifier(group.name);
            if interface.variants.contains_key(&name) {
                db.error(
                    "variants.duplicate-group",
                    &file,
                    group.name_range,
                    format!("Duplicate variant group `{name}`"),
                );
                continue;
            }
            let mut members = Vec::new();
            let mut locations = VariantLocations {
                group: Location {
                    file: file.clone(),
                    range: group.name_range,
                },
                members: BTreeMap::new(),
            };
            for (member, range) in group.members {
                let member = decode_identifier(member);
                if members.contains(&member) {
                    db.error(
                        "variants.duplicate-member",
                        &file,
                        range,
                        format!("Duplicate variant `{member}`"),
                    );
                }
                if !interface.classes.contains(&member) {
                    db.error(
                        "variants.unknown-class",
                        &file,
                        range,
                        format!("Variant `{member}` is not an exported local class"),
                    );
                }
                locations
                    .members
                    .entry(member.clone())
                    .or_insert_with(|| Location {
                        file: file.clone(),
                        range,
                    });
                members.push(member);
            }
            interface.variant_locations.insert(name.clone(), locations);
            interface.variants.insert(name, members);
        }
        db.interfaces.insert(file, interface);
    }
}
