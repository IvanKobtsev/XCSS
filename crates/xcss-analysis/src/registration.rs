use super::*;
pub fn registration_initial_is_independent(value: &str) -> bool {
    let parse = xcss_syntax::parse(std::sync::Arc::new(xcss_source::SourceFile::new(
        FileId("value".into()),
        0,
        value.to_owned(),
    )));
    !parse.tokens.iter().any(|token| {
        let text = parse.slice(token.range).to_ascii_lowercase();
        matches!(
            text.as_str(),
            "var"
                | "env"
                | "attr"
                | "currentcolor"
                | "inherit"
                | "initial"
                | "unset"
                | "revert"
                | "revert-layer"
        ) || text.starts_with("--")
            || text
                .find(|c: char| c.is_ascii_alphabetic())
                .is_some_and(|i| {
                    text[..i].parse::<f64>().is_ok()
                        && matches!(
                            &text[i..],
                            "em" | "rem"
                                | "ex"
                                | "rex"
                                | "cap"
                                | "rcap"
                                | "ch"
                                | "rch"
                                | "ic"
                                | "ric"
                                | "lh"
                                | "rlh"
                                | "cqw"
                                | "cqh"
                                | "cqi"
                                | "cqb"
                                | "cqmin"
                                | "cqmax"
                        )
                })
    })
}
