use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::io;
use std::path::Path;

use threebcad_kernel::mate::Pose;
use threebcad_kernel::{extract_solid, CompressedSolid, Solid};
use truck_modeling::*;
use truck_stepio::out::{CompleteStepDisplay, StepHeaderDescriptor, StepModels};
use truck_stepio::r#in::alias::{self as step, Conic3D, Curve3D, ElementarySurface, SweptCurve};
use truck_stepio::r#in::Table;
use truck_topology::compress::{CompressedEdge, CompressedFace, CompressedShell};

/// truck_modeling exports its own `Result`; conversions here fail with a message.
type Result<T, E = String> = std::result::Result<T, E>;

/// Exact B-Rep export (truck writes the AP203/AP214-compatible `ISO-10303-042` schema) in
/// millimetres, with `product` as the product name. `colors` is the sRGB colour of each face of
/// the solids in order (as `RenderMesh::faces` lists them); `None` and missing faces get none.
pub fn write_step<'a>(
    path: &Path,
    product: &str,
    solids: impl IntoIterator<Item = &'a Solid>,
    colors: &[Option<[u8; 3]>],
) -> io::Result<()> {
    let text = name_products(&part_text(path, solids), &[product]);
    std::fs::write(path, color_faces(&text, colors))
}

/// The entity numbers in `s`.
fn ids(s: &str) -> impl Iterator<Item = usize> + '_ {
    s.split('#').skip(1).filter_map(|t| {
        let digits = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
        t[..digits].parse().ok()
    })
}

/// Adds AP214 face colours to a file truck wrote (whose shells list the faces in the solids'
/// order): a `STYLED_ITEM` per coloured face, gathered in a
/// `MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION`, which OpenCASCADE and most CAD read.
fn color_faces(text: &str, colors: &[Option<[u8; 3]>]) -> String {
    let faces: Vec<usize> = text
        .lines()
        .filter_map(|l| l.split_once("_SHELL("))
        .flat_map(|(_, list)| ids(list))
        .collect();
    let context = text
        .find("GEOMETRIC_REPRESENTATION_CONTEXT")
        .and_then(|at| ids(&text[text[..at].rfind('#')?..at]).next());
    let (Some(context), Some(end)) = (context, text.rfind("ENDSEC;")) else {
        return text.to_string();
    };
    if colors.iter().all(Option::is_none) {
        return text.to_string();
    }
    let mut next = ids(text).max().unwrap_or(0) + 1;
    let mut add = |out: &mut String, entity: String| {
        *out += &format!("#{next} = {entity};\n");
        next += 1;
        next - 1
    };
    let mut out = String::new();
    let mut styles: Vec<([u8; 3], usize)> = Vec::new();
    let mut items = Vec::new();
    for (&face, c) in faces.iter().zip(colors) {
        let Some(c) = *c else { continue };
        let style = match styles.iter().find(|s| s.0 == c) {
            Some(s) => s.1,
            None => {
                let [r, g, b] = c.map(|v| f64::from(v) / 255.0);
                let rgb = add(&mut out, format!("COLOUR_RGB('', {r:.6}, {g:.6}, {b:.6})"));
                let fill = add(&mut out, format!("FILL_AREA_STYLE_COLOUR('', #{rgb})"));
                let area = add(&mut out, format!("FILL_AREA_STYLE('', (#{fill}))"));
                let surface = add(&mut out, format!("SURFACE_STYLE_FILL_AREA(#{area})"));
                let side = add(&mut out, format!("SURFACE_SIDE_STYLE('', (#{surface}))"));
                let usage = add(&mut out, format!("SURFACE_STYLE_USAGE(.BOTH., #{side})"));
                let s = add(&mut out, format!("PRESENTATION_STYLE_ASSIGNMENT((#{usage}))"));
                styles.push((c, s));
                s
            }
        };
        items.push(add(&mut out, format!("STYLED_ITEM('color', (#{style}), #{face})")));
    }
    let list: Vec<String> = items.iter().map(|i| format!("#{i}")).collect();
    add(
        &mut out,
        format!(
            "MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION('', ({}), #{context})",
            list.join(", ")
        ),
    );
    format!("{}{out}{}", &text[..end], &text[end..])
}

