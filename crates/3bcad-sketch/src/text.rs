//! Sketch text: lettering from the system's TrueType / OpenType fonts as closed profile loops,
//! straight or along a sketch curve, for embossed and debossed labels and logos.
//!
//! The glyph outlines are kept in the sketch, so a document keeps its lettering on a computer
//! without the font; only changing the text or the font needs the font itself.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use ttf_parser::{name_id, Face, OutlineBuilder};

use crate::geom::{self, P2};
use crate::profile::Segment;
use crate::{EntityId, PointId, Sketch};

/// The font that ships with 3BCAD (Ubuntu Light, from egui's default fonts): always there, and
/// what tests use.
pub const BUILTIN_FONT: &str = "Ubuntu Light (built in)";

/// A line of text in a sketch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Text {
    pub text: String,
    /// Full font name ("Arial Bold"), or [`BUILTIN_FONT`].
    pub font: String,
    /// Height of the capital letters.
    pub height: f64,
    /// Left end of the baseline. Along a curve the letters start where this point sits on it.
    pub at: PointId,
    /// Baseline direction (radians, counter-clockwise from the sketch's x axis); not used along a curve.
    pub angle: f64,
    /// The letters stand on this curve.
    pub curve: Option<EntityId>,
    /// Walk the curve the other way, which puts the letters on its other side.
    pub flip: bool,
    /// Outline of each letter at a capital height of 1.
    pub glyphs: Vec<Glyph>,
}

/// One letter: closed loops in its own frame (baseline along x, origin at its left end).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Glyph {
    /// Distance to the next letter's origin.
    pub advance: f64,
    pub loops: Vec<Vec<Segment>>,
}

/// An installed font face.
#[derive(Clone, Debug, PartialEq)]
pub struct FontFace {
    pub name: String,
    pub path: PathBuf,
    /// Face number inside a font collection (`.ttc`).
    pub index: u32,
}

impl Text {
    /// Text in `font` with its outlines; an unknown font falls back to the built-in one.
    pub fn new(text: &str, font: &str, height: f64, at: PointId) -> Self {
        let mut t = Self {
            text: String::new(),
            font: font.into(),
            height,
            at,
            angle: 0.0,
            curve: None,
            flip: false,
            glyphs: Vec::new(),
        };
        t.set(text, font);
        t
    }

    /// Changes the text or the font and redoes the outlines. Returns false (and uses the built-in
    /// font) when `font` is not installed.
    pub fn set(&mut self, text: &str, font: &str) -> bool {
        let (data, index, found) = match load_font(font) {
            Some((d, i)) => (d, i, true),
            None => (builtin_data().to_vec(), 0, false),
        };
        self.text = text.into();
        self.font = if found { font.into() } else { BUILTIN_FONT.into() };
        self.glyphs = Face::parse(&data, index).map(|f| shape(&f, text)).unwrap_or_default();
        found
    }

    /// Length of the baseline at this height.
    pub fn width(&self) -> f64 {
        self.glyphs.iter().map(|g| g.advance).sum::<f64>() * self.height
    }
}

fn builtin_data() -> &'static [u8] {
    epaint_default_fonts::UBUNTU_LIGHT
}

/// Outlines of `text`, scaled so capitals are 1 high. Characters the font lacks become spaces.
fn shape(face: &Face, text: &str) -> Vec<Glyph> {
    let cap = cap_height(face);
    let scale = 1.0 / cap;
    let space = face
        .glyph_index(' ')
        .and_then(|g| face.glyph_hor_advance(g))
        .map_or(0.3 * face.units_per_em() as f64, f64::from);
    text.chars()
        .filter(|c| !c.is_control())
        .map(|c| match face.glyph_index(c) {
            Some(g) => {
                let mut b = Builder {
                    scale,
                    ..Default::default()
                };
                face.outline_glyph(g, &mut b);
                b.finish();
                Glyph {
                    advance: face.glyph_hor_advance(g).map_or(space, f64::from) * scale,
                    loops: b.loops,
                }
            }
            None => Glyph {
                advance: space * scale,
                loops: Vec::new(),
            },
        })
        .collect()
}

