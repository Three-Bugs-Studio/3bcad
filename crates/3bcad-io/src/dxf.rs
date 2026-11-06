//! DXF sketches: ASCII import of 2D curves and a minimal R12 export.

use std::collections::HashMap;
use std::fmt::{Display, Write as _};
use std::io;
use std::path::Path;

use threebcad_sketch::geom::{self, P2};
use threebcad_sketch::{Geometry, PointId, Sketch};

/// Endpoints closer than this (mm) become one sketch point, so outlines close.
const TOL: f64 = 1e-6;

struct Ent<'a> {
    kind: &'a str,
    pairs: Vec<(i32, &'a str)>,
}

impl Ent<'_> {
    fn raw(&self, code: i32) -> Option<&str> {
        self.pairs.iter().find(|p| p.0 == code).map(|p| p.1)
    }

    fn num(&self, code: i32) -> io::Result<f64> {
        self.raw(code).map_or(Ok(0.0), num)
    }

    fn int(&self, code: i32) -> i64 {
        self.raw(code).and_then(|v| v.trim().parse().ok()).unwrap_or(0)
    }

    /// Repeated `xc`/`yc` coordinates with their bulge (42), in file order.
    fn vertices(&self, xc: i32, yc: i32) -> io::Result<Vec<(P2, f64)>> {
        let mut v: Vec<(P2, f64)> = Vec::new();
        for &(code, val) in &self.pairs {
            match (code, v.last_mut()) {
                (c, _) if c == xc => v.push(([num(val)?, 0.0], 0.0)),
                (c, Some(last)) if c == yc => last.0[1] = num(val)?,
                (42, Some(last)) => last.1 = num(val)?,
                _ => {}
            }
        }
        Ok(v)
    }
}

struct Builder {
    sk: Sketch,
    grid: HashMap<(i64, i64), Vec<PointId>>,
    scale: f64,
}

impl Builder {
    /// File coordinates to millimetres; `mx` is -1 for entities whose OCS normal points down (mirrored X).
    fn mm(&self, p: P2, mx: f64) -> io::Result<P2> {
        let q = [p[0] * mx * self.scale, p[1] * self.scale];
        if q.iter().all(|c| c.is_finite()) {
            Ok(q)
        } else {
            Err(bad("non-finite DXF coordinate"))
        }
    }

    /// An endpoint, shared with any earlier endpoint within `TOL`.
    fn end(&mut self, p: P2) -> PointId {
        let k = ((p[0] / TOL).floor() as i64, (p[1] / TOL).floor() as i64);
        for dx in -1..=1 {
            for dy in -1..=1 {
                let cell = (k.0.saturating_add(dx), k.1.saturating_add(dy));
                let mut hit = self.grid.get(&cell).into_iter().flatten();
                if let Some(&id) = hit.find(|&&id| geom::dist(self.sk.point(id), p) <= TOL) {
                    return id;
                }
            }
        }
        let id = self.sk.add_point(p[0], p[1]);
        self.grid.entry(k).or_default().push(id);
        id
    }

    fn free(&mut self, p: P2) -> io::Result<PointId> {
        if p.iter().all(|c| c.is_finite()) {
            Ok(self.sk.add_point(p[0], p[1]))
        } else {
            Err(bad("non-finite DXF coordinate"))
        }
    }

    /// Straight or, with a bulge (tan of a quarter of the included angle, + = CCW), circular.
    fn segment(&mut self, a: P2, b: P2, bulge: f64) -> io::Result<()> {
        let (pa, pb) = (self.end(a), self.end(b));
        if pa == pb {
            return Ok(());
        }
        if bulge.abs() < 1e-9 {
            self.sk.add_line(pa, pb);
            return Ok(());
        }
        let off = geom::scale(geom::perp(geom::sub(b, a)), (1.0 - bulge * bulge) / (4.0 * bulge));
        let c = self.free(geom::add(geom::lerp(a, b, 0.5), off))?;
        let (s, e) = if bulge > 0.0 { (pa, pb) } else { (pb, pa) };
        self.sk.add_arc(c, s, e);
        Ok(())
    }

