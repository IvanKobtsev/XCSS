use super::*;

/// Advisory messages have no diagnostic code and never fail a check.
#[derive(Clone, Debug)]
pub struct Warning {
    pub category: &'static str,
    pub message: String,
    pub range: Range,
}

impl ServiceSnapshot {
    pub fn class_warnings(&self, file: &FileId) -> Vec<Warning> {
        let Some(parse) = self.0.syntax(file) else {
            return vec![];
        };
        let can_check_usage = self.1.config.is_some() && self.typescript_check().is_ok();
        let mut warnings = vec![];
        for symbol in self
            .0
            .symbols()
            .iter()
            .filter(|s| s.kind == SymbolKind::Class && !s.global && s.location.file == *file)
        {
            let locations = std::iter::once(&symbol.location).chain(
                self.0
                    .references()
                    .iter()
                    .filter(|r| r.target == Some(symbol.id) && r.role == ReferenceRole::Declaration)
                    .map(|r| &r.location),
            );
            for location in locations {
                if parse.tree.descendants().any(|node| {
                    node.kind == NodeKind::Rule
                        && node.header.start <= location.range.start
                        && node.header.end >= location.range.end
                        && node.children.is_empty()
                        && node.missing.is_empty()
                }) {
                    warnings.push(Warning {
                        category: "empty-class",
                        message: format!("Class '{}' has an empty declaration.", symbol.name),
                        range: self.css_range(file, location.range),
                    });
                }
            }
            let variant = self.0.module_interface(file).is_some_and(|i| {
                i.variants
                    .values()
                    .any(|members| members.contains(&symbol.name))
            });
            if can_check_usage
                && !variant
                && !self.editor_index().occurrences.iter().any(|occurrence| {
                    occurrence.symbols.contains(&xcss_typescript::Symbol::Class(
                        file.clone(),
                        symbol.name.clone(),
                    ))
                })
            {
                warnings.push(Warning {
                    category: "unused-class",
                    message: format!(
                        "Class '{}' is unused in TypeScript and @variants.",
                        symbol.name
                    ),
                    range: self.css_range(file, symbol.location.range),
                });
            }
        }
        warnings.sort_by_key(|w| {
            (
                w.range.start.line,
                w.range.start.character,
                w.message.clone(),
            )
        });
        warnings
    }
}