/// Capital height in font units: from the OS/2 table, else the height of "H", else 70 % of the em.
fn cap_height(face: &Face) -> f64 {
    face.capital_height()
        .filter(|&h| h > 0)
        .map(f64::from)
        .or_else(|| {
            let g = face.glyph_index('H')?;
            face.glyph_bounding_box(g)
                .map(|b| f64::from(b.y_max))
                .filter(|&h| h > 0.0)
        })
        .unwrap_or(0.7 * face.units_per_em() as f64)
}

#[derive(Default)]
struct Builder {
    scale: f64,
    loops: Vec<Vec<Segment>>,
    cur: Vec<Segment>,
    start: P2,
    last: P2,
}

impl Builder {
    fn p(&self, x: f32, y: f32) -> P2 {
        [x as f64 * self.scale, y as f64 * self.scale]
    }

    fn push(&mut self, s: Segment) {
        if geom::dist(s.start(), s.end()) > 1e-9 || matches!(s, Segment::Bezier(_)) {
            self.last = s.end();
            self.cur.push(s);
        }
    }

    fn finish(&mut self) {
        if !self.cur.is_empty() {
            if geom::dist(self.last, self.start) > 1e-9 {
                let (from, to) = (self.last, self.start);
                self.cur.push(Segment::Line { from, to });
            }
            // A contour too small to be a face (a stray point) is dropped.
            if self.cur.len() >= 2 {
                self.loops.push(std::mem::take(&mut self.cur));
            }
            self.cur.clear();
        }
    }
}

impl OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        self.start = self.p(x, y);
        self.last = self.start;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let to = self.p(x, y);
        self.push(Segment::Line { from: self.last, to });
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        // The same curve as a cubic: control points two thirds of the way to the quadratic's.
        let (p0, q, p3) = (self.last, self.p(x1, y1), self.p(x, y));
        let c1 = geom::lerp(p0, q, 2.0 / 3.0);
        let c2 = geom::lerp(p3, q, 2.0 / 3.0);
        self.push(Segment::Bezier([p0, c1, c2, p3]));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let b = [self.last, self.p(x1, y1), self.p(x2, y2), self.p(x, y)];
        self.push(Segment::Bezier(b));
    }

    fn close(&mut self) {
        self.finish();
    }
}

/// Moves a segment by a rotation through `angle` about the origin, then by `to`.
fn place(s: &Segment, angle: f64, to: P2) -> Segment {
    let m = |p: P2| geom::add(geom::rotate(p, angle), to);
    match *s {
        Segment::Line { from, to: end } => Segment::Line {
            from: m(from),
            to: m(end),
        },
        Segment::Arc {
            center,
            from,
            to: end,
            ccw,
        } => Segment::Arc {
            center: m(center),
            from: m(from),
            to: m(end),
            ccw,
        },
        Segment::Circle { center, radius, ccw } => Segment::Circle {
            center: m(center),
            radius,
            ccw,
        },
        Segment::Bezier(b) => Segment::Bezier(b.map(m)),
    }
}

/// Position and direction `s` along a polyline (past its ends: straight on, or round a closed one).
fn along(poly: &[P2], cum: &[f64], s: f64) -> (P2, f64) {
    let total = *cum.last().unwrap_or(&0.0);
    let closed = poly.len() > 2 && geom::dist(poly[0], *poly.last().unwrap()) < 1e-9 * (1.0 + total);
    let s = if closed && total > 0.0 { s.rem_euclid(total) } else { s };
    let i = cum.partition_point(|&c| c <= s).saturating_sub(1).min(poly.len() - 2);
    let (a, b) = (poly[i], poly[i + 1]);
    let seg = (cum[i + 1] - cum[i]).max(1e-12);
    let d = geom::scale(geom::sub(b, a), 1.0 / seg);
    (geom::add(a, geom::scale(d, s - cum[i])), geom::angle_of(d))
}

impl Sketch {
    pub fn texts(&self) -> &[Text] {
        &self.texts
    }

    pub fn text_mut(&mut self, i: usize) -> &mut Text {
        &mut self.texts[i]
    }

