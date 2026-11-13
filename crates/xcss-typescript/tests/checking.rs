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
fn xclass_checks_branded_inputs_and_effective_spreads() {
    let mut host = AnalysisHost::new();
    host.set_file(FileId("a.xcss".into()), 1, ".button {} .other {}")
        .unwrap();
    let p = project(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"strict":true,"jsx":"preserve","module":"esnext","moduleResolution":"bundler"},"files":["app.tsx"]}"#,
        ),
        (
            "app.tsx",
            r#"
import { styles } from './a.xcss';
declare global { namespace JSX { interface IntrinsicElements { div: any } } }
type ClassName = string & { readonly __xcssClassName__: true };
declare const condition: boolean;
declare const external: string;
declare const optional: { xClass?: readonly [ClassName] };
declare function Custom(props: { xClass: string }): any;
const choice = condition ? styles.button : styles.other;
const bad = { xClass: external };
const fixed = [choice, condition && styles.other, null] as const;
<div className={styles.button} xClass={fixed} />;
<div xClass={[external as ClassName]} />;
<div {...bad} xClass={[choice]} />;
<div xClass={external} {...{ xClass: [styles.other] as const }} />;
<Custom xClass={external} />;
<div xClass="plain" />;
<div xClass={123} />;
<div xClass={{ plain: true }} />;
<div xClass={[choice, external]} />;
<div {...bad} />;
<div xClass={external} {...optional} />;
<div xClass={[styles.missing]} />;
<div xClass={[...fixed]} />;
<div xClass={[[styles.button]]} />;
"#,
        ),
    ]);
    let diagnostics = check(&p, &host.snapshot()).unwrap();
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        [
            "XC-009", "XC-009", "XC-009", "XC-009", "XC-009", "XC-009", "XC-001", "XC-009",
            "XC-009"
        ],
        "{diagnostics:?}"
    );
}

#[test]
fn empty_variable_dictionary_and_optional_branded_component_input() {
    let mut host = AnalysisHost::new();
    host.set_file(FileId("a.xcss".into()), 1, ".button {} .other {}")
        .unwrap();
    let p = project(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"strict":true,"jsx":"preserve","module":"esnext","moduleResolution":"bundler"},"files":["app.tsx"]}"#,
        ),
        (
            "app.tsx",
            r#"
import { styles, cssVars as vars } from './a.xcss';
type ClassName = string & { readonly __xcssClassName__: true };
declare global { namespace JSX { interface IntrinsicElements { div: any } } }
function Button({ className }: { className?: ClassName }) {
  return <div xClass={[styles.button, className]} xVars={{ [vars.none]: '2px' }} />;
}
"#,
        ),
    ]);
    let diagnostics = check(&p, &host.snapshot()).unwrap();
    assert_eq!(
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        ["XC-006"],
        "{diagnostics:?}"
    );
}

#[test]
fn missing_exports_do_not_cascade_into_container_errors() {
    let mut host = AnalysisHost::new();
    host.set_file(
        FileId("a.xcss".into()),
        1,
        ".button {} @vars { @provided --width = 1px; }",
    )
    .unwrap();
    let p = project(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"strict":true,"jsx":"preserve","module":"esnext","moduleResolution":"bundler"},"files":["app.tsx"]}"#,
        ),
        (
            "app.tsx",
            r#"
import { styles as s, cssVars as v } from './a.xcss';
declare global { namespace JSX { interface IntrinsicElements { div: any } } }
const missing = s.absent;
const map = { [v.absent]: '2px', [v.width]: '3px' };
<div xClass={[s.button, missing]} xVars={map} />;
<div xClass={[s.absent, 'plain']} xVars={{ [v.absent]: '2px', '--private': 1 }} />;
"#,
        ),
    ]);
    let diagnostics = check(&p, &host.snapshot()).unwrap();
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes.iter().filter(|&&code| code == "XC-001").count(),
        2,
        "{diagnostics:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&code| code == "XC-006").count(),
        2,
        "{diagnostics:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&code| code == "XC-009").count(),
        1,
        "{diagnostics:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&code| code == "XC-007").count(),
        1,
        "{diagnostics:?}"
    );
}

#[test]
fn xclass_rejects_ambiguous_variant_labels() {
    let mut host = AnalysisHost::new();
    for file in ["a.xcss", "b.xcss"] {
        host.set_file(
            FileId(file.into()),
            1,
            "@variants Button: .primary; .primary {}",
        )
        .unwrap();
    }
    let p = project(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"strict":true,"jsx":"preserve","module":"esnext","moduleResolution":"bundler"},"files":["app.tsx"]}"#,
        ),
        (
            "app.tsx",
            r#"
import type { Variants as A } from './a.xcss';
import type { Variants as B } from './b.xcss';
declare const variant: A<'Button'> | B<'Button'>;
<button xClass={[variant]} />;
"#,
        ),
    ]);
    let diagnostics = check(&p, &host.snapshot()).unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "XC-009" && d.message.contains("more than one stylesheet")),
        "{diagnostics:?}"
    );
}

