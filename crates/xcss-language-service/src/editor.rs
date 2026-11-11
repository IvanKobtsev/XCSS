use std::collections::BTreeSet;

use xcss_source::{FileId, Location, Position, TextRange};
use xcss_syntax::{NodeKind, TokenKind, ast::decode_identifier};
use xcss_typescript::{EditorIndex, NavigationTarget};

use super::{
    EditorLocation, Range, ReferenceRole, SemanticToken, ServiceSnapshot, SymbolKind, TOKEN_TYPES,
};

pub use xcss_typescript::Symbol as ExportedSymbol;

pub struct Completion {
    pub label: String,
    pub kind: &'static str,
    pub range: Range,
    pub new_text: String,
}

pub struct DefinitionDetails {
    pub origin: Range,
    pub targets: Vec<EditorLocation>,
}

impl ServiceSnapshot {
    pub fn definition_details(
        &self,
        file: &FileId,
        position: Position,
    ) -> Option<DefinitionDetails> {
        let offset = self.offset(file, position)?;
        if self.0.syntax(file).is_some() {
            let targets = self.definition(file, position);
            let origin = self
                .hover(file, position)
                .map(|hover| hover.range)
                .unwrap_or(Range {
                    start: position,
                    end: position,
                });
            return (!targets.is_empty()).then_some(DefinitionDetails { origin, targets });
        }
        let index = self.editor_index();
        let mut candidates = index
            .navigation
            .get(file)
            .into_iter()
            .flatten()
            .filter(|item| item.range.contains(offset))
            .map(|item| (item.range, self.navigation_targets(&item.target)))
            .chain(
                index
                    .occurrences
                    .iter()
                    .filter(|item| {
                        item.location.file == *file && item.location.range.contains(offset)
                    })
                    .map(|item| {
                        (
                            item.location.range,
                            self.exported_definitions(&item.symbols),
                        )
                    }),
            );
        let (range, targets) = candidates
            .by_ref()
            .min_by_key(|(range, _)| range.end - range.start)?;
        let origin = self
            .location(&Location {
                file: file.clone(),
                range,
            })?
            .range;
        (!targets.is_empty()).then_some(DefinitionDetails { origin, targets })
    }

    fn navigation_targets(&self, target: &NavigationTarget) -> Vec<EditorLocation> {
        let (file, kind, keyword) = match target {
            NavigationTarget::Module(file) => {
                return self
                    .location(&Location {
                        file: file.clone(),
                        range: TextRange::default(),
                    })
                    .into_iter()
                    .collect();
            }
            NavigationTarget::Provided(file, key) => {
                return self
                    .0
                    .module_interface(file)
                    .and_then(|interface| interface.provided_variables.get(key))
                    .and_then(|variable| self.location(&variable.location))
                    .into_iter()
                    .collect();
            }
            NavigationTarget::Vars(file) => (file, NodeKind::Vars, "@vars"),
            NavigationTarget::Variants(file) => (file, NodeKind::Variants, "@variants"),
        };
        self.0
            .syntax(file)
            .into_iter()
            .flat_map(|parse| &parse.tree.children)
            .filter(|node| node.kind == kind)
            .filter_map(|node| {
                self.location(&Location {
                    file: file.clone(),
                    range: TextRange::new(node.header.start, node.header.start + keyword.len()),
                })
            })
            .collect()
    }

    pub(super) fn typescript_tokens(&self, file: &FileId) -> Vec<SemanticToken> {
        if !self.1.files.contains_key(file) {
            return Vec::new();
        }
        let index = self.editor_index();
        let members = index
            .occurrences
            .iter()
            .filter(|item| {
                item.location.file == *file
                    && matches!(item.symbols.first(), Some(ExportedSymbol::Class(..)))
            })
            .map(|item| (item.location.range, "xcssClassMember"));
        let extras = index
            .navigation
            .get(file)
            .into_iter()
            .flatten()
            .filter_map(|item| Some((item.range, item.token?)));
        let mut tokens: Vec<_> = members
            .chain(extras)
            .filter_map(|(range, kind)| {
                let range = self
                    .location(&Location {
                        file: file.clone(),
                        range,
                    })?
                    .range;
                if range.start.line != range.end.line
                    || range.start.character >= range.end.character
                {
                    return None;
                }
                Some(SemanticToken {
                    line: range.start.line,
                    character: range.start.character,
                    length: range.end.character - range.start.character,
                    token_type: TOKEN_TYPES.iter().position(|&token| token == kind)?,
                    declaration: kind == "xcssImport",
                })
            })
            .collect();
        tokens.sort_by_key(|token| (token.line, token.character, token.length, token.token_type));
        tokens.dedup();
        tokens
    }
    fn editor_index(&self) -> &EditorIndex {
        self.3.get_or_init(|| {
            let Some(config) = &self.1.config else {
                return EditorIndex::default();
            };
            xcss_typescript::editor_index(
                &xcss_typescript::Project {
                    config: config.clone(),
                    files: self.1.files.clone(),
                },
                &self.0,
            )
            .unwrap_or_default()
        })
    }