    /// Adds text with the left end of its baseline at `at` (a new point).
    pub fn add_text(&mut self, text: &str, font: &str, height: f64, at: P2) -> usize {
        let p = self.add_point(at[0], at[1]);
        self.texts.push(Text::new(text, font, height, p));
        self.texts.len() - 1
    }

    /// The text whose baseline starts at `p`.
    pub fn text_at(&self, p: PointId) -> Option<usize> {
        self.texts.iter().position(|t| t.at == p)
    }

    /// Lays text `i` along `curve` (or straight again with `None`), keeping its start point on it.
    /// Arcs and circles run counter-clockwise, so their text starts flipped: upright along the top,
    /// outside the curve.
    pub fn set_text_curve(&mut self, i: usize, curve: Option<EntityId>) {
        let at = self.texts[i].at;
        if let Some(old) = self.texts[i].curve {
            if let Some(k) = self
                .constraints()
                .iter()
                .position(|c| *c == crate::Constraint::PointOnCurve(at, old))
            {
                self.remove_constraint(k);
            }
        }
        self.texts[i].curve = curve;
        if let Some(e) = curve {
            self.texts[i].flip = self.entity(e).is_round();
            self.constrain(crate::Constraint::PointOnCurve(at, e));
        }
    }

    /// Outline loops of text `i`, placed in the sketch, as straight lines within 0.2 % of the
    /// height (at most 0.02 mm) of the letters' curves: truck's booleans fail on extruded Béziers.
    pub fn text_loops(&self, i: usize) -> Vec<Vec<Segment>> {
        let t = &self.texts[i];
        let h = t.height;
        let origin = self.point(t.at);
        let tol = (0.002 * h).clamp(1e-4, 0.02);
        let scaled = |lp: &Vec<Segment>, dx: f64| -> Vec<Segment> {
            let mut pts = Vec::new();
            for s in lp {
                match place(&scale_segment(s, h), 0.0, [dx, 0.0]) {
                    Segment::Bezier(b) => {
                        pts.push(b[0]);
                        flatten(&b, tol, 0, &mut pts);
                        pts.pop();
                    }
                    other => pts.push(other.start()),
                }
            }
            // Fonts have points a hair apart; walls that short break the mesh of an export.
            let mut kept: Vec<P2> = Vec::new();
            for p in pts {
                if kept.last().is_none_or(|&q| geom::dist(p, q) > 3.0 * tol) {
                    kept.push(p);
                }
            }
            while kept.len() > 2 && geom::dist(kept[0], *kept.last().unwrap()) <= 3.0 * tol {
                kept.pop();
            }
            (0..kept.len())
                .map(|k| Segment::Line {
                    from: kept[k],
                    to: kept[(k + 1) % kept.len()],
                })
                .collect()
        };
        let mut out = Vec::new();
        let curve = t
            .curve
            .filter(|e| e.0 < self.entities.len())
            .map(|e| self.polyline(e))
            .filter(|p| p.len() >= 2);
        let Some(mut poly) = curve else {
            let mut x = 0.0;
            for g in &t.glyphs {
                for lp in &g.loops {
                    out.push(scaled(lp, x).iter().map(|s| place(s, t.angle, origin)).collect());
                }
                x += g.advance * h;
            }
            return out;
        };
        if t.flip {
            poly.reverse();
        }
        let mut cum = vec![0.0];
        for w in poly.windows(2) {
            cum.push(cum.last().unwrap() + geom::dist(w[0], w[1]));
        }
        // The letters start where the start point sits on the curve.
        let mut s0 = 0.0;
        let mut best = f64::INFINITY;
        for (k, w) in poly.windows(2).enumerate() {
            let (q, f) = geom::closest_on_segment(w[0], w[1], origin);
            let d = geom::dist(q, origin);
            if d < best {
                best = d;
                s0 = cum[k] + f * (cum[k + 1] - cum[k]);
            }
        }
        let mut x = 0.0;
        for g in &t.glyphs {
            // Each letter stands upright on the curve at its middle.
            let mid = g.advance * h / 2.0;
            let (p, dir) = along(&poly, &cum, s0 + x + mid);
            for lp in &g.loops {
                out.push(scaled(lp, -mid).iter().map(|s| place(s, dir, p)).collect());
            }
            x += g.advance * h;
        }
        out
    }
}

