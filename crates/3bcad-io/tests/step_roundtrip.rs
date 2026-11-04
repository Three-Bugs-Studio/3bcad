//! STEP export read back by our importer, and (in CI) by OpenCASCADE: `STEP_OUT=dir cargo test -p
//! threebcad-io --test step_roundtrip -- --ignored` writes the files that
//! `.github/scripts/occt_step_check.py` checks.

use threebcad_io::{read_step, write_step};
use threebcad_kernel::boolean::{subtract, transformed};
use threebcad_kernel::mass::mass_properties;
use threebcad_kernel::mesher::print_mesh;
use threebcad_kernel::operations::{extrude_circle, extrude_polygon};
use threebcad_kernel::{extract_solid, Solid};

fn parts() -> Vec<(&'static str, Solid)> {
    let plate = extrude_polygon(&[[0.0, 0.0], [40.0, 0.0], [40.0, 25.0], [0.0, 25.0]], 8.0).unwrap();
    let hole = extrude_circle([20.0, 12.5], 6.0, 20.0).unwrap();
    let hole = transformed(
        &hole,
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, -5.0, 1.0],
        ],
    );
    let plate_hole = subtract(&plate, &hole).unwrap().remove(0);
    vec![
        ("plate_hole", plate_hole),
        ("rod", extrude_circle([0.0, 0.0], 7.5, 30.0).unwrap()),
        (
            "l_block",
            extrude_polygon(
                &[
                    [0.0, 0.0],
                    [30.0, 0.0],
                    [30.0, 8.0],
                    [8.0, 8.0],
                    [8.0, 20.0],
                    [0.0, 20.0],
                ],
                12.0,
            )
            .unwrap(),
        ),
    ]
}

fn volume(s: &Solid) -> f64 {
    mass_properties(&[print_mesh(s, 0.002)]).volume
}

#[test]
fn exported_step_reads_back_with_name_units_and_volume() {
    let dir = std::env::temp_dir().join(format!("3bcad-step-rt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, solid) in parts() {
        let path = dir.join(format!("{name}.step"));
        write_step(&path, name, [&solid]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains(&format!("PRODUCT('{name}','{name}'")),
            "{name}: product name"
        );
        assert!(text.contains("SI_UNIT(.MILLI.,.METRE.)"), "{name}: millimetres");
        let back = read_step(text.as_bytes()).unwrap();
        assert_eq!(back.solids.len(), 1, "{name}");
        let (v0, v1) = (volume(&solid), volume(&extract_solid(&back.solids[0]).unwrap()));
        assert!((v1 - v0).abs() < 1e-3 * v0, "{name}: {v0} became {v1}");
    }
    std::fs::remove_dir_all(dir).ok();
}

/// Writes the parts and their volumes for the OpenCASCADE check in CI.
#[test]
#[ignore = "run by CI with STEP_OUT set"]
fn write_parts_for_occt() {
    let dir = std::path::PathBuf::from(std::env::var("STEP_OUT").expect("set STEP_OUT to a folder"));
    std::fs::create_dir_all(&dir).unwrap();
    let mut list = String::new();
    for (name, solid) in parts() {
        write_step(&dir.join(format!("{name}.step")), name, [&solid]).unwrap();
        list += &format!("{name} {:.4}\n", volume(&solid));
    }
    std::fs::write(dir.join("volumes.txt"), list).unwrap();
}
