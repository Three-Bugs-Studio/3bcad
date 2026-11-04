use std::f64::consts::{FRAC_PI_2, PI};
use std::io;
use std::path::Path;

use threebcad_kernel::{extract_solid, CompressedSolid, Solid};
use truck_modeling::*;
use truck_stepio::out::{CompleteStepDisplay, StepHeaderDescriptor, StepModels};
use truck_stepio::r#in::alias::{self as step, Conic3D, Curve3D, ElementarySurface, SweptCurve};
use truck_stepio::r#in::Table;
use truck_topology::compress::{CompressedEdge, CompressedFace, CompressedShell};

/// truck_modeling exports its own `Result`; conversions here fail with a message.
type Result<T, E = String> = std::result::Result<T, E>;

/// Exact B-Rep export (truck writes the AP203/AP214-compatible `ISO-10303-042` schema) in
/// millimetres, with `product` as the product name.
pub fn write_step<'a>(path: &Path, product: &str, solids: impl IntoIterator<Item = &'a Solid>) -> io::Result<()> {
    let mut compressed: Vec<_> = solids.into_iter().map(Solid::compress).collect();
    // Boolean results have intersection curves, which few STEP readers (truck's own included)
    // understand. Their leader is the sampled curve truck itself works with, and reads everywhere.
    for e in compressed
        .iter_mut()
        .flat_map(|s| s.boundaries.iter_mut())
        .flat_map(|s| s.edges.iter_mut())
    {
        if let Curve::IntersectionCurve(ic) = &e.curve {
            let leader = match ic.leader() {
                Leader::BSpline(b) => Some(b.clone()),
                Leader::Polyline(p) => polyline(p.0.clone()).ok(),
            };
            if let Some(b) = leader {
                e.curve = Curve::BSplineCurve(b);
            }
        }
    }
    let models: StepModels<'_, _, _, _> = compressed.iter().collect();
    let header = StepHeaderDescriptor {
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        organization_system: "3BCAD".into(),
        ..Default::default()
    };
    let text = CompleteStepDisplay::new(models, header).to_string();
    let name = product.replace('\'', "''");
    let text = text.replace("PRODUCT('','',", &format!("PRODUCT('{name}','{name}',"));
    std::fs::write(path, text)
}

/// What a STEP file gave: its solids in millimetres, and why any other shells were left out.
#[derive(Debug, Default)]
pub struct StepImport {
    pub solids: Vec<CompressedSolid>,
    pub skipped: Vec<String>,
}