/// A whole STEP file holding one unnamed product with these solids.
fn part_text<'a>(path: &Path, solids: impl IntoIterator<Item = &'a Solid>) -> String {
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
    CompleteStepDisplay::new(models, header).to_string()
}

/// Gives the unnamed products truck wrote their names, in order.
fn name_products(text: &str, names: &[&str]) -> String {
    let mut out = text.to_string();
    for name in names {
        let name = name.replace('\'', "''");
        out = out.replacen("PRODUCT('','',", &format!("PRODUCT('{name}','{name}',"), 1);
    }
    out
}

/// `text` with every entity number raised by `by`, and the largest number in it.
fn renumber(text: &str, by: usize) -> (String, usize) {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut max = 0;
    let mut rest = text;
    while let Some(at) = rest.find('#') {
        out += &rest[..=at];
        rest = &rest[at + 1..];
        let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if let Ok(n) = rest[..digits].parse::<usize>() {
            max = max.max(n + by);
            out += &(n + by).to_string();
            rest = &rest[digits..];
        }
    }
    out += rest;
    (out, max)
}

fn real(v: f64) -> String {
    format!("{v:.15E}")
}

/// An `AXIS2_PLACEMENT_3D` numbered `id` (with its point and directions as the next three
/// numbers) at where `pose` puts a part's origin.
fn placement(id: usize, pose: &Pose) -> String {
    let col = |j: usize| (0..3).map(|i| real(pose.r[i][j])).collect::<Vec<_>>().join(",");
    let t = pose.t.map(real).join(",");
    let (pt, z, x) = (id + 1, id + 2, id + 3);
    format!(
        "#{id} = AXIS2_PLACEMENT_3D('', #{pt}, #{z}, #{x});\n#{pt} = CARTESIAN_POINT('', ({t}));\n#{z} = DIRECTION('', ({}));\n#{x} = DIRECTION('', ({}));\n",
        col(2),
        col(0),
    )
}

