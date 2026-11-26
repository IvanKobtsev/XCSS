use super::*;

pub const VALUE_KINDS: &[&str] = &[
    "length",
    "length-percentage",
    "number",
    "integer",
    "percentage",
    "angle",
    "time",
    "frequency",
    "resolution",
    "flex",
    "color",
    "image",
    "url",
    "transform-function",
    "transform-list",
];

pub const DIMENSION_UNITS: &[(&str, &[&str])] = &[
    ("percentage", &["%"]),
    (
        "length",
        &[
            "px", "em", "rem", "vh", "vw", "vmin", "vmax", "dvh", "dvw", "svh", "lvh", "ch", "ex",
            "cm", "mm", "in", "pt", "pc", "q", "lh", "rlh", "cqw", "cqh", "cqi", "cqb", "rex",
            "cap", "rcap", "rch", "ic", "ric", "vi", "vb", "svw", "lvw", "cqmin", "cqmax",
        ],
    ),
    ("angle", &["deg", "rad", "grad", "turn"]),
    ("time", &["s", "ms"]),
    ("frequency", &["hz", "khz"]),
    ("resolution", &["dpi", "dpcm", "dppx", "x"]),
    ("flex", &["fr"]),
];

pub(super) fn checked_type(
    db: &mut Database,
    file: &FileId,
    range: TextRange,
    annotation: Option<&str>,
    initial: Option<&str>,
) -> ValueType {
    checked_annotation(db, file, range, annotation, initial, false)
}

pub(super) fn checked_parameter_type(
    db: &mut Database,
    file: &FileId,
    range: TextRange,
    annotation: Option<&str>,
    initial: Option<&str>,
) -> ValueType {
    checked_annotation(db, file, range, annotation, initial, true)
}

fn checked_annotation(
    db: &mut Database,
    file: &FileId,
    range: TextRange,
    annotation: Option<&str>,
    initial: Option<&str>,
    allow_union: bool,
) -> ValueType {
    let actual = initial.map(infer_type).unwrap_or(ValueType::Unknown);
    if let Some(annotation) = annotation {
        let cleaned = without_comments(annotation);
        let members: Vec<_> = cleaned.split('|').map(str::trim).collect();
        if !allow_union && members.len() > 1 {
            db.error(
                "type.unknown-type",
                file,
                range,
                "Union type annotations are only allowed on @injection and @util parameters",
            );
            return ValueType::Unknown;
        }
        if let Some(invalid) = members.iter().find(|member| !VALUE_KINDS.contains(member)) {
            db.error(
                "type.unknown-type",
                file,
                range,
                if invalid.is_empty() {
                    "Expected a type name in parameter annotation".into()
                } else {
                    format!("Unknown type `{invalid}`")
                },
            );
            return ValueType::Unknown;
        }
        let expected = ValueType::union(members.into_iter().map(ValueType::named));
        if !expected.accepts(&actual) && !initial.is_some_and(|v| numeric_zero(&expected, v)) {
            db.error(
                "type.mismatch",
                file,
                range,
                format!("Expected {expected}, found {actual}"),
            );
        }
        expected
    } else {
        if actual == ValueType::Unknown {
            db.error(
                "type.cannot-infer",
                file,
                range,
                "Specify a type when the initial value cannot be inferred",
            );
        }
        actual
    }
}
pub(super) fn binary_type(operator: char, a: &ValueType, b: &ValueType) -> ValueType {
    if let ValueType::Union(members) = a {
        return ValueType::union(
            members
                .iter()
                .map(|member| binary_type(operator, member, b)),
        );
    }
    if let ValueType::Union(members) = b {
        return ValueType::union(
            members
                .iter()
                .map(|member| binary_type(operator, a, member)),
        );
    }
    if a == &ValueType::Unknown || b == &ValueType::Unknown {
        return ValueType::Unknown;
    }
    let numeric =
        |t: &ValueType| matches!(t, ValueType::Named(n) if n == "number" || n == "integer");
    let dimensional = |t: &ValueType| matches!(t, ValueType::Named(n) if matches!(n.as_str(), "length" | "length-percentage" | "percentage" | "angle" | "time" | "frequency" | "resolution" | "flex"));
    let length_percentage = |t: &ValueType| matches!(t, ValueType::Named(n) if matches!(n.as_str(), "length" | "percentage" | "length-percentage"));
    match operator {
        '+' | '-' if a == b && (numeric(a) || dimensional(a)) => a.clone(),
        '+' | '-' if numeric(a) && numeric(b) => ValueType::named("number"),
        '+' | '-' if length_percentage(a) && length_percentage(b) => {
            ValueType::named("length-percentage")
        }
        '*' if numeric(b) && (numeric(a) || dimensional(a)) => a.clone(),
        '*' if numeric(a) && dimensional(b) => b.clone(),
        '/' if numeric(b) && (numeric(a) || dimensional(a)) => {
            if numeric(a) {
                ValueType::named("number")
            } else {
                a.clone()
            }
        }
        _ => ValueType::Unknown,
    }
}

