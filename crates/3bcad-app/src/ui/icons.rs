//! Command and tree icons, painted as small shaded 3D icons: teal solids lit
//! from the upper left, blue sketches and translucent planes, orange "add" and red "remove" arrows,
//! dark outlines (green only where it means OK, or the Y axis). Solids are built in 3D (z up) and drawn in isometric view; flat marks are placed in
//! a unit square.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use eframe::egui::{
    self, epaint::Mesh, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2,
};
use threebcad_core::FeatureKind;

use crate::ui::theme;

/// A shading ramp from shadow to highlight, and the outline colour.
#[derive(Clone, Copy)]
struct Mat {
    dark: Color32,
    base: Color32,
    light: Color32,
    ink: Color32,
}

const fn mat(dark: (u8, u8, u8), base: (u8, u8, u8), light: (u8, u8, u8), ink: (u8, u8, u8)) -> Mat {
    Mat {
        dark: Color32::from_rgb(dark.0, dark.1, dark.2),
        base: Color32::from_rgb(base.0, base.1, base.2),
        light: Color32::from_rgb(light.0, light.1, light.2),
        ink: Color32::from_rgb(ink.0, ink.1, ink.2),
    }
}

const TEAL: Mat = mat((14, 110, 106), (31, 169, 160), (168, 230, 223), (8, 71, 68));
const GREY: Mat = mat((96, 100, 110), (166, 171, 181), (232, 235, 240), (58, 61, 70));
const DARK: Mat = mat((34, 35, 40), (76, 79, 87), (150, 155, 166), (20, 20, 25));
const WHITE: Mat = mat((178, 183, 193), (232, 235, 240), (255, 255, 255), (80, 85, 96));
// The logo's blues: #1B5AA8, #2F86E0, #6CC4FF.
const BLUE: Mat = mat((27, 90, 168), (47, 134, 224), (108, 196, 255), (14, 52, 104));
/// OK and accept (a check mark, a green light) and the Y axis; "add" is orange.
const GREEN: Mat = mat((22, 112, 32), (64, 178, 72), (178, 238, 166), (16, 80, 24));
// The logo's orange, #FB8500: add, move and other accents, warnings.
const ORANGE: Mat = mat((184, 92, 0), (251, 133, 0), (255, 204, 140), (112, 56, 0));
const RED: Mat = mat((150, 26, 34), (214, 47, 58), (250, 172, 170), (104, 16, 22));
const WOOD: Mat = mat((168, 118, 70), (226, 182, 132), (250, 226, 192), (104, 70, 8));
const SKETCH: Color32 = Color32::from_rgb(27, 90, 168);
const INK: Color32 = Color32::from_rgb(50, 52, 62);

type V3 = [f32; 3];

/// The isometric view direction (towards the viewer) and the key light.
const VIEW: V3 = [1.0, 1.0, 1.0];
const LIGHT: V3 = [0.214, 0.471, 0.856];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: V3) -> V3 {
    mul(a, 1.0 / dot(a, a).sqrt().max(1e-6))
}

/// Brightness of a face with outward normal `n`.
fn shade(n: V3) -> f32 {
    0.32 + 0.68 * dot(n, LIGHT).max(0.0)
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()), f(a.a(), b.a()))
}

/// Colour of `m` at brightness `l` (0 shadow, 0.55 base, 1 highlight).
fn ramp(m: Mat, l: f32) -> Color32 {
    let l = l.clamp(0.0, 1.0);
    if l < 0.55 {
        mix(m.dark, m.base, l / 0.55)
    } else {
        mix(m.base, m.light, (l - 0.55) / 0.45)
    }
}

/// Axis-aligned rectangle at height `z`, counter-clockwise seen from above.
fn rect3(x0: f32, y0: f32, x1: f32, y1: f32, z: f32) -> Vec<V3> {
    vec![[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]]
}

/// Circle of radius `r` around `c` in the horizontal plane.
fn ring(c: V3, r: f32, n: usize) -> Vec<V3> {
    (0..n)
        .map(|i| {
            let a = TAU * i as f32 / n as f32;
            [c[0] + r * a.cos(), c[1] + r * a.sin(), c[2]]
        })
        .collect()
}

/// Polygon normal of a planar loop, and the outward normal of each edge in that plane.
fn edge_normals(prof: &[V3]) -> (V3, Vec<V3>) {
    let n = prof.len();
    let pn = norm((0..n).fold([0.0; 3], |acc, i| add(acc, cross(prof[i], prof[(i + 1) % n]))));
    let edges = (0..n)
        .map(|i| norm(cross(sub(prof[(i + 1) % n], prof[i]), pn)))
        .collect();
    (pn, edges)
}

/// Triangle indices for a simple (possibly concave) polygon, by ear clipping.
fn triangulate(p: &[Pos2]) -> Vec<u32> {
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b - a).x * (c - a).y - (b - a).y * (c - a).x;
    let area: f32 = (0..p.len())
        .map(|i| cross(Pos2::ZERO, p[i], p[(i + 1) % p.len()]))
        .sum();
    let mut idx: Vec<usize> = (0..p.len()).collect();
    if area < 0.0 {
        idx.reverse();
    }
    let mut tris = Vec::new();
    while idx.len() > 3 {
        let m = idx.len();
        let corner = |i: usize| (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
        let ear = (0..m)
            .find(|&i| {
                let (a, b, c) = corner(i);
                // A vertex on the ear's edge blocks it too, or a reflex corner could be cut off.
                let inside = |j: usize| {
                    cross(p[a], p[b], p[j]) >= 0.0 && cross(p[b], p[c], p[j]) >= 0.0 && cross(p[c], p[a], p[j]) >= 0.0
                };
                cross(p[a], p[b], p[c]) > 0.0
                    && idx
                        .iter()
                        .all(|&j| [a, b, c].iter().any(|&k| p[k] == p[j]) || !inside(j))
            })
            // Only degenerate (zero-area) leftovers have no ear; any cut is fine then.
            .unwrap_or(0);
        let (a, b, c) = corner(ear);
        tris.extend([a as u32, b as u32, c as u32]);
        idx.remove(ear);
    }
    tris.extend(idx.iter().map(|&i| i as u32));
    tris
}