/// The points after the start of a cubic Bézier along a polyline within `tol` of it.
fn flatten(b: &[P2; 4], tol: f64, depth: u32, out: &mut Vec<P2>) {
    let off = |p: P2| {
        let (q, _) = geom::closest_on_segment(b[0], b[3], p);
        geom::dist(p, q)
    };
    // The curve lies within 3/4 of its control points' distance from the chord.
    if depth >= 10 || 0.75 * off(b[1]).max(off(b[2])) <= tol {
        out.push(b[3]);
        return;
    }
    let m = |a: P2, c: P2| geom::lerp(a, c, 0.5);
    let (ab, bc, cd) = (m(b[0], b[1]), m(b[1], b[2]), m(b[2], b[3]));
    let (abc, bcd) = (m(ab, bc), m(bc, cd));
    let mid = m(abc, bcd);
    flatten(&[b[0], ab, abc, mid], tol, depth + 1, out);
    flatten(&[mid, bcd, cd, b[3]], tol, depth + 1, out);
}

fn scale_segment(s: &Segment, k: f64) -> Segment {
    let m = |p: P2| geom::scale(p, k);
    match *s {
        Segment::Line { from, to } => Segment::Line {
            from: m(from),
            to: m(to),
        },
        Segment::Arc { center, from, to, ccw } => Segment::Arc {
            center: m(center),
            from: m(from),
            to: m(to),
            ccw,
        },
        Segment::Circle { center, radius, ccw } => Segment::Circle {
            center: m(center),
            radius: radius * k,
            ccw,
        },
        Segment::Bezier(b) => Segment::Bezier(b.map(m)),
    }
}

// ---------- system fonts ----------

/// Folders the system keeps fonts in.
fn font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if let Some(w) = env("WINDIR") {
        dirs.push(w.join("Fonts"));
    }
    if let Some(l) = env("LOCALAPPDATA") {
        dirs.push(l.join("Microsoft").join("Windows").join("Fonts"));
    }
    dirs.extend(
        [
            "/System/Library/Fonts",
            "/Library/Fonts",
            "/usr/share/fonts",
            "/usr/local/share/fonts",
        ]
        .map(PathBuf::from),
    );
    if let Some(h) = env("HOME") {
        dirs.extend([h.join("Library/Fonts"), h.join(".local/share/fonts"), h.join(".fonts")]);
    }
    dirs
}

fn collect_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() && depth > 0 {
            collect_files(&p, depth - 1, out);
        } else if p
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| ["ttf", "otf", "ttc", "otc"].contains(&x.to_ascii_lowercase().as_str()))
        {
            out.push(p);
        }
    }
}