pub fn infer_type(value: &str) -> ValueType {
    let cleaned = without_comments(value);
    let value = cleaned.trim();
    if value.starts_with("--")
        || value.contains('$')
        || value.starts_with("var(")
        || value.starts_with("calc(")
        || value.starts_with("min(")
        || value.starts_with("max(")
        || value.starts_with("clamp(")
    {
        return ValueType::Unknown;
    }
    let lower = value.to_ascii_lowercase();
    const NAMED_COLORS: &str = "aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue blueviolet brown burlywood cadetblue chartreuse chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan darkgoldenrod darkgray darkgreen darkgrey darkkhaki darkmagenta darkolivegreen darkorange darkorchid darkred darksalmon darkseagreen darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink deepskyblue dimgray dimgrey dodgerblue firebrick floralwhite forestgreen fuchsia gainsboro ghostwhite gold goldenrod gray green greenyellow grey honeydew hotpink indianred indigo ivory khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen lightskyblue lightslategray lightslategrey lightsteelblue lightyellow lime limegreen linen magenta maroon mediumaquamarine mediumblue mediumorchid mediumpurple mediumseagreen mediumslateblue mediumspringgreen mediumturquoise mediumvioletred midnightblue mintcream mistyrose moccasin navajowhite navy oldlace olive olivedrab orange orangered orchid palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink plum powderblue purple rebeccapurple red rosybrown royalblue saddlebrown salmon sandybrown seagreen seashell sienna silver skyblue slateblue slategray slategrey snow springgreen steelblue tan teal thistle tomato turquoise violet wheat white whitesmoke yellow yellowgreen transparent currentcolor";
    if value.starts_with('#')
        && matches!(value.len(), 4 | 5 | 7 | 9)
        && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
        || [
            "rgb(",
            "rgba(",
            "hsl(",
            "hsla(",
            "hwb(",
            "oklch(",
            "oklab(",
            "lab(",
            "lch(",
            "color(",
            "color-mix(",
        ]
        .iter()
        .any(|s| lower.starts_with(s) && lower.ends_with(')'))
        || NAMED_COLORS
            .split_ascii_whitespace()
            .any(|name| name == lower)
    {
        return ValueType::named("color");
    }
    if value.starts_with("url(") {
        return ValueType::named("url");
    }
    if value.contains("gradient(") {
        return ValueType::named("image");
    }
    if value == "0" {
        return ValueType::named("length");
    }
    if value.parse::<i64>().is_ok() {
        return ValueType::named("integer");
    }
    if value.parse::<f64>().is_ok() {
        return ValueType::named("number");
    }
    let boundary = value
        .char_indices()
        .filter(|(i, _)| *i > 0 && value[..*i].parse::<f64>().is_ok())
        .map(|(i, _)| i)
        .next_back()
        .unwrap_or(0);
    if boundary > 0 && value[..boundary].parse::<f64>().is_ok() {
        let unit = value[boundary..].to_ascii_lowercase();
        return DIMENSION_UNITS
            .iter()
            .find(|(_, units)| units.contains(&unit.as_str()))
            .map_or(ValueType::Unknown, |(kind, _)| ValueType::named(kind));
    }
    if [
        "translate(",
        "translateX(",
        "translateY(",
        "rotate(",
        "scale(",
        "matrix(",
        "perspective(",
    ]
    .iter()
    .any(|s| value.starts_with(s))
    {
        return ValueType::named(if value.contains(") ") {
            "transform-list"
        } else {
            "transform-function"
        });
    }
    ValueType::Unknown
}

pub(super) fn numeric_zero(expected: &ValueType, value: &str) -> bool {
    if let ValueType::Union(members) = expected {
        return members.iter().any(|member| numeric_zero(member, value));
    }
    matches!(expected, ValueType::Named(n) if n == "integer" || n == "number")
        && without_comments(value).trim() == "0"
}

pub(super) fn without_comments(value: &str) -> String {
    if !value.contains("/*") && !value.contains("//") {
        return value.to_owned();
    }
    let source = SourceFile::new(FileId("value".into()), 0, value.to_owned());
    let parsed = xcss_syntax::parse(std::sync::Arc::new(source));
    let mut bytes = value.as_bytes().to_vec();
    for token in parsed
        .tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Comment)
    {
        bytes[token.range.start..token.range.end].fill(b' ');
    }
    String::from_utf8(bytes).unwrap()
}