    fn polyline(&mut self, v: &[(P2, f64)], closed: bool) -> io::Result<()> {
        let n = v.len();
        for i in 0..if closed { n } else { n.saturating_sub(1) } {
            self.segment(v[i].0, v[(i + 1) % n].0, v[i].1)?;
        }
        Ok(())
    }
}

/// ASCII DXF entities as an unconstrained sketch in millimetres. `mm_per_unit` overrides `$INSUNITS`.
pub fn read_dxf(bytes: &[u8], mm_per_unit: Option<f64>) -> io::Result<Sketch> {
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Err(bad("binary DXF is not supported, save it as ASCII DXF"));
    }
    let mut lines = bytes
        .split(|&b| b == b'\n')
        .map(|l| std::str::from_utf8(l).unwrap_or("").trim());
    let mut pairs = Vec::new();
    while let (Some(c), Some(v)) = (lines.next(), lines.next()) {
        pairs.push((c.parse::<i32>().map_err(|_| bad("bad DXF group code"))?, v));
    }

    let (mut section, mut units, mut ents) = ("", 0, Vec::new());
    for (i, &(code, v)) in pairs.iter().enumerate() {
        let next = pairs.get(i + 1).copied().unwrap_or((-1, ""));
        match (code, v) {
            (0, "EOF") => break,
            (0, "SECTION") => section = next.1,
            (0, "ENDSEC") => section = "",
            (0, kind) if section == "ENTITIES" => ents.push(Ent {
                kind,
                pairs: Vec::new(),
            }),
            (9, "$INSUNITS") if next.0 == 70 => units = next.1.parse().unwrap_or(0),
            _ if section == "ENTITIES" => ents.last_mut().into_iter().for_each(|e| e.pairs.push((code, v))),
            _ => {}
        }
    }
    let scale = mm_per_unit.unwrap_or(match units {
        1 => 25.4,
        2 => 304.8,
        5 => 10.0,
        6 => 1000.0,
        _ => 1.0,
    });
    if !(scale.is_finite() && scale > 0.0) {
        return Err(bad("bad DXF unit scale"));
    }

    let mut b = Builder {
        sk: Sketch::default(),
        grid: HashMap::new(),
        scale,
    };
    let (mut skipped, mut i) = (0, 0);
    while i < ents.len() {
        let e = &ents[i];
        i += 1;
        if e.int(67) == 1 {
            continue; // paper space
        }
        let mx = if e.raw(230).map_or(Ok(1.0), num)? < 0.0 {
            -1.0
        } else {
            1.0
        };
        match e.kind {
            "LINE" => {
                let p = b.mm([e.num(10)?, e.num(20)?], 1.0)?;
                let q = b.mm([e.num(11)?, e.num(21)?], 1.0)?;
                b.segment(p, q, 0.0)?;
            }
            "CIRCLE" | "ARC" => {
                let c = b.mm([e.num(10)?, e.num(20)?], mx)?;
                let r = e.num(40)? * scale;
                if !(r > 0.0 && r.is_finite()) {
                    skipped += 1;
                    continue;
                }
                let center = b.free(c)?;
                if e.kind == "CIRCLE" {
                    b.sk.add_circle(center, r);
                    continue;
                }
                let at = |deg: f64| {
                    let (s, co) = deg.to_radians().sin_cos();
                    [c[0] + r * mx * co, c[1] + r * s]
                };
                let (s, t) = (at(e.num(50)?), at(e.num(51)?));
                // A mirrored OCS turns the counter-clockwise arc clockwise.
                let (s, t) = if mx < 0.0 { (t, s) } else { (s, t) };
                let (ps, pt) = (b.end(s), b.end(t));
                if ps == pt {
                    b.sk.add_circle(center, r);
                } else {
                    b.sk.add_arc(center, ps, pt);
                }
            }
            "LWPOLYLINE" | "POLYLINE" => {
                let mut v = Vec::new();
                if e.kind == "LWPOLYLINE" {
                    v = e.vertices(10, 20)?;
                } else {
                    while let Some(ve) = ents.get(i).filter(|ve| ve.kind == "VERTEX") {
                        i += 1;
                        if ve.int(70) & 16 == 0 {
                            v.push(([ve.num(10)?, ve.num(20)?], ve.num(42)?));
                        }
                    }
                    if ents.get(i).is_some_and(|s| s.kind == "SEQEND") {
                        i += 1;
                    }
                    if e.int(70) & (16 | 64) != 0 {
                        skipped += 1; // polygon / polyface meshes
                        continue;
                    }
                }
                let v = v
                    .into_iter()
                    .map(|(p, bulge)| Ok((b.mm(p, mx)?, bulge * mx)))
                    .collect::<io::Result<Vec<_>>>()?;
                b.polyline(&v, e.int(70) & 1 != 0)?;
            }
            "SPLINE" => {
                // shortcut: the curve is interpolated through the fit points, or through the control
                // points when there are none (close to, not on, the true B-spline); upgrade when
                // sketches get NURBS curves.
                let fit = e.vertices(11, 21)?;
                let src = if fit.is_empty() { e.vertices(10, 20)? } else { fit };
                let mut pts = src
                    .into_iter()
                    .map(|(p, _)| b.mm(p, 1.0))
                    .collect::<io::Result<Vec<_>>>()?;
                pts.dedup_by(|p, q| geom::dist(*p, *q) < TOL);
                if e.int(70) & 1 != 0 && pts.len() > 1 && geom::dist(pts[0], pts[pts.len() - 1]) >= TOL {
                    pts.push(pts[0]);
                }
                if pts.len() < 2 {
                    skipped += 1;
                    continue;
                }
                let n = pts.len();
                let mut ids = Vec::with_capacity(n);
                for (k, p) in pts.into_iter().enumerate() {
                    ids.push(if k == 0 || k == n - 1 { b.end(p) } else { b.free(p)? });
                }
                b.sk.add_spline(ids);
            }
            // shortcut: blocks (INSERT), text, dimensions, hatches and ellipses are skipped,
            // upgrade when users bring drawings that rely on them.
            _ => skipped += 1,
        }
    }
    if b.sk.entities().is_empty() {
        return Err(bad(&format!(
            "no lines, arcs, circles, polylines or splines in the DXF ({skipped} other entities skipped)"
        )));
    }
    Ok(b.sk)
}

