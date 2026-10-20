//! Parser-backed, lossless source transformation for Cross-CSS.

use std::collections::BTreeSet;

use raffia::{
    ParserBuilder, ParserOptions, Spanned,
    ast::{
        ComponentValue, Declaration, DelimiterKind, FunctionName, InterpolableIdent,
        SassBinaryOperatorKind, SassVariableDeclaration, SimpleBlock, Statement, Stylesheet,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Syntax {
    #[default]
    Css,
    Scss,
}

impl Syntax {
    /// Infer the source syntax from an XCSS input filename.
    pub fn from_input_filename(filename: &str) -> Option<Self> {
        if filename.ends_with(".cross.css") {
            Some(Self::Css)
        } else if filename.ends_with(".cross.scss") {
            Some(Self::Scss)
        } else {
            None
        }
    }

    /// Required output suffix for this source syntax.
    pub const fn output_suffix(self) -> &'static str {
        match self {
            Self::Css => ".module.css",
            Self::Scss => ".module.scss",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TransformOptions<'a> {
    pub syntax: Syntax,
    pub filename: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransformOutput {
    pub code: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub severity: Severity,
    pub span: SourceSpan,
    pub line: usize,
    pub column: usize,
    pub filename: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
struct ValueRange {
    start: usize,
    end: usize,
    property_name: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum CalcWrapper {
    Insert { start: usize, end: usize },
    ReplaceGrouping { start: usize },
}

#[derive(Clone, Debug)]
struct Edit {
    start: usize,
    end: usize,
    replacement: String,
    priority: u8,
}

pub fn transform(
    source: &str,
    options: TransformOptions<'_>,
) -> Result<TransformOutput, Vec<Diagnostic>> {
    let parser_options = ParserOptions {
        try_parsing_value_in_custom_property: true,
        ..ParserOptions::default()
    };
    let syntax = match options.syntax {
        // Cross-CSS deliberately permits arithmetic that is not valid CSS
        // until this compiler wraps it in calc(). Parse with Raffia's SCSS
        // expression grammar first, then validate the generated CSS below.
        Syntax::Css => raffia::Syntax::Scss,
        Syntax::Scss => raffia::Syntax::Scss,
    };
    let mut parser = ParserBuilder::new(source)
        .syntax(syntax)
        .options(parser_options)
        .build();

    let stylesheet = match parser.parse::<Stylesheet>() {
        Ok(stylesheet) => stylesheet,
        Err(error) => {
            return Err(vec![diagnostic_from_error(
                source,
                options.filename,
                &error,
            )]);
        }
    };
    let mut value_ranges = Vec::new();
    let mut declaration_name_ranges = Vec::new();
    let mut reserved_import_diagnostics = Vec::new();
    collect_reserved_import_diagnostics(
        &stylesheet.statements,
        source,
        options.filename,
        &mut reserved_import_diagnostics,
    );
    if !reserved_import_diagnostics.is_empty() {
        return Err(reserved_import_diagnostics);
    }
    collect_statements(
        &stylesheet.statements,
        &mut value_ranges,
        &mut declaration_name_ranges,
    );

    let mut references = BTreeSet::new();
    let mut calc_wrappers = BTreeSet::new();
    let mut expression_diagnostics = Vec::new();
    let mut transition_edits = Vec::new();
    for range in &value_ranges {
        let is_transition = range
            .property_name
            .as_deref()
            .is_some_and(|name| name == "transition");
        if is_transition {
            match transform_transition_value(source, range, options.filename) {
                Ok(Some(replacement)) => transition_edits.push(Edit {
                    start: range.start,
                    end: range.end,
                    replacement,
                    priority: 2,
                }),
                Ok(None) => {}
                Err(diagnostic) => expression_diagnostics.push(diagnostic),
            }
        } else if !range
            .property_name
            .as_deref()
            .is_some_and(is_transition_property)
        {
            scan_value(
                source,
                range.start,
                range.end,
                &declaration_name_ranges,
                &mut references,
            );
        }
        if !is_transition {
            analyze_value_for_calc(
                source,
                range,
                options.filename,
                &mut calc_wrappers,
                &mut expression_diagnostics,
            );
        }
    }
    let accepted_expression_spans: Vec<SourceSpan> = calc_wrappers
        .iter()
        .map(|wrapper| match wrapper {
            CalcWrapper::Insert { start, end } => SourceSpan {
                start: *start,
                end: *end,
            },
            CalcWrapper::ReplaceGrouping { start } => SourceSpan {
                start: *start,
                end: *start + 1,
            },
        })
        .chain(
            expression_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.span),
        )
        .chain(value_ranges.iter().filter_map(|range| {
            (range.property_name.as_deref() == Some("transition")).then_some(SourceSpan {
                start: range.start,
                end: range.end,
            })
        }))
        .collect();
    let mut parse_diagnostics: Vec<_> = parser
        .recoverable_errors()
        .iter()
        .filter(|error| {
            let handled_transition = value_ranges.iter().any(|range| {
                range.property_name.as_deref() == Some("transition")
                    && error.span.start >= range.start
                    && error.span.end <= range.end
            });
            !handled_transition
                && (!matches!(error.kind, raffia::error::ErrorKind::ExpectComponentValue)
                    || !accepted_expression_spans
                        .iter()
                        .any(|span| error.span.start >= span.start && error.span.end <= span.end))
        })
        .map(|error| diagnostic_from_error(source, options.filename, error))
        .collect();
    if !parse_diagnostics.is_empty() || !expression_diagnostics.is_empty() {
        parse_diagnostics.extend(expression_diagnostics);
        return Err(parse_diagnostics);
    }

    let mut edits = transition_edits;
    for (start, end) in references {
        edits.push(Edit {
            start,
            end: start,
            replacement: "var(".into(),
            priority: 0,
        });
        edits.push(Edit {
            start: end,
            end,
            replacement: ")".into(),
            priority: 0,
        });
    }
    for wrapper in calc_wrappers {
        match wrapper {
            CalcWrapper::Insert { start, end } => {
                edits.push(Edit {
                    start,
                    end: start,
                    replacement: "calc(".into(),
                    priority: 1,
                });
                edits.push(Edit {
                    start: end,
                    end,
                    replacement: ")".into(),
                    priority: 1,
                });
            }
            CalcWrapper::ReplaceGrouping { start } => edits.push(Edit {
                start,
                end: start + 1,
                replacement: "calc(".into(),
                priority: 1,
            }),
        }
    }
    edits.sort_by(|left, right| {
        right
            .start
            .cmp(&left.start)
            .then(left.priority.cmp(&right.priority))
    });
    let mut code = source.to_owned();
    for edit in &edits {
        code.replace_range(edit.start..edit.end, &edit.replacement);
    }
    if options.syntax == Syntax::Css {
        validate_generated_css(&code, source, options.filename, &edits)?;
    }
    Ok(TransformOutput { code })
}

fn collect_reserved_import_diagnostics(
    statements: &[Statement<'_>],
    source: &str,
    filename: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in statements {
        match statement {
            Statement::AtRule(rule) => {
                if matches!(
                    rule.name.name.to_ascii_lowercase().as_str(),
                    "import" | "use" | "forward"
                ) {
                    push_reserved_import_diagnostic(&rule.span, source, filename, diagnostics);
                }
                if let Some(block) = &rule.block {
                    collect_reserved_import_diagnostics(
                        &block.statements,
                        source,
                        filename,
                        diagnostics,
                    );
                }
            }
            Statement::QualifiedRule(rule) => collect_reserved_import_diagnostics(
                &rule.block.statements,
                source,
                filename,
                diagnostics,
            ),
            Statement::KeyframeBlock(rule) => collect_reserved_import_diagnostics(
                &rule.block.statements,
                source,
                filename,
                diagnostics,
            ),
            Statement::SassIfAtRule(rule) => {
                collect_reserved_import_diagnostics(
                    &rule.if_clause.block.statements,
                    source,
                    filename,
                    diagnostics,
                );
                for clause in &rule.else_if_clauses {
                    collect_reserved_import_diagnostics(
                        &clause.block.statements,
                        source,
                        filename,
                        diagnostics,
                    );
                }
                if let Some(block) = &rule.else_clause {
                    collect_reserved_import_diagnostics(
                        &block.statements,
                        source,
                        filename,
                        diagnostics,
                    );
                }
            }
            Statement::UnknownSassAtRule(rule) => {
                if let InterpolableIdent::Literal(name) = &rule.name
                    && matches!(
                        name.name.to_ascii_lowercase().as_str(),
                        "import" | "use" | "forward"
                    )
                {
                    push_reserved_import_diagnostic(&rule.span, source, filename, diagnostics);
                }
                if let Some(block) = &rule.block {
                    collect_reserved_import_diagnostics(
                        &block.statements,
                        source,
                        filename,
                        diagnostics,
                    );
                }
            }
            _ => {}
        }
    }
}

fn push_reserved_import_diagnostic(
    span: &raffia::Span,
    source: &str,
    filename: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let start = span.start.min(source.len());
    let end = span.end.min(source.len()).max(start);
    let (line, column) = line_column(source, start);
    diagnostics.push(Diagnostic {
        message: "external stylesheet imports are reserved for the future XCSS import system"
            .into(),
        severity: Severity::Error,
        span: SourceSpan { start, end },
        line,
        column,
        filename: filename.map(str::to_owned),
    });
}

fn validate_generated_css(
    generated: &str,
    original: &str,
    filename: Option<&str>,
    edits: &[Edit],
) -> Result<(), Vec<Diagnostic>> {
    let mut parser = ParserBuilder::new(generated)
        .syntax(raffia::Syntax::Css)
        .options(ParserOptions {
            try_parsing_value_in_custom_property: true,
            ..ParserOptions::default()
        })
        .build();
    let fatal = parser.parse::<Stylesheet>().err();
    let errors: Vec<_> = fatal
        .iter()
        .chain(parser.recoverable_errors().iter())
        .map(|error| diagnostic_from_generated_error(original, filename, error, edits))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn diagnostic_from_generated_error(
    original: &str,
    filename: Option<&str>,
    error: &raffia::error::Error,
    edits: &[Edit],
) -> Diagnostic {
    let start = generated_to_original_offset(error.span.start, edits).min(original.len());
    let end = generated_to_original_offset(error.span.end, edits)
        .min(original.len())
        .max(start);
    let (line, column) = line_column(original, start);
    Diagnostic {
        message: error.kind.to_string(),
        severity: Severity::Error,
        span: SourceSpan { start, end },
        line,
        column,
        filename: filename.map(str::to_owned),
    }
}

fn generated_to_original_offset(offset: usize, edits_descending: &[Edit]) -> usize {
    let mut edits: Vec<_> = edits_descending.iter().collect();
    edits.sort_by_key(|edit| (edit.start, edit.priority));
    let mut delta = 0isize;
    for edit in edits {
        let generated_start = edit.start.saturating_add_signed(delta);
        let generated_end = generated_start + edit.replacement.len();
        if offset < generated_start {
            break;
        }
        if offset <= generated_end {
            return edit.start;
        }
        delta += edit.replacement.len() as isize - (edit.end - edit.start) as isize;
    }
    offset.saturating_add_signed(-delta)
}

fn diagnostic_from_error(
    source: &str,
    filename: Option<&str>,
    error: &raffia::error::Error,
) -> Diagnostic {
    let start = error.span.start.min(source.len());
    let end = error.span.end.min(source.len()).max(start);
    let (line, column) = line_column(source, start);
    Diagnostic {
        message: error.kind.to_string(),
        severity: Severity::Error,
        span: SourceSpan { start, end },
        line,
        column,
        filename: filename.map(str::to_owned),
    }
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..floor_char_boundary(source, offset)];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    (line, prefix[line_start..].chars().count() + 1)
}

fn floor_char_boundary(source: &str, mut offset: usize) -> usize {
    offset = offset.min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn collect_statements(
    statements: &[Statement<'_>],
    values: &mut Vec<ValueRange>,
    names: &mut Vec<(usize, usize)>,
) {
    for statement in statements {
        match statement {
            Statement::Declaration(declaration) => collect_declaration(declaration, values, names),
            Statement::SassVariableDeclaration(declaration) => {
                let span = declaration.value.span();
                values.push(ValueRange {
                    start: span.start,
                    end: span.end,
                    property_name: None,
                });
            }
            Statement::QualifiedRule(rule) => collect_block(&rule.block, values, names),
            Statement::AtRule(rule) => {
                if let Some(block) = &rule.block {
                    collect_block(block, values, names);
                }
            }
            Statement::KeyframeBlock(rule) => collect_block(&rule.block, values, names),
            Statement::SassIfAtRule(rule) => {
                collect_block(&rule.if_clause.block, values, names);
                for clause in &rule.else_if_clauses {
                    collect_block(&clause.block, values, names);
                }
                if let Some(block) = &rule.else_clause {
                    collect_block(block, values, names);
                }
            }
            Statement::UnknownSassAtRule(rule) => {
                if let Some(block) = &rule.block {
                    collect_block(block, values, names);
                }
            }
            _ => {}
        }
    }
}

fn collect_block(
    block: &SimpleBlock<'_>,
    values: &mut Vec<ValueRange>,
    names: &mut Vec<(usize, usize)>,
) {
    collect_statements(&block.statements, values, names);
}

fn collect_declaration(
    declaration: &Declaration<'_>,
    values: &mut Vec<ValueRange>,
    names: &mut Vec<(usize, usize)>,
) {
    let name_span = raffia::Spanned::span(&declaration.name);
    names.push((name_span.start, name_span.end));
    if !declaration.value.is_empty() {
        let end = declaration
            .important
            .as_ref()
            .map_or(declaration.span.end, |important| important.span.start);
        values.push(ValueRange {
            start: declaration.colon_span.end,
            end,
            property_name: Some(declaration_name(declaration)),
        });
    }
    for value in &declaration.value {
        if let ComponentValue::SassNestingDeclaration(nested) = value {
            collect_block(&nested.block, values, names);
        }
    }
}

fn declaration_name(declaration: &Declaration<'_>) -> String {
    match &declaration.name {
        InterpolableIdent::Literal(ident) => ident.name.to_ascii_lowercase(),
        _ => String::new(),
    }
}

fn analyze_value_for_calc(
    source: &str,
    range: &ValueRange,
    filename: Option<&str>,
    wrappers: &mut BTreeSet<CalcWrapper>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    const PREFIX: &str = "$x:";
    let fragment = format!("{PREFIX}{}", &source[range.start..range.end]);
    let mut parser = ParserBuilder::new(&fragment)
        .syntax(raffia::Syntax::Scss)
        .options(ParserOptions {
            try_parsing_value_in_custom_property: true,
            ..ParserOptions::default()
        })
        .build();
    let Ok(declaration) = parser.parse::<SassVariableDeclaration>() else {
        // The complete stylesheet was already validated. Some modern CSS
        // values are intentionally outside Sass's expression grammar; those
        // values are simply not candidates for this optional transform.
        return;
    };
    if !parser.recoverable_errors().is_empty() {
        return;
    }

    let context = ExpressionContext {
        source,
        source_offset: range.start,
        fragment_prefix: PREFIX.len(),
        filename,
        division_allowed: range
            .property_name
            .as_deref()
            .is_none_or(division_allowed_for_property),
    };
    analyze_component(&declaration.value, &context, wrappers, diagnostics);
}

#[derive(Clone, Copy)]
struct ExpressionContext<'a> {
    source: &'a str,
    source_offset: usize,
    fragment_prefix: usize,
    filename: Option<&'a str>,
    division_allowed: bool,
}

impl ExpressionContext<'_> {
    fn global_span(&self, span: &raffia::Span) -> SourceSpan {
        SourceSpan {
            start: self.source_offset + span.start.saturating_sub(self.fragment_prefix),
            end: self.source_offset + span.end.saturating_sub(self.fragment_prefix),
        }
    }

    fn with_division_allowed(self, division_allowed: bool) -> Self {
        Self {
            division_allowed,
            ..self
        }
    }
}

fn analyze_sequence(
    values: &[ComponentValue<'_>],
    context: &ExpressionContext<'_>,
    wrappers: &mut BTreeSet<CalcWrapper>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut group_start = 0;
    for (index, value) in values.iter().enumerate() {
        if matches!(
            value,
            ComponentValue::Delimiter(delimiter) if delimiter.kind == DelimiterKind::Comma
        ) {
            analyze_space_group(&values[group_start..index], context, wrappers, diagnostics);
            group_start = index + 1;
        }
    }
    analyze_space_group(&values[group_start..], context, wrappers, diagnostics);
}

fn analyze_space_group(
    values: &[ComponentValue<'_>],
    context: &ExpressionContext<'_>,
    wrappers: &mut BTreeSet<CalcWrapper>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if values.len() == 1 {
        analyze_component(&values[0], context, wrappers, diagnostics);
        return;
    }

    for value in values {
        if is_supported_arithmetic(value, context) {
            push_ambiguity_diagnostic(value.span(), context, diagnostics);
        } else {
            analyze_component(value, context, wrappers, diagnostics);
        }
    }
}

fn analyze_component(
    value: &ComponentValue<'_>,
    context: &ExpressionContext<'_>,
    wrappers: &mut BTreeSet<CalcWrapper>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if is_supported_arithmetic(value, context) {
        let span = context.global_span(value.span());
        wrappers.insert(CalcWrapper::Insert {
            start: span.start,
            end: span.end,
        });
        return;
    }

    match value {
        ComponentValue::SassParenthesizedExpression(parenthesized)
            if is_supported_arithmetic(&parenthesized.expr, context) =>
        {
            let span = context.global_span(&parenthesized.span);
            wrappers.insert(CalcWrapper::ReplaceGrouping { start: span.start });
        }
        ComponentValue::Function(function) => {
            let name = function_name(function);
            if is_math_function(&name) {
                return;
            }
            let nested_context = context.with_division_allowed(
                context.division_allowed && !is_slash_separator_function(&name),
            );
            analyze_sequence(&function.args, &nested_context, wrappers, diagnostics);
        }
        ComponentValue::SassList(list) => {
            if list.comma_spans.is_some() {
                for element in &list.elements {
                    analyze_component(element, context, wrappers, diagnostics);
                }
            } else {
                analyze_space_group(&list.elements, context, wrappers, diagnostics);
            }
        }
        ComponentValue::BracketBlock(block) => {
            analyze_sequence(&block.value, context, wrappers, diagnostics);
        }
        ComponentValue::SassKeywordArgument(argument) => {
            analyze_component(&argument.value, context, wrappers, diagnostics);
        }
        // Interpolated identifiers/strings/URLs are deliberately opaque.
        _ => {}
    }
}

fn is_supported_arithmetic(value: &ComponentValue<'_>, context: &ExpressionContext<'_>) -> bool {
    let ComponentValue::SassBinaryExpression(expression) = value else {
        return false;
    };
    let operator_supported = match expression.op.kind {
        SassBinaryOperatorKind::Plus | SassBinaryOperatorKind::Minus => {
            additive_operator_has_whitespace(&expression.op.span, context)
        }
        SassBinaryOperatorKind::Multiply => true,
        SassBinaryOperatorKind::Division => context.division_allowed,
        _ => false,
    };
    operator_supported
        && arithmetic_operand_supported(&expression.left, context)
        && arithmetic_operand_supported(&expression.right, context)
}

fn arithmetic_operand_supported(
    value: &ComponentValue<'_>,
    context: &ExpressionContext<'_>,
) -> bool {
    match value {
        ComponentValue::SassBinaryExpression(_) => is_supported_arithmetic(value, context),
        ComponentValue::SassParenthesizedExpression(parenthesized) => {
            arithmetic_operand_supported(&parenthesized.expr, context)
        }
        ComponentValue::Number(_)
        | ComponentValue::Dimension(_)
        | ComponentValue::Percentage(_)
        | ComponentValue::SassVariable(_)
        | ComponentValue::InterpolableIdent(_)
        | ComponentValue::Function(_)
        | ComponentValue::Calc(_) => true,
        _ => false,
    }
}

fn additive_operator_has_whitespace(span: &raffia::Span, context: &ExpressionContext<'_>) -> bool {
    let span = context.global_span(span);
    let bytes = context.source.as_bytes();
    span.start > 0
        && span.end < bytes.len()
        && bytes[span.start - 1].is_ascii_whitespace()
        && bytes[span.end].is_ascii_whitespace()
}

fn push_ambiguity_diagnostic(
    span: &raffia::Span,
    context: &ExpressionContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let span = context.global_span(span);
    if diagnostics.iter().any(|diagnostic| diagnostic.span == span) {
        return;
    }
    let (line, column) = line_column(context.source, span.start);
    diagnostics.push(Diagnostic {
        message: "arithmetic in a whitespace-separated value list is ambiguous; wrap the expression in parentheses"
            .into(),
        severity: Severity::Error,
        span,
        line,
        column,
        filename: context.filename.map(str::to_owned),
    });
}

fn function_name(function: &raffia::ast::Function<'_>) -> String {
    match &function.name {
        FunctionName::Ident(InterpolableIdent::Literal(ident)) => ident.name.to_ascii_lowercase(),
        _ => String::new(),
    }
}

fn is_math_function(name: &str) -> bool {
    matches!(name, "calc" | "min" | "max" | "clamp")
}

fn is_slash_separator_function(name: &str) -> bool {
    matches!(
        name,
        "rgb" | "rgba" | "hsl" | "hsla" | "hwb" | "lab" | "lch" | "oklab" | "oklch" | "color"
    )
}

fn division_allowed_for_property(name: &str) -> bool {
    !matches!(
        name,
        "aspect-ratio"
            | "background"
            | "border-radius"
            | "font"
            | "grid"
            | "grid-area"
            | "grid-column"
            | "grid-row"
            | "grid-template"
            | "mask"
            | "offset"
    )
}

fn is_transition_property(name: &str) -> bool {
    matches!(name, "transition" | "transition-property")
}

fn transform_transition_value(
    source: &str,
    range: &ValueRange,
    filename: Option<&str>,
) -> Result<Option<String>, Diagnostic> {
    let value = &source[range.start..range.end];
    let leading_len = value.len() - value.trim_start_matches(char::is_whitespace).len();
    let trailing_start = value.trim_end_matches(char::is_whitespace).len();
    let core = &value[leading_len..trailing_start];
    if core.is_empty() {
        return Ok(None);
    }

    let items = split_top_level(core, ',').map_err(|offset| {
        transition_diagnostic(
            source,
            filename,
            range.start + leading_len + offset,
            "malformed transition value",
        )
    })?;
    let mut output_items = Vec::new();
    let mut changed = false;

    for (item_start, item_end) in items {
        let raw_item = &core[item_start..item_end];
        let item_leading = raw_item.len() - raw_item.trim_start().len();
        let item_trailing_start = raw_item.trim_end().len();
        let item = &raw_item[item_leading..item_trailing_start];
        if item.is_empty() {
            return Err(transition_diagnostic(
                source,
                filename,
                range.start + leading_len + item_start,
                "transition items must not be empty",
            ));
        }
        if let Some((open, close)) = standalone_group(item).map_err(|offset| {
            transition_diagnostic(
                source,
                filename,
                range.start + leading_len + item_start + item_leading + offset,
                "malformed transition property group",
            )
        })? {
            if !only_trivia(&item[..open]) {
                return Err(transition_diagnostic(
                    source,
                    filename,
                    range.start + leading_len + item_start + item_leading + open,
                    "transition property groups must be the first component of an item",
                ));
            }
            let members = parse_transition_group(&item[open + 1..close]).map_err(|offset| {
                transition_diagnostic(
                    source,
                    filename,
                    range.start + leading_len + item_start + item_leading + open + 1 + offset,
                    "transition groups accept only comma-separated literal property names",
                )
            })?;
            let prefix = item[..open].trim_end();
            let suffix = item[close + 1..].trim_start();
            let wrapped_suffix = wrap_transition_references(suffix, None);
            let mut expanded_items = Vec::new();
            for (member, property_name) in members {
                let mut expanded = String::new();
                if !prefix.is_empty() {
                    expanded.push_str(prefix);
                    expanded.push(' ');
                }
                expanded.push_str(member);
                if !wrapped_suffix.is_empty() {
                    expanded.push(' ');
                    expanded.push_str(&wrapped_suffix);
                }
                expanded_items.push(add_discrete_behavior(&expanded, Some(&property_name)));
            }
            output_items.push(format!(
                "{}{}{}",
                &raw_item[..item_leading],
                expanded_items.join(", "),
                &raw_item[item_trailing_start..]
            ));
            changed = true;
        } else {
            let references = transition_reference_ranges(item);
            let property = first_top_level_word(item);
            let exempt = property.and_then(|(start, end, word)| {
                (word.starts_with("--") && references.len() == 1).then_some((start, end))
            });
            if property.is_some_and(|(_, _, word)| word.starts_with("--")) && references.len() > 1 {
                return Err(transition_diagnostic(
                    source,
                    filename,
                    range.start + leading_len + item_start + item_leading + references[0].0,
                    "ambiguous transition item: wrap custom property names in parentheses",
                ));
            }
            let wrapped = wrap_transition_references(item, exempt);
            let discrete_property =
                find_discrete_property(&wrapped).or_else(|| property.map(|(_, _, word)| word));
            let transformed = add_discrete_behavior(&wrapped, discrete_property);
            changed |= transformed != item;
            output_items.push(format!(
                "{}{}{}",
                &raw_item[..item_leading],
                transformed,
                &raw_item[item_trailing_start..]
            ));
        }
    }

    if !changed {
        return Ok(None);
    }
    let mut replacement = String::new();
    replacement.push_str(&value[..leading_len]);
    replacement.push_str(&output_items.join(","));
    replacement.push_str(&value[trailing_start..]);
    Ok(Some(replacement))
}

fn only_trivia(value: &str) -> bool {
    without_comments(value).trim().is_empty()
}

fn transition_reference_ranges(value: &str) -> Vec<(usize, usize)> {
    let mut references = BTreeSet::new();
    scan_value(value, 0, value.len(), &[], &mut references);
    references.into_iter().collect()
}

fn wrap_transition_references(value: &str, exempt: Option<(usize, usize)>) -> String {
    let references = transition_reference_ranges(value);
    if references.iter().all(|range| Some(*range) == exempt) {
        return value.to_owned();
    }

    let mut output = value.to_owned();
    for (start, end) in references.into_iter().rev() {
        if Some((start, end)) != exempt {
            output.insert(end, ')');
            output.insert_str(start, "var(");
        }
    }
    output
}

fn first_top_level_word(value: &str) -> Option<(usize, usize, &str)> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else if bytes[index] == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'*' {
            index = skip_comment(bytes, index + 2, bytes.len());
        } else if is_name_byte(bytes[index]) && !bytes[index].is_ascii_digit() {
            let start = index;
            index += 1;
            while index < bytes.len() && is_name_byte(bytes[index]) {
                index += 1;
            }
            if bytes.get(index) != Some(&b'(') {
                return Some((start, index, &value[start..index]));
            }
            index = matching_paren(value, index).map_or(bytes.len(), |close| close + 1);
        } else if bytes[index] == b'(' {
            index = matching_paren(value, index).map_or(bytes.len(), |close| close + 1);
        } else if bytes[index] == b'\'' || bytes[index] == b'"' {
            index = skip_string(bytes, index, bytes.len());
        } else {
            index += utf8_char_len(bytes[index]);
        }
    }
    None
}

fn split_top_level(value: &str, delimiter: char) -> Result<Vec<(usize, usize)>, usize> {
    let bytes = value.as_bytes();
    let mut result = Vec::new();
    let mut start = 0;
    let mut index = 0;
    let mut depth = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'*' => {
                index = skip_comment(bytes, index + 2, bytes.len());
            }
            b'\'' | b'"' => index = skip_string(bytes, index, bytes.len()),
            b'(' => {
                depth += 1;
                index += 1;
            }
            b')' => {
                if depth == 0 {
                    return Err(index);
                }
                depth -= 1;
                index += 1;
            }
            byte if byte == delimiter as u8 && depth == 0 => {
                result.push((start, index));
                start = index + 1;
                index += 1;
            }
            _ => index += utf8_char_len(bytes[index]),
        }
    }
    if depth != 0 {
        return Err(value.len().saturating_sub(1));
    }
    result.push((start, value.len()));
    Ok(result)
}

fn standalone_group(item: &str) -> Result<Option<(usize, usize)>, usize> {
    let bytes = item.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'*' => {
                index = skip_comment(bytes, index + 2, bytes.len());
            }
            b'\'' | b'"' => index = skip_string(bytes, index, bytes.len()),
            b'(' => {
                let previous = item[..index].trim_end().as_bytes().last().copied();
                if previous.is_some_and(is_name_byte) {
                    let close = matching_paren(item, index)?;
                    index = close + 1;
                } else {
                    return Ok(Some((index, matching_paren(item, index)?)));
                }
            }
            _ => index += utf8_char_len(bytes[index]),
        }
    }
    Ok(None)
}

fn matching_paren(value: &str, open: usize) -> Result<usize, usize> {
    let bytes = value.as_bytes();
    let mut depth = 1usize;
    let mut index = open + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'*' => {
                index = skip_comment(bytes, index + 2, bytes.len());
            }
            b'\'' | b'"' => index = skip_string(bytes, index, bytes.len()),
            b'(' => {
                depth += 1;
                index += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(index);
                }
                index += 1;
            }
            _ => index += utf8_char_len(bytes[index]),
        }
    }
    Err(open)
}

