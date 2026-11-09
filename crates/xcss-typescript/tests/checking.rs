use std::sync::Arc;
use xcss_analysis::AnalysisHost;
use xcss_source::{FileId, SourceFile};
use xcss_typescript::{Project, check};

fn project(files: &[(&str, &str)]) -> Project {
    Project {
        config: "tsconfig.json".into(),
        files: files
            .iter()
            .map(|(path, text)| {
                let id = FileId((*path).into());
                (id.clone(), Arc::new(SourceFile::new(id, 1, *text)))
            })
            .collect(),
    }
}

#[test]
fn exact_classes_variants_and_cross_file_jsx() {
    let mut host = AnalysisHost::new();
    host.set_file(FileId("Button.xcss".into()), 1, "@variants Button: .primary, .secondary; .button {} .primary {} .secondary {} :global(.global) {}").unwrap();
    let snapshot = host.snapshot();
    assert!(
        snapshot.diagnostics().is_empty(),
        "{:?}",
        snapshot.diagnostics()
    );
    let p = project(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"strict":true,"jsx":"preserve","module":"esnext","moduleResolution":"bundler","types":["missing-types"]},"include":["*.ts","*.tsx"]}"#,
        ),
        (
            "Button.tsx",
            r#"import styles, { type Variants as V } from './Button.xcss';
import { helper } from 'app/helpers';
declare global { namespace JSX { interface IntrinsicElements { button: any } } }
export interface Props { variant: V<'Button'> }
export function Button({ variant }: Props) { return <button className={styles.button} />; }
export type Missing = V<'Missing'>;
styles.nonExisting;
styles.buttoN;
styles['global'];
const bad: V<'Button'> = 'wrong';
const number: V<'Button'> = 42;
const good: V<'Button'> = 'primary';
declare const dynamic: string;
const uncertain: V<'Button'> = dynamic;
const escape: V<'Button'> = dynamic as any;
const unrelated: number = 'ignore this';
const unrelatedObject = { button: 'button' };
unrelatedObject.nonExisting;
function shadow(styles: { button: string }) { return styles.nonExisting; }
type Variants<G extends 'Other'> = G;
type Other = Variants<'Missing'>;
type Unrelated = V<'Button'> extends 'primary' | 'secondary' ? number : string;
const conditional: Unrelated = 'ignore this too';
"#,
        ),
        (
            "index.tsx",
            "import { Button } from './Button'; const bad = <Button variant='wrong' />; const number = <Button variant={42} />; const shorthand = <Button variant />; const good = <Button variant='primary' />;",
        ),
    ]);
    let diagnostics = check(&p, &snapshot).unwrap();
    let actual = diagnostics
        .iter()
        .map(|d| {
            let source = p.files[&d.location.file].text();
            let range = d.location.range;
            (d.code, &source[range.start..range.end], d.message.as_str())
        })
        .collect::<Vec<_>>();
    let invalid_value = "Value is not valid for variant group 'Button' in 'Button.xcss'.";
    assert_eq!(
        actual,
        [
            (
                "XC-002",
                "'Missing'",
                "'Missing' is not a variant group in 'Button.xcss'."
            ),
            (
                "XC-001",
                "nonExisting",
                "Class 'nonExisting' can't be found in 'Button.xcss'."
            ),
            (
                "XC-001",
                "buttoN",
                "Class 'buttoN' can't be found in 'Button.xcss'."
            ),
            (
                "XC-001",
                "'global'",
                "Class 'global' can't be found in 'Button.xcss'."
            ),
            ("XC-003", "'wrong'", invalid_value),
            ("XC-003", "42", invalid_value),
            ("XC-003", "dynamic", invalid_value),
            ("XC-003", "'wrong'", invalid_value),
            ("XC-003", "42", invalid_value),
            ("XC-003", "variant", invalid_value),
        ]
    );
    assert_eq!(diagnostics.last().unwrap().location.file.0, "index.tsx");
}

#[test]
fn module_isolation_and_alias_resolution() {
    let mut host = AnalysisHost::new();
    host.set_file(
        FileId("a.xcss".into()),
        1,
        "@variants Size: .small; .small {}",
    )
    .unwrap();
    host.set_file(
        FileId("b.xcss".into()),
        1,
        "@variants Size: .large; .large {}",
    )
    .unwrap();
    let p = project(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"noImplicitAny":false,"module":"esnext","moduleResolution":"bundler","paths":{"@/*":["./*"]}},"files":["index.ts"]}"#,
        ),
        (
            "index.ts",
            r#"import aliased, { type Variants as A } from '@/a.xcss';
import { type Variants as B } from './b.xcss';
aliased.small;
aliased['large'];
type WrongGroup = A<'Other'>;
const a: A<'Size'> = 'large';
const b: B<'Size'> = 'small';
declare function set(value: A<'Size'>): void;
set('large');
const options: { variant: A<'Size'> } = { variant: 'large' };
"#,
        ),
    ]);
    let diagnostics = check(&p, &host.snapshot()).unwrap();
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>(),
        [
            "Class 'large' can't be found in 'a.xcss'.",
            "'Other' is not a variant group in 'a.xcss'.",
            "Value is not valid for variant group 'Size' in 'a.xcss'.",
            "Value is not valid for variant group 'Size' in 'b.xcss'.",
            "Value is not valid for variant group 'Size' in 'a.xcss'.",
            "Value is not valid for variant group 'Size' in 'a.xcss'.",
        ]
    );
}

#[test]
fn missing_modules_and_config_errors_are_not_hidden() {
    let snapshot = AnalysisHost::new().snapshot();
    let p = project(&[
        ("tsconfig.json", r#"{"files":["index.ts","client.d.ts"]}"#),
        (
            "client.d.ts",
            "declare module '*.xcss' { const styles: Record<string, string>; export default styles; }",
        ),
        ("index.ts", "import s from './missing.xcss'; s.anything;"),
    ]);
    let diagnostics = check(&p, &snapshot).unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "XC-004");
    assert_eq!(
        diagnostics[0].message,
        "Can't resolve XCSS module './missing.xcss'."
    );
    let missing_config = check(
        &Project {
            config: "missing.json".into(),
            files: Default::default(),
        },
        &snapshot,
    )
    .unwrap_err();
    assert!(missing_config.contains("missing.json"), "{missing_config}");

    let malformed = project(&[("tsconfig.json", "{ invalid:"), ("index.ts", "")]);
    let reason = check(&malformed, &snapshot).unwrap_err();
    assert!(reason.contains("Property value"), "{reason}");

    for (config, missing) in [
        (
            r#"{"extends":"./missing-base.json","files":["index.ts"]}"#,
            "missing-base.json",
        ),
        (r#"{"files":["missing-entry.ts"]}"#, "missing-entry.ts"),
    ] {
        let inputs = project(&[("tsconfig.json", config), ("index.ts", "")]);
        let reason = check(&inputs, &snapshot).unwrap_err();
        assert!(reason.contains(missing), "{reason}");
    }
}