/// Exports an assembly: each different part once, as a product of its own with its exact
/// geometry, and one placed instance of it per component (`instances`: name, index into `parts`,
/// pose), so readers get the component names and transforms the way AP214 lays them out.
// shortcut: sub-assemblies are written as their parts, one level under the top assembly; nest
// NEXT_ASSEMBLY_USAGE_OCCURRENCEs if other tools need the sub-assembly structure.
pub fn write_step_assembly(
    path: &Path,
    name: &str,
    parts: &[(&str, Vec<&Solid>)],
    instances: &[(&str, usize, Pose)],
) -> io::Result<()> {
    if parts.is_empty() || instances.is_empty() || instances.iter().any(|&(_, k, _)| k >= parts.len()) {
        return Err(io::Error::other("the assembly has no parts to export"));
    }
    struct Part {
        definition: usize,
        shape: usize,
        origin: usize,
    }
    let mut data = String::new();
    let mut placed = Vec::new();
    let mut last = 0;
    for (_, solids) in parts {
        let text = part_text(path, solids.iter().copied());
        let body = text
            .split_once("DATA;\n")
            .and_then(|(_, rest)| rest.rsplit_once("ENDSEC;"))
            .map_or("", |(body, _)| body);
        let by = last;
        let (body, max) = renumber(body, by);
        // The part's origin: the anchor its instances are placed by.
        let origin = max + 1;
        data += &body.replacen(
            "ADVANCED_BREP_SHAPE_REPRESENTATION('', (",
            &format!("ADVANCED_BREP_SHAPE_REPRESENTATION('', (#{origin}, "),
            1,
        );
        data += &placement(origin, &Pose::IDENTITY);
        // truck numbers a product's definition #5 and its shape #10.
        placed.push(Part {
            definition: 5 + by,
            shape: 10 + by,
            origin,
        });
        last = origin + 3;
    }
    let mut id = last;
    let mut next = || {
        id += 1;
        id
    };
    let [ac, pc, p, pdf, pdc, pd, pds, sr, sdr] = std::array::from_fn(|_| next());
    let origin = next();
    let mut rest = placement(origin, &Pose::IDENTITY);
    let mut axes = vec![origin];
    let skip = |next: &mut dyn FnMut() -> usize| {
        for _ in 0..3 {
            next();
        }
    };
    skip(&mut next);
    for (k, &(inst, part, pose)) in instances.iter().enumerate() {
        let ax = next();
        skip(&mut next);
        let [nauo, ipds, idt, rr, cdsr] = std::array::from_fn(|_| next());
        axes.push(ax);
        let inst = inst.replace('\'', "''");
        let Part {
            definition,
            shape,
            origin: anchor,
        } = placed[part];
        rest += &placement(ax, &pose);
        rest += &format!(
            "#{nauo} = NEXT_ASSEMBLY_USAGE_OCCURRENCE('{}','{inst}','',#{pd},#{definition},$);\n\
             #{ipds} = PRODUCT_DEFINITION_SHAPE('','',#{nauo});\n\
             #{idt} = ITEM_DEFINED_TRANSFORMATION('','',#{anchor},#{ax});\n\
             #{rr} = ( REPRESENTATION_RELATIONSHIP('','',#{shape},#{sr}) REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION(#{idt}) SHAPE_REPRESENTATION_RELATIONSHIP() );\n\
             #{cdsr} = CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#{rr},#{ipds});\n",
            k + 1
        );
    }
    let items = axes.iter().map(|a| format!("#{a}")).collect::<Vec<_>>().join(",");
    let name = name.replace('\'', "''");
    // The first part's representation context (#11, millimetres) serves the assembly too.
    data += &format!(
        "#{ac} = APPLICATION_CONTEXT('core data for automotive mechanical design processes');\n\
         #{pc} = PRODUCT_CONTEXT('',#{ac},'mechanical');\n\
         #{p} = PRODUCT('{name}','{name}','',(#{pc}));\n\
         #{pdf} = PRODUCT_DEFINITION_FORMATION('','',#{p});\n\
         #{pdc} = PRODUCT_DEFINITION_CONTEXT('part definition',#{ac},'design');\n\
         #{pd} = PRODUCT_DEFINITION('design','',#{pdf},#{pdc});\n\
         #{pds} = PRODUCT_DEFINITION_SHAPE('','',#{pd});\n\
         #{sr} = SHAPE_REPRESENTATION('',({items}),#11);\n\
         #{sdr} = SHAPE_DEFINITION_REPRESENTATION(#{pds},#{sr});\n"
    );
    data += &rest;
    let header = part_text(path, std::iter::empty());
    let header = header.split_once("DATA;\n").map_or("", |(h, _)| h);
    let names: Vec<&str> = parts.iter().map(|(n, _)| *n).collect();
    let text = format!("{header}DATA;\n{data}ENDSEC;\nEND-ISO-10303-21;\n");
    std::fs::write(path, name_products(&text, &names))
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
    let text = edges_on_3d_curves(&cones_in_radians(&without_empty_lists(text)));
    let table = Table::from_step(&text).ok_or("this is not a STEP (ISO 10303-21) file")?;
    let scale = mm_per_unit(&text);
    let mut shells: Vec<_> = table.shell.iter().collect();
    shells.sort_by_key(|(id, _)| **id);
    let mut out = StepImport::default();
    // shortcut: every closed shell becomes its own solid, so the voids of a BREP_WITH_VOIDS
    // import as extra bodies; group shells by their MANIFOLD_SOLID_BREP when that matters.
    for (id, holder) in shells {
        let solid = table
            .to_compressed_shell(holder)
            .map_err(|e| e.to_string())
            .map(|shell| in_mm(shell, scale))
            .and_then(convert_shell)
            .and_then(|shell| {
                let solid = extract_solid(&CompressedSolid {
                    boundaries: vec![shell],
                })
                .map_err(|e| format!("not a closed solid ({e})"))?;
                Ok(solid.compress())
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

/// The unit of `kind` (`LENGTH_UNIT(`, `PLANE_ANGLE_UNIT(`) the file's B-Rep shapes are in, from
/// their representation context: PMI may be given in other units (NIST's AP242 parts are in mm
/// and radians with inches and degrees for the annotations), and a part may well be called
/// 'Foot'. Without a context, a converted unit (an inch, a degree) beats the SI one it is
/// defined from. Uppercase, without spaces.
fn unit_of(text: &str, kind: &str) -> String {
    let t = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    let entities: std::collections::HashMap<&str, &str> = t.split(';').filter_map(|e| e.split_once('=')).collect();
    let refs = |body: &str| -> Vec<String> {
        body.split('#')
            .skip(1)
            .map(|r| format!("#{}", r.split(|c: char| !c.is_ascii_digit()).next().unwrap_or("")))
            .collect()
    };
    let in_context = entities
        .values()
        .filter(|b| b.contains("BREP_SHAPE_REPRESENTATION(") || b.contains("MANIFOLD_SURFACE_SHAPE_REPRESENTATION("))
        .filter_map(|b| refs(b).pop())
        .filter_map(|ctx| entities.get(ctx.as_str()))
        .filter_map(|ctx| ctx.split("GLOBAL_UNIT_ASSIGNED_CONTEXT(").nth(1))
        .flat_map(|units| refs(units.split(')').next().unwrap_or("")))
        .filter_map(|u| entities.get(u.as_str()).copied())
        .find(|u| u.contains(kind));
    let units: Vec<&str> = t.split(';').filter(|e| e.contains(kind)).collect();
    in_context
        .or_else(|| units.iter().find(|u| u.contains("CONVERSION_BASED_UNIT(")).copied())
        .or_else(|| units.first().copied())
        .unwrap_or("")
        .to_string()
}

/// The file's length unit in mm.
fn mm_per_unit(text: &str) -> f64 {
    let unit = unit_of(text, "LENGTH_UNIT(");
    if unit.contains("CONVERSION_BASED_UNIT('INCH'") {
        return 25.4;
    }
    if unit.contains("CONVERSION_BASED_UNIT('FOOT'") {
        return 304.8;
    }
    [
        (".MILLI.,.METRE.", 1.0),
        (".CENTI.,.METRE.", 10.0),
        (".MICRO.,.METRE.", 0.001),
        ("$,.METRE.", 1000.0),
    ]
    .into_iter()
    .find(|(k, _)| unit.contains(&format!("SI_UNIT({k})")))
    .map_or(1.0, |(_, mm)| mm)
}

/// An edge's `SURFACE_CURVE` / `SEAM_CURVE` swapped for its own 3D curve (the master geometry,
/// `.CURVE_3D.`): truck-stepio 0.3 gives up on some (OCCT's fillet corners: "the point is not on
/// surface") and, as it numbers an edge before converting its curve, then shifts every later edge,
/// so whole faces come apart.
fn edges_on_3d_curves(text: &str) -> String {
    let id_end = |s: &str| s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    // `#k=SURFACE_CURVE('name',#c,...` -> k -> c
    let mut base = std::collections::HashMap::new();
    for stmt in text.split(';') {
        let stmt = stmt.trim_start();
        let Some((id, body)) = stmt.split_once('=') else {
            continue;
        };
        let body = body.trim_start();
        if !(body.starts_with("SURFACE_CURVE") || body.starts_with("SEAM_CURVE")) {
            continue;
        }
        if let Some(c) = body.split('#').nth(1) {
            base.insert(id.trim().to_string(), format!("#{}", &c[..id_end(c)]));
        }
    }
    if base.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for (i, stmt) in text.split(';').enumerate() {
        if i > 0 {
            out.push(';');
        }
        let Some(at) = stmt.find("EDGE_CURVE(") else {
            out.push_str(stmt);
            continue;
        };
        // EDGE_CURVE('name', #start, #end, #curve, .T.): the third reference is the curve.
        let mut done = stmt[..at].to_string();
        let mut rest = &stmt[at..];
        for n in 0..3 {
            let Some(h) = rest.find('#') else { break };
            let len = 1 + id_end(&rest[h + 1..]);
            done.push_str(&rest[..h]);
            let r = &rest[h..h + len];
            done.push_str(if n == 2 {
                base.get(r).map_or(r, String::as_str)
            } else {
                r
            });
            rest = &rest[h + len..];
        }
        out.push_str(&done);
        out.push_str(rest);
    }
    out
}

/// truck reads a cone's semi-angle as radians, but a file whose angle unit is the degree (NIST's
/// AP203 test parts, for one) writes `59.` for 59 degrees: those are turned into radians here.
fn cones_in_radians(text: &str) -> String {
    if !unit_of(text, "PLANE_ANGLE_UNIT(").contains("CONVERSION_BASED_UNIT('DEGREE'") {
        return text.to_string();
    }
    let upper = text.to_ascii_uppercase();
    let mut out = String::with_capacity(text.len());
    let mut rest = 0;
    for (at, _) in upper.match_indices("CONICAL_SURFACE(") {
        let open = at + "CONICAL_SURFACE(".len();
        let Some(close) = text[open..].find(')').map(|i| open + i) else {
            break;
        };
        let args: Vec<&str> = text[open..close].split(',').collect();
        if let [label @ .., angle] = &args[..] {
            if let Ok(deg) = angle.trim().parse::<f64>() {
                out.push_str(&text[rest..open]);
                out.push_str(&label.join(","));
                out.push_str(&format!(",{:?}", deg.to_radians()));
                rest = close;
            }
        }
    }
    out.push_str(&text[rest..]);
    out
}

/// ruststep 0.4 cannot read an empty list `()`, which AP242 PMI entities use (tessellated
/// annotation, annotation planes); an unset `$` reads the same for the geometry 3BCAD imports.
/// An entity's empty attribute list (`LENGTH_UNIT()`) follows a name and is kept, as are strings.
fn without_empty_lists(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            in_string = !in_string;
        } else if c == '(' && !in_string && matches!(out.trim_end().chars().last(), Some('(' | ',')) {
            let mut space = String::new();
            while let Some(&w) = chars.peek().filter(|w| w.is_whitespace()) {
                space.push(w);
                chars.next();
            }
            if chars.peek() == Some(&')') {
                chars.next();
                out.push('$');
            } else {
                out.push('(');
                out.push_str(&space);
            }
            continue;
        }
        out.push(c);
    }
    out
}

type StepShell = CompressedShell<Point3, Curve3D, step::Surface>;

/// The shell scaled to millimetres before it is converted: truck's parameter searches work to a
/// fixed tolerance (about 1e-6), which misses on a 2 mm hole written in metres (Onshape does).
fn in_mm(mut shell: StepShell, scale: f64) -> StepShell {
    if scale != 1.0 {
        let m = Matrix4::from_scale(scale);
        shell.vertices.iter_mut().for_each(|p| *p = m.transform_point(*p));
        shell.edges.iter_mut().for_each(|e| e.curve.transform_by(m));
        shell.faces.iter_mut().for_each(|f| f.surface.transform_by(m));
    }
    shell
}

fn convert_shell(shell: StepShell) -> Result<CompressedShell<Point3, Curve, Surface>, String> {
    let StepShell { vertices, edges, faces } = shell;
    let size = vertices
        .iter()
        .flat_map(|a| vertices.iter().take(64).map(move |b| a.distance(*b)))
        .fold(1.0, f64::max);
    let edges: Vec<CompressedEdge<Curve>> = edges
        .into_iter()
        .map(|e| {
            Ok(CompressedEdge {
                vertices: e.vertices,
                curve: curve(e.curve, Some((vertices[e.vertices.0], vertices[e.vertices.1])))?,
            })
        })
        .collect::<Result<_, String>>()?;
    let faces = faces
        .into_iter()
        .map(|f| {
            // Where the face is: each wire's edges' starts and middles, in order.
            let loops: Vec<Vec<Point3>> = f
                .boundaries
                .iter()
                .map(|w| {
                    w.iter()
                        .filter_map(|i| Some((edges.get(i.index)?, i.orientation)))
                        .flat_map(|(e, forward)| {
                            let (t0, t1) = e.curve.range_tuple();
                            [if forward { t0 } else { t1 }, (t0 + t1) / 2.0].map(|t| e.curve.subs(t))
                        })
                        .collect()
                })
                .collect();
            Ok(CompressedFace {
                surface: surface(f.surface, size, &loops)?,
                boundaries: f.boundaries,
                orientation: f.orientation,
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

/// `ends` are an edge's start and end vertex, when the curve is an edge's.
fn curve(c: Curve3D, ends: Option<(Point3, Point3)>) -> Result<Curve, String> {
    let converted = match c {
        Curve3D::Line(l) => Curve::Line(l),
        Curve3D::BSplineCurve(b) => Curve::BSplineCurve(b),
        Curve3D::NurbsCurve(n) => Curve::NurbsCurve(n),
        Curve3D::Polyline(p) => Curve::BSplineCurve(polyline(p.0)?),
        Curve3D::Conic(Conic3D::Ellipse(e)) => {
            let (mut t0, mut t1) = e.entity().range_tuple();
            // truck's trimming comes out NaN for some files' arcs (NIST's STC-07 fillet ends): take
            // the angles from the edge's own ends instead.
            if let (false, Some((a, b)), Some(inv)) = (t0.is_finite() && t1.is_finite(), ends, e.transform().invert()) {
                let angle = |p: Point3| {
                    let q = inv.transform_point(p);
                    q.y.atan2(q.x)
                };
                let (from, to) = if e.orientation() { (a, b) } else { (b, a) };
                t0 = angle(from);
                let turn = (angle(to) - t0).rem_euclid(TAU);
                t1 = t0 + if turn < 1e-9 { TAU } else { turn };
            }
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
    };
    // A curve the mesher would chase off to infinity (it ran out of memory on one) is a bad file.
    let (t0, t1) = converted.range_tuple();
    let finite = |p: Point3| p.x.is_finite() && p.y.is_finite() && p.z.is_finite();
    if ![t0, (t0 + t1) / 2.0, t1].iter().all(|&t| finite(converted.subs(t))) {
        return Err("an edge's curve could not be read".into());
    }
    Ok(converted)
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

/// `loops` are points along the wires of the face the surface is for.
fn surface(s: step::Surface, size: f64, loops: &[Vec<Point3>]) -> Result<Surface, String> {
    let points = &loops.concat();
    let original = s.clone();
    let converted = match s {
        step::Surface::BSplineSurface(b) => return Ok(Surface::BSplineSurface(*b)),
        step::Surface::NurbsSurface(n) => return Ok(Surface::NurbsSurface(*n)),
        step::Surface::ElementarySurface(e) => match *e {
            ElementarySurface::Plane(p) => return Ok(Surface::Plane(p)),
            ElementarySurface::CylindricalSurface(c) | ElementarySurface::ConicalSurface(c) => {
                let local = local_points(c.transform(), points);
                let rc = c.entity();
                let line = spanning(*rc.entity_curve(), rc.origin(), rc.axis(), &local);
                Surface::RevolutedCurve(revolved(c.transform(), Curve::Line(line), rc.origin(), rc.axis()))
            }
            ElementarySurface::Sphere(sp) => {
                let (c, r) = (sp.entity().0.center(), sp.entity().0.radius());
                // Near a pole the turn angle is undefined and truck's parameter search wanders off,
                // so a patch (a corner blend) goes on the equator, opposite the seam, with the poles
                // off to its sides. A hemisphere's edge points only give its centre: its pole goes in
                // its middle instead, where truck closes the one loop round it by itself.
                let m = sp.transform();
                let local = local_points(m, points);
                let mean = local.iter().fold(Vector3::zero(), |m, p| m + (p - c)) / local.len().max(1) as f64;
                let winding = loops.iter().fold(Vector3::zero(), |n, w| {
                    let w = local_points(m, w);
                    (0..w.len()).fold(n, |n, i| n + (w[i] - c).cross(w[(i + 1) % w.len()] - c))
                });
                let across = |v: Vector3| {
                    v.cross(if v.z.abs() < 0.9 {
                        Vector3::unit_z()
                    } else {
                        Vector3::unit_x()
                    })
                };
                let (x, z) = if mean.magnitude() > 0.3 * r {
                    let toward = mean.normalize();
                    (-toward, across(toward).normalize())
                } else if winding.magnitude() > 0.1 * r * r {
                    let z = winding.normalize();
                    (across(z).normalize(), z)
                } else {
                    (Vector3::unit_x(), Vector3::unit_z())
                };
                let meridian = arc(c, x * r, z * r, -FRAC_PI_2, FRAC_PI_2);
                Surface::RevolutedCurve(revolved(sp.transform(), Curve::NurbsCurve(meridian), c, z))
            }
            ElementarySurface::ToroidalSurface(t) => {
                let e = t.entity();
                let (c, big, small) = (e.center(), e.large_radius(), e.small_radius());
                // Only the stretch of the tube the face uses: truck sees no period in the circle
                // round the tube, and a spindle torus (a screw head's dome, major radius smaller than
                // minor) folds through its axis elsewhere.
                let tube: Vec<f64> = local_points(t.transform(), points)
                    .iter()
                    .map(|p| {
                        let q = p - c;
                        let rho = q.x.hypot(q.y);
                        // A spindle torus's point near the axis may sit on the other side's circle.
                        let side = |r: f64| ((r - big).hypot(q.z) - small).abs();
                        let rho = if side(-rho) < side(rho) { -rho } else { rho };
                        q.z.atan2(rho - big)
                    })
                    .collect();
                let (mid, gap) = widest_gap(tube);
                let margin = (gap / 4.0).min(0.05);
                let meridian = arc(
                    c + Vector3::unit_x() * big,
                    Vector3::unit_x() * small,
                    Vector3::unit_z() * small,
                    mid + gap / 2.0 - margin,
                    mid + TAU - gap / 2.0 + margin,
                );
                Surface::RevolutedCurve(revolved(
                    t.transform(),
                    Curve::NurbsCurve(meridian),
                    c,
                    Vector3::unit_z(),
                ))
            }
        },
        step::Surface::SweptCurve(sc) => match *sc {
            SweptCurve::ExtrudedCurve(ec) => {
                let base = match curve(ec.entity_curve().clone(), None)? {
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
                let profile = curve(rc.entity().entity_curve().clone(), None)?;
                let (o, a) = (rc.entity().origin(), rc.entity().axis());
                Surface::RevolutedCurve(revolved(rc.transform(), profile, o, a))
            }
        },
    };
    Ok(facing(&original, converted, points.first().copied()))
}

/// `profile` (in a surface's own frame `m`) turned round `axis` through `o`, rebuilt in world
/// millimetres and never inverted: truck-geometry 0.5's processor searches in its entity's own
/// units, and an inverted one hands the search hint on unswapped, which sends the mesher's search
/// astray (holes meshed halfway, blends ballooning). `facing` turns the profile round instead.
fn revolved(m: &Matrix4, mut profile: Curve, o: Point3, axis: Vector3) -> Processor<RevolutedCurve<Curve>, Matrix4> {
    profile.transform_by(*m);
    let axis = m.transform_vector(axis).normalize();
    Processor::new(RevolutedCurve::by_revolution(profile, m.transform_point(o), axis))
}

/// World points in a surface's own frame.
fn local_points(m: &Matrix4, points: &[Point3]) -> Vec<Point3> {
    let inv = m.invert().unwrap_or(Matrix4::identity());
    points.iter().map(|p| inv.transform_point(*p)).collect()
}

/// The middle and width of the widest empty arc between the angles.
fn widest_gap(mut angles: Vec<f64>) -> (f64, f64) {
    angles.iter_mut().for_each(|a| *a = a.rem_euclid(TAU));
    angles.sort_by(f64::total_cmp);
    let (Some(&first), Some(&last)) = (angles.first(), angles.last()) else {
        return (PI, TAU);
    };
    let mut best = (first + TAU - last, last);
    for w in angles.windows(2) {
        if w[1] - w[0] > best.0 {
            best = (w[1] - w[0], w[0]);
        }
    }
    (best.1 + best.0 / 2.0, best.0)
}

/// A cylinder's or cone's line, which STEP gives as one unit long, stretched along the axis over
/// the face's `points` with room to spare: truck meshes a revolved line only between its ends (and
/// a far longer one sends its parameter search astray). A cone's stops at the apex, so the surface
/// never folds through its axis.
fn spanning(l: Line<Point3>, o: Point3, axis: Vector3, points: &[Point3]) -> Line<Point3> {
    let d = l.1 - l.0;
    let a = axis.normalize();
    let along = d.dot(a);
    if along.abs() < 1e-12 || points.is_empty() {
        return l;
    }
    let ts = points.iter().map(|p| (p - l.0).dot(a) / along);
    let (mut lo, mut hi) = ts.fold((f64::MAX, f64::MIN), |(lo, hi), t| (lo.min(t), hi.max(t)));
    let perp = |v: Vector3| v - a * v.dot(a);
    let (r0, rd) = (perp(l.0 - o), perp(d));
    // A cone always reaches its apex: a drill point's face has one circle and ends there.
    let apex = (rd.magnitude2() > 1e-12 * d.magnitude2()).then(|| -r0.dot(rd) / rd.magnitude2());
    if let Some(apex) = apex {
        (lo, hi) = (lo.min(apex), hi.max(apex));
    }
    let pad = (hi - lo) * 0.1 + 1e-6;
    let (mut t0, mut t1) = (lo - pad, hi + pad);
    if let Some(apex) = apex {
        if apex <= lo {
            t0 = apex;
        } else {
            t1 = apex;
        }
    }
    Line(l.0 + d * t0, l.0 + d * t1)
}

/// The converted surface turned to face the same way as the original at `probe` (a point on the
/// face, so on both surfaces), or at an arbitrary parameter of the original.
fn facing(original: &step::Surface, s: Surface, probe: Option<Point3>) -> Surface {
    let (u, v) = probe
        .and_then(|p| original.search_parameter(p, None, 100))
        .unwrap_or((0.37, 0.41));
    let p = original.subs(u, v);
    match s.search_parameter(p, None, 100) {
        Some((u1, v1)) if original.normal(u, v).dot(s.normal(u1, v1)) < 0.0 => match s {
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
        },
        _ => s,
    }
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
        let inch = "#1=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n\
                    #2=(CONVERSION_BASED_UNIT('INCH',#3)LENGTH_UNIT()NAMED_UNIT(#4));";
        assert_eq!(mm_per_unit(inch), 25.4);
        // SolidWorks' Foot.STEP: a product name is not a unit.
        let foot = "#5=PRODUCT('Foot','Foot','',(#6));\n#7=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));";
        assert_eq!(mm_per_unit(foot), 1.0);
        // NIST's AP242 parts: the B-Rep is in mm, PMI is dimensioned in inches.
        let mixed = "#1=(CONVERSION_BASED_UNIT('INCH',#9)LENGTH_UNIT()NAMED_UNIT(#8));\n\
                     #2=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n\
                     #3=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#2,#4))REPRESENTATION_CONTEXT('',''));\n\
                     #5=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#6),#3);";
        assert_eq!(mm_per_unit(mixed), 1.0);
    }

    #[test]
    fn cone_angles_in_degrees_become_radians() {
        let deg = "#1=(CONVERSION_BASED_UNIT('DEGREE',#2)NAMED_UNIT(#3)PLANE_ANGLE_UNIT());\n#4=CONICAL_SURFACE('',#5,10.,59.);";
        assert_eq!(
            cones_in_radians(deg),
            format!("#1=(CONVERSION_BASED_UNIT('DEGREE',#2)NAMED_UNIT(#3)PLANE_ANGLE_UNIT());\n#4=CONICAL_SURFACE('',#5,10.,{:?});", 59f64.to_radians())
        );
        let rad = "#1=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));\n#4=CONICAL_SURFACE('',#5,10.,0.5);";
        assert_eq!(cones_in_radians(rad), rad);
    }

    #[test]
    fn edges_use_their_3d_curves() {
        let text = "#1=EDGE_CURVE('',#2,#3,#4,.T.);\n#4=SURFACE_CURVE('',#5,(#6,#7),.CURVE_3D.);\n#8=EDGE_CURVE('',#2,#3,#9,.F.);";
        assert_eq!(
            edges_on_3d_curves(text),
            "#1=EDGE_CURVE('',#2,#3,#5,.T.);\n#4=SURFACE_CURVE('',#5,(#6,#7),.CURVE_3D.);\n#8=EDGE_CURVE('',#2,#3,#9,.F.);"
        );
    }

    #[test]
    fn empty_lists_are_filled_outside_strings_and_attribute_lists() {
        let text =
            "#1=ANNOTATION_PLANE('a ()',(#2),#3,());\n#4=(LENGTH_UNIT() NAMED_UNIT(*));\n#5=X('',( ),((1,2),()));";
        assert_eq!(
            without_empty_lists(text),
            "#1=ANNOTATION_PLANE('a ()',(#2),#3,$);\n#4=(LENGTH_UNIT() NAMED_UNIT(*));\n#5=X('',$,((1,2),$));"
        );
    }
}