/// The sketch's non-construction curves as an R12 DXF in millimetres (splines as polylines).
pub fn write_dxf(path: &Path, sketch: &Sketch) -> io::Result<()> {
    let mut s = header();
    for e in sketch.entity_ids().filter(|&e| !sketch.entity(e).construction) {
        match sketch.entity(e).geom {
            Geometry::Line { a, b } => {
                put(&mut s, 0, "LINE");
                put(&mut s, 8, 0);
                xy(&mut s, 10, sketch.point(a));
                xy(&mut s, 11, sketch.point(b));
            }
            Geometry::Circle { center, .. } => {
                put(&mut s, 0, "CIRCLE");
                put(&mut s, 8, 0);
                xy(&mut s, 10, sketch.point(center));
                put(&mut s, 40, sketch.radius(e));
            }
            Geometry::Arc { center, start, end } => {
                let c = sketch.point(center);
                let deg = |p| geom::angle_of(geom::sub(sketch.point(p), c)).to_degrees();
                put(&mut s, 0, "ARC");
                put(&mut s, 8, 0);
                xy(&mut s, 10, c);
                put(&mut s, 40, sketch.radius(e));
                put(&mut s, 50, deg(start));
                put(&mut s, 51, deg(end));
            }
            Geometry::Spline { .. } => polyline(&mut s, &sketch.polyline(e), false),
        }
    }
    finish(path, s)
}

