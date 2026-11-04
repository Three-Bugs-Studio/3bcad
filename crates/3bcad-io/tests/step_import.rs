//! STEP files written by OpenCASCADE (the kernel behind FreeCAD); `tests/data/occt_samples.py`
//! regenerates them and prints the volumes OCCT computes.

use threebcad_io::read_step;
use threebcad_kernel::extract_solid;
use threebcad_kernel::mass::mass_properties;
use threebcad_kernel::mesher::print_mesh;

fn volume(file: &str) -> f64 {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(file);
    let import = read_step(&std::fs::read(path).unwrap()).unwrap_or_else(|e| panic!("{file}: {e}"));
    assert!(import.skipped.is_empty(), "{file}: {:?}", import.skipped);
    assert_eq!(import.solids.len(), 1, "{file}");
    let solid = extract_solid(&import.solids[0]).unwrap();
    mass_properties(&[print_mesh(&solid, 0.002)]).volume
}

#[test]
fn occt_samples_import_with_their_volume() {
    for (file, occt) in [
        ("occt_block_hole.step", 11214.6018),
        ("occt_sphere.step", 2144.6606),
        ("occt_rounded_rod.step", 6231.6595),
        ("occt_cone.step", 2450.4423),
        ("occt_fillet.step", 7961.3717),
    ] {
        let v = volume(file);
        // The volume comes from a 0.002 mm mesh, which is slightly smaller than the true shape.
        assert!(
            (v - occt).abs() < 2e-3 * occt,
            "{file}: volume {v:.3}, OCCT says {occt}"
        );
    }
}

#[test]
fn garbage_is_refused_not_panicked_on() {
    assert!(read_step(b"").is_err());
    assert!(read_step(b"ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=FOO(;\nENDSEC;\nEND-ISO-10303-21;").is_err());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/occt_block_hole.step");
    let good = std::fs::read(path).unwrap();
    for cut in [good.len() / 3, good.len() / 2, good.len() - 40] {
        let _ = read_step(&good[..cut]);
    }
}
