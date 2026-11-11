use xcss_language_service::{LanguageService, ServiceSnapshot};
use xcss_source::{FileId, Position};

fn id(name: &str) -> FileId {
    FileId(name.into())
}

fn at(source: &str, marker: &str, skip: usize) -> Position {
    let offset = source.find(marker).unwrap() + skip;
    let prefix = &source[..offset];
    Position {
        line: prefix.bytes().filter(|&b| b == b'\n').count(),
        character: prefix.rsplit('\n').next().unwrap().encode_utf16().count(),
    }
}

fn labels(snapshot: &ServiceSnapshot, file: &str, pos: Position) -> Vec<String> {
    snapshot
        .completion(&id(file), pos)
        .into_iter()
        .map(|item| item.label)
        .collect()
}

#[test]
fn stylesheet_classes_groups_and_members_have_separate_usages() {
    let mut service = LanguageService::new();
    let style = "@variants Button: .primary, .secondary;\n@variants Other: .primary;\n.primary {} .secondary {} .hyphen-name {} :global(.hidden) {}";
    service.set_file(id("a.xcss"), 1, style.into()).unwrap();
    service
        .set_file(
            id("b.xcss"),
            1,
            "@variants Button: .primary; .primary {}".into(),
        )
        .unwrap();
    service.set_typescript_project(Some("tsconfig.json".into()));
    service.set_typescript_file(id("tsconfig.json"), 1,
        r#"{"compilerOptions":{"strict":true,"jsx":"preserve"},"files":["index.tsx","closed.ts"]}"#.into()).unwrap();
    let ts = r#"import { styles, type Variants as V } from './a.xcss';
import { styles as other, type Variants as W } from './b.xcss';
/* 😀 */ styles.primary; styles["hyphen-name"]; other.primary;
const button: V<"Button"> = "primary";
const otherGroup: V<"Other"> = "primary";
const foreign: W<"Button"> = "primary";
declare function Control(props: { variant: V<"Button"> }): any;
const view = <Control variant="primary" />;
const both: V<"Button" | "Other"> = "primary";
const unrelated: "primary" = "primary";
styles.pri;
styles.;
styles[];
styles[""];
const group: V<"Bu"> = "" as any;
const emptyGroup: V<> = "" as any;
const pending: V<"Button"> = "sec";
const pendingJsx = <Control variant="" />;"#;
    service
        .set_typescript_file(id("index.tsx"), 1, ts.into())
        .unwrap();
    service
        .set_typescript_file(
            id("closed.ts"),
            1,
            "import type { Variants } from './a.xcss'; const v: Variants<'Button'> = 'primary';"
                .into(),
        )
        .unwrap();
    let snapshot = service.snapshot();
    let definition = |marker, skip| snapshot.definition(&id("index.tsx"), at(ts, marker, skip));
    let class = definition("styles.primary", 7);
    assert_eq!(class[0].range.start, at(style, ".primary {}", 1));
    let group = definition(r#"V<"Button">"#, 3);
    assert_eq!(group[0].range.start, at(style, "Button:", 0));
    let jsx = definition(r#"variant="primary""#, 9);
    assert_eq!(jsx[0].range.start, at(style, ".primary,", 1));
    assert_eq!(definition(r#"| "Other"> = "primary""#, 14).len(), 2);
    let member = at(style, ".primary,", 1);
    let usages = snapshot.references(&id("a.xcss"), member, false);
    assert_eq!(usages.len(), 4, "{usages:?}");
    assert!(usages.iter().any(|usage| usage.file == id("closed.ts")));
    assert_eq!(snapshot.references(&id("a.xcss"), member, true).len(), 5);
    assert_eq!(snapshot.definition(&id("a.xcss"), member), class);
    assert_eq!(
        snapshot
            .references(&id("a.xcss"), class[0].range.start, false)
            .len(),
        3
    );
    assert_eq!(
        labels(&snapshot, "index.tsx", at(ts, "styles.pri;", 10)),
        ["primary", "secondary"]
    );
    assert_eq!(
        labels(&snapshot, "index.tsx", at(ts, "styles.;", 7)),
        ["primary", "secondary"]
    );
    assert_eq!(
        labels(&snapshot, "index.tsx", at(ts, "styles[]", 7)),
        ["hyphen-name", "primary", "secondary"]
    );
    assert_eq!(
        labels(&snapshot, "index.tsx", at(ts, r#"styles[""]"#, 8)),
        ["hyphen-name", "primary", "secondary"]
    );
    assert_eq!(
        labels(&snapshot, "index.tsx", at(ts, r#"V<"Bu">"#, 5)),
        ["Button", "Other"]
    );
    assert_eq!(
        labels(&snapshot, "index.tsx", at(ts, "V<>", 2)),
        ["Button", "Other"]
    );
    assert_eq!(
        labels(&snapshot, "index.tsx", at(ts, "\"sec\"", 4)),
        ["primary", "secondary"]
    );
    assert_eq!(
        labels(&snapshot, "index.tsx", at(ts, r#"variant="" />"#, 9)),
        ["primary", "secondary"]
    );
}

#[test]
fn stylesheet_completion_uses_module_exports() {
    let mut service = LanguageService::new();
    let style = r#"@import recipes from "./recipes";
@variants Button: .first, .;
.first { @:recipes.paint; }
.second {}
:global(.hidden) {}"#;
    let recipes = ".other {} @injection paint { .injected {} } @injection unused { .absent {} }";
    service.set_file(id("a.xcss"), 1, style.into()).unwrap();
    service
        .set_file(id("recipes.xcss"), 1, recipes.into())
        .unwrap();
    service.set_resolution(id("a.xcss"), "./recipes".into(), Some(id("recipes.xcss")));
    let snapshot = service.snapshot();
    assert_eq!(
        labels(&snapshot, "a.xcss", at(style, ".;", 1)),
        ["injected", "second"]
    );
}