/// Reads the closed shells of a STEP file (AP203, AP214 or AP242 B-Rep) as solids.
pub fn read_step(bytes: &[u8]) -> io::Result<StepImport> {
    let text = String::from_utf8_lossy(bytes);
    // truck can panic deep inside a malformed file; that is a bad file, not a crash.
    let result =
        std::panic::catch_unwind(|| import(&text)).unwrap_or_else(|_| Err("the STEP file is malformed".into()));
    result.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn import(text: &str) -> Result<StepImport, String> {
    let table = Table::from_step(text).ok_or("this is not a STEP (ISO 10303-21) file")?;
    let scale = mm_per_unit(text);
    let mut shells: Vec<_> = table.shell.iter().collect();
    shells.sort_by_key(|(id, _)| **id);
    let mut out = StepImport::default();
    // shortcut: every closed shell becomes its own solid, so the voids of a BREP_WITH_VOIDS
    // import as extra bodies; group shells by their MANIFOLD_SOLID_BREP when that matters.
    for (id, holder) in shells {
        let solid = table
            .to_compressed_shell(holder)
            .map_err(|e| e.to_string())
            .and_then(convert_shell)
            .and_then(|shell| {
                let solid = extract_solid(&CompressedSolid {
                    boundaries: vec![shell],
                })
                .map_err(|e| format!("not a closed solid ({e})"))?;
                Ok(match scale {
                    1.0 => solid.compress(),
                    s => builder::transformed(&solid, Matrix4::from_scale(s)).compress(),
                })
            });
        match solid {
            Ok(s) => out.solids.push(s),
            Err(e) => out.skipped.push(format!("shell #{id}: {e}")),
        }
    }
    if out.solids.is_empty() {
        let why = if out.skipped.is_empty() {
            "it has no B-Rep shells".to_string()
        } else {
            out.skipped.join("; ")
        };
        return Err(format!("no solid could be imported: {why}"));
    }
    Ok(out)
}

/// The file's length unit in mm (STEP files say it once, for the whole model).
fn mm_per_unit(text: &str) -> f64 {
    let t: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    if t.contains("'INCH'") {
        25.4
    } else if t.contains("'FOOT'") {
        304.8
    } else if t.contains("SI_UNIT(.CENTI.,.METRE.)") {
        10.0
    } else if t.contains("SI_UNIT($,.METRE.)") && !t.contains("SI_UNIT(.MILLI.,.METRE.)") {
        1000.0
    } else {
        1.0
    }
}

type StepShell = CompressedShell<Point3, Curve3D, step::Surface>;

fn convert_shell(shell: StepShell) -> Result<CompressedShell<Point3, Curve, Surface>, String> {
    let StepShell { vertices, edges, faces } = shell;
    let size = vertices
        .iter()
        .flat_map(|a| vertices.iter().take(64).map(move |b| a.distance(*b)))
        .fold(1.0, f64::max);
    let faces = faces
        .into_iter()
        .map(|f| {
            Ok(CompressedFace {
                surface: surface(f.surface, size)?,
                boundaries: f.boundaries,
                orientation: f.orientation,
            })
        })
        .collect::<Result<_, String>>()?;
    let edges = edges
        .into_iter()
        .map(|e| {
            Ok(CompressedEdge {
                vertices: e.vertices,
                curve: curve(e.curve)?,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(split_closed_edges(CompressedShell { vertices, edges, faces }))
}

/// truck has no edges that start and end at one vertex (a full circle around a hole), so each
/// is cut in two at its middle.
fn split_closed_edges(mut s: CompressedShell<Point3, Curve, Surface>) -> CompressedShell<Point3, Curve, Surface> {
    let mut halves = std::collections::HashMap::new();
    for i in 0..s.edges.len() {
        let (a, b) = s.edges[i].vertices;
        if a != b {
            continue;
        }
        let (t0, t1) = s.edges[i].curve.range_tuple();
        let mut first = s.edges[i].curve.clone();
        let second = first.cut((t0 + t1) / 2.0);
        let mid = s.vertices.len();
        s.vertices.push(first.back());
        s.edges[i] = CompressedEdge {
            vertices: (a, mid),
            curve: first,
        };
        halves.insert(i, s.edges.len());
        s.edges.push(CompressedEdge {
            vertices: (mid, a),
            curve: second,
        });
    }
    for wire in s.faces.iter_mut().flat_map(|f| f.boundaries.iter_mut()) {
        *wire = wire
            .iter()
            .flat_map(|e| match halves.get(&e.index) {
                Some(&j) if e.orientation => vec![*e, (j, true).into()],
                Some(&j) => vec![(j, false).into(), *e],
                None => vec![*e],
            })
            .collect();
    }
    s
}

fn curve(c: Curve3D) -> Result<Curve, String> {
    Ok(match c {
        Curve3D::Line(l) => Curve::Line(l),
        Curve3D::BSplineCurve(b) => Curve::BSplineCurve(b),
        Curve3D::NurbsCurve(n) => Curve::NurbsCurve(n),
        Curve3D::Polyline(p) => Curve::BSplineCurve(polyline(p.0)?),
        Curve3D::Conic(Conic3D::Ellipse(e)) => {
            let (t0, t1) = e.entity().range_tuple();
            let mut arc = arc(Point3::origin(), Vector3::unit_x(), Vector3::unit_y(), t0, t1);
            arc.transform_by(*e.transform());
            if !e.orientation() {
                arc.invert();
            }
            Curve::NurbsCurve(arc)
        }
        other => Curve::BSplineCurve(
            approximate(&other).ok_or("a hyperbola, parabola or surface curve could not be converted")?,
        ),
    })
}

/// A polyline as a degree-1 B-spline through its points.
fn polyline(points: Vec<Point3>) -> Result<BSplineCurve<Point3>> {
    let n = points.len();
    if n < 2 {
        return Err("a polyline edge has fewer than two points".into());
    }
    let mut knots: Vec<f64> = (0..n).map(|i| i as f64).collect();
    knots.insert(0, 0.0);
    knots.push((n - 1) as f64);
    BSplineCurve::try_new(KnotVec::from(knots), points).map_err(|e| e.to_string())
}

fn approximate<C: ParametricCurve<Point = Point3, Vector = Vector3> + BoundedCurve>(
    c: &C,
) -> Option<BSplineCurve<Point3>> {
    BSplineCurve::cubic_approximation(c, c.range_tuple(), 1e-5, 1e-4, 12)
}

/// Exact rational arc `center + cos t * u + sin t * v` for `t` in `t0..t1`, in quarter-turn pieces.
fn arc(center: Point3, u: Vector3, v: Vector3, t0: f64, t1: f64) -> NurbsCurve<Vector4> {
    let n = ((t1 - t0).abs() / FRAC_PI_2).ceil().max(1.0) as usize;
    let d = (t1 - t0) / n as f64;
    let w = (d / 2.0).cos();
    let at = |t: f64, weight: f64, reach: f64| {
        let p = center + (u * t.cos() + v * t.sin()) * reach;
        Vector4::new(p.x * weight, p.y * weight, p.z * weight, weight)
    };
    let mut points = vec![at(t0, 1.0, 1.0)];
    let mut knots = vec![0.0; 3];
    for i in 0..n {
        let a = t0 + d * i as f64;
        points.push(at(a + d / 2.0, w, 1.0 / w));
        points.push(at(a + d, 1.0, 1.0));
        let k = (i + 1) as f64;
        knots.extend(if i + 1 < n { vec![k, k] } else { vec![k; 3] });
    }
    NurbsCurve::new(BSplineCurve::new(KnotVec::from(knots), points))
}

fn surface(s: step::Surface, size: f64) -> Result<Surface, String> {
    let original = s.clone();
    let converted = match s {
        step::Surface::BSplineSurface(b) => return Ok(Surface::BSplineSurface(*b)),
        step::Surface::NurbsSurface(n) => return Ok(Surface::NurbsSurface(*n)),
        step::Surface::ElementarySurface(e) => match *e {
            ElementarySurface::Plane(p) => return Ok(Surface::Plane(p)),
            ElementarySurface::CylindricalSurface(c) | ElementarySurface::ConicalSurface(c) => {
                return Ok(Surface::RevolutedCurve(c.map_ref(|rc| {
                    RevolutedCurve::by_revolution(Curve::Line(*rc.entity_curve()), rc.origin(), rc.axis())
                })))
            }
            ElementarySurface::Sphere(sp) => {
                let (c, r) = (sp.entity().0.center(), sp.entity().0.radius());
                let meridian = arc(c, Vector3::unit_x() * r, Vector3::unit_z() * r, -FRAC_PI_2, FRAC_PI_2);
                Surface::RevolutedCurve(
                    sp.map_ref(|_| RevolutedCurve::by_revolution(Curve::NurbsCurve(meridian), c, Vector3::unit_z())),
                )
            }
            // shortcut: only fillet-style torus patches are tested; a full torus (an O-ring, closed
            // both ways) may mesh with holes. Add a sample when such parts matter.
            ElementarySurface::ToroidalSurface(t) => {
                let e = t.entity();
                let (c, big, small) = (e.center(), e.large_radius(), e.small_radius());
                let meridian = arc(
                    c + Vector3::unit_x() * big,
                    Vector3::unit_x() * small,
                    Vector3::unit_z() * small,
                    -PI,
                    PI,
                );
                Surface::RevolutedCurve(
                    t.map_ref(|_| RevolutedCurve::by_revolution(Curve::NurbsCurve(meridian), c, Vector3::unit_z())),
                )
            }
        },
        step::Surface::SweptCurve(sc) => match *sc {
            SweptCurve::ExtrudedCurve(ec) => {
                let base = match curve(ec.entity_curve().clone())? {
                    Curve::BSplineCurve(b) => b,
                    Curve::Line(l) => {
                        // A STEP line is endless: extend it well past the part.
                        let d = (l.1 - l.0).normalize() * size * 2.0;
                        Line(l.0 - d, l.1 + d).to_bspline()
                    }
                    other => approximate(&other).ok_or("an extruded surface could not be converted")?,
                };
                // The extrusion is endless too; the face's edges trim it.
                let v = ec.extruding_vector().normalize() * size * 2.0;
                Surface::BSplineSurface(BSplineSurface::homotopy(
                    base.transformed(Matrix4::from_translation(-v)),
                    base.transformed(Matrix4::from_translation(v)),
                ))
            }
            SweptCurve::RevolutedCurve(rc) => {
                let profile = curve(rc.entity().entity_curve().clone())?;
                let (o, a) = (rc.entity().origin(), rc.entity().axis());
                Surface::RevolutedCurve(rc.map_ref(|_| RevolutedCurve::by_revolution(profile, o, a)))
            }
        },
    };
    Ok(facing(&original, converted))
}

/// The converted surface turned to face the same way as the original. Any parameter works as a
/// probe: both are the same surface, and this one is off the poles of spheres and tori.
fn facing(original: &step::Surface, mut s: Surface) -> Surface {
    let (u, v) = (0.37, 0.41);
    let p = original.subs(u, v);
    if let Some((u1, v1)) = s.search_parameter(p, None, 100) {
        if original.normal(u, v).dot(s.normal(u1, v1)) < 0.0 {
            s = match s {
                // Turning the profile round keeps truck's mesher happy; a flipped processor does not.
                Surface::RevolutedCurve(p) => {
                    let (o, a) = (p.entity().origin(), p.entity().axis());
                    let mut profile = p.entity().entity_curve().clone();
                    profile.invert();
                    Surface::RevolutedCurve(p.map_ref(|_| RevolutedCurve::by_revolution(profile, o, a)))
                }
                mut other => {
                    other.invert();
                    other
                }
            };
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;

    use super::*;

    #[test]
    fn arcs_are_exact() {
        let a = arc(
            Point3::new(1.0, 2.0, 3.0),
            Vector3::unit_x() * 5.0,
            Vector3::unit_z() * 5.0,
            -1.0,
            TAU - 1.0,
        );
        let (t0, t1) = a.range_tuple();
        for i in 0..=50 {
            let p = a.subs(t0 + (t1 - t0) * i as f64 / 50.0);
            assert!((p.distance(Point3::new(1.0, 2.0, 3.0)) - 5.0).abs() < 1e-12);
            assert!((p.y - 2.0).abs() < 1e-12);
        }
        assert!(a.front().distance(a.back()) < 1e-12);
    }

    #[test]
    fn units_are_read() {
        assert_eq!(
            mm_per_unit("#1=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));"),
            1.0
        );
        assert_eq!(
            mm_per_unit("#1=( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT( $, .METRE. ) );"),
            1000.0
        );
        assert_eq!(mm_per_unit("#2=CONVERSION_BASED_UNIT('INCH',#3);"), 25.4);
    }
}