struct Pen<'a> {
    p: &'a Painter,
    r: Rect,
    /// Multiplies every colour (greyed out when disabled).
    fade: f32,
    /// Isometric view: where the 3D origin lands in the unit square, and the scale.
    view: (f32, f32, f32),
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.r.min + vec2(x * self.r.width(), y * self.r.height())
    }

    /// A 3D point in unit-square coordinates.
    fn u(&self, x: f32, y: f32, z: f32) -> (f32, f32) {
        let (cx, cy, s) = self.view;
        (cx + s * (x - y) * 0.866, cy + s * ((x + y) * 0.5 - z))
    }

    fn iso(&self, v: V3) -> Pos2 {
        let (x, y) = self.u(v[0], v[1], v[2]);
        self.at(x, y)
    }

    /// Scales the view so the box `lo..hi` fills the icon.
    fn fit(&mut self, lo: V3, hi: V3) {
        self.view = (0.0, 0.0, 1.0);
        let mut b = Rect::NOTHING;
        for i in 0..8 {
            let pick = |axis: usize| if i >> axis & 1 == 0 { lo[axis] } else { hi[axis] };
            let (x, y) = self.u(pick(0), pick(1), pick(2));
            b.extend_with(pos2(x, y));
        }
        let s = 0.92 / b.width().max(b.height());
        self.view = (0.5 - s * b.center().x, 0.5 - s * b.center().y, s);
    }

    fn c(&self, c: Color32) -> Color32 {
        if self.fade >= 1.0 {
            c
        } else {
            let a = c.a() as f32 / 255.0;
            let g = ((c.r() as f32 + c.g() as f32 + c.b() as f32) / 3.0 / a.max(0.01)).min(255.0) as u8;
            Color32::from_gray(g).gamma_multiply(a * self.fade)
        }
    }

    fn w(&self, px: f32) -> f32 {
        px * self.r.width() / 24.0
    }

    /// Outline width: about a pixel at every size.
    fn ow(&self) -> f32 {
        self.w(1.0).max(0.8)
    }

    fn fill(&self, pts: &[Pos2], cols: &[Color32]) {
        let mut mesh = Mesh::default();
        for (&p, &c) in pts.iter().zip(cols) {
            mesh.colored_vertex(p, self.c(c));
        }
        mesh.indices = triangulate(pts);
        self.p.add(Shape::mesh(mesh));
    }

    fn line(&self, pts: &[(f32, f32)], color: Color32, width: f32) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p.add(Shape::line(pts, Stroke::new(self.w(width), self.c(color))));
    }

    fn dashed(&self, a: (f32, f32), b: (f32, f32), color: Color32) {
        let pts = [self.at(a.0, a.1), self.at(b.0, b.1)];
        let s = Stroke::new(self.w(1.2), self.c(color));
        self.p.extend(Shape::dashed_line(&pts, s, self.w(2.5), self.w(1.8)));
    }

    /// A flat shape shaded from light to dark along `dark`, with an outline.
    fn flat(&self, pts: &[(f32, f32)], m: Mat, dark: Vec2) {
        let s: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        let t: Vec<f32> = pts.iter().map(|&(x, y)| x * dark.x + y * dark.y).collect();
        let (lo, hi) = t.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        let cols: Vec<Color32> = t
            .iter()
            .map(|&v| ramp(m, 0.98 - 0.6 * (v - lo) / (hi - lo).max(1e-4)))
            .collect();
        self.fill(&s, &cols);
        self.p.add(Shape::closed_line(s, Stroke::new(self.ow(), self.c(m.ink))));
    }

    /// A flat shape with a little thickness, like a badge standing off the page.
    fn badge(&self, pts: &[(f32, f32)], m: Mat) {
        let back: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x + 0.035, y + 0.045)).collect();
        self.fill(&back, &vec![ramp(m, 0.12); back.len()]);
        self.flat(pts, m, vec2(0.6, 0.8));
    }

    /// A round tube along `pts`: outline, body and a highlight.
    fn tube(&self, pts: &[(f32, f32)], width: f32, m: Mat) {
        self.line(pts, m.ink, width);
        self.line(pts, m.base, width * 0.6);
        let o = width / 24.0 * 0.16;
        let hi: Vec<(f32, f32)> = pts.iter().map(|&(x, y)| (x - o, y - o)).collect();
        self.line(&hi, m.light, width * 0.22);
    }

    /// A shaded sphere.
    fn bead(&self, x: f32, y: f32, r: f32, m: Mat) {
        let c = self.at(x, y);
        let rr = r * self.r.width();
        self.p
            .circle_filled(c + vec2(rr * 0.3, rr * 0.35), rr, self.c(Color32::from_black_alpha(45)));
        self.p.circle_filled(c, rr, self.c(ramp(m, 0.3)));
        if rr > 2.0 {
            self.p
                .circle_filled(c - vec2(rr, rr) * 0.18, rr * 0.74, self.c(ramp(m, 0.62)));
            self.p
                .circle_filled(c - vec2(rr, rr) * 0.34, rr * 0.34, self.c(ramp(m, 1.0)));
        }
        self.p.circle_stroke(c, rr, Stroke::new(self.ow() * 0.8, self.c(m.ink)));
    }

    /// Arrow from `a` to `b`, shaded across its width.
    fn arrow(&self, a: (f32, f32), b: (f32, f32), m: Mat) {
        let a = vec2(a.0, a.1);
        let d = vec2(b.0, b.1) - a;
        let len = d.length();
        let d = d / len;
        let n = vec2(-d.y, d.x);
        let (shaft, head, head_len) = (0.055, 0.14, 0.26_f32.min(len * 0.6));
        let at = |t: f32, o: f32| {
            let p = a + d * t + n * o;
            (p.x, p.y)
        };
        let pts = [
            at(0.0, shaft),
            at(len - head_len, shaft),
            at(len - head_len, head),
            at(len, 0.0),
            at(len - head_len, -head),
            at(len - head_len, -shaft),
            at(0.0, -shaft),
        ];
        // The side facing the upper-left light is the bright one.
        let dark = if n.x + n.y > 0.0 { n } else { -n };
        self.flat(&pts, m, dark);
    }

    /// Curved arrow along an ellipse around `c`, from angle `a0` to the head at `a1`.
    fn arc_arrow(&self, c: (f32, f32), (rx, ry): (f32, f32), a0: f32, a1: f32, m: Mat) {
        let p = |a: f32, o: f32| (c.0 + (rx + o) * a.cos(), c.1 + (ry + o) * a.sin());
        let (band, head) = (0.05, 0.12);
        let base = a1 - (0.22 / rx).copysign(a1 - a0);
        let steps = 16;
        let along = |i: usize| a0 + (base - a0) * i as f32 / steps as f32;
        let mut pts: Vec<(f32, f32)> = (0..=steps).map(|i| p(along(i), band)).collect();
        pts.extend([p(base, head), p(a1, 0.0), p(base, -head)]);
        pts.extend((0..=steps).rev().map(|i| p(along(i), -band)));
        let mut cols = vec![ramp(m, 0.95); steps + 2];
        cols.extend([ramp(m, 0.7), ramp(m, 0.4)]);
        cols.extend(vec![ramp(m, 0.4); steps + 1]);
        let s: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.fill(&s, &cols);
        self.p.add(Shape::closed_line(s, Stroke::new(self.ow(), self.c(m.ink))));
    }

    /// A blue sketch curve with a soft drop shadow.
    fn sk(&self, pts: &[(f32, f32)]) {
        let o = 0.04;
        let shadow: Vec<(f32, f32)> = pts.iter().map(|&(x, y)| (x + o, y + o)).collect();
        self.line(&shadow, Color32::from_black_alpha(55), 2.0);
        self.line(pts, SKETCH, 2.0);
    }

    /// Sketch end points.
    fn ends(&self, pts: &[(f32, f32)]) {
        for &(x, y) in pts {
            self.bead(x, y, 0.06, TEAL);
        }
    }

    fn arc_pts(&self, (cx, cy): (f32, f32), r: f32, (a0, a1): (f32, f32)) -> Vec<(f32, f32)> {
        (0..=24)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / 24.0;
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    }

    fn sk_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.sk(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]);
    }

    fn sk_circle(&self, x: f32, y: f32, r: f32) {
        self.sk(&self.arc_pts((x, y), r, (0.0, TAU)));
    }

    fn face3(&self, pts: &[V3], lum: &[f32], m: Mat) {
        let s: Vec<Pos2> = pts.iter().map(|&v| self.iso(v)).collect();
        let (y0, y1) = s
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
        // Each face is a touch brighter at its top, for a little gloss.
        let cols: Vec<Color32> = s
            .iter()
            .zip(lum)
            .map(|(p, &l)| ramp(m, l + 0.16 * (0.5 - (p.y - y0) / (y1 - y0).max(1e-3))))
            .collect();
        self.fill(&s, &cols);
    }

    fn seg(&self, a: V3, b: V3, m: Mat) {
        self.p
            .line_segment([self.iso(a), self.iso(b)], Stroke::new(self.ow(), self.c(m.ink)));
    }

    fn outline3(&self, pts: &[V3], color: Color32) {
        let s = pts.iter().map(|&v| self.iso(v)).collect();
        self.p.add(Shape::closed_line(s, Stroke::new(self.ow(), self.c(color))));
    }

    /// Side faces between the loops `a` and `b` that face the viewer, far ones first. Edges between
    /// faces that meet smoothly (round faces) are shaded smoothly and not outlined. `sign` -1 shows
    /// the inside walls of a pocket.
    fn walls(&self, a: &[V3], b: &[V3], normals: &[V3], sign: f32, m: Mat) {
        let n = a.len();
        let nv = |i: usize| mul(normals[i], sign);
        let prev = |i: usize| (i + n - 1) % n;
        let vis = |i: usize| dot(nv(i), VIEW) > 0.0;
        let smooth = |i: usize| dot(normals[prev(i)], normals[i]) > 0.9;
        let vnorm = |i: usize, own: usize| {
            if smooth(i) {
                norm(add(nv(prev(i)), nv(i)))
            } else {
                nv(own)
            }
        };
        let depth = |i: usize| dot(add(a[i], b[(i + 1) % n]), VIEW);
        let mut order: Vec<usize> = (0..n).filter(|&i| vis(i)).collect();
        order.sort_by(|&i, &j| depth(i).total_cmp(&depth(j)));
        for i in order {
            let j = (i + 1) % n;
            let (li, lj) = (shade(vnorm(i, i)), shade(vnorm(j, i)));
            self.face3(&[a[i], a[j], b[j], b[i]], &[li, lj, lj, li], m);
            self.seg(a[i], a[j], m);
            self.seg(b[i], b[j], m);
            if !smooth(i) || !vis(prev(i)) {
                self.seg(a[i], b[i], m);
            }
            if !smooth(j) || !vis(j) {
                self.seg(a[j], b[j], m);
            }
        }
    }

    /// The planar loop `prof` swept along `ext`.
    fn prism(&self, prof: &[V3], ext: V3, m: Mat) {
        let top: Vec<V3> = prof.iter().map(|&p| add(p, ext)).collect();
        let (pn, normals) = edge_normals(prof);
        self.walls(prof, &top, &normals, 1.0, m);
        let up = if dot(pn, ext) > 0.0 { pn } else { mul(pn, -1.0) };
        let (cap, n) = if dot(up, VIEW) > 0.0 {
            (top.as_slice(), up)
        } else {
            (prof, mul(up, -1.0))
        };
        if dot(n, VIEW) > 0.0 {
            self.face3(cap, &vec![shade(n); cap.len()], m);
            self.outline3(cap, m.ink);
        }
    }

    /// A pocket of depth `depth` sunk into an upward face along the loop `prof`.
    fn pocket(&self, prof: &[V3], depth: f32, m: Mat) {
        let floor: Vec<V3> = prof.iter().map(|&p| sub(p, [0.0, 0.0, depth])).collect();
        let rim: Vec<Pos2> = prof.iter().map(|&v| self.iso(v)).collect();
        self.fill(&rim, &vec![ramp(m, 0.22); rim.len()]);
        self.walls(prof, &floor, &edge_normals(prof).1, -1.0, m);
        self.outline3(prof, m.ink);
    }

    /// A translucent reference plane.
    fn plane3(&self, pts: &[V3]) {
        let s: Vec<Pos2> = pts.iter().map(|&v| self.iso(v)).collect();
        let (y0, y1) = s
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
        let cols: Vec<Color32> = s
            .iter()
            .map(|p| ramp(BLUE, 0.95 - 0.3 * (p.y - y0) / (y1 - y0).max(1e-3)).gamma_multiply(0.5))
            .collect();
        self.fill(&s, &cols);
        self.outline3(pts, BLUE.base);
    }

    fn sk3(&self, pts: &[V3]) {
        let mut s: Vec<(f32, f32)> = pts.iter().map(|&v| self.u(v[0], v[1], v[2])).collect();
        s.push(s[0]);
        self.sk(&s);
    }

    fn pencil(&self, tip: (f32, f32), end: (f32, f32)) {
        let t = vec2(tip.0, tip.1);
        let d = vec2(end.0, end.1) - t;
        let len = d.length();
        let d = d / len;
        let n = vec2(-d.y, d.x);
        let dark = if n.x + n.y > 0.0 { n } else { -n };
        let at = |a: f32, o: f32| {
            let p = t + d * a + n * o;
            (p.x, p.y)
        };
        let w = 0.09;
        self.flat(&[at(0.0, 0.0), at(0.22, w), at(0.22, -w)], WOOD, dark);
        self.flat(&[at(0.0, 0.0), at(0.09, w * 0.4), at(0.09, -w * 0.4)], DARK, dark);
        self.flat(
            &[at(0.22, w), at(len - 0.14, w), at(len - 0.14, -w), at(0.22, -w)],
            TEAL,
            dark,
        );
        self.flat(
            &[at(len - 0.14, w), at(len, w), at(len, -w), at(len - 0.14, -w)],
            RED,
            dark,
        );
    }

    /// A sketch on a plane, for the sketch commands.
    fn sketch_sheet(&mut self) {
        self.fit([0.0, 0.0, 0.0], [1.0, 1.0, 0.0]);
        self.view.1 += 0.08;
        self.plane3(&rect3(0.0, 0.0, 1.0, 1.0, 0.0));
        self.sk3(&rect3(0.2, 0.2, 0.62, 0.62, 0.0));
    }
}

