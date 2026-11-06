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
            r#"{"compilerOptions":{"strict":true,"jsx":"preserve","module":"esnext","moduleResolution":"bundler"},"include":["*.ts","*.tsx"]}"#,
        ),
        (
            "Button.tsx",
            r#"import styles, { type Variants as V } from './Button.xcss';
declare global { namespace JSX { interface IntrinsicElements { button: any } } }
export interface Props { variant: V<'Button'> }
export function Button({ variant }: Props) { return <button className={styles.button} />; }
export type Missing = V<'Missing'>;
styles.nonExisting;
styles['global'];
const bad: V<'Button'> = 'wrong';
const good: V<'Button'> = 'primary';
declare const dynamic: string;
const uncertain: V<'Button'> = dynamic;
const escape: V<'Button'> = dynamic as any;
const unrelated: number = 'ignore this';
"#,
        ),
        (
            "index.tsx",
            "import { Button } from './Button'; const bad = <Button variant='wrong' />; const good = <Button variant='primary' />;",
        ),
    ]);
    let diagnostics = check(&p, &snapshot);
    assert_eq!(diagnostics.len(), 6, "{diagnostics:#?}");
    assert!(
        diagnostics
            .iter()
            .all(|d| !d.message.contains("ignore this"))
    );
    assert!(diagnostics.iter().any(|d| d.location.file.0 == "index.tsx"));
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
            r#"{"compilerOptions":{"strict":true,"module":"esnext","moduleResolution":"bundler","paths":{"@/*":["./*"]}},"files":["index.ts"]}"#,
        ),
        (
            "index.ts",
            "import { type Variants as A } from '@/a.xcss'; import { type Variants as B } from './b.xcss'; const a: A<'Size'> = 'large'; const b: B<'Size'> = 'small';",
        ),
    ]);
    assert_eq!(check(&p, &host.snapshot()).len(), 2);
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
    let diagnostics = check(&p, &snapshot);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("Cannot resolve XCSS")),
        "{diagnostics:?}"
    );
    let missing_config = check(
        &Project {
            config: "missing.json".into(),
            files: Default::default(),
        },
        &snapshot,
    );
    assert!(!missing_config.is_empty());
    assert_eq!(missing_config[0].location.file.0, "missing.json");
}