fn parse_transition_group(group: &str) -> Result<Vec<(&str, String)>, usize> {
    let members = split_top_level(group, ',')?;
    let mut result = Vec::new();
    for (start, end) in members {
        let member = group[start..end].trim();
        let offset = start + group[start..end].find(member).unwrap_or(0);
        let property_name = without_comments(member);
        let property_name = property_name.trim();
        if property_name.is_empty()
            || property_name.contains("#{")
            || !property_name.chars().all(|character| {
                character.is_alphanumeric()
                    || character == '-'
                    || character == '_'
                    || character == '\\'
                    || !character.is_ascii()
            })
            || property_name
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
        {
            return Err(offset);
        }
        result.push((member, property_name.to_owned()));
    }
    Ok(result)
}

fn without_comments(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'*' {
            index = skip_comment(bytes, index + 2, bytes.len());
        } else {
            let length = utf8_char_len(bytes[index]);
            output.push_str(&value[index..(index + length).min(value.len())]);
            index += length;
        }
    }
    output
}

fn find_discrete_property(item: &str) -> Option<&str> {
    top_level_words(item).into_iter().find(|word| {
        matches!(
            word.to_ascii_lowercase().as_str(),
            "display" | "content-visibility" | "overlay"
        )
    })
}

fn add_discrete_behavior(item: &str, property: Option<&str>) -> String {
    let words = top_level_words(item);
    let has_explicit_behavior = words.iter().any(|word| {
        matches!(
            word.to_ascii_lowercase().as_str(),
            "normal" | "allow-discrete"
        )
    });
    if property.is_some_and(|name| {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "display" | "content-visibility" | "overlay"
        )
    }) && !has_explicit_behavior
    {
        format!("{} allow-discrete", item.trim_end())
    } else {
        item.to_owned()
    }
}