fn read_at(f: &mut File, offset: u64, len: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0; len];
    f.seek(SeekFrom::Start(offset)).ok()?;
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn be32(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// Full names of the faces in a font file, read from the name tables alone (fonts can be large).
fn face_names(path: &Path) -> Vec<(String, u32)> {
    let Ok(mut f) = File::open(path) else {
        return Vec::new();
    };
    let Some(head) = read_at(&mut f, 0, 12) else {
        return Vec::new();
    };
    let offsets: Vec<u64> = if &head[..4] == b"ttcf" {
        let n = be32(&head, 8).unwrap_or(0).min(64) as usize;
        read_at(&mut f, 12, 4 * n)
            .map(|b| (0..n).filter_map(|i| be32(&b, 4 * i)).map(u64::from).collect())
            .unwrap_or_default()
    } else {
        vec![0]
    };
    let mut out = Vec::new();
    for (index, off) in offsets.into_iter().enumerate() {
        let Some(dir) = read_at(&mut f, off, 12) else { continue };
        let tables = u16::from_be_bytes([dir[4], dir[5]]) as usize;
        let Some(records) = read_at(&mut f, off + 12, 16 * tables.min(256)) else {
            continue;
        };
        let name = records.chunks_exact(16).find(|r| &r[..4] == b"name").and_then(|r| {
            let (at, len) = (be32(r, 8)?, be32(r, 12)?);
            let data = read_at(&mut f, at.into(), (len as usize).min(1 << 20))?;
            best_name(&ttf_parser::name::Table::parse(&data)?)
        });
        if let Some(n) = name {
            out.push((n, index as u32));
        }
    }
    out
}

/// The face's full name, in US English when the font has it.
fn best_name(t: &ttf_parser::name::Table) -> Option<String> {
    let mut found: Option<String> = None;
    for n in t.names.into_iter().filter(|n| n.name_id == name_id::FULL_NAME) {
        if let Some(s) = n.to_string().filter(|s| !s.trim().is_empty()) {
            if n.language_id == 0x0409 {
                return Some(s);
            }
            found.get_or_insert(s);
        }
    }
    found
}

/// Installed font faces by name (scanned once), with the built-in font first.
pub fn system_fonts() -> &'static [FontFace] {
    static FONTS: OnceLock<Vec<FontFace>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut files = Vec::new();
        for d in font_dirs() {
            collect_files(&d, 4, &mut files);
        }
        let mut faces: Vec<FontFace> = files
            .iter()
            .flat_map(|p| {
                face_names(p).into_iter().map(|(name, index)| FontFace {
                    name,
                    path: p.clone(),
                    index,
                })
            })
            .collect();
        faces.sort_by_key(|a| a.name.to_lowercase());
        faces.dedup_by(|a, b| a.name == b.name);
        faces.insert(
            0,
            FontFace {
                name: BUILTIN_FONT.into(),
                path: PathBuf::new(),
                index: 0,
            },
        );
        faces
    })
}

/// A font's data and face number; `None` when it is not installed.
pub fn load_font(name: &str) -> Option<(Vec<u8>, u32)> {
    if name == BUILTIN_FONT {
        return Some((builtin_data().to_vec(), 0));
    }
    let f = system_fonts().iter().find(|f| f.name == name)?;
    let data = std::fs::read(&f.path).ok()?;
    Face::parse(&data, f.index).ok()?;
    Some((data, f.index))
}