/// Draws icon `name` into `r`.
pub fn paint(p: &Painter, r: Rect, name: &str, enabled: bool) {
    let mut k = Pen {
        p,
        r,
        fade: if enabled { 1.0 } else { 0.45 },
        view: (0.5, 0.5, 0.4),
    };
    let o = [0.0; 3];
    match name {
        "extrude" => {
            k.fit(o, [1.0, 1.0, 1.3]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.55], TEAL);
            k.arrow(k.u(0.5, 0.5, 0.5), k.u(0.5, 0.5, 1.3), ORANGE);
        }
        "cut" => {
            k.fit(o, [1.0, 1.0, 1.3]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.55], TEAL);
            k.pocket(&rect3(0.25, 0.25, 0.75, 0.75, 0.55), 0.28, TEAL);
            k.arrow(k.u(0.5, 0.5, 1.3), k.u(0.5, 0.5, 0.5), RED);
        }
        "revolve" | "revolve_cut" => {
            k.fit([-0.1, -0.1, -0.2], [1.1, 1.1, 1.0]);
            k.dashed(k.u(0.5, 0.5, -0.2), k.u(0.5, 0.5, 0.4), INK);
            k.prism(&ring([0.5, 0.5, 0.0], 0.5, 32), [0.0, 0.0, 0.4], TEAL);
            let m = if name == "revolve" {
                ORANGE
            } else {
                k.pocket(&ring([0.5, 0.5, 0.4], 0.26, 24), 0.14, TEAL);
                RED
            };
            k.dashed(k.u(0.5, 0.5, 0.4), k.u(0.5, 0.5, 1.0), INK);
            // A circle of radius r around the axis is an ellipse of r * (1.22, 0.71) on screen.
            let r = 0.66 * k.view.2;
            k.arc_arrow(k.u(0.5, 0.5, 0.2), (r * 1.22, r * 0.71), -PI * 0.08, PI * 0.9, m);
        }
        "fillet" | "chamfer" => {
            k.fit(o, [1.0, 1.0, 0.9]);
            let mut prof = vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 1.0, 0.45]];
            if name == "fillet" {
                prof.extend((1..=8).map(|i| {
                    let a = FRAC_PI_2 * i as f32 / 8.0;
                    [0.0, 0.55 + 0.45 * a.cos(), 0.45 + 0.45 * a.sin()]
                }));
            } else {
                prof.push([0.0, 0.55, 0.9]);
            }
            prof.push([0.0, 0.0, 0.9]);
            k.prism(&prof, [1.0, 0.0, 0.0], TEAL);
        }
        "shell" => {
            k.fit(o, [1.0, 1.0, 0.7]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.7], TEAL);
            k.pocket(&rect3(0.14, 0.14, 0.86, 0.86, 0.7), 0.48, TEAL);
        }
        "hole" => {
            k.fit(o, [1.0, 1.0, 0.45]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.45], TEAL);
            k.pocket(&ring([0.5, 0.5, 0.45], 0.33, 32), 0.08, TEAL);
            k.pocket(&ring([0.5, 0.5, 0.37], 0.2, 24), 0.22, TEAL);
        }
        "draft" => {
            k.fit(o, [1.0, 1.0, 0.9]);
            let prof = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.72, 0.0, 0.9], [0.0, 0.0, 0.9]];
            k.prism(&prof, [0.0, 1.0, 0.0], TEAL);
            k.dashed(k.u(1.0, 1.0, 0.0), k.u(1.0, 1.0, 0.95), SKETCH);
        }
        "rib" => {
            k.fit(o, [1.0, 1.0, 1.0]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.2], GREY);
            k.prism(&rect3(0.0, 0.0, 0.2, 1.0, 0.2), [0.0, 0.0, 0.8], GREY);
            let web = [[0.2, 0.42, 0.2], [0.85, 0.42, 0.2], [0.2, 0.42, 0.85]];
            k.prism(&web, [0.0, 0.16, 0.0], TEAL);
        }
        "lpattern" => {
            k.fit(o, [1.15, 0.75, 0.3]);
            for i in 0..3 {
                let x = i as f32 * 0.42;
                k.prism(&rect3(x, 0.0, x + 0.3, 0.3, 0.0), [0.0, 0.0, 0.3], TEAL);
            }
            k.arrow(k.u(0.0, 0.62, 0.0), k.u(1.15, 0.62, 0.0), BLUE);
        }
        "cpattern" => {
            k.fit([-0.6, -0.6, 0.0], [0.6, 0.6, 0.3]);
            let mut at: Vec<V3> = ring(o, 0.44, 6);
            at.sort_by(|a, b| dot(*a, VIEW).total_cmp(&dot(*b, VIEW)));
            for c in at {
                k.prism(&ring(c, 0.14, 16), [0.0, 0.0, 0.3], TEAL);
            }
            k.dashed(k.u(0.0, 0.0, -0.2), k.u(0.0, 0.0, 0.6), SKETCH);
        }
        "mirror" => {
            k.fit([0.0, -0.15, -0.1], [1.0, 0.75, 0.95]);
            let wedge =
                |x0: f32, x1: f32, h0: f32, h1: f32| vec![[x0, 0.0, 0.0], [x1, 0.0, 0.0], [x1, 0.0, h1], [x0, 0.0, h0]];
            k.prism(&wedge(0.0, 0.36, 0.35, 0.8), [0.0, 0.6, 0.0], TEAL);
            k.plane3(&[
                [0.5, -0.15, -0.1],
                [0.5, 0.75, -0.1],
                [0.5, 0.75, 0.95],
                [0.5, -0.15, 0.95],
            ]);
            k.prism(&wedge(0.64, 1.0, 0.8, 0.35), [0.0, 0.6, 0.0], TEAL);
        }
        "plane" => {
            k.fit(o, [1.0, 0.8, 0.0]);
            k.plane3(&rect3(0.0, 0.0, 1.0, 0.8, 0.0));
        }
        "axis" => {
            k.fit([-0.4, -0.4, -0.2], [0.4, 0.4, 1.2]);
            k.dashed(k.u(0.0, 0.0, -0.2), k.u(0.0, 0.0, 1.2), SKETCH);
            k.prism(&ring(o, 0.13, 16), [0.0, 0.0, 1.0], BLUE);
        }
        "point" => {
            k.p.add(Shape::ellipse_filled(
                k.at(0.55, 0.8),
                vec2(0.3, 0.08) * r.width(),
                k.c(Color32::from_black_alpha(40)),
            ));
            k.bead(0.5, 0.48, 0.24, BLUE);
        }
        "csys" | "origin" => {
            k.fit([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            k.arrow(k.u(0.0, 0.0, 0.0), k.u(0.0, 1.0, 0.0), GREEN);
            k.arrow(k.u(0.0, 0.0, 0.0), k.u(1.0, 0.0, 0.0), RED);
            k.arrow(k.u(0.0, 0.0, 0.0), k.u(0.0, 0.0, 1.0), BLUE);
            let (x, y) = k.u(0.0, 0.0, 0.0);
            k.bead(x, y, 0.1, GREY);
        }
        "box" => {
            k.fit(o, [1.0, 1.0, 0.8]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.8], TEAL);
        }
        "cylinder" => {
            k.fit(o, [1.0, 1.0, 0.9]);
            k.prism(&ring([0.5, 0.5, 0.0], 0.5, 32), [0.0, 0.0, 0.9], TEAL);
        }
        "sketch" => {
            k.sketch_sheet();
            k.pencil(k.u(0.62, 0.62, 0.0), (0.97, 0.03));
        }
        "exit_sketch" => {
            k.sketch_sheet();
            k.tube(&[(0.42, 0.52), (0.6, 0.72), (0.94, 0.2)], 4.0, GREEN);
        }
        "convert" => {
            k.fit([-0.15, -0.15, 0.0], [1.15, 1.15, 1.05]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.4], TEAL);
            k.outline3(&rect3(0.0, 0.0, 1.0, 1.0, 0.4), SKETCH);
            for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                k.dashed(k.u(x, y, 0.4), k.u(x, y, 1.05), SKETCH);
            }
            k.plane3(&rect3(-0.15, -0.15, 1.15, 1.15, 1.05));
            k.sk3(&rect3(0.0, 0.0, 1.0, 1.0, 1.05));
        }
        "measure" => {
            k.fit(o, [1.6, 0.35, 0.65]);
            k.prism(&rect3(0.0, 0.0, 1.6, 0.35, 0.0), [0.0, 0.0, 0.1], TEAL);
            for i in 1..8 {
                let x = i as f32 * 0.2;
                let l = if i % 2 == 0 { 0.16 } else { 0.09 };
                k.seg([x, 0.35, 0.1], [x, 0.35 - l, 0.1], TEAL);
            }
            let mid = k.u(0.8, 0.15, 0.55);
            k.arrow(mid, k.u(0.05, 0.15, 0.55), BLUE);
            k.arrow(mid, k.u(1.55, 0.15, 0.55), BLUE);
        }
        "mass" => {
            k.fit(o, [1.0, 0.7, 1.05]);
            let ring_pts: Vec<(f32, f32)> = (0..=16)
                .map(|i| {
                    let a = PI * i as f32 / 16.0;
                    k.u(0.5 + 0.22 * a.cos(), 0.35, 0.6 + 0.4 * a.sin())
                })
                .collect();
            k.tube(&ring_pts, 3.0, GREY);
            let prof = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.82, 0.0, 0.65], [0.18, 0.0, 0.65]];
            k.prism(&prof, [0.0, 0.7, 0.0], DARK);
        }
        "whatswrong" => {
            k.badge(&[(0.5, 0.06), (0.94, 0.86), (0.06, 0.86)], ORANGE);
            k.line(&[(0.5, 0.34), (0.5, 0.6)], INK, 2.8);
            k.p.circle_filled(k.at(0.5, 0.72), k.w(1.6), k.c(INK));
        }
        "equations" => {
            k.badge(&[(0.14, 0.08), (0.86, 0.08), (0.86, 0.92), (0.14, 0.92)], WHITE);
            k.line(&[(0.3, 0.3), (0.7, 0.3)], INK, 1.6);
            k.line(&[(0.3, 0.46), (0.7, 0.46)], INK, 1.6);
            k.line(&[(0.3, 0.7), (0.52, 0.7)], INK, 1.2);
            k.bead(0.66, 0.72, 0.08, BLUE);
        }
        "appearance" => {
            k.bead(0.5, 0.28, 0.2, RED);
            k.bead(0.3, 0.66, 0.2, ORANGE);
            k.bead(0.7, 0.66, 0.2, BLUE);
        }
        "rebuild" => {
            k.badge(&[(0.3, 0.04), (0.68, 0.04), (0.68, 0.92), (0.3, 0.92)], DARK);
            k.bead(0.49, 0.2, 0.11, RED);
            k.bead(0.49, 0.48, 0.11, ORANGE);
            k.bead(0.49, 0.76, 0.11, GREEN);
        }
        "export" => {
            k.badge(
                &[(0.1, 0.3), (0.48, 0.3), (0.62, 0.44), (0.62, 0.92), (0.1, 0.92)],
                WHITE,
            );
            k.flat(&[(0.48, 0.3), (0.62, 0.44), (0.48, 0.44)], GREY, vec2(0.6, 0.8));
            k.arrow((0.34, 0.7), (0.94, 0.1), ORANGE);
        }
        "import" => {
            k.badge(
                &[(0.38, 0.3), (0.76, 0.3), (0.9, 0.44), (0.9, 0.92), (0.38, 0.92)],
                WHITE,
            );
            k.flat(&[(0.76, 0.3), (0.9, 0.44), (0.76, 0.44)], GREY, vec2(0.6, 0.8));
            k.arrow((0.06, 0.1), (0.62, 0.66), BLUE);
        }
        "mesh" => {
            k.badge(&[(0.5, 0.08), (0.94, 0.88), (0.06, 0.88)], GREY);
            k.line(&[(0.28, 0.48), (0.72, 0.48), (0.5, 0.88), (0.28, 0.48)], INK, 1.2);
        }
        "slicer" => {
            k.fit([-0.05, 0.0, 0.0], [1.05, 1.0, 0.95]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.08], GREY);
            k.prism(&rect3(0.3, 0.3, 0.7, 0.7, 0.08), [0.0, 0.0, 0.3], TEAL);
            let (nx, ny) = k.u(0.5, 0.5, 0.48);
            let (hx, hy) = k.u(0.5, 0.5, 0.66);
            k.flat(&[(nx, ny), (hx + 0.07, hy), (hx - 0.07, hy)], DARK, vec2(1.0, 0.0));
            k.prism(&rect3(-0.05, 0.44, 1.05, 0.56, 0.8), [0.0, 0.0, 0.08], GREY);
            k.prism(&rect3(0.36, 0.36, 0.64, 0.64, 0.66), [0.0, 0.0, 0.26], GREY);
        }
        "help" => {
            k.bead(0.5, 0.5, 0.44, BLUE);
            k.p.text(
                k.at(0.5, 0.53),
                Align2::CENTER_CENTER,
                "?",
                FontId::proportional(r.height() * 0.66),
                k.c(Color32::WHITE),
            );
        }
        // Sketch tools.
        "select" => k.badge(
            &[
                (0.22, 0.06),
                (0.22, 0.82),
                (0.4, 0.64),
                (0.54, 0.94),
                (0.67, 0.88),
                (0.53, 0.59),
                (0.78, 0.59),
            ],
            WHITE,
        ),
        "dimension" => {
            k.line(&[(0.12, 0.2), (0.12, 0.88)], INK, 1.0);
            k.line(&[(0.88, 0.2), (0.88, 0.88)], INK, 1.0);
            k.arrow((0.5, 0.4), (0.13, 0.4), BLUE);
            k.arrow((0.5, 0.4), (0.87, 0.4), BLUE);
            k.sk(&[(0.12, 0.82), (0.88, 0.82)]);
            k.ends(&[(0.12, 0.82), (0.88, 0.82)]);
        }
        "line" => {
            k.sk(&[(0.14, 0.84), (0.86, 0.16)]);
            k.ends(&[(0.14, 0.84), (0.86, 0.16)]);
        }
        "centerline" => {
            k.dashed((0.14, 0.84), (0.86, 0.16), INK);
            k.ends(&[(0.14, 0.84), (0.86, 0.16)]);
        }
        "rect" | "rect_center" => {
            k.sk_rect(0.1, 0.22, 0.9, 0.8);
            if name == "rect" {
                k.ends(&[(0.1, 0.22), (0.9, 0.8)]);
            } else {
                k.dashed((0.1, 0.22), (0.9, 0.8), INK);
                k.ends(&[(0.5, 0.51), (0.9, 0.8)]);
            }
        }
        "rect_3pt" => {
            let c = [(0.1, 0.55), (0.45, 0.15), (0.9, 0.5), (0.55, 0.9)];
            k.sk(&[c[0], c[1], c[2], c[3], c[0]]);
            k.ends(&c[..3]);
        }
        "circle" | "circle_perim" => {
            k.sk_circle(0.5, 0.5, 0.38);
            if name == "circle" {
                k.ends(&[(0.5, 0.5)]);
            } else {
                k.ends(&[(0.12, 0.5), (0.77, 0.23), (0.77, 0.77)]);
            }
        }
        "arc" | "arc_tangent" | "arc_3pt" => {
            k.sk(&k.arc_pts((0.5, 0.65), 0.38, (PI, TAU)));
            match name {
                "arc" => k.ends(&[(0.5, 0.65), (0.12, 0.65), (0.88, 0.65)]),
                "arc_tangent" => {
                    k.sk(&[(0.0, 0.95), (0.12, 0.65)]);
                    k.ends(&[(0.12, 0.65), (0.88, 0.65)]);
                }
                _ => k.ends(&[(0.12, 0.65), (0.5, 0.27), (0.88, 0.65)]),
            }
        }
        "spline" => {
            let pts: Vec<(f32, f32)> = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0;
                    (0.1 + 0.8 * t, 0.5 - 0.3 * (t * TAU).sin())
                })
                .collect();
            k.sk(&pts);
            k.ends(&[(0.1, 0.5), (0.3, 0.2), (0.7, 0.8), (0.9, 0.5)]);
        }
        "slot" => {
            let mut pts = k.arc_pts((0.7, 0.5), 0.2, (-FRAC_PI_2, FRAC_PI_2));
            pts.extend(k.arc_pts((0.3, 0.5), 0.2, (FRAC_PI_2, PI * 1.5)));
            pts.push(pts[0]);
            k.sk(&pts);
            k.ends(&[(0.3, 0.5), (0.7, 0.5)]);
        }
        "polygon" => {
            let pts: Vec<(f32, f32)> = (0..=6)
                .map(|i| {
                    let a = TAU * i as f32 / 6.0;
                    (0.5 + 0.4 * a.cos(), 0.5 + 0.4 * a.sin())
                })
                .collect();
            k.sk(&pts);
            k.ends(&[(0.5, 0.5)]);
        }
        "sketch_point" => k.bead(0.5, 0.5, 0.14, TEAL),
        "sketch_fillet" | "sketch_chamfer" => {
            k.dashed((0.15, 0.5), (0.15, 0.15), INK);
            k.dashed((0.15, 0.15), (0.5, 0.15), INK);
            let mut pts = vec![(0.15, 0.9)];
            if name == "sketch_fillet" {
                pts.extend(k.arc_pts((0.5, 0.5), 0.35, (PI, PI * 1.5)));
            } else {
                pts.extend([(0.15, 0.5), (0.5, 0.15)]);
            }
            pts.push((0.9, 0.15));
            k.sk(&pts);
            k.ends(&[(0.15, 0.5), (0.5, 0.15)]);
        }
        "trim" => {
            k.sk(&[(0.1, 0.5), (0.9, 0.5)]);
            k.sk(&[(0.5, 0.1), (0.5, 0.9)]);
            k.tube(&[(0.62, 0.62), (0.88, 0.88)], 3.0, RED);
            k.tube(&[(0.88, 0.62), (0.62, 0.88)], 3.0, RED);
        }
        "extend" => {
            k.sk(&[(0.86, 0.1), (0.86, 0.9)]);
            k.sk(&[(0.08, 0.5), (0.4, 0.5)]);
            k.arrow((0.38, 0.5), (0.84, 0.5), ORANGE);
        }
        "offset" => {
            k.line(&[(0.1, 0.1), (0.6, 0.1), (0.6, 0.6), (0.1, 0.6), (0.1, 0.1)], INK, 1.0);
            k.sk_rect(0.26, 0.26, 0.9, 0.9);
            k.arrow((0.6, 0.6), (0.86, 0.86), ORANGE);
        }
        "sketch_mirror" => {
            k.dashed((0.5, 0.04), (0.5, 0.96), INK);
            k.sk(&[(0.1, 0.8), (0.4, 0.25)]);
            k.sk(&[(0.9, 0.8), (0.6, 0.25)]);
            k.ends(&[(0.1, 0.8), (0.4, 0.25), (0.9, 0.8), (0.6, 0.25)]);
        }
        "sketch_lpattern" => {
            for i in 0..3 {
                k.sk_circle(0.18 + 0.32 * i as f32, 0.42, 0.12);
            }
            k.arrow((0.06, 0.8), (0.94, 0.8), ORANGE);
        }
        "sketch_cpattern" => {
            for i in 0..5 {
                let a = TAU * i as f32 / 5.0 - FRAC_PI_2;
                k.sk_circle(0.5 + 0.33 * a.cos(), 0.5 + 0.33 * a.sin(), 0.1);
            }
            k.ends(&[(0.5, 0.5)]);
        }
        // v0.4 features.
        "sweep" => {
            k.fit([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let path: Vec<(f32, f32)> = (0..=20)
                .map(|i| {
                    let a = FRAC_PI_2 * i as f32 / 20.0;
                    k.u(0.15 + 0.7 * (1.0 - a.cos()), 0.5, 0.15 + 0.7 * a.sin())
                })
                .collect();
            k.tube(&path, 6.0, TEAL);
            k.sk3(&ring([0.15, 0.5, 0.0], 0.16, 16));
        }
        "loft" => {
            k.fit([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            let n = 16;
            let base: Vec<V3> = (0..n)
                .map(|i| {
                    let a = TAU * i as f32 / n as f32;
                    let s = 0.5 / a.cos().abs().max(a.sin().abs());
                    [0.5 + s * a.cos(), 0.5 + s * a.sin(), 0.0]
                })
                .collect();
            let top = ring([0.5, 0.5, 1.0], 0.24, n);
            let (_, normals) = edge_normals(&base);
            k.walls(&base, &top, &normals, 1.0, TEAL);
            k.face3(&top, &vec![shade([0.0, 0.0, 1.0]); n], TEAL);
            k.outline3(&top, TEAL.ink);
        }
        "helix" => {
            k.fit([0.0, 0.0, 0.0], [1.0, 1.0, 1.1]);
            let pts: Vec<(f32, f32)> = (0..=96)
                .map(|i| {
                    let t = i as f32 / 96.0;
                    let a = TAU * 3.0 * t;
                    k.u(0.5 + 0.45 * a.cos(), 0.5 + 0.45 * a.sin(), 1.1 * t)
                })
                .collect();
            k.sk(&pts);
        }
        "thread" => {
            k.fit([0.0, 0.0, 0.0], [1.0, 1.0, 1.1]);
            k.prism(&ring([0.5, 0.5, 0.0], 0.4, 24), [0.0, 0.0, 1.1], TEAL);
            for i in 0..5 {
                let z = 0.12 + 0.2 * i as f32;
                let pts: Vec<(f32, f32)> = (0..=12)
                    .map(|j| {
                        let a = -FRAC_PI_2 * 0.2 + PI * 0.9 * j as f32 / 12.0 - PI * 0.25;
                        k.u(0.5 + 0.4 * a.cos(), 0.5 + 0.4 * a.sin(), z + 0.12 * j as f32 / 12.0)
                    })
                    .collect();
                k.line(&pts, TEAL.ink, 1.3);
            }
        }
        "variable_fillet" | "face_fillet" => {
            k.fit(o, [1.0, 1.0, 0.9]);
            let mut prof = vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 1.0, 0.45]];
            prof.extend((1..=8).map(|i| {
                let a = FRAC_PI_2 * i as f32 / 8.0;
                [0.0, 0.55 + 0.45 * a.cos(), 0.45 + 0.45 * a.sin()]
            }));
            prof.push([0.0, 0.0, 0.9]);
            k.prism(&prof, [1.0, 0.0, 0.0], TEAL);
            if name == "variable_fillet" {
                let (a, b) = (k.u(0.05, 0.86, 0.75), k.u(0.95, 0.86, 0.75));
                k.bead(a.0, a.1, 0.06, BLUE);
                k.bead(b.0, b.1, 0.11, BLUE);
            } else {
                k.plane3(&[[0.0, 0.0, 0.9], [1.0, 0.0, 0.9], [1.0, 0.55, 0.9], [0.0, 0.55, 0.9]]);
            }
        }
        "combine" => {
            k.fit(o, [1.3, 1.0, 0.7]);
            k.prism(&rect3(0.0, 0.0, 0.8, 0.8, 0.0), [0.0, 0.0, 0.5], TEAL);
            k.prism(&rect3(0.5, 0.3, 1.3, 1.0, 0.2), [0.0, 0.0, 0.5], GREY);
            k.bead(0.8, 0.22, 0.13, ORANGE);
        }
        "split" => {
            k.fit([0.0, -0.1, -0.1], [1.2, 1.0, 0.8]);
            k.prism(&rect3(0.0, 0.0, 0.5, 0.9, 0.0), [0.0, 0.0, 0.6], TEAL);
            k.plane3(&[[0.6, -0.1, -0.1], [0.6, 1.0, -0.1], [0.6, 1.0, 0.8], [0.6, -0.1, 0.8]]);
            k.prism(&rect3(0.7, 0.0, 1.2, 0.9, 0.0), [0.0, 0.0, 0.6], TEAL);
        }
        "delete_body" => {
            k.fit(o, [1.0, 1.0, 0.8]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.8], GREY);
            k.tube(&[(0.55, 0.55), (0.92, 0.92)], 3.5, RED);
            k.tube(&[(0.92, 0.55), (0.55, 0.92)], 3.5, RED);
        }
        "move_body" => {
            k.fit(o, [1.0, 1.0, 0.8]);
            k.prism(&rect3(0.0, 0.0, 0.7, 0.7, 0.0), [0.0, 0.0, 0.5], TEAL);
            k.arrow((0.5, 0.5), (0.96, 0.5), ORANGE);
            k.arrow((0.5, 0.5), (0.5, 0.04), ORANGE);
        }
        "scale" => {
            k.fit(o, [1.0, 1.0, 1.0]);
            k.outline3(&rect3(0.0, 0.0, 1.0, 1.0, 1.0), INK);
            k.prism(&rect3(0.0, 0.0, 0.55, 0.55, 0.0), [0.0, 0.0, 0.55], TEAL);
            k.arrow(k.u(0.55, 0.55, 0.55), k.u(1.0, 1.0, 1.0), ORANGE);
        }
        "sketch_pattern" => {
            k.fit(o, [1.0, 1.0, 0.3]);
            k.plane3(&rect3(0.0, 0.0, 1.0, 1.0, 0.0));
            for c in [[0.2, 0.25, 0.0], [0.75, 0.2, 0.0], [0.45, 0.75, 0.0]] {
                k.prism(&ring(c, 0.13, 16), [0.0, 0.0, 0.3], TEAL);
            }
        }
        "curve_pattern" => {
            k.fit([-0.1, -0.1, 0.0], [1.1, 1.1, 0.3]);
            let pts: Vec<(f32, f32)> = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0;
                    k.u(t, 0.5 + 0.4 * (t * PI).sin() - 0.2, 0.0)
                })
                .collect();
            k.sk(&pts);
            for i in 0..4 {
                let t = i as f32 / 3.0;
                k.prism(
                    &ring([t, 0.3 + 0.4 * (t * PI).sin(), 0.0], 0.11, 14),
                    [0.0, 0.0, 0.3],
                    TEAL,
                );
            }
        }
        "dome" => {
            k.fit([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
            k.prism(&ring([0.5, 0.5, 0.0], 0.5, 32), [0.0, 0.0, 0.5], TEAL);
            let rings: Vec<Vec<V3>> = (0..=6)
                .map(|i| {
                    let a = FRAC_PI_2 * 0.96 * i as f32 / 6.0;
                    ring([0.5, 0.5, 0.5 + 0.5 * a.sin()], 0.5 * a.cos(), 32)
                })
                .collect();
            for w in rings.windows(2) {
                let (_, normals) = edge_normals(&w[0]);
                k.walls(&w[0], &w[1], &normals, 1.0, TEAL);
            }
        }
        "wrap" => {
            k.fit([0.0, 0.0, 0.0], [1.0, 1.0, 1.1]);
            k.prism(&ring([0.5, 0.5, 0.0], 0.45, 32), [0.0, 0.0, 1.1], TEAL);
            let band = |z: f32| -> Vec<(f32, f32)> {
                (0..=10)
                    .map(|j| {
                        let a = 0.1 + 0.9 * j as f32 / 10.0;
                        k.u(0.5 + 0.46 * a.cos(), 0.5 + 0.46 * a.sin(), z)
                    })
                    .collect()
            };
            let mut pts = band(0.35);
            pts.extend(band(0.75).into_iter().rev());
            pts.push(pts[0]);
            k.sk(&pts);
        }
        // 3D printing.
        "fit" => {
            k.fit(o, [1.0, 1.0, 1.0]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.35], TEAL);
            k.pocket(&ring([0.5, 0.5, 0.35], 0.3, 32), 0.3, TEAL);
            k.prism(&ring([0.5, 0.5, 0.1], 0.2, 24), [0.0, 0.0, 0.9], GREY);
        }
        "overhang" => {
            k.fit(o, [1.0, 1.0, 1.0]);
            k.prism(&rect3(0.35, 0.0, 0.65, 1.0, 0.0), [0.0, 0.0, 0.6], TEAL);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.6), [0.0, 0.0, 0.3], TEAL);
            k.arrow(k.u(0.12, 0.5, 0.55), k.u(0.12, 0.5, 0.0), RED);
        }
        "thin_wall" => {
            k.fit(o, [1.0, 1.0, 0.9]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.3], TEAL);
            k.prism(&rect3(0.45, 0.0, 0.55, 1.0, 0.3), [0.0, 0.0, 0.6], RED);
        }
        "lay_flat" => {
            k.fit([-0.1, -0.1, 0.0], [1.1, 1.1, 1.2]);
            k.prism(&rect3(-0.1, -0.1, 1.1, 1.1, 0.0), [0.0, 0.0, 0.08], GREY);
            k.prism(&rect3(0.25, 0.25, 0.75, 0.75, 0.08), [0.0, 0.0, 0.45], TEAL);
            k.arrow(k.u(0.5, 0.5, 1.2), k.u(0.5, 0.5, 0.6), ORANGE);
        }
        // Assemblies.
        "assembly" => {
            k.fit(o, [1.0, 1.0, 1.0]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.4], TEAL);
            k.prism(&rect3(0.25, 0.25, 0.75, 0.75, 0.4), [0.0, 0.0, 0.6], BLUE);
        }
        "insert" => {
            k.fit(o, [1.0, 1.0, 1.2]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.35], TEAL);
            k.prism(&rect3(0.3, 0.3, 0.7, 0.7, 0.75), [0.0, 0.0, 0.4], BLUE);
            k.arrow(k.u(0.5, 0.5, 0.75), k.u(0.5, 0.5, 0.4), ORANGE);
        }
        "mate" => {
            k.fit(o, [1.0, 1.0, 1.0]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.45], TEAL);
            k.prism(&rect3(0.2, 0.2, 0.8, 0.8, 0.55), [0.0, 0.0, 0.45], BLUE);
        }
        "fix" => {
            k.fit(o, [1.0, 1.0, 1.0]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.5], TEAL);
            k.prism(&ring([0.5, 0.5, 0.5], 0.12, 16), [0.0, 0.0, 0.5], RED);
        }
        "interference" => {
            k.fit(o, [1.2, 1.0, 0.8]);
            k.prism(&rect3(0.0, 0.0, 0.75, 1.0, 0.0), [0.0, 0.0, 0.8], TEAL);
            k.prism(&rect3(0.45, 0.0, 0.75, 1.0, 0.0), [0.0, 0.0, 0.8], RED);
            k.outline3(&rect3(0.45, 0.0, 1.2, 1.0, 0.8), INK);
        }
        "explode" => {
            k.fit(o, [1.0, 1.0, 1.3]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.35], TEAL);
            k.prism(&rect3(0.25, 0.25, 0.75, 0.75, 0.9), [0.0, 0.0, 0.35], BLUE);
            k.arrow(k.u(0.5, 0.5, 0.4), k.u(0.5, 0.5, 0.85), ORANGE);
        }
        "bom" => {
            k.fit(o, [1.0, 1.0, 0.1]);
            k.prism(&rect3(0.0, 0.0, 1.0, 1.0, 0.0), [0.0, 0.0, 0.1], WHITE);
            for y in [0.25, 0.5, 0.75] {
                k.line(&[k.u(0.15, y, 0.1), k.u(0.85, y, 0.1)], INK, 1.2);
            }
        }
        "edit_part" => {
            k.fit(o, [1.0, 1.0, 1.0]);
            k.prism(&rect3(0.0, 0.0, 0.8, 0.8, 0.0), [0.0, 0.0, 0.6], TEAL);
            k.pencil(k.u(0.55, 0.55, 0.6), k.u(1.0, 1.0, 1.0));
        }
        // Drawings: marks on a white sheet.
        "sheet" | "std_views" | "model_view" | "projected_view" | "section_view" | "detail_view" | "note"
        | "model_items" => {
            k.flat(
                &[(0.1, 0.08), (0.9, 0.08), (0.9, 0.92), (0.1, 0.92)],
                WHITE,
                vec2(0.4, 0.6),
            );
            let frame = |x0: f32, y0: f32, x1: f32, y1: f32| {
                k.line(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)], INK, 1.2);
            };
            match name {
                "sheet" => {
                    k.line(&[(0.45, 0.92), (0.45, 0.74), (0.9, 0.74)], INK, 1.0);
                    k.line(&[(0.45, 0.83), (0.9, 0.83)], INK, 0.8);
                }
                "std_views" => {
                    frame(0.2, 0.18, 0.48, 0.42);
                    frame(0.2, 0.54, 0.48, 0.8);
                    frame(0.58, 0.18, 0.8, 0.42);
                }
                "model_view" => {
                    k.line(
                        &[
                            (0.5, 0.2),
                            (0.8, 0.35),
                            (0.8, 0.7),
                            (0.5, 0.85),
                            (0.2, 0.7),
                            (0.2, 0.35),
                            (0.5, 0.2),
                        ],
                        INK,
                        1.2,
                    );
                    k.line(&[(0.2, 0.35), (0.5, 0.5), (0.8, 0.35)], INK, 1.0);
                    k.line(&[(0.5, 0.5), (0.5, 0.85)], INK, 1.0);
                }
                "projected_view" => {
                    frame(0.17, 0.35, 0.42, 0.65);
                    frame(0.62, 0.35, 0.83, 0.65);
                    k.arrow((0.44, 0.5), (0.6, 0.5), BLUE);
                }
                "section_view" => {
                    frame(0.2, 0.28, 0.8, 0.72);
                    for i in 0..5 {
                        let x = 0.2 + 0.12 * i as f32;
                        k.line(
                            &[(x, 0.72), ((x + 0.44).min(0.8), 0.72 - (x + 0.44).min(0.8) + x)],
                            INK,
                            0.7,
                        );
                    }
                    k.line(&[(0.5, 0.14), (0.5, 0.86)], RED.base, 1.0);
                }
                "detail_view" => {
                    k.line(&k.arc_pts((0.3, 0.62), 0.11, (0.0, TAU)), INK, 1.0);
                    k.line(&k.arc_pts((0.62, 0.38), 0.22, (0.0, TAU)), BLUE.base, 1.4);
                }
                "note" => {
                    for y in [0.24, 0.36, 0.48] {
                        k.line(&[(0.45, y), (0.82, y)], INK, 1.2);
                    }
                    k.line(&[(0.43, 0.36), (0.2, 0.75)], INK, 1.0);
                    k.bead(0.2, 0.76, 0.06, BLUE);
                }
                _ => {
                    frame(0.25, 0.45, 0.75, 0.8);
                    k.line(&[(0.25, 0.42), (0.25, 0.2)], INK, 0.8);
                    k.line(&[(0.75, 0.42), (0.75, 0.2)], INK, 0.8);
                    k.arrow((0.5, 0.28), (0.27, 0.28), BLUE);
                    k.arrow((0.5, 0.28), (0.73, 0.28), BLUE);
                }
            }
        }
        _ => k.bead(0.5, 0.5, 0.14, GREY),
    }
}

