use super::*;
use crate::lower::{Frame, Lowerer};
use xcss_syntax::ast::{Value, ValueKind, decode_identifier};
#[derive(Clone, Copy)]
pub(super) struct Context {
    pub name: bool,
    pub arithmetic: bool,
    pub math: bool,
}
impl Default for Context {
    fn default() -> Self {
        Self {
            name: false,
            arithmetic: true,
            math: false,
        }
    }
}
pub(super) fn property_name(
    lower: &mut Lowerer<'_>,
    file: &FileId,
    name: &str,
    _range: TextRange,
) -> String {
    let decoded = decode_identifier(name);
    if let Some(symbol) =
        lower.snapshot.symbols().iter().find(|s| {
            s.location.file == *file && s.kind == SymbolKind::Variable && s.name == decoded
        })
    {
        lower.registrations.insert(symbol.id);
        let name = scoped_property(file, &decoded);
        if file == lower.entry {
            lower.properties.insert(decoded, name.clone());
        }
        name
    } else {
        name.into()
    }
}
pub(super) fn render(
    lower: &mut Lowerer<'_>,
    parse: &ParseResult,
    value: &Value,
    frame: &Frame,
    context: Context,
) -> Result<Mapped, Vec<Diagnostic>> {
    let origin = Location {
        file: parse.source.id.clone(),
        range: value.range,
    };
    let mut out = Mapped::default();
    match &value.kind {
        ValueKind::CustomProperty(name) => {
            let name = property_name(lower, &parse.source.id, name, value.range);
            out.push(
                &if context.name {
                    name
                } else {
                    format!("var({name})")
                },
                &origin,
            );
        }
        ValueKind::Parameter(_) => {
            let symbol = lower
                .snapshot
                .symbol_at(&parse.source.id, value.range.start)
                .map(|s| s.id);
            if let Some(replacement) = symbol.and_then(|id| frame.parameters.get(&id)) {
                out.append(replacement);
            } else {
                return Err(vec![Diagnostic::error(
                    "generate.parameter",
                    &origin.file,
                    origin.range,
                    "Parameter has no expansion value",
                )]);
            }
        }
        ValueKind::Sequence(parts) | ValueKind::Group(parts) => {
            let group = matches!(value.kind, ValueKind::Group(_));
            // Slash is also a CSS separator; other arithmetic remains supported
            // in properties such as background and border-radius.
            let has_operator = parts.iter().any(|p| match p.kind {
                ValueKind::Operator('/') => context.arithmetic,
                ValueKind::Operator(_) => true,
                _ => false,
            });
            let calc = !context.math && has_operator;
            if group {
                out.push(if calc { "calc(" } else { "(" }, &origin);
            } else if calc {
                out.push("calc(", &origin);
            }
            let mut cursor = value.range.start + usize::from(group);
            for part in parts {
                if part.range.start >= cursor {
                    out.push(
                        &parse.source.text()[cursor..part.range.start],
                        &Location {
                            file: origin.file.clone(),
                            range: TextRange::new(cursor, part.range.start),
                        },
                    );
                }
                out.append(&render(
                    lower,
                    parse,
                    part,
                    frame,
                    Context {
                        math: context.math || calc,
                        ..context
                    },
                )?);
                cursor = part.range.end;
            }
            let end = value.range.end.saturating_sub(usize::from(group));
            if end >= cursor {
                out.push(
                    &parse.source.text()[cursor..end],
                    &Location {
                        file: origin.file.clone(),
                        range: TextRange::new(cursor, end),
                    },
                );
            }
            if group || calc {
                out.push(")", &origin);
            }
        }
        ValueKind::Function { name, arguments } => {
            let lower_name = name.to_ascii_lowercase();
            if lower_name == "url" {
                let raw = parse.slice(value.range);
                let inside = raw
                    .find('(')
                    .map(|i| raw[i + 1..raw.len() - 1].trim())
                    .unwrap_or("");
                let quote = inside.starts_with(['\'', '"']);
                let path = inside.trim_matches(['\'', '"']);
                let rebased = lower.asset_url(&origin.file, path);
                out.push(
                    &format!(
                        "url({}{}{})",
                        if quote { "\"" } else { "" },
                        rebased,
                        if quote { "\"" } else { "" }
                    ),
                    &origin,
                );
            } else {
                out.push(&format!("{name}("), &origin);
                let start = parse.slice(value.range).find('(').unwrap() + value.range.start + 1;
                let mut cursor = start;
                let mut first_argument = true;
                // Group each comma-separated function argument so arithmetic gets its own calc().
                let end = value.range.end.saturating_sub(1);
                for (index, range) in parse
                    .split_ranges(TextRange::new(start, end), ",")
                    .into_iter()
                    .enumerate()
                {
                    if index > 0 {
                        out.push(",", &origin);
                    }
                    let arg = parse.value(range);
                    let math = context.math
                        || matches!(
                            lower_name.as_str(),
                            "calc"
                                | "min"
                                | "max"
                                | "clamp"
                                | "round"
                                | "mod"
                                | "rem"
                                | "abs"
                                | "sign"
                                | "sin"
                                | "cos"
                                | "tan"
                                | "sqrt"
                                | "pow"
                                | "hypot"
                        );
                    let is_var = lower_name == "var" && first_argument;
                    out.append(&render(
                        lower,
                        parse,
                        &arg,
                        frame,
                        Context {
                            name: is_var,
                            arithmetic: context.arithmetic
                                && !matches!(
                                    lower_name.as_str(),
                                    "rgb"
                                        | "rgba"
                                        | "hsl"
                                        | "hsla"
                                        | "hwb"
                                        | "lab"
                                        | "lch"
                                        | "oklab"
                                        | "oklch"
                                        | "color"
                                ),
                            math,
                        },
                    )?);
                    first_argument = false;
                    cursor = range.end;
                }
                let _ = (arguments, cursor);
                out.push(")", &origin);
            }
        }
        _ => {
            let global = lower
                .snapshot
                .symbol_at(&origin.file, value.range.start)
                .filter(|s| s.kind == SymbolKind::Keyframe && s.global);
            if let Some(symbol) = global {
                let placeholder = crate::lower::global_keyframe(&symbol.name);
                lower
                    .globals
                    .insert(placeholder.clone(), symbol.name.clone());
                out.push(&placeholder, &origin);
            } else {
                out.push(parse.slice(value.range), &origin);
            }
        }
    }
    Ok(out)
}

