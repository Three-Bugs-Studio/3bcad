//! Export (STL, 3MF, STEP) and the bridge to desktop slicers.

pub mod slicer;
pub mod step;
pub mod stl;
pub mod threemf;

pub use slicer::Slicer;
pub use step::write_step;
pub use stl::write_stl;
pub use threemf::write_3mf;

#[cfg(test)]
mod tests {
    use std::io::Read;

    use threebcad_kernel::{mesher::print_mesh, operations::extrude_polygon};

    use super::*;

    #[test]
    fn exports_round_trip_basic_structure() {
        let solid = extrude_polygon(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], 10.0).unwrap();
        let mesh = print_mesh(&solid, 0.01);
        let dir = std::env::temp_dir().join(format!("3bcad-io-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let stl = dir.join("cube.stl");
        write_stl(&stl, std::slice::from_ref(&mesh)).unwrap();
        assert_eq!(
            std::fs::metadata(&stl).unwrap().len(),
            84 + 50 * mesh.triangles.len() as u64
        );

        let tmf = dir.join("cube.3mf");
        write_3mf(&tmf, std::slice::from_ref(&mesh)).unwrap();
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&tmf).unwrap()).unwrap();
        let mut model = String::new();
        zip.by_name("3D/3dmodel.model")
            .unwrap()
            .read_to_string(&mut model)
            .unwrap();
        assert_eq!(model.matches("<triangle ").count(), mesh.triangles.len());
        assert!(zip.by_name("[Content_Types].xml").is_ok() && zip.by_name("_rels/.rels").is_ok());

        let step = dir.join("cube.step");
        write_step(&step, [&solid]).unwrap();
        let text = std::fs::read_to_string(&step).unwrap();
        assert!(text.starts_with("ISO-10303-21;") && text.contains("MANIFOLD_SOLID_BREP"));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn step_exports_a_boolean_result() {
        // A cut leaves intersection-curve edges; the exporter must still write them.
        let cube = extrude_polygon(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], 10.0).unwrap();
        let hole = threebcad_kernel::operations::extrude_circle([5.0, 5.0], 2.0, 20.0).unwrap();
        let hole = threebcad_kernel::boolean::transformed(
            &hole,
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, -5.0, 1.0],
            ],
        );
        let cut = threebcad_kernel::boolean::subtract(&cube, &hole).unwrap().remove(0);
        let path = std::env::temp_dir().join(format!("3bcad-step-bool-{}.step", std::process::id()));
        write_step(&path, [&cut]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let faces = text.matches("= ADVANCED_FACE(").count() + text.matches("= FACE_SURFACE(").count();
        assert!(text.contains("MANIFOLD_SOLID_BREP"));
        assert_eq!(faces, cut.face_iter().count(), "every face is written");
        std::fs::remove_file(path).ok();
    }
}