/// Icon for a feature in the feature tree.
pub fn of_feature(kind: &FeatureKind) -> &'static str {
    match kind {
        FeatureKind::Sketch { .. } => "sketch",
        FeatureKind::Extrude(e) if e.cut => "cut",
        FeatureKind::Extrude(_) => "extrude",
        FeatureKind::Revolve(r) if r.cut => "revolve_cut",
        FeatureKind::Revolve(_) => "revolve",
        FeatureKind::Fillet { .. } => "fillet",
        FeatureKind::Chamfer { .. } => "chamfer",
        FeatureKind::Shell { .. } => "shell",
        FeatureKind::Hole(_) => "hole",
        FeatureKind::LinearPattern(_) => "lpattern",
        FeatureKind::CircularPattern(_) => "cpattern",
        FeatureKind::Mirror(_) => "mirror",
        FeatureKind::RefPlane(_) => "plane",
        FeatureKind::RefAxis(_) => "axis",
        FeatureKind::RefPoint(_) => "point",
        FeatureKind::CoordSys(_) => "csys",
        FeatureKind::Draft { .. } => "draft",
        FeatureKind::Rib { .. } => "rib",
        FeatureKind::Sweep(_) => "sweep",
        FeatureKind::Loft(_) => "loft",
        FeatureKind::Helix(_) => "helix",
        FeatureKind::Thread(_) => "thread",
        FeatureKind::VariableFillet { .. } => "variable_fillet",
        FeatureKind::FaceFillet { .. } => "face_fillet",
        FeatureKind::Combine { .. } => "combine",
        FeatureKind::Split { .. } => "split",
        FeatureKind::DeleteBody { .. } => "delete_body",
        FeatureKind::MoveBody(_) => "move_body",
        FeatureKind::ScaleBody { .. } => "scale",
        FeatureKind::SketchPattern { .. } => "sketch_pattern",
        FeatureKind::CurvePattern { .. } => "curve_pattern",
        FeatureKind::Dome { .. } => "dome",
        FeatureKind::Wrap { .. } => "wrap",
        FeatureKind::Fit { .. } => "fit",
        FeatureKind::Imported { .. } => "import",
        FeatureKind::ImportedMesh { .. } => "mesh",
    }
}

