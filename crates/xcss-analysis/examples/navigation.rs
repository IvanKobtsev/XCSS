use xcss_analysis::{AnalysisHost, SymbolKind};
use xcss_source::FileId;

fn main() {
    let mut host = AnalysisHost::new();
    let library = FileId("theme.xcss".into());
    let consumer = FileId("button.xcss".into());
    host.set_file(
        library.clone(),
        1,
        "@injection tone($color: color) { color: $color; }",
    )
    .unwrap();
    host.set_file(
        consumer.clone(),
        1,
        "@import { tone } from \"./theme.xcss\"; .button { @:tone(red); }",
    )
    .unwrap();
    host.set_resolution(consumer.clone(), "./theme.xcss", Some(library));
    let snapshot = host.snapshot();
    assert!(
        snapshot.diagnostics().is_empty(),
        "{:?}",
        snapshot.diagnostics()
    );
    let symbol = snapshot
        .symbols()
        .iter()
        .find(|symbol| symbol.kind == SymbolKind::Injection)
        .unwrap();
    for reference in snapshot.references_to(symbol.id) {
        println!(
            "{}:{:?} -> {}:{:?}",
            reference.location.file.0,
            reference.location.range,
            symbol.location.file.0,
            symbol.location.range
        );
    }
    host.set_file(consumer, 2, ".button { @:tone(blue); }")
        .unwrap();
    assert!(host.snapshot().diagnostics().is_empty());
    assert_eq!(snapshot.references_to(symbol.id).count(), 2);
}