fn header() -> String {
    let mut s = String::new();
    for (c, v) in [
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$ACADVER"),
        (1, "AC1009"),
        (9, "$INSUNITS"),
        (70, "4"),
    ] {
        put(&mut s, c, v);
    }
    for (c, v) in [(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")] {
        put(&mut s, c, v);
    }
    s
}

fn finish(path: &Path, mut s: String) -> io::Result<()> {
    put(&mut s, 0, "ENDSEC");
    put(&mut s, 0, "EOF");
    std::fs::write(path, s)
}

fn polyline(s: &mut String, pts: &[P2], closed: bool) {
    for (c, v) in [
        (0, "POLYLINE"),
        (8, "0"),
        (66, "1"),
        (70, if closed { "1" } else { "0" }),
    ] {
        put(s, c, v);
    }
    for &p in pts {
        put(s, 0, "VERTEX");
        put(s, 8, 0);
        xy(s, 10, p);
    }
    put(s, 0, "SEQEND");
}

pub(crate) fn put(s: &mut String, code: i32, value: impl Display) {
    let _ = write!(s, "{code}\n{value}\n");
}

pub(crate) fn xy(s: &mut String, code: i32, p: P2) {
    put(s, code, p[0]);
    put(s, code + 10, p[1]);
}

fn num(v: &str) -> io::Result<f64> {
    v.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| bad("bad DXF number"))
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use threebcad_sketch::profile::loop_polygon;
    use threebcad_sketch::Segment;

    use super::*;

    /// One token per line, CRLF like most Windows exporters.
    fn dxf(tokens: &str) -> Vec<u8> {
        tokens.split_whitespace().collect::<Vec<_>>().join("\r\n").into_bytes()
    }

    fn kinds(sk: &Sketch) -> Vec<&'static str> {
        let mut k: Vec<_> = sk.entities().iter().map(|e| e.kind_name()).collect();
        k.sort();
        k
    }

    const PLATE: &str = "0 SECTION 2 ENTITIES \
        0 LWPOLYLINE 8 0 90 4 70 1 10 0 20 0 10 40 20 0 10 40 20 20 42 1 10 0 20 20 \
        0 CIRCLE 8 0 10 20 20 10 40 5 \
        0 TEXT 8 0 10 0 20 0 1 hello \
        0 ENDSEC 0 EOF";

    #[test]
    fn closed_polyline_with_bulge_and_hole_is_one_region() {
        let sk = read_dxf(&dxf(PLATE), None).unwrap();
        assert_eq!(kinds(&sk), ["Arc", "Circle", "Line", "Line", "Line"]);
        assert_eq!(sk.point_count(), 6, "4 shared corners + arc center + circle center");
        let regions = sk.regions().unwrap();
        assert_eq!((regions.len(), regions[0].holes.len()), (1, 1));
        let area = |lp: &[Segment]| geom::polygon_area(&loop_polygon(lp, 1024)).abs();
        assert!((area(&regions[0].outer) - (800.0 + 200.0 * PI)).abs() < 0.1);
        assert!((area(&regions[0].holes[0]) - 25.0 * PI).abs() < 0.1);
    }

    #[test]
    fn units_and_mirrored_ocs() {
        let inch = "0 SECTION 2 HEADER 9 $INSUNITS 70 1 0 ENDSEC 0 SECTION 2 ENTITIES \
                    0 LINE 8 0 10 0 20 0 11 1 21 2 0 ENDSEC 0 EOF";
        let sk = read_dxf(&dxf(inch), None).unwrap();
        assert_eq!(sk.point(PointId(1)), [25.4, 50.8]);
        assert_eq!(read_dxf(&dxf(inch), Some(1.0)).unwrap().point(PointId(1)), [1.0, 2.0]);

        let arc = "0 SECTION 2 ENTITIES 0 ARC 10 10 20 0 40 5 50 0 51 90 230 -1 0 ENDSEC 0 EOF";
        let sk = read_dxf(&dxf(arc), None).unwrap();
        let Geometry::Arc { center, start, end } = sk.entity(threebcad_sketch::EntityId(0)).geom else {
            panic!("an arc")
        };
        let near = |a: P2, b: P2| geom::dist(a, b) < 1e-9;
        assert!(near(sk.point(center), [-10.0, 0.0]) && near(sk.point(start), [-10.0, 5.0]));
        assert!(near(sk.point(end), [-15.0, 0.0]));

        assert!(read_dxf(&dxf("0 SECTION 2 ENTITIES 0 TEXT 1 hi 0 ENDSEC 0 EOF"), None).is_err());
    }

    const ALL: &str = "0 SECTION 2 HEADER 9 $INSUNITS 70 4 0 ENDSEC 0 SECTION 2 ENTITIES \
        0 LINE 8 0 10 0 20 0 11 10 21 0 \
        0 LINE 8 0 67 1 10 0 20 0 11 99 21 99 \
        0 ARC 8 0 10 10 20 5 40 5 50 270 51 90 \
        0 POLYLINE 8 0 66 1 70 0 10 0 20 0 0 VERTEX 10 10 20 10 42 -0.5 0 VERTEX 10 0 20 10 \
        0 VERTEX 10 0 20 0 0 SEQEND \
        0 SPLINE 8 0 70 8 71 3 72 8 73 4 10 0 20 20 10 5 20 25 10 10 20 20 10 15 20 25 10 20 20 20 \
        0 SPLINE 8 0 70 1 74 3 10 0 20 0 11 30 21 0 11 35 21 5 11 30 21 10 \
        0 INSERT 2 BLOCK 10 0 20 0 \
        0 ENDSEC 0 EOF";

    #[test]
    fn every_entity_kind() {
        let sk = read_dxf(&dxf(ALL), None).unwrap();
        assert_eq!(kinds(&sk), ["Arc", "Arc", "Line", "Line", "Spline", "Spline"]);
        // Line, arc and polyline share their ends: 4 corners + 2 arc centers, then 5 + 3 spline points.
        assert_eq!(sk.point_count(), 14);
        let closed = sk
            .entities()
            .iter()
            .find(|e| e.kind_name() == "Spline" && e.points().len() == 4);
        let pts = closed.unwrap().points();
        assert_eq!(pts[0], pts[3], "closed fit-point spline ends where it starts");
        assert_eq!(sk.point(pts[1]), [35.0, 5.0]);
    }

    #[test]
    fn write_read_round_trip() {
        let mut sk = Sketch::rectangle(30.0, 10.0);
        let c = sk.add_point(15.0, 5.0);
        sk.add_circle(c, 3.0);
        let (c, s, e) = (
            sk.add_point(50.0, 0.0),
            sk.add_point(60.0, 0.0),
            sk.add_point(50.0, 10.0),
        );
        sk.add_arc(c, s, e);
        let (a, b) = (sk.add_point(0.0, -5.0), sk.add_point(30.0, -5.0));
        let axis = sk.add_line(a, b);
        sk.set_construction(axis, true);

        let dir = std::env::temp_dir();
        let path = dir.join(format!("3bcad-dxf-{}.dxf", std::process::id()));
        write_dxf(&path, &sk).unwrap();
        let back = read_dxf(&std::fs::read(&path).unwrap(), None).unwrap();
        assert_eq!(kinds(&back), ["Arc", "Circle", "Line", "Line", "Line", "Line"]);
        let radius = |sk: &Sketch, kind| {
            let e = sk.entity_ids().find(|&e| sk.entity(e).kind_name() == kind).unwrap();
            sk.radius(e)
        };
        assert!((radius(&back, "Circle") - 3.0).abs() < 1e-12 && (radius(&back, "Arc") - 10.0).abs() < 1e-12);
        let arc = back.entities().iter().find(|e| e.kind_name() == "Arc").unwrap();
        let Geometry::Arc { start, end, .. } = arc.geom else {
            unreachable!()
        };
        assert!(geom::dist(back.point(start), [60.0, 0.0]) < 1e-9);
        assert!(geom::dist(back.point(end), [50.0, 10.0]) < 1e-9);

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn parser_survives_truncation_and_mutation() {
        crate::mesh::shake(&dxf(ALL), |b| read_dxf(b, None));
        crate::mesh::shake(&dxf(PLATE), |b| read_dxf(b, None));
    }
}
