use super::*;
use crate::lower::Lowerer;
use xcss_analysis::ValueType;
pub(super) fn emit(lower: &mut Lowerer<'_>, id: SymbolId) -> Result<Mapped, Vec<Diagnostic>> {
    let symbol = lower.snapshot.symbol(id).unwrap();
    let file = &symbol.location.file;
    let variable = lower.snapshot.variable_registration(id).unwrap();
    let kind = match &symbol.value_type {
        ValueType::Named(name) => name.as_str(),
        ValueType::Unknown => "*",
    };
    let default = match kind {
        "number" | "integer" => "0",
        "percentage" => "0%",
        "length" | "length-percentage" => "0px",
        "angle" => "0deg",
        "time" => "0s",
        "resolution" => "1dppx",
        "color" => "transparent",
        "image" => "linear-gradient(transparent, transparent)",
        "url" => "url(\"data:,\")",
        "transform-function" | "transform-list" => "matrix(1, 0, 0, 1, 0, 0)",
        "frequency" => "0Hz",
        "flex" => "0fr",
        _ => "",
    };
    let initial = variable
        .initial
        .as_deref()
        .or_else(|| (kind != "*").then_some(default));
    let browser_kind = if matches!(kind, "frequency" | "flex") {
        let mut warning = Diagnostic::error(
            "generate.registration-wildcard",
            file,
            variable.location.range,
            format!("Browser registration uses wildcard syntax for XCSS {kind}"),
        );
        warning.severity = Severity::Warning;
        lower.diagnostics.push(warning);
        "*"
    } else {
        kind
    };
    if let Some(initial) = initial {
        let valid =
            lightningcss::values::syntax::SyntaxString::parse_string(&format!("<{browser_kind}>"))
                .is_ok_and(|syntax| syntax.parse_value_from_string(initial).is_ok());
        if browser_kind != "*"
            && (!valid
                || !xcss_analysis::registration_initial_is_independent(initial)
                || !xcss_syntax::valid_css_value(initial))
        {
            return Err(vec![Diagnostic::error(
                "generate.property-initial",
                file,
                variable.location.range,
                "Typed @property initial values must be valid and computationally independent",
            )]);
        }
    }
    let name = scoped_property(file, &symbol.name);
    if file == lower.entry {
        lower.properties.insert(symbol.name.clone(), name.clone());
    }
    let syntax = if browser_kind == "*" {
        "*".into()
    } else {
        format!("<{browser_kind}>")
    };
    let origin = symbol.location.clone();
    let mut out = Mapped::default();
    out.push(
        &format!(
            "@property {name} {{\nsyntax: \"{syntax}\";\ninherits: {};\n",
            variable.inherits
        ),
        &origin,
    );
    if let Some(initial) = initial {
        let initial = if initial.trim().starts_with("url(") {
            let raw = initial
                .trim()
                .trim_start_matches("url(")
                .trim_end_matches(')')
                .trim()
                .trim_matches(['\'', '"']);
            format!("url(\"{}\")", lower.asset_url(file, raw))
        } else {
            initial.to_owned()
        };
        out.push(&format!("initial-value: {initial};\n"), &origin);
    }
    out.push("}\n", &origin);
    Ok(out)
}
