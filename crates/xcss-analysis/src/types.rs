use super::*;

pub(super) fn checked_type(
    db: &mut Database,
    file: &FileId,
    range: TextRange,
    annotation: Option<&str>,
    initial: Option<&str>,
) -> ValueType {
    let actual = initial.map(infer_type).unwrap_or(ValueType::Unknown);
    if let Some(annotation) = annotation {
        if !matches!(
            annotation,
            "length"
                | "length-percentage"
                | "number"
                | "integer"
                | "percentage"
                | "angle"
                | "time"
                | "frequency"
                | "resolution"
                | "flex"
                | "color"
                | "image"
                | "url"
                | "transform-function"
                | "transform-list"
        ) {
            db.error(
                "type.unknown-type",
                file,
                range,
                format!("Unknown type `{annotation}`"),
            );
            return ValueType::Unknown;
        }
        let expected = ValueType::named(annotation);
        if !expected.accepts(&actual) {
            db.error(
                "type.mismatch",
                file,
                range,
                format!("Expected {annotation}, found {actual:?}"),
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
        let kind = match unit.as_str() {
            "%" => "percentage",
            "px" | "em" | "rem" | "vh" | "vw" | "vmin" | "vmax" | "dvh" | "dvw" | "svh" | "lvh"
            | "ch" | "ex" | "cm" | "mm" | "in" | "pt" | "pc" | "q" | "lh" | "rlh" | "cqw"
            | "cqh" | "cqi" | "cqb" | "rex" | "cap" | "rcap" | "rch" | "ic" | "ric" | "vi"
            | "vb" | "svw" | "lvw" | "cqmin" | "cqmax" => "length",
            "deg" | "rad" | "grad" | "turn" => "angle",
            "s" | "ms" => "time",
            "hz" | "khz" => "frequency",
            "dpi" | "dpcm" | "dppx" | "x" => "resolution",
            "fr" => "flex",
            _ => return ValueType::Unknown,
        };
        return ValueType::named(kind);
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

fn without_comments(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    let mut quote = None;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == q {
                quote = None;
            }
        } else if matches!(c, '\'' | '"') {
            quote = Some(c);
            out.push(c);
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(next) = chars.next() {
                if next == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}