fn top_level_words(value: &str) -> Vec<&str> {
    let bytes = value.as_bytes();
    let mut words = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else if bytes[index] == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'*' {
            index = skip_comment(bytes, index + 2, bytes.len());
        } else if bytes[index] == b'\'' || bytes[index] == b'"' {
            index = skip_string(bytes, index, bytes.len());
        } else if bytes[index] == b'(' {
            index = matching_paren(value, index).map_or(bytes.len(), |close| close + 1);
        } else if is_name_byte(bytes[index]) && !bytes[index].is_ascii_digit() {
            let start = index;
            index += 1;
            while index < bytes.len() && is_name_byte(bytes[index]) {
                index += 1;
            }
            if bytes.get(index) != Some(&b'(') {
                words.push(&value[start..index]);
            }
        } else {
            index += utf8_char_len(bytes[index]);
        }
    }
    words
}

fn transition_diagnostic(
    source: &str,
    filename: Option<&str>,
    start: usize,
    message: &str,
) -> Diagnostic {
    let start = start.min(source.len());
    let (line, column) = line_column(source, start);
    Diagnostic {
        message: message.into(),
        severity: Severity::Error,
        span: SourceSpan {
            start,
            end: (start + 1).min(source.len()),
        },
        line,
        column,
        filename: filename.map(str::to_owned),
    }
}

