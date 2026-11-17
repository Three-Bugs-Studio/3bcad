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
    assert!(
        import.skipped.is_empty() && import.warnings.is_empty(),
        "{file}: {:?} {:?}",
        import.skipped,
        import.warnings
    );
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

/// The sum of a mesh's triangles' vector areas over their area: zero when the mesh closes, and what
/// is left bounds how far its volume can be off.
fn gap(meshes: &[threebcad_kernel::mesher::PrintMesh]) -> f64 {
    let (mut sum, mut area) = ([0.0f64; 3], 0.0);
    for m in meshes {
        for t in &m.triangles {
            let [a, b, c] = t.map(|i| m.vertices[i]);
            let (u, v) = ([0, 1, 2].map(|k| b[k] - a[k]), [0, 1, 2].map(|k| c[k] - a[k]));
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            (0..3).for_each(|k| sum[k] += n[k] / 2.0);
            area += n.iter().map(|x| x * x).sum::<f64>().sqrt() / 2.0;
        }
    }
    sum.iter().map(|x| x * x).sum::<f64>().sqrt() / area
}

/// Shapes that came in wrong before v1.2, rebuilt with OpenCASCADE (`occt_samples.py`) from the
/// downloaded files they stand for, and NIST's CTC-02 (AP203): a concave fillet round a boss (its
/// shell did not close), holes along x placed with no reference direction (NIST STC-07: refused),
/// drill points placed at their apex (NIST CTC-02: cones built the wrong way, the mesh open), and a
/// slotted screw head domed by a spindle torus with a chamfer (OCCT's screw.step came in 44% small,
/// this one at twice its volume). Each closes and has OCCT's volume.
#[test]
fn v1_2_cases_import_closed_with_their_volume() {
    let mut wrong = Vec::new();
    for (file, occt) in [
        ("occt_boss_fillet.step", 5439.5494),
        ("occt_x_holes.step", 9643.8055),
        ("occt_drill_points.step", 62365.1837),
        ("occt_screw_head.step", 16347.8833),
        ("nist_ctc_02_asme1_rc.stp", 47100535.4928),
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
                let (v, g) = (mass_properties(&meshes).volume, gap(&meshes));
                // The volume comes from a 0.01 mm mesh, a little smaller than the true shape.
                if (v - occt).abs() > 1e-3 * occt || g > 1e-5 || !import.warnings.is_empty() {
                    wrong.push(format!(
                        "{file}: volume {v:.3}, OCCT says {occt}; gap {g:.1e}; {:?}",
                        import.warnings
                    ));
                }
            }
            Err(e) => wrong.push(format!("{file}: {e}")),
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Faces truck's mesher cannot place: a face whose edges are off its surface (OCCT's linkrods.step
/// came in 1.4% small so) imports with a warning; a periodic B-spline wall (NIST CTC-02 AP203) is
/// refused, saying why, as truck's mesher never finishes it.
#[test]
fn faces_the_mesher_cannot_place_are_reported() {
    let read = |file: &str| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(file);
        read_step(&std::fs::read(path).unwrap())
    };
    let lifted = read("occt_lifted_face.step").unwrap();
    assert_eq!(lifted.solids.len(), 1);
    assert!(
        lifted.warnings.iter().any(|w| w.contains("face(s) 5 ")),
        "{:?}",
        lifted.warnings
    );
    let tube = read("occt_tube_bspline.step").unwrap_err().to_string();
    assert!(tube.contains("closed B-spline surface"), "{tube}");
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
