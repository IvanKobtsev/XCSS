//! Explicit class-specific selector alternatives, including functional guards.
use crate::{SelectorKind, TokenKind, analyze_selectors};
use std::collections::BTreeMap;
use xcss_source::TextRange;

pub type OverrideMarkers = BTreeMap<String, BTreeMap<String, String>>;

#[derive(Default)]
pub struct SelectorOverrides {
    pub text: String,
    /// Generated fragments and the authored selector fragments they came from.
    pub spans: Vec<(TextRange, TextRange)>,
}

struct Frame {
    name: String,
    full: TextRange,
    args: TextRange,
}
struct Use {
    range: TextRange,
    name: String,
    frames: Vec<usize>,
    negative: bool,
}

fn splice(text: &str, range: TextRange, edits: &[(TextRange, String)]) -> String {
    let mut out = String::new();
    let mut cursor = range.start;
    for (r, value) in edits {
        if r.start < range.start || r.end > range.end {
            continue;
        }
        out.push_str(&text[cursor..r.start]);
        out.push_str(value);
        cursor = r.end;
    }
    out.push_str(&text[cursor..range.end]);
    out
}

fn branch(text: &str, args: TextRange, target: TextRange) -> TextRange {
    let tokens = super::selector_expansion::tokens(&text[args.start..args.end]);
    super::selector_expansion::members(&text[args.start..args.end], &tokens)
        .into_iter()
        .map(|r| TextRange::new(r.start + args.start, r.end + args.start))
        .find(|r| r.start <= target.start && r.end >= target.end)
        .unwrap_or(args)
}

fn guard(text: &str, usage: &Use, frames: &[Frame], marker: &str) -> String {
    let mut target = usage.range;
    let mut value = format!(".{marker}");
    for &index in usage.frames.iter().rev() {
        let frame = &frames[index];
        let member = branch(text, frame.args, target);
        let selected = splice(text, member, &[(target, value)]);
        value = if matches!(frame.name.as_str(), "nth-child" | "nth-last-child") {
            format!(
                "{}:where({selected})",
                &text[frame.full.start..frame.full.end]
            )
        } else {
            let prefix = &text[frame.full.start..frame.args.start];
            format!("{prefix}{selected})")
        };
        target = frame.full;
    }
    value
}