#[derive(Clone, Debug)]
struct FunctionContext {
    name: String,
    saw_comma: bool,
}

fn scan_value(
    source: &str,
    start: usize,
    end: usize,
    declaration_names: &[(usize, usize)],
    references: &mut BTreeSet<(usize, usize)>,
) {
    let bytes = source.as_bytes();
    let mut index = start;
    let mut functions: Vec<FunctionContext> = Vec::new();

    while index < end {
        match bytes[index] {
            b'/' if index + 1 < end && bytes[index + 1] == b'*' => {
                index = skip_comment(bytes, index + 2, end);
            }
            b'\'' | b'"' => index = skip_string(bytes, index, end),
            b'(' => {
                functions.push(FunctionContext {
                    name: function_name_before(source, start, index).unwrap_or_default(),
                    saw_comma: false,
                });
                index += 1;
            }
            b')' => {
                functions.pop();
                index += 1;
            }
            b',' => {
                if let Some(function) = functions.last_mut() {
                    function.saw_comma = true;
                }
                index += 1;
            }
            b'-' if index + 1 < end && bytes[index + 1] == b'-' => {
                let reference_end = dashed_ident_end(source, index, end);
                let is_declaration_name = declaration_names
                    .iter()
                    .any(|&(s, e)| index < e && reference_end > s);
                let in_url = functions.iter().any(|function| function.name == "url");
                let in_var_name = functions
                    .iter()
                    .rev()
                    .find(|function| function.name == "var")
                    .is_some_and(|function| !function.saw_comma);
                let valid = reference_end > index + 2
                    && !(index > start && is_name_byte(bytes[index - 1]))
                    && !is_declaration_name
                    && !in_url
                    && !in_var_name
                    && next_non_trivia_byte(bytes, reference_end, end) != Some(b'(');
                if valid {
                    references.insert((index, reference_end));
                }
                index = reference_end.max(index + 2);
            }
            _ => index += utf8_char_len(bytes[index]),
        }
    }
}

