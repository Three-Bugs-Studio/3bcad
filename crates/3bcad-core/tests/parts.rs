//! Geometry regression corpus: every reference part must rebuild without errors and keep its
//! body count, face count and volume (to 0.1 %). After an intended change, regenerate the
//! expectations with `UPDATE_PARTS=1 cargo test -p threebcad-core --test parts` and review the diff.

#[path = "parts/corpus.rs"]
mod corpus;

use threebcad_kernel::mass::mass_properties;
use threebcad_kernel::mesher::print_mesh;

const EXPECTED: &str = "tests/parts/expected.txt";

fn measure(doc: &threebcad_core::PartDocument) -> (usize, usize, f64) {
    let meshes: Vec<_> = doc.bodies().iter().map(|b| print_mesh(&b.solid, 0.005)).collect();
    let faces = doc.bodies().iter().map(|b| b.topo.faces.len()).sum();
    (doc.bodies().len(), faces, mass_properties(&meshes).volume)
}

#[test]
fn reference_parts_rebuild_unchanged() {
    let actual: Vec<String> = corpus::all()
        .into_iter()
        .map(|(name, build)| {
            let doc = build();
            assert!(doc.errors().is_empty(), "{name}: {:?}", doc.errors());
            let (bodies, faces, volume) = measure(&doc);
            format!("{name} bodies={bodies} faces={faces} volume={volume:.3}")
        })
        .collect();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(EXPECTED);
    if std::env::var_os("UPDATE_PARTS").is_some() {
        std::fs::write(&path, actual.join("\n") + "\n").unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).expect("run with UPDATE_PARTS=1 once to create the expectations");
    let field = |line: &str, key: &str| -> f64 {
        line.split_whitespace()
            .find_map(|w| w.strip_prefix(&format!("{key}=")))
            .and_then(|v| v.parse().ok())
            .unwrap_or(f64::NAN)
    };
    for (want, got) in expected.lines().zip(&actual) {
        let name = want.split_whitespace().next().unwrap();
        assert!(got.starts_with(name), "corpus order changed: {want} vs {got}");
        for key in ["bodies", "faces"] {
            assert_eq!(
                field(want, key),
                field(got, key),
                "{name}: {key} changed\n want {want}\n got  {got}"
            );
        }
        let (v0, v1) = (field(want, "volume"), field(got, "volume"));
        assert!(
            (v1 - v0).abs() <= 1e-3 * v0.abs(),
            "{name}: volume changed\n want {want}\n got  {got}"
        );
    }
    assert_eq!(
        expected.lines().count(),
        actual.len(),
        "the corpus gained or lost parts"
    );
}
