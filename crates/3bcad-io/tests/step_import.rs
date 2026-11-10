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

/// Shapes that came in wrong from downloaded files before v1.0: a hole in a part written in metres
/// (Onshape), sphere corners (OCCT's edges on surface curves), a drill point and a countersink, a tube whose
/// cylinders have two circle loops and no seam (SolidWorks, Parasolid), and NIST's FTC-10 (AP203,
/// angles in degrees, many blends). truck once meshed these halfway or ballooned them.
#[test]
fn downloaded_file_cases_import_with_their_volume() {
    let mut wrong = Vec::new();
    for (file, occt, tol) in [
        ("occt_tiny_hole_m.step", 32.8584, 0.005),
        ("occt_rounded_box.step", 2659.6342, 0.005),
        ("occt_drill_point.step", 5587.1704, 0.01),
        ("occt_tube_two_loops.step", 1343.0309, 0.005),
        ("nist_ftc_10_asme1_rb.stp", 188318.1, 0.005),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(file);
        match read_step(&std::fs::read(path).unwrap()) {
            Ok(import) => {
                let meshes: Vec<_> = import
                    .solids
                    .iter()
                    .map(|s| print_mesh(&extract_solid(s).unwrap(), 0.01))
                    .collect();
                let v = mass_properties(&meshes).volume;
                if (v - occt).abs() > tol * occt {
                    wrong.push(format!("{file}: volume {v:.3}, OCCT says {occt}"));
                }
            }
            Err(e) => wrong.push(format!("{file}: {e}")),
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
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