/// The original branch is always retained. Selected classes inside selector
/// functions keep their original alternatives and receive a zero-weight guard;
/// this preserves nth-filter membership and prevents unrelated branches winning.
pub fn override_selectors(
    text: &str,
    markers: &OverrideMarkers,
    budget: &mut usize,
) -> Result<SelectorOverrides, &'static str> {
    let tokens = super::selector_expansion::tokens(text);
    let analyses = analyze_selectors(text, 0, None);
    let mut frames = Vec::new();
    for part in analyses.iter().flat_map(|s| &s.parts).filter(|p| {
        matches!(
            p.kind,
            SelectorKind::PseudoClass | SelectorKind::PseudoElement
        )
    }) {
        let Some(open) = tokens.iter().position(|t| {
            t.range.start == part.range.end && &text[t.range.start..t.range.end] == "("
        }) else {
            continue;
        };
        let mut depth = 1;
        let mut close = open + 1;
        while close < tokens.len() {
            if tokens[close].kind == TokenKind::Punctuation {
                match &text[tokens[close].range.start..tokens[close].range.end] {
                    "(" => depth += 1,
                    ")" => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            close += 1;
        }
        if close == tokens.len() {
            return Err("Unclosed selector function");
        }
        let name = text[part.range.start..part.range.end]
            .trim_start_matches(':')
            .to_ascii_lowercase();
        let mut start = tokens[open].range.end;
        if matches!(name.as_str(), "nth-child" | "nth-last-child")
            && let Some(of) = tokens[open + 1..close].iter().find(|t| {
                t.kind == TokenKind::Word
                    && text[t.range.start..t.range.end].eq_ignore_ascii_case("of")
            })
        {
            start = of.range.end;
        }
        frames.push(Frame {
            name,
            full: TextRange::new(part.range.start, tokens[close].range.end),
            args: TextRange::new(start, tokens[close].range.start),
        });
    }
    let mut result = SelectorOverrides::default();
    for selector in analyses {
        let mut uses = Vec::new();
        for part in selector
            .parts
            .iter()
            .filter(|p| p.kind == SelectorKind::Class)
        {
            let name = crate::ast::decode_identifier(&text[part.range.start..part.range.end]);
            if !markers.contains_key(&name) {
                continue;
            }
            let mut path: Vec<_> = frames
                .iter()
                .enumerate()
                .filter(|(_, f)| f.args.start <= part.range.start && f.args.end >= part.range.end)
                .map(|(i, _)| i)
                .collect();
            path.sort_by_key(|&i| frames[i].full.start);
            let negative = path.iter().any(|&i| frames[i].name == "not");
            uses.push(Use {
                range: TextRange::new(part.range.start - 1, part.range.end),
                name,
                frames: path,
                negative,
            });
        }
        let mut choices: Vec<Vec<(usize, String, String)>> = vec![Vec::new()];
        for (i, usage) in uses.iter().enumerate().filter(|(_, u)| !u.negative) {
            let levels = &markers[&usage.name];
            let count = choices
                .len()
                .checked_mul(levels.len() + 1)
                .ok_or("Override selector expansion exceeds the generation limit")?;
            if count > *budget {
                return Err("Override selector expansion exceeds the generation limit");
            }
            let previous = choices.clone();
            for (level, marker) in levels {
                for choice in &previous {
                    let mut next = choice.clone();
                    next.push((i, level.clone(), marker.clone()));
                    choices.push(next);
                }
            }
        }
        for choice in choices {
            *budget = budget
                .checked_sub(1)
                .ok_or("Override selector expansion exceeds the generation limit")?;
            let mut edits = Vec::new();
            let mut guards = BTreeMap::<usize, Vec<String>>::new();
            for (i, usage) in uses.iter().enumerate() {
                if usage.negative {
                    let aliases = markers[&usage.name]
                        .values()
                        .map(|m| format!(".{m}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    edits.push((
                        usage.range,
                        format!(
                            ":is({}, {aliases})",
                            &text[usage.range.start..usage.range.end]
                        ),
                    ));
                } else if let Some((_, level, marker)) =
                    choice.iter().find(|(index, _, _)| *index == i)
                {
                    let count = level
                        .parse::<usize>()
                        .map_err(|_| "Override output exceeds the generation limit")?;
                    if count > *budget {
                        return Err("Override output exceeds the generation limit");
                    }
                    *budget -= count;
                    let boosted = format!(".{marker}:is(*, {})", "#_".repeat(count));
                    if let Some(&outer) = usage.frames.first() {
                        edits.push((
                            usage.range,
                            format!(
                                ":is({}, {boosted})",
                                &text[usage.range.start..usage.range.end]
                            ),
                        ));
                        guards
                            .entry(outer)
                            .or_default()
                            .push(guard(text, usage, &frames, marker));
                    } else {
                        edits.push((usage.range, boosted));
                    }
                }
            }
            for (index, guards) in guards {
                let frame = &frames[index];
                // ::slotted's argument denotes a different element than its host.
                let at = if frame.name == "slotted" {
                    frame.args.end
                } else {
                    frame.full.end
                };
                let values = if frame.name == "slotted" {
                    guards
                        .into_iter()
                        .map(|g| {
                            g.strip_prefix("::slotted(")
                                .and_then(|g| g.strip_suffix(')'))
                                .unwrap_or(&g)
                                .to_owned()
                        })
                        .collect::<Vec<_>>()
                } else {
                    guards
                };
                edits.push((
                    TextRange::new(at, at),
                    values.into_iter().map(|g| format!(":where({g})")).collect(),
                ));
            }
            edits.sort_by_key(|(r, _)| (r.start, r.end));
            if !result.text.is_empty() {
                result.text.push_str(", ");
            }
            let mut cursor = selector.range.start;
            for (range, replacement) in edits {
                if cursor < range.start {
                    let start = result.text.len();
                    result.text.push_str(&text[cursor..range.start]);
                    result.spans.push((
                        TextRange::new(start, result.text.len()),
                        TextRange::new(cursor, range.start),
                    ));
                }
                let start = result.text.len();
                result.text.push_str(&replacement);
                result
                    .spans
                    .push((TextRange::new(start, result.text.len()), range));
                cursor = range.end;
            }
            let start = result.text.len();
            result.text.push_str(&text[cursor..selector.range.end]);
            result.spans.push((
                TextRange::new(start, result.text.len()),
                TextRange::new(cursor, selector.range.end),
            ));
            if result.text.len() > 16 * 1024 * 1024 {
                return Err("Override output exceeds the generation limit");
            }
        }
    }
    if let Some(last) = analyze_selectors(text, 0, None).last() {
        let start = result.text.len();
        result.text.push_str(&text[last.range.end..]);
        result.spans.push((
            TextRange::new(start, result.text.len()),
            TextRange::new(last.range.end, text.len()),
        ));
    }
    Ok(result)
}