fn skip_comment(bytes: &[u8], mut index: usize, end: usize) -> usize {
    while index + 1 < end {
        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return index + 2;
        }
        index += 1;
    }
    end
}

fn skip_string(bytes: &[u8], start: usize, end: usize) -> usize {
    let quote = bytes[start];
    let mut index = start + 1;
    while index < end {
        if bytes[index] == b'\\' {
            index = (index + 2).min(end);
        } else if bytes[index] == quote {
            return index + 1;
        } else {
            index += utf8_char_len(bytes[index]);
        }
    }
    end
}

fn dashed_ident_end(source: &str, start: usize, end: usize) -> usize {
    let bytes = source.as_bytes();
    let mut index = start + 2;
    while index < end {
        if is_name_byte(bytes[index]) {
            index += 1;
        } else if bytes[index] >= 0x80 {
            index += utf8_char_len(bytes[index]);
        } else if bytes[index] == b'\\' {
            index = consume_escape(bytes, index, end);
        } else if index + 1 < end && bytes[index] == b'#' && bytes[index + 1] == b'{' {
            index = consume_interpolation(bytes, index + 2, end);
        } else {
            break;
        }
    }
    index
}

fn consume_escape(bytes: &[u8], index: usize, end: usize) -> usize {
    if index + 1 >= end {
        return end;
    }
    let mut next = index + 1;
    let mut digits = 0;
    while next < end && digits < 6 && bytes[next].is_ascii_hexdigit() {
        next += 1;
        digits += 1;
    }
    if digits > 0 && next < end && bytes[next].is_ascii_whitespace() {
        next += 1;
    } else if digits == 0 {
        next += utf8_char_len(bytes[next]);
    }
    next.min(end)
}