pub(super) fn transition(
    lower: &mut Lowerer<'_>,
    parse: &ParseResult,
    value: &Value,
    frame: &Frame,
) -> Result<Mapped, Vec<Diagnostic>> {
    let mut out = Mapped::default();
    let origin = Location {
        file: parse.source.id.clone(),
        range: value.range,
    };
    let mut first = true;
    for range in parse.split_ranges(value.range, ",") {
        let item = parse.value(range);
        let ValueKind::Sequence(parts) = &item.kind else {
            continue;
        };
        let Some(head) = parts.first() else {
            continue;
        };
        let names = if let ValueKind::Group(properties) = &head.kind {
            properties
                .iter()
                .filter(|p| parse.slice(p.range) != ",")
                .cloned()
                .collect::<Vec<_>>()
        } else {
            vec![head.clone()]
        };
        let explicit = matches!(head.kind, ValueKind::Group(_))
            || !matches!(head.kind, ValueKind::CustomProperty(_))
            || parts.get(1).is_some_and(|p| {
                xcss_analysis::infer_type(parse.slice(p.range))
                    == xcss_analysis::ValueType::named("time")
            });
        for property in names {
            if !first {
                out.push(", ", &origin);
            }
            first = false;
            if explicit {
                out.append(&render(
                    lower,
                    parse,
                    &property,
                    frame,
                    Context {
                        name: true,
                        arithmetic: false,
                        math: false,
                    },
                )?);
                let tail = parse.value(TextRange::new(head.range.end, range.end));
                out.append(&render(
                    lower,
                    parse,
                    &tail,
                    frame,
                    Context {
                        arithmetic: false,
                        ..Context::default()
                    },
                )?);
                let property_text = parse.slice(property.range).trim();
                let tail_text = parse.slice(tail.range);
                if matches!(property_text, "display" | "content-visibility" | "overlay")
                    && !tail_text
                        .split_whitespace()
                        .any(|w| matches!(w, "normal" | "allow-discrete"))
                {
                    out.push(" allow-discrete", &origin);
                }
            } else {
                out.append(&render(
                    lower,
                    parse,
                    &item,
                    frame,
                    Context {
                        arithmetic: false,
                        ..Context::default()
                    },
                )?);
            }
        }
    }
    Ok(out)
}

/// Pure path rebasing; module identities are normalized host-relative paths.
pub fn rebase(origin: &FileId, target: &str, url: &str) -> String {
    if url.is_empty() || url.starts_with(['/', '#', '?']) || url.contains(':') {
        return url.into();
    }
    let split = url.find(['?', '#']).unwrap_or(url.len());
    let (path, suffix) = url.split_at(split);
    fn normalize(path: &str) -> Vec<String> {
        let mut parts = Vec::new();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    if parts.last().is_some_and(|s| s != "..") {
                        parts.pop();
                    } else {
                        parts.push(part.to_owned());
                    }
                }
                _ => parts.push(part.to_owned()),
            }
        }
        parts
    }
    let origin_dir = origin.0.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
    let target_dir = target.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
    let source = normalize(&format!("{origin_dir}/{path}"));
    let destination = normalize(target_dir);
    let shared = source
        .iter()
        .zip(&destination)
        .take_while(|(a, b)| a == b)
        .count();
    let mut relative = vec!["..".to_owned(); destination.len() - shared];
    relative.extend(source[shared..].iter().cloned());
    let path = relative.join("/");
    format!(
        "{}{path}{suffix}",
        if path.starts_with("../") { "" } else { "./" }
    )
}