/// A small icon placed inline (tree rows, small buttons).
pub fn inline(ui: &mut Ui, name: &str, enabled: bool) {
    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
    paint(ui.painter(), r, name, enabled);
}

fn frame(ui: &Ui, resp: &Response, selected: bool) {
    let r = resp.rect;
    if selected {
        ui.painter().rect_filled(r, 3.0, theme::SELECTED_FILL);
    }
    if resp.hovered() && resp.sense.senses_click() {
        ui.painter().rect_filled(r, 3.0, theme::HOVER_FILL);
        ui.painter()
            .rect_stroke(r, 3.0, Stroke::new(1.0_f32, theme::HOVER_STROKE), StrokeKind::Inside);
    }
}

/// Ribbon button: a large icon over its label.
pub fn tile(ui: &mut Ui, icon: &str, label: &str, enabled: bool, selected: bool) -> Response {
    let color = if enabled {
        Color32::from_rgb(30, 30, 30)
    } else {
        theme::MUTED
    };
    let mut job = egui::text::LayoutJob::simple(label.to_owned(), FontId::proportional(11.5), color, 74.0);
    job.halign = egui::Align::Center;
    let galley = ui.fonts(|f| f.layout_job(job));
    let size = vec2(galley.size().x.max(40.0) + 10.0, 30.0 + galley.size().y + 10.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    frame(ui, &resp, selected);
    let icon_rect = Rect::from_center_size(pos2(rect.center().x, rect.top() + 18.0), vec2(26.0, 26.0));
    paint(ui.painter(), icon_rect, icon, enabled);
    // The job is centred, so its lines are laid out around this x.
    let text_pos = pos2(rect.center().x, rect.top() + 34.0);
    ui.painter().galley(text_pos, galley, color);
    resp
}

/// Small command: icon and label on one line (stacked three high in the ribbon).
pub fn small(ui: &mut Ui, icon: &str, label: &str, enabled: bool, selected: bool) -> Response {
    let color = if enabled {
        Color32::from_rgb(30, 30, 30)
    } else {
        theme::MUTED
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(12.0), color);
    let size = vec2(galley.size().x + 28.0, 21.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    frame(ui, &resp, selected);
    paint(
        ui.painter(),
        Rect::from_min_size(rect.min + vec2(3.0, 2.5), vec2(16.0, 16.0)),
        icon,
        enabled,
    );
    ui.painter()
        .galley(rect.min + vec2(24.0, (21.0 - galley.size().y) / 2.0), galley, color);
    resp
}

/// Label used as a tab under the command row.
pub fn tab(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(12.5),
        Color32::from_rgb(30, 30, 30),
    );
    let size = vec2(galley.size().x + 18.0, 22.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(
            rect,
            egui::CornerRadius {
                nw: 0,
                ne: 0,
                sw: 3,
                se: 3,
            },
            Color32::WHITE,
        );
        painter.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(2.0_f32, theme::HOVER_STROKE),
        );
    } else if resp.hovered() {
        painter.rect_filled(rect, 3.0, theme::HOVER_FILL);
    }
    painter.galley(
        rect.min + vec2(9.0, (22.0 - galley.size().y) / 2.0),
        galley,
        Color32::from_rgb(30, 30, 30),
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concave_polygons_triangulate_to_their_area() {
        let area = |a: Pos2, b: Pos2, c: Pos2| ((b - a).x * (c - a).y - (b - a).y * (c - a).x).abs() / 2.0;
        // An L shape (area 3) in both windings.
        let mut l = vec![
            pos2(0.0, 0.0),
            pos2(2.0, 0.0),
            pos2(2.0, 1.0),
            pos2(1.0, 1.0),
            pos2(1.0, 2.0),
            pos2(0.0, 2.0),
        ];
        for _ in 0..2 {
            let t = triangulate(&l);
            assert_eq!(t.len(), 12);
            let sum: f32 = t
                .chunks(3)
                .map(|c| area(l[c[0] as usize], l[c[1] as usize], l[c[2] as usize]))
                .sum();
            assert!((sum - 3.0).abs() < 1e-5, "{sum}");
            l.reverse();
        }
    }
}