fn consume_interpolation(bytes: &[u8], mut index: usize, end: usize) -> usize {
    let mut depth = 1usize;
    while index < end && depth > 0 {
        match bytes[index] {
            b'\'' | b'"' => index = skip_string(bytes, index, end),
            b'{' => {
                depth += 1;
                index += 1;
            }
            b'}' => {
                depth -= 1;
                index += 1;
            }
            _ => index += utf8_char_len(bytes[index]),
        }
    }
    index
}

fn function_name_before(source: &str, range_start: usize, lparen: usize) -> Option<String> {
    let bytes = source.as_bytes();
    let mut end = lparen;
    while end > range_start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > range_start && is_name_byte(bytes[start - 1]) {
        start -= 1;
    }
    (start < end).then(|| source[start..end].to_ascii_lowercase())
}

fn next_non_trivia_byte(bytes: &[u8], mut index: usize, end: usize) -> Option<u8> {
    while index < end {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else if index + 1 < end && bytes[index] == b'/' && bytes[index + 1] == b'*' {
            index = skip_comment(bytes, index + 2, end);
        } else {
            return Some(bytes[index]);
        }
    }
    None
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
}

fn utf8_char_len(first_byte: u8) -> usize {
    match first_byte {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str, syntax: Syntax) -> String {
        transform(
            source,
            TransformOptions {
                syntax,
                filename: None,
            },
        )
        .unwrap()
        .code
    }

    #[test]
    fn wraps_references_throughout_values() {
        let input = ".button {\n  color: --primary-color;\n  border: 1px solid --border-color;\n  padding: calc(--spacing * 2);\n  --active-color: --primary-color;\n}";
        let expected = ".button {\n  color: var(--primary-color);\n  border: 1px solid var(--border-color);\n  padding: calc(var(--spacing) * 2);\n  --active-color: var(--primary-color);\n}";
        assert_eq!(run(input, Syntax::Css), expected);
    }

    #[test]
    fn is_idempotent_and_transforms_var_fallbacks() {
        assert_eq!(
            run("a { color: var(--primary, --fallback); }", Syntax::Css),
            "a { color: var(--primary, var(--fallback)); }"
        );
    }

    #[test]
    fn preserves_custom_property_names_in_transition_declarations() {
        let input = ".button { transition: --button-color 200ms ease, opacity 100ms; transition-property: --button-color, --button-size; color: --button-color; }";
        let expected = ".button { transition: --button-color 200ms ease, opacity 100ms; transition-property: --button-color, --button-size; color: var(--button-color); }";
        assert_eq!(run(input, Syntax::Css), expected);
    }

    #[test]
    fn expands_grouped_transition_properties() {
        let input = "a { transition: (color, --my-prop) .5s ease, width .3s; }";
        let expected = "a { transition: color .5s ease, --my-prop .5s ease, width .3s; }";
        assert_eq!(run(input, Syntax::Css), expected);
    }

    #[test]
    fn expands_multiple_transition_groups_and_preserves_shared_parts() {
        let input = "a { transition: (opacity) 100ms, (color, transform) 200ms cubic-bezier(.1, .2, .3, .4) 50ms; }";
        let expected = "a { transition: opacity 100ms, color 200ms cubic-bezier(.1, .2, .3, .4) 50ms, transform 200ms cubic-bezier(.1, .2, .3, .4) 50ms; }";
        assert_eq!(run(input, Syntax::Css), expected);
    }

    #[test]
    fn wraps_transition_variables_outside_explicit_property_groups() {
        assert_eq!(
            run("a { transition: (--my-prop) --time --ease; }", Syntax::Css),
            "a { transition: --my-prop var(--time) var(--ease); }"
        );
        assert_eq!(
            run(
                "a { transition: (--my-prop, width, color) --time --ease; }",
                Syntax::Css
            ),
            "a { transition: --my-prop var(--time) var(--ease), width var(--time) var(--ease), color var(--time) var(--ease); }"
        );
        assert_eq!(
            run(
                "a { transition: (--my-prop, width) .3s ease, color .5s; }",
                Syntax::Css
            ),
            "a { transition: --my-prop .3s ease, width .3s ease, color .5s; }"
        );
    }

    #[test]
    fn handles_unambiguous_ungrouped_transition_variables() {
        assert_eq!(
            run(
                "a { transition: --my-prop .3s ease, width --time --ease; }",
                Syntax::Css
            ),
            "a { transition: --my-prop .3s ease, width var(--time) var(--ease); }"
        );
    }

    #[test]
    fn rejects_ambiguous_ungrouped_transition_variables() {
        let source = "a {\n  transition: --prop --time ease;\n}";
        let diagnostics = transform(
            source,
            TransformOptions {
                syntax: Syntax::Css,
                filename: Some("ambiguous.cross.css"),
            },
        )
        .unwrap_err();
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("ambiguous transition item")
                && diagnostic.filename.as_deref() == Some("ambiguous.cross.css")
                && diagnostic.line == 2
                && diagnostic.column > 1
        }));
    }

    #[test]
    fn preserves_transition_comments_and_multiline_layout() {
        let input = "a {\n  transition:\n    (color /* theme */, display) 200ms ease,\n    opacity 100ms;\n}";
        let expected = "a {\n  transition:\n    color /* theme */ 200ms ease, display 200ms ease allow-discrete,\n    opacity 100ms;\n}";
        assert_eq!(run(input, Syntax::Css), expected);
    }

    #[test]
    fn adds_discrete_behavior_to_entry_and_exit_properties() {
        let input = "a { transition: (opacity, display, content-visibility, overlay) .2s, .3s display, all .4s, color .5s; }";
        let expected = "a { transition: opacity .2s, display .2s allow-discrete, content-visibility .2s allow-discrete, overlay .2s allow-discrete, .3s display allow-discrete, all .4s, color .5s; }";
        assert_eq!(run(input, Syntax::Css), expected);
    }

    #[test]
    fn respects_explicit_transition_behavior() {
        let input = "a { transition: display .2s normal, overlay .3s allow-discrete, content-visibility .4s; }";
        let expected = "a { transition: display .2s normal, overlay .3s allow-discrete, content-visibility .4s allow-discrete; }";
        assert_eq!(run(input, Syntax::Css), expected);
    }

    #[test]
    fn rejects_invalid_transition_groups_with_location() {
        for source in [
            "a { transition: () .2s; }",
            "a { transition: (color,) .2s; }",
            "a { transition: (color, #{\"display\"}) .2s; }",
            "a { transition: ((color), width) .2s; }",
        ] {
            let diagnostics = transform(
                source,
                TransformOptions {
                    syntax: Syntax::Scss,
                    filename: Some("transition.cross.scss"),
                },
            )
            .unwrap_err();
            assert!(diagnostics.iter().any(|diagnostic| {
                diagnostic.message.contains("transition group")
                    && diagnostic.filename.as_deref() == Some("transition.cross.scss")
                    && diagnostic.line == 1
                    && diagnostic.column > 1
            }));
        }
    }

    #[test]
    fn preserves_non_value_occurrences_exactly() {
        let input = "/* --comment */\n.--selector {\n  --property: url(--asset.png);\n  content: \"--text\";\n}\n@media (--feature: --query) {}";
        assert_eq!(run(input, Syntax::Css), input);
    }

    #[test]
    fn handles_scss_nesting_directives_and_interpolation() {
        let input = "$part: color;\n@mixin themed {\n  .item {\n    border-#{$part}: --border-#{$part};\n    @if $part { color: --primary; }\n  }\n}";
        let expected = "$part: color;\n@mixin themed {\n  .item {\n    border-#{$part}: var(--border-#{$part});\n    @if $part { color: var(--primary); }\n  }\n}";
        assert_eq!(run(input, Syntax::Scss), expected);
    }

    #[test]
    fn reports_filename_and_one_based_location() {
        let diagnostics = transform(
            "a {\n  color: \"unterminated;\n}",
            TransformOptions {
                syntax: Syntax::Css,
                filename: Some("broken.cross.css"),
            },
        )
        .unwrap_err();
        assert_eq!(diagnostics[0].filename.as_deref(), Some("broken.cross.css"));
        assert!(diagnostics[0].line >= 2);
        assert!(diagnostics[0].column >= 1);
    }

    #[test]
    fn maps_xcss_filenames_to_output_suffixes() {
        assert_eq!(
            Syntax::from_input_filename("a.cross.css"),
            Some(Syntax::Css)
        );
        assert_eq!(
            Syntax::from_input_filename("a.cross.scss"),
            Some(Syntax::Scss)
        );
        assert_eq!(Syntax::from_input_filename("a.css"), None);
        assert_eq!(Syntax::Scss.output_suffix(), ".module.scss");
    }

    #[test]
    fn wraps_css_arithmetic_and_composes_with_variables() {
        let input = ".card {\n  width: 100% - --sidebar-width;\n  height: 2px * 3 + 1px;\n  opacity: 1 / 2;\n}";
        let expected = ".card {\n  width: calc(100% - var(--sidebar-width));\n  height: calc(2px * 3 + 1px);\n  opacity: calc(1 / 2);\n}";
        assert_eq!(run(input, Syntax::Css), expected);
    }

    #[test]
    fn wraps_scss_assignments_and_nested_function_arguments() {
        let input = "$gap: 4px * 2;\n.card { transform: translate(10px + 2px, --offset * 2); }";
        let expected = "$gap: calc(4px * 2);\n.card { transform: translate(calc(10px + 2px), calc(var(--offset) * 2)); }";
        assert_eq!(run(input, Syntax::Scss), expected);
    }

    #[test]
    fn existing_math_functions_are_calc_contexts() {
        let input = "a { width: calc(--size * 2); height: min(100%, --size - 2px); max-width: max(--min + 1px, --max); padding: clamp(1px, --ideal * 2, 10px); }";
        let expected = "a { width: calc(var(--size) * 2); height: min(100%, var(--size) - 2px); max-width: max(var(--min) + 1px, var(--max)); padding: clamp(1px, var(--ideal) * 2, 10px); }";
        assert_eq!(run(input, Syntax::Css), expected);
    }

    #[test]
    fn rejects_ambiguous_space_separated_arithmetic() {
        let diagnostics = transform(
            "a { margin: 10px + 2px 20px; }",
            TransformOptions {
                syntax: Syntax::Css,
                filename: Some("ambiguous.cross.css"),
            },
        )
        .unwrap_err();
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("wrap the expression in parentheses")
        );
        assert_eq!(
            diagnostics[0].filename.as_deref(),
            Some("ambiguous.cross.css")
        );
    }

    #[test]
    fn parentheses_disambiguate_mixed_values_and_are_replaced() {
        assert_eq!(
            run("a { margin: (10px + 2px) 20px; }", Syntax::Scss),
            "a { margin: calc(10px + 2px) 20px; }"
        );
    }

    #[test]
    fn leaves_compact_unary_slash_and_interpolation_syntax_alone() {
        let css = "a-b { width: 100%-20px; margin: -2px; border-radius: 10px / 20px; aspect-ratio: 16 / 9; color: rgb(0 0 0 / 50%); }";
        assert_eq!(run(css, Syntax::Css), css);

        let scss = "$base: 2px;\na { width: #{$base + 2}px; }";
        assert_eq!(run(scss, Syntax::Scss), scss);
    }

    #[test]
    fn supports_parenthesized_operands_in_custom_property_arithmetic() {
        let input =
            ".test {\n  --some-freaking-calculation: 100px + --opacity / (25vw - --width);\n}";
        let expected = ".test {\n  --some-freaking-calculation: calc(100px + var(--opacity) / (25vw - var(--width)));\n}";
        assert_eq!(run(input, Syntax::Css), expected);
    }

    #[test]
    fn rejects_reserved_external_import_directives() {
        for (source, syntax) in [
            ("@import './theme.css';", Syntax::Css),
            ("@use './tokens';", Syntax::Scss),
            ("@forward './mixins';", Syntax::Scss),
            ("a { @import './nested.css'; }", Syntax::Scss),
            ("a { @use './nested-tokens'; }", Syntax::Scss),
            ("a { @forward './nested-mixins'; }", Syntax::Scss),
        ] {
            let diagnostics = transform(
                source,
                TransformOptions {
                    syntax,
                    filename: Some("imports.cross.scss"),
                },
            )
            .unwrap_err();
            assert_eq!(diagnostics.len(), 1, "source: {source}");
            assert!(diagnostics[0].message.contains("XCSS import system"));
        }
    }

    #[test]
    fn preserves_non_import_at_rules() {
        let source = "@layer base { @supports (display: grid) { @media (width > 1px) { a { width: 1px + 2px; } } } }";
        let expected = "@layer base { @supports (display: grid) { @media (width > 1px) { a { width: calc(1px + 2px); } } } }";
        assert_eq!(run(source, Syntax::Css), expected);
    }
}