#[test]
fn provided_vars_check_effective_jsx_maps() {
    let mut host = AnalysisHost::new();
    host.set_file(
        FileId("a.xcss".into()),
        1,
        "
        @vars {
            @provided --width = 0px;
            @provided --size: length-percentage;
            --private = 0px;
        }
    ",
    )
    .unwrap();
    host.set_file(
        FileId("b.xcss".into()),
        1,
        "@vars { @provided { --angle = 0deg; --count: integer; --color: color; } }",
    )
    .unwrap();
    let snapshot = host.snapshot();
    assert!(
        snapshot.diagnostics().is_empty(),
        "{:?}",
        snapshot.diagnostics()
    );
    let p = project(&[
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"allowJs":true,"strict":true,"jsx":"preserve","module":"esnext","moduleResolution":"bundler"},"include":["*.ts","*.tsx","*.jsx"]}"#,
        ),
        (
            "props.ts",
            r#"
import { cssVars as a } from './a.xcss';
import { cssVars as b } from './b.xcss';
export declare function px(n: number): `${number}px`;
export declare function percentage(n: number): `${number}%`;
export const width = a.width;
export const ok = { [width]: px(3), [b.angle]: '1rad' as const };
export const wrong = { [width]: percentage(3) };
export declare const maybe: typeof ok | typeof wrong;
export declare const dynamic: string;
export declare const optional: { xVars?: typeof ok };
export declare const typedColor: string & { readonly __xcssCssValue__: 'color' };
"#,
        ),
        (
            "app.tsx",
            r#"
import { cssVars as a } from './a.xcss';
import { cssVars as b } from './b.xcss';
import { width, ok, wrong, maybe, px, percentage, dynamic, optional, typedColor } from './props';
declare global { namespace JSX { interface IntrinsicElements { div: any } } }
declare function Custom(props: { xVars: unknown }): unknown;
const unrelated: number = 'ignore';
const dict = a;
dict.private;
b['missing'];
const misspelled = 'misspelled';
b[misspelled];
<div xVars={{ [width]: '2REM', [a.size]: percentage(2), [b.angle]: '1turn', [b.count]: 2, [b.color]: 'red' }} />;
<div xVars={{ ...ok, [b.color]: typedColor }} />;
<div {...{ xVars: { ...wrong, [width]: px(2) } }} />;
<div xVars={wrong} {...{ xVars: ok }} />;
<div xVars={null} />;
<div xVars={{}} />;
<div xVars={{ [width]: percentage(2) }} />;
<div xVars={{ [b.count]: 2.5 }} />;
<div xVars={maybe} />;
<div xVars={wrong} {...optional} />;
<div {...{ xVars: wrong }} />;
<div xVars={{ '--private': px(2) }} />;
<div xVars={{ [dynamic]: px(2) }} />;
<div xVars />;
<Custom xVars={wrong} />;
"#,
        ),
        (
            "legacy.jsx",
            r#"
import { cssVars as a } from './a.xcss';
a.unknown;
<div xVars={{ [a.width]: '2%' }} />;
"#,
        ),
    ]);
    let diagnostics = check(&p, &snapshot).unwrap();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|d| {
            let source = p.files[&d.location.file].text();
            let range = d.location.range;
            (d.code, &source[range.start..range.end])
        })
        .collect();
    assert_eq!(
        actual,
        [
            ("XC-006", "private"),
            ("XC-006", "'missing'"),
            ("XC-006", "misspelled"),
            ("XC-008", "percentage(2)"),
            ("XC-008", "2.5"),
            ("XC-008", "maybe"),
            ("XC-008", "wrong"),
            ("XC-008", "wrong"),
            ("XC-007", "{ '--private': px(2) }"),
            ("XC-007", "{ [dynamic]: px(2) }"),
            ("XC-007", "xVars"),
            ("XC-006", "unknown"),
            ("XC-008", "'2%'"),
        ],
        "{diagnostics:?}"
    );
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
            r#"import { styles, type Variants as V } from './Button.xcss';
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
            r#"import { styles as aliased, type Variants as A } from '@/a.xcss';
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
            "declare module '*.xcss' { export const styles: Record<string, string>; }",
        ),
        (
            "index.ts",
            "import { styles as s } from './missing.xcss'; s.anything;",
        ),
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