/// The font new text starts in: a common sans serif when the system has one.
pub fn default_font() -> &'static str {
    ["Arial", "Helvetica", "DejaVu Sans", "Liberation Sans", "Noto Sans"]
        .into_iter()
        .find_map(|n| system_fonts().iter().find(|f| f.name == n))
        .map_or(BUILTIN_FONT, |f| f.name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::loop_polygon;

    fn bounds(loops: &[Vec<Segment>]) -> (P2, P2) {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for lp in loops {
            for p in loop_polygon(lp, 64) {
                for k in 0..2 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        (lo, hi)
    }

    fn area(s: &Sketch) -> f64 {
        s.regions()
            .unwrap()
            .iter()
            .map(|r| {
                geom::polygon_area(&loop_polygon(&r.outer, 256))
                    + r.holes
                        .iter()
                        .map(|h| geom::polygon_area(&loop_polygon(h, 256)))
                        .sum::<f64>()
            })
            .sum()
    }

    #[test]
    fn capital_letters_have_the_set_height_and_sit_on_the_baseline() {
        let mut s = Sketch::default();
        let i = s.add_text("HE", BUILTIN_FONT, 10.0, [5.0, 2.0]);
        let (lo, hi) = bounds(&s.text_loops(i));
        assert!((lo[1] - 2.0).abs() < 1e-6, "baseline {lo:?}");
        assert!((hi[1] - 12.0).abs() < 1e-6, "cap height {hi:?}");
        assert!(lo[0] > 5.0 && lo[0] < 7.0, "left side bearing {lo:?}");
        assert!(hi[0] < 5.0 + s.texts()[i].width());
    }

    #[test]
    fn letters_become_regions_with_holes() {
        let mut s = Sketch::default();
        s.add_text("O", BUILTIN_FONT, 10.0, [0.0, 0.0]);
        let r = s.regions().unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].holes.len(), 1, "O has a counter");
        // "B" has two counters, "i" two separate pieces; areas scale with the square of the height.
        let mut b = Sketch::default();
        b.add_text("Bi", BUILTIN_FONT, 10.0, [0.0, 0.0]);
        let r = b.regions().unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r.iter().map(|r| r.holes.len()).sum::<usize>(), 2);
        let a10 = area(&b);
        b.text_mut(0).height = 20.0;
        assert!((area(&b) / a10 - 4.0).abs() < 1e-2, "{}", area(&b) / a10);
    }

    #[test]
    fn text_follows_a_curve() {
        let mut s = Sketch::default();
        let c = s.add_point(0.0, 0.0);
        let circle = s.add_circle(c, 30.0);
        s.set_construction(circle, true);
        let i = s.add_text("IIII", BUILTIN_FONT, 5.0, [30.0, 0.0]);
        s.set_text_curve(i, Some(circle));
        assert!(s.solve().converged);
        let radii = |s: &Sketch| -> Vec<(f64, f64)> {
            s.text_loops(i)
                .iter()
                .map(|lp| {
                    let r = loop_polygon(lp, 64).iter().map(|p| geom::len(*p)).collect::<Vec<_>>();
                    r.iter()
                        .fold((f64::INFINITY, 0.0f64), |(a, b), &v| (a.min(v), b.max(v)))
                })
                .collect()
        };
        // On a circle the letters stand outside it, reading clockwise: the lowest point of each is
        // 30 from the center, its top 35.
        assert!(s.texts()[i].flip);
        let r = radii(&s);
        assert_eq!(r.len(), 4);
        for (rmin, rmax) in r {
            assert!((rmin - 30.0).abs() < 0.05 && (rmax - 35.0).abs() < 0.1, "{rmin} {rmax}");
        }
        assert_eq!(s.regions().unwrap().len(), 4);
        // Not flipped, they follow the circle's own (counter-clockwise) direction and hang inside it.
        s.text_mut(i).flip = false;
        for (rmin, rmax) in radii(&s) {
            assert!((rmin - 25.0).abs() < 0.1 && (rmax - 30.0).abs() < 0.05, "{rmin} {rmax}");
        }
    }

    #[test]
    fn text_survives_saving_and_deleting_other_geometry() {
        let mut s = Sketch::rectangle(80.0, 20.0);
        let i = s.add_text("3BCAD", BUILTIN_FONT, 8.0, [5.0, 5.0]);
        // Inside the plate the letters are holes in it (a stencil); B, A and D's counters are islands.
        let r = s.regions().unwrap();
        assert_eq!(r.len(), 1 + 4);
        assert_eq!(r.iter().map(|r| r.holes.len()).max(), Some(5));
        let json = serde_json::to_string(&s).unwrap();
        let back: Sketch = serde_json::from_str(&json).unwrap();
        assert!(back.is_consistent());
        assert_eq!(back.texts()[0].text, "3BCAD");
        assert_eq!(
            back.regions().unwrap().len(),
            5,
            "the outlines come back without the font"
        );
        // Deleting the plate's lines renumbers the points; the text keeps its start point.
        let at = s.point(s.texts()[i].at);
        let lines: Vec<EntityId> = s.entity_ids().collect();
        s.remove(&[], &lines, &[]);
        assert_eq!(s.texts().len(), 1);
        assert_eq!(s.point(s.texts()[0].at), at);
        // Deleting the start point deletes the text.
        s.remove(&[s.texts()[0].at], &[], &[]);
        assert!(s.texts().is_empty());
    }

    #[test]
    fn missing_fonts_fall_back_to_the_built_in_one() {
        let mut t = Text::new("A", BUILTIN_FONT, 5.0, PointId(0));
        assert!(!t.set("AB", "No Such Font 123"));
        assert_eq!(t.font, BUILTIN_FONT);
        assert_eq!(t.glyphs.len(), 2);
        assert!(system_fonts()[0].name == BUILTIN_FONT);
        assert!(load_font(default_font()).is_some());
    }
}