    pub fn symbols_at(&self, file: &FileId, position: Position) -> Vec<ExportedSymbol> {
        self.offset(file, position)
            .map_or_else(Vec::new, |offset| self.exported_symbols_at(file, offset))
    }

    pub(super) fn exported_symbols_at(&self, file: &FileId, offset: usize) -> Vec<ExportedSymbol> {
        if self.0.syntax(file).is_none() {
            return self
                .editor_index()
                .occurrences
                .iter()
                .find(|occurrence| {
                    occurrence.location.file == *file && occurrence.location.range.contains(offset)
                })
                .map_or_else(Vec::new, |occurrence| occurrence.symbols.clone());
        }
        if let Some(interface) = self.0.module_interface(file) {
            for (group, locations) in &interface.variant_locations {
                if locations.group.range.contains(offset) {
                    return vec![ExportedSymbol::Group(file.clone(), group.clone())];
                }
                for (name, member) in &locations.members {
                    if member.range.contains(offset) {
                        return vec![ExportedSymbol::Member(
                            file.clone(),
                            group.clone(),
                            name.clone(),
                        )];
                    }
                }
            }
        }
        let Some(class) = self
            .0
            .symbol_at(file, offset)
            .filter(|symbol| symbol.kind == SymbolKind::Class && !symbol.global)
        else {
            return Vec::new();
        };
        self.0
            .files()
            .filter_map(|module| {
                let locations = self
                    .0
                    .module_interface(module)?
                    .class_locations
                    .get(&class.name)?;
                locations
                    .iter()
                    .any(|location| {
                        self.0
                            .symbol_at(&location.file, location.range.start)
                            .is_some_and(|origin| origin.id == class.id)
                    })
                    .then(|| ExportedSymbol::Class(module.clone(), class.name.clone()))
            })
            .collect()
    }

