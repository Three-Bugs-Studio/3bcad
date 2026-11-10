//! The 3D Print tab's model side: the part laid out on the printer's bed, the printability overlays
//! and the export meshes placed for slicing.

use threebcad_core::PartDocument;
use threebcad_io::printer;
use threebcad_kernel::mesher::{self, PrintMesh};
use threebcad_kernel::print::{self as checks, Rot, Tri};
use threebcad_kernel::topology::P3;
use threebcad_render::{SceneGeometry, NO_ID};

/// Analysis colours over the body colour (sRGB and strength).
pub const OVERHANG: [u8; 4] = [200, 40, 170, 210];
pub const TOO_THIN: [u8; 4] = [225, 30, 30, 230];
pub const THIN: [u8; 4] = [250, 150, 20, 210];

const BED_LINE: [f32; 3] = [0.35, 0.37, 0.42];
const BED_GRID: [f32; 3] = [0.72, 0.74, 0.78];
const BED_PLATE: [f32; 3] = [0.55, 0.57, 0.62];
const OUT_OF_BOUNDS: [f32; 3] = [0.85, 0.15, 0.1];

/// Named meshes to export, one per body or imported mesh.
pub type Objects = Vec<(String, PrintMesh)>;

/// What the 3D Print tab shows; the scene is rebuilt when it changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrintView {
    /// Outward normal (model space) of the face that rests on the bed.
    pub down: P3,
    pub bed: [f64; 3],
    /// The overhang limit (radians from vertical) when overhangs are shown.
    pub overhang: Option<f64>,
    /// Nozzle diameter and thinnest wall (mm) when thin walls are shown.
    pub thin: Option<(f64, f64)>,
}

/// What the print checks found.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PrintReport {
    /// The part's size as it sits on the bed (mm).
    pub size: [f64; 3],
    /// Area (mm²) that needs support, under walls thinner than the nozzle, and under walls thinner
    /// than the thinnest wall.
    pub overhang: f64,
    pub too_thin: f64,
    pub thin: f64,
}

/// Where printing puts a part: turned onto its bed face, standing on z = 0 with its footprint
/// centred on `center`. The rotation, and the shift after it.
pub fn placement(down: P3, points: impl Iterator<Item = P3>, center: [f64; 2]) -> (Rot, P3) {
    let r = checks::lay_flat(down);
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in points {
        let q = checks::rotate(&r, p);
        for i in 0..3 {
            lo[i] = lo[i].min(q[i]);
            hi[i] = hi[i].max(q[i]);
        }
    }
    if !lo[0].is_finite() {
        return (r, [0.0; 3]);
    }
    (
        r,
        [
            center[0] - (lo[0] + hi[0]) / 2.0,
            center[1] - (lo[1] + hi[1]) / 2.0,
            -lo[2],
        ],
    )
}

fn place(r: &Rot, shift: P3, p: P3) -> P3 {
    let q = checks::rotate(r, p);
    std::array::from_fn(|i| q[i] + shift[i])
}

/// Colours what the checks flag, stands the part on the bed (centred on the origin) and draws the
/// bed. `g` holds only the part (triangles, edges, vertices) in model space.
pub fn lay_out(g: &mut SceneGeometry, v: &PrintView) -> PrintReport {
    let tris: Vec<Tri> = g
        .triangles
        .chunks_exact(3)
        .map(|t| [0, 1, 2].map(|k| t[k].pos.map(f64::from)))
        .collect();
    let mut report = PrintReport::default();
    let mut tint = vec![[0u8; 4]; tris.len()];
    if let Some(max) = v.overhang {
        for (i, hangs) in checks::overhangs(&tris, v.down.map(|c| -c), max)
            .into_iter()
            .enumerate()
        {
            if hangs {
                tint[i] = OVERHANG;
                report.overhang += checks::area(&tris[i]);
            }
        }
    }
    if let Some((nozzle, min_wall)) = v.thin {
        // Up to the thicker limit, so walls thinner than the nozzle show even below a smaller min wall;
        // the display mesh is f32, so a wall exactly at the limit must not flicker in.
        for (i, w) in checks::wall_thickness(&tris, min_wall.max(nozzle) - 1e-4)
            .into_iter()
            .enumerate()
        {
            let Some(w) = w else { continue };
            if w < nozzle {
                tint[i] = TOO_THIN;
                report.too_thin += checks::area(&tris[i]);
            } else {
                tint[i] = THIN;
                report.thin += checks::area(&tris[i]);
            }
        }
    }
    for (t, c) in g.triangles.chunks_exact_mut(3).zip(&tint).filter(|(_, c)| c[3] > 0) {
        t.iter_mut().for_each(|v| v.tint = *c);
    }

    let (r, shift) = placement(v.down, tris.iter().flatten().copied(), [0.0; 2]);
    let f32x3 = |p: P3| p.map(|c| c as f32);
    let moved = |p: [f32; 3]| f32x3(place(&r, shift, p.map(f64::from)));
    for t in &mut g.triangles {
        t.pos = moved(t.pos);
        t.nor = f32x3(checks::rotate(&r, t.nor.map(f64::from)));
    }
    for l in g.lines.iter_mut().chain(&mut g.edges) {
        l.pos = moved(l.pos);
    }
    for p in &mut g.points {
        p.0 = moved(p.0);
    }
    if let Some((lo, hi)) = g.bounds() {
        report.size = std::array::from_fn(|i| f64::from(hi[i] - lo[i]));
    }

    // The bed and the build volume above it, red when the part does not fit.
    let fits = printer::overflow(report.size, v.bed) == [0.0; 3];
    let [w, d, h] = v.bed.map(|c| c as f32 / 2.0);
    let h = h * 2.0;
    let corner = |x: f32, y: f32, z: f32| [x * w, y * d, z];
    let outline = if fits { BED_LINE } else { OUT_OF_BOUNDS };
    for x in (1..).map(|i| i as f32 * 10.0).take_while(|x| *x < w) {
        for s in [-x, x] {
            g.line([s, -d, 0.0], [s, d, 0.0], BED_GRID, NO_ID);
        }
    }
    for y in (1..).map(|i| i as f32 * 10.0).take_while(|y| *y < d) {
        for s in [-y, y] {
            g.line([-w, s, 0.0], [w, s, 0.0], BED_GRID, NO_ID);
        }
    }
    g.line([-w, 0.0, 0.0], [w, 0.0, 0.0], BED_GRID, NO_ID);
    g.line([0.0, -d, 0.0], [0.0, d, 0.0], BED_GRID, NO_ID);
    let ring = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    for k in 0..4 {
        let ((x0, y0), (x1, y1)) = (ring[k], ring[(k + 1) % 4]);
        g.line(corner(x0, y0, 0.0), corner(x1, y1, 0.0), outline, NO_ID);
        g.line(corner(x0, y0, h), corner(x1, y1, h), BED_GRID, NO_ID);
        g.line(corner(x0, y0, 0.0), corner(x0, y0, h), BED_GRID, NO_ID);
    }
    g.add_plane(ring.map(|(x, y)| corner(x, y, -0.01)), BED_PLATE, NO_ID);
    report
}

