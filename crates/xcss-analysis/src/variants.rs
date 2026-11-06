use super::*;

/// The static contract of an XCSS module, independent of CSS generation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModuleInterface {
    pub classes: BTreeSet<String>,
    pub variants: BTreeMap<String, Vec<String>>,
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
    out: &mut BTreeSet<String>,
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
            NodeKind::Rule => out.extend(
                xcss_syntax::selector_classes(parse, node)
                    .into_iter()
                    .filter(|(_, _, global)| !global)
                    .map(|(name, _, _)| name),
            ),
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
            &mut interface.classes,
            None,
            &mut 0,
        );
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
                members.push(member);
            }
            interface.variants.insert(name, members);
        }
        db.interfaces.insert(file, interface);
    }
}