    fn declarations<'a>(&'a self, symbol: &ExportedSymbol) -> &'a [Location] {
        match symbol {
            ExportedSymbol::Class(file, name) => self
                .0
                .module_interface(file)
                .and_then(|interface| interface.class_locations.get(name))
                .map_or(&[], Vec::as_slice),
            ExportedSymbol::Group(file, name) => self
                .0
                .module_interface(file)
                .and_then(|interface| interface.variant_locations.get(name))
                .map_or(&[], |variant| std::slice::from_ref(&variant.group)),
            ExportedSymbol::Member(file, group, name) => self
                .0
                .module_interface(file)
                .and_then(|interface| interface.variant_locations.get(group))
                .and_then(|variant| variant.members.get(name))
                .map_or(&[], std::slice::from_ref),
        }
    }

    pub(super) fn exported_definitions(&self, symbols: &[ExportedSymbol]) -> Vec<EditorLocation> {
        self.editor_locations(symbols.iter().flat_map(|symbol| self.declarations(symbol)))
    }

    /// References by exported identity allow hosts to query multiple TS project snapshots.
    pub fn exported_references(
        &self,
        symbols: &[ExportedSymbol],
        include_declaration: bool,
    ) -> Vec<EditorLocation> {
        if symbols.is_empty() {
            return Vec::new();
        }
        let mut locations = Vec::new();
        for symbol in symbols {
            let declarations = self.declarations(symbol);
            if include_declaration {
                locations.extend(declarations);
            }
            if let ExportedSymbol::Class(file, name) = symbol
                && let Some(interface) = self.0.module_interface(file)
            {
                locations.extend(
                    interface
                        .variant_locations
                        .values()
                        .filter_map(|variant| variant.members.get(name)),
                );
                let origins: BTreeSet<_> = declarations
                    .iter()
                    .filter_map(|location| {
                        self.0
                            .symbol_at(&location.file, location.range.start)
                            .map(|class| class.id)
                    })
                    .collect();
                for origin in origins {
                    locations.extend(
                        self.0
                            .references_to(origin)
                            .filter(|reference| {
                                include_declaration || reference.role != ReferenceRole::Declaration
                            })
                            .map(|reference| &reference.location),
                    );
                }
            }
        }
        locations.extend(
            self.editor_index()
                .occurrences
                .iter()
                .filter(|occurrence| {
                    occurrence
                        .symbols
                        .iter()
                        .any(|symbol| symbols.contains(symbol))
                })
                .map(|occurrence| &occurrence.location),
        );
        self.editor_locations(locations)
    }

    fn editor_locations<'a>(
        &self,
        locations: impl IntoIterator<Item = &'a Location>,
    ) -> Vec<EditorLocation> {
        let mut locations: Vec<_> = locations
            .into_iter()
            .filter_map(|location| self.location(location))
            .collect();
        locations.sort_by(|a, b| {
            (
                &a.file,
                a.range.start.line,
                a.range.start.character,
                a.range.end.line,
                a.range.end.character,
            )
                .cmp(&(
                    &b.file,
                    b.range.start.line,
                    b.range.start.character,
                    b.range.end.line,
                    b.range.end.character,
                ))
        });
        locations.dedup();
        locations
    }

    pub fn completion(&self, file: &FileId, position: Position) -> Vec<Completion> {
        let Some(offset) = self.offset(file, position) else {
            return Vec::new();
        };
        let items = if self.0.syntax(file).is_some() {
            self.stylesheet_completion(file, offset).unwrap_or_default()
        } else {
            self.editor_index().completion(&self.0, file, offset)
        };
        items
            .into_iter()
            .filter_map(|item| {
                let range = self
                    .location(&Location {
                        file: file.clone(),
                        range: item.range,
                    })?
                    .range;
                Some(Completion {
                    label: item.label,
                    kind: item.kind,
                    range,
                    new_text: item.new_text,
                })
            })
            .collect()
    }

    fn stylesheet_completion(
        &self,
        file: &FileId,
        offset: usize,
    ) -> Option<Vec<xcss_typescript::Completion>> {
        let parse = self.0.syntax(file)?;
        let node = parse
            .tree
            .descendants()
            .filter(|node| {
                node.header.start <= offset
                    && offset <= node.header.end
                    && node.kind == NodeKind::Variants
            })
            .min_by_key(|node| node.header.end - node.header.start)?;
        if parse
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::Comment && token.range.contains(offset))
        {
            return None;
        }
        let tokens: Vec<_> = parse
            .tokens
            .iter()
            .filter(|token| {
                token.range.start >= node.header.start
                    && token.range.end <= node.header.end
                    && !matches!(token.kind, TokenKind::Whitespace | TokenKind::Comment)
            })
            .collect();
        let text = |index: usize| tokens.get(index).map(|token| parse.slice(token.range));
        if !parse
            .tree
            .children
            .iter()
            .any(|child| std::ptr::eq(child, node))
            || text(3) != Some(":")
        {
            return None;
        }
        let start = 4;
        let end = tokens.len();
        let left = tokens.get(start - 1)?.range.end;
        let right = tokens
            .get(end)
            .map_or(node.header.end, |token| token.range.start);
        if offset < left || offset > right {
            return None;
        }
        let names = &tokens[start..end];
        let current = names.iter().position(|token| {
            token.kind == TokenKind::Word
                && token.range.start <= offset
                && offset <= token.range.end
        });
        let range = current.map_or(TextRange::new(offset, offset), |i| names[i].range);
        let previous = names
            .iter()
            .rev()
            .find(|token| token.range.end <= range.start);
        let previous_text = previous.map(|token| parse.slice(token.range));
        let has_dot = previous_text == Some(".")
            && previous.is_some_and(|token| token.range.end == range.start);
        if !has_dot && !matches!(previous_text, None | Some(",")) {
            return None;
        }
        let used: BTreeSet<_> = names
            .iter()
            .enumerate()
            .filter(|(i, token)| Some(*i) != current && token.kind == TokenKind::Word)
            .map(|(_, token)| decode_identifier(parse.slice(token.range)))
            .collect();
        let interface = self.0.module_interface(file)?;
        Some(
            interface
                .classes
                .iter()
                .filter(|name| !used.contains(*name))
                .map(|name| xcss_typescript::Completion {
                    label: name.clone(),
                    kind: "class",
                    range,
                    new_text: format!(
                        "{}{}",
                        if !has_dot { "." } else { "" },
                        css_identifier(name)
                    ),
                })
                .collect(),
        )
    }
}

fn css_identifier(name: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_alphabetic()
            || character == '_'
            || !character.is_ascii()
            || character == '-' && name.len() > 1
            || character.is_ascii_digit() && index > usize::from(name.starts_with('-'))
        {
            out.push(character);
        } else {
            write!(&mut out, "\\{:x} ", character as u32).unwrap();
        }
    }
    out
}
