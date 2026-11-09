// Codes are assigned once. Append new codes within each family without renumbering.
const ENTRIES: &[(&str, &str, &str)] = &[
    ("XC-001", "XC-001", "Unknown XCSS class"),
    ("XC-002", "XC-002", "Unknown XCSS variant group"),
    ("XC-003", "XC-003", "Invalid XCSS variant value"),
    ("XC-004", "XC-004", "Unresolved XCSS module"),
    (
        "binding.ambiguous-injection",
        "XC-200",
        "Ambiguous injection",
    ),
    (
        "binding.composition-conflict",
        "XC-201",
        "Conflicting class composition",
    ),
    (
        "binding.composition-cycle",
        "XC-202",
        "Class composition cycle",
    ),
    (
        "binding.composition-owner",
        "XC-203",
        "Invalid class composition owner",
    ),
    (
        "binding.constant-position",
        "XC-204",
        "Invalid constant reference position",
    ),
    (
        "binding.constant-redeclaration",
        "XC-205",
        "Constant redeclaration",
    ),
    ("binding.content-scope", "XC-206", "Invalid content scope"),
    ("binding.duplicate", "XC-207", "Duplicate declaration"),
    ("binding.import-conflict", "XC-208", "Conflicting import"),
    ("binding.injection-cycle", "XC-209", "Injection cycle"),
    ("binding.missing-export", "XC-210", "Missing export"),
    (
        "binding.parameter-scope",
        "XC-211",
        "Invalid parameter scope",
    ),
    ("binding.unknown-injection", "XC-212", "Unknown injection"),
    (
        "binding.unresolved-import",
        "XC-213",
        "Unresolved stylesheet import",
    ),
    ("syntax.css", "XC-100", "Invalid CSS syntax"),
    (
        "syntax.depth",
        "XC-101",
        "Stylesheet nesting limit exceeded",
    ),
    (
        "syntax.injection-placement",
        "XC-102",
        "Invalid injection placement",
    ),
    (
        "syntax.interpolation",
        "XC-103",
        "Unsupported interpolation",
    ),
    (
        "syntax.invalid-construct",
        "XC-104",
        "Invalid XCSS construct",
    ),
    (
        "syntax.invalid-statement",
        "XC-105",
        "Invalid XCSS statement",
    ),
    ("syntax.missing-token", "XC-106", "Missing token"),
    (
        "syntax.parameter",
        "XC-107",
        "Invalid parameter declaration",
    ),
    ("syntax.sass", "XC-108", "Unsupported Sass syntax"),
    ("syntax.unclosed-comment", "XC-109", "Unclosed comment"),
    ("syntax.unclosed-string", "XC-110", "Unclosed string"),
    (
        "syntax.unexpected-close",
        "XC-111",
        "Unexpected closing token",
    ),
    ("syntax.util-content", "XC-112", "Invalid utility content"),
    (
        "syntax.util-local-class",
        "XC-113",
        "Local class in utility",
    ),
    ("syntax.util-modifier", "XC-114", "Invalid utility modifier"),
    ("syntax.util-order", "XC-115", "Invalid utility order"),
    (
        "syntax.util-placement",
        "XC-116",
        "Invalid utility placement",
    ),
    ("syntax.util-selector", "XC-117", "Invalid utility selector"),
    ("syntax.variable", "XC-118", "Invalid variable declaration"),
    (
        "syntax.vars-placement",
        "XC-119",
        "Invalid variable block placement",
    ),
    (
        "type.ambiguous-arithmetic",
        "XC-300",
        "Ambiguous arithmetic",
    ),
    (
        "type.ambiguous-transition",
        "XC-301",
        "Ambiguous transition",
    ),
    ("type.argument-count", "XC-302", "Wrong number of arguments"),
    ("type.argument-mismatch", "XC-303", "Argument type mismatch"),
    ("type.arithmetic", "XC-304", "Invalid arithmetic"),
    ("type.cannot-infer", "XC-305", "Cannot infer type"),
    ("type.mismatch", "XC-306", "Type mismatch"),
    ("type.parameter-order", "XC-307", "Invalid parameter order"),
    (
        "type.property-initial",
        "XC-308",
        "Invalid property initial value",
    ),
    (
        "type.transition-group",
        "XC-309",
        "Invalid transition group",
    ),
    ("type.unexpected-content", "XC-310", "Unexpected content"),
    ("type.unknown-type", "XC-311", "Unknown type"),
    (
        "variants.duplicate-group",
        "XC-400",
        "Duplicate variant group",
    ),
    (
        "variants.duplicate-member",
        "XC-401",
        "Duplicate variant member",
    ),
    (
        "variants.placement",
        "XC-402",
        "Invalid variant group placement",
    ),
    ("variants.unknown-class", "XC-403", "Unknown variant class"),
];

pub(super) fn metadata(code: &'static str) -> (&'static str, &'static str, Option<&'static str>) {
    let Ok(index) = ENTRIES.binary_search_by_key(&code, |&(key, _, _)| key) else {
        return (code, "XCSS diagnostic", None);
    };
    let (key, code, label) = ENTRIES[index];
    (code, label, (key != code).then_some(key))
}

#[cfg(test)]
mod tests {
    use super::ENTRIES;
    use std::collections::BTreeSet;

    #[test]
    fn entries_are_sorted_and_codes_are_unique() {
        assert!(ENTRIES.windows(2).all(|pair| pair[0].0 < pair[1].0));
        let codes: BTreeSet<_> = ENTRIES.iter().map(|&(_, code, _)| code).collect();
        assert_eq!(codes.len(), ENTRIES.len());
        assert!(!codes.contains("XC-005"));
    }
}