/// Bodies and imported meshes meshed at the export quality (chord in mm, angle in degrees), named.
pub fn export_meshes(doc: &PartDocument, chord: f64, angle_deg: f64) -> Objects {
    let bodies = doc.bodies();
    bodies
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let name = if bodies.len() == 1 {
                doc.name.clone()
            } else {
                format!("{} - Body {}", doc.name, i + 1)
            };
            (name, mesher::print_mesh_within(&b.solid, chord, angle_deg.to_radians()))
        })
        .chain(doc.built().meshes.iter().map(|(id, m)| {
            let name = doc.history().get(*id).map_or_else(|| "Mesh".into(), |f| f.name.clone());
            (name, (**m).clone())
        }))
        .collect()
}

/// `objects` as the part is printed: turned onto its bed face `down` and centred on the plate.
pub fn on_bed(objects: &Objects, down: P3, bed: [f64; 3]) -> Objects {
    let (r, shift) = placement(
        down,
        objects.iter().flat_map(|(_, m)| m.vertices.iter().copied()),
        [bed[0] / 2.0, bed[1] / 2.0],
    );
    objects
        .iter()
        .map(|(name, m)| {
            let vertices = m.vertices.iter().map(|&v| place(&r, shift, v)).collect();
            let mesh = PrintMesh {
                vertices,
                triangles: m.triangles.clone(),
                face_of: m.face_of.clone(),
            };
            (name.clone(), mesh)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use threebcad_core::DEFAULT_PRINT_DOWN;

    use super::*;
    use crate::files::Settings;

    #[test]
    fn exports_stand_on_the_bed_face_in_the_middle_of_the_plate() {
        // The demo's 30 mm cube and 20 x 40 cylinder, laid on the cube's +X face.
        let mut doc = PartDocument::demo();
        let s = Settings::default();
        for down in [DEFAULT_PRINT_DOWN, [1.0, 0.0, 0.0]] {
            doc.set_print_down(down);
            let objects = on_bed(&export_meshes(&doc, s.chord, s.angle_deg), doc.print_down(), s.bed);
            assert_eq!(objects.len(), 2);
            let pts = || objects.iter().flat_map(|(_, m)| m.vertices.iter());
            let lo = |i: usize| pts().map(|p| p[i]).fold(f64::INFINITY, f64::min);
            let hi = |i: usize| pts().map(|p| p[i]).fold(f64::NEG_INFINITY, f64::max);
            assert!(lo(2).abs() < 1e-9, "on the bed");
            assert!(((lo(0) + hi(0)) / 2.0 - s.bed[0] / 2.0).abs() < 1e-9);
            assert!(((lo(1) + hi(1)) / 2.0 - s.bed[1] / 2.0).abs() < 1e-9);
            // Upright the 40 mm cylinder is the tallest; on its +X face the part's 65 mm run along
            // X (cube 0..30, cylinder 45..65) stands up.
            let height = hi(2) - lo(2);
            let want = if down == DEFAULT_PRINT_DOWN { 40.0 } else { 65.0 };
            assert!((height - want).abs() < 1e-2, "{down:?}: {height}");
        }
    }
}
