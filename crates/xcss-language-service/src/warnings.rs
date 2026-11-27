use super::*;

/// Coded advisory messages never fail a check.
#[derive(Clone, Debug)]
pub struct Warning {
    pub code: &'static str,
    pub category: &'static str,
    pub message: String,
    pub range: Range,
}

impl ServiceSnapshot {
    /// Class and at-rule warnings, filtered by source-line suppression directives.
    pub fn warnings(&self, file: &FileId) -> Vec<Warning> {
        let mut warnings = self.class_warning_inputs(file);
        if let Some(parse) = self.0.syntax(file) {
            for node in parse
                .tree
                .descendants()
                .filter(|n| n.kind == NodeKind::AtRule)
            {
                // Unsupported Sass constructs already have a specific checker error.
                if parse
                    .diagnostics
                    .iter()
                    .any(|d| d.code == "syntax.sass" && d.location.range == node.header)
                {
                    continue;
                }
                let start = parse
                    .tokens
                    .partition_point(|t| t.range.start < node.header.start);
                let Some(at) = parse.tokens.get(start) else {
                    continue;
                };
                let Some(name) = parse.tokens.get(start + 1).filter(|t| {
                    t.kind == TokenKind::Word
                        && t.range.start == at.range.end
                        && t.range.end <= node.header.end
                }) else {
                    continue;
                };
                if parse.slice(at.range) != "@" || parse.slice(name.range) == "-" {
                    continue;
                }
                let decoded = xcss_syntax::ast::decode_identifier(parse.slice(name.range));
                if CSS_AT_RULES.contains(&decoded.to_ascii_lowercase().as_str()) {
                    continue;
                }
                let range = TextRange::new(at.range.start, name.range.end);
                warnings.push(Warning {
                    code: "XC-505",
                    category: "unknown-at-rule",
                    message: format!("Unknown at-rule '{}'.", parse.slice(range)),
                    range: self.css_range(file, range),
                });
            }
        }
        self.filter_warnings(file, warnings)
    }

    pub fn class_warnings(&self, file: &FileId) -> Vec<Warning> {
        self.filter_warnings(file, self.class_warning_inputs(file))
    }

    fn class_warning_inputs(&self, file: &FileId) -> Vec<Warning> {
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
                        code: "XC-503",
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
                    code: "XC-504",
                    category: "unused-class",
                    message: format!(
                        "Class '{}' is unused in TypeScript and @variants.",
                        symbol.name
                    ),
                    range: self.css_range(file, symbol.location.range),
                });
            }
        }
        warnings
    }

    fn filter_warnings(&self, file: &FileId, mut warnings: Vec<Warning>) -> Vec<Warning> {
        let suppressions = self.suppressions(file);
        warnings.retain(|w| !suppression::suppresses(&suppressions, w.range.start.line, w.code));
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

// Names from https://drafts.csswg.org/indexes/#at-rules (2026-11-27),
// including nested rules and draft features, plus legacy document rules.
// Recognition here checks names only, not browser support or placement.
const CSS_AT_RULES: &[&str] = &[
    "-moz-document",
    "-webkit-keyframes",
    "annotation",
    "apply",
    "bottom-center",
    "bottom-left",
    "bottom-left-corner",
    "bottom-right",
    "bottom-right-corner",
    "character-variant",
    "charset",
    "color-profile",
    "container",
    "contents",
    "counter-style",
    "custom-media",
    "custom-selector",
    "document",
    "else",
    "font-face",
    "font-feature-values",
    "font-palette-values",
    "function",
    "historical-forms",
    "import",
    "keyframes",
    "layer",
    "left-bottom",
    "left-middle",
    "left-top",
    "location",
    "media",
    "mixin",
    "namespace",
    "navigation",
    "ornaments",
    "page",
    "position-try",
    "private",
    "property",
    "right-bottom",
    "right-middle",
    "right-top",
    "scope",
    "starting-style",
    "styleset",
    "stylistic",
    "supports",
    "supports-condition",
    "swash",
    "top-center",
    "top-left",
    "top-left-corner",
    "top-right",
    "top-right-corner",
    "view-transition",
    "when",
];
