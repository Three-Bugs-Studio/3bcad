//! Export meshing (the 3D Print tab's quality) of imported STEP files with faces that never meet
//! the angle tolerance, however finely they are meshed.

use threebcad_io::read_step;
use threebcad_kernel::extract_solid;
use threebcad_kernel::mesher::print_mesh_within;

/// FTC-10's drill points meet at an apex whose triangles turn 62° at any chord, and truck lays the
/// OCCT drill point's tip off its surface. These faces once drove the whole part's mesh down to
/// 1e-4 mm (FTC-10: 1.7 M triangles and seconds; NIST CTC-02 never finished).
#[test]
fn exports_stop_refining_faces_that_never_straighten() {
    for (file, before, most) in [
        ("nist_ftc_10_asme1_rb.stp", 1_685_602, 100_000),
        ("occt_drill_point.step", 4_285, 2_000),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(file);
        let import = read_step(&std::fs::read(path).unwrap()).unwrap();
        for (quality, chord, angle) in [("Coarse", 0.1, 30f64), ("Fine", 0.01, 10.0)] {
            let n: usize = import
                .solids
                .iter()
                .map(|s| print_mesh_within(&extract_solid(s).unwrap(), chord, angle.to_radians()))
                .map(|m| m.triangles.len())
                .sum();
            assert!(
                n < most,
                "{file} at {quality}: {n} triangles (v1.1 made {before} at Coarse)"
            );
        }
    }
}
