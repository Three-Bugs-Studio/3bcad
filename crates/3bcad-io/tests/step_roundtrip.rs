//! STEP export read back by our importer, and (in CI) by OpenCASCADE: `STEP_OUT=dir cargo test -p
//! threebcad-io --test step_roundtrip -- --ignored` writes the files that
//! `.github/scripts/occt_step_check.py` checks.

use threebcad_io::{read_step, write_step, write_step_assembly};
use threebcad_kernel::boolean::{subtract, transformed};
use threebcad_kernel::mass::mass_properties;
use threebcad_kernel::mate::Pose;
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
        write_step(&path, name, [&solid], &[]).unwrap();
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

/// A fixture: the L block where it was made, a rod beside it and a rod lying along Y above it.
/// Returns the file's text, its volume and its bounding box.
fn write_fixture(path: &std::path::Path) -> (String, f64, [f64; 6]) {
    let all = parts();
    let (rod, block) = (&all[1].1, &all[2].1);
    let lying = Pose::IDENTITY
        .rotated([std::f64::consts::FRAC_PI_2, 0.0, 0.0], [0.0; 3])
        .moved([0.0, 50.0, 10.0]);
    write_step_assembly(
        path,
        "fixture",
        &[("rod", vec![rod]), ("l_block", vec![block])],
        &[
            ("l_block<1>", 1, Pose::IDENTITY),
            ("rod<1>", 0, Pose::at([100.0, 0.0, 0.0])),
            ("rod<2>", 0, lying),
        ],
    )
    .unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    // Rods: 15 across and 30 long; the block 30 x 20 x 12.
    (
        text,
        2.0 * volume(rod) + volume(block),
        [-7.5, -7.5, 0.0, 107.5, 50.0, 30.0],
    )
}

#[test]
fn an_assembly_keeps_its_parts_once_and_places_each_component() {
    let path = std::env::temp_dir().join(format!("3bcad-step-asm-{}.step", std::process::id()));
    let (text, ..) = write_fixture(&path);
    for name in [
        "PRODUCT('fixture','fixture'",
        "PRODUCT('rod','rod'",
        "PRODUCT('l_block','l_block'",
    ] {
        assert!(text.contains(name), "{name}");
    }
    assert_eq!(text.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE(").count(), 3);
    assert_eq!(text.matches("ITEM_DEFINED_TRANSFORMATION(").count(), 3);
    assert!(text.contains("'rod<2>'"));
    // Every entity is defined once, and every reference points at one.
    let defined: Vec<&str> = text
        .lines()
        .filter_map(|l| l.split_once(" = ").map(|(id, _)| id))
        .collect();
    let mut unique = defined.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), defined.len(), "entity numbers are unique");
    let data = text.split_once("DATA;").unwrap().1;
    for r in data.split('#').skip(1) {
        let n: String = r.chars().take_while(char::is_ascii_digit).collect();
        assert!(
            unique.contains(&format!("#{n}").as_str()),
            "#{n} is used but not defined"
        );
    }
    // Each different part is in the file once: truck reads back two solids.
    let back = read_step(text.as_bytes()).unwrap();
    assert_eq!(back.solids.len(), 2);
    std::fs::remove_file(path).ok();
}

/// Writes the parts and their volumes for the OpenCASCADE check in CI.
#[test]
#[ignore = "run by CI with STEP_OUT set"]
fn write_parts_for_occt() {
    let dir = std::path::PathBuf::from(std::env::var("STEP_OUT").expect("set STEP_OUT to a folder"));
    std::fs::create_dir_all(&dir).unwrap();
    let mut list = String::new();
    for (name, solid) in parts() {
        write_step(&dir.join(format!("{name}.step")), name, [&solid], &[]).unwrap();
        list += &format!("{name} {:.4}\n", volume(&solid));
    }
    // The assembly: OpenCASCADE must find its components where they were placed.
    let (_, v, b) = write_fixture(&dir.join("fixture.step"));
    list += &format!("fixture {v:.4} {}\n", b.map(|x| x.to_string()).join(" "));
    std::fs::write(dir.join("volumes.txt"), list).unwrap();
    // Face colours: the plate's bottom red and its top blue, each found by where its centre is.
    let plate = &parts()[0].1;
    let faces = threebcad_kernel::mesher::render_mesh(plate, 0.01).faces;
    let facing = |z: f64| {
        faces
            .iter()
            .position(|f| f.is_some_and(|p| p.normal()[2] * z > 0.5))
            .unwrap()
    };
    let mut colors = vec![None; faces.len()];
    colors[facing(-1.0)] = Some([255, 0, 0]);
    colors[facing(1.0)] = Some([0, 0, 255]);
    write_step(&dir.join("colored_plate.step"), "colored_plate", [plate], &colors).unwrap();
    std::fs::write(
        dir.join("colors.txt"),
        "colored_plate 255 0 0 20 12.5 0\ncolored_plate 0 0 255 20 12.5 8\n",
    )
    .unwrap();
}
