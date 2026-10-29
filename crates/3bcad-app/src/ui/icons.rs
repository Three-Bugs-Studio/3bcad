//! Command and tree icons drawn with the painter, in the SolidWorks style: yellow solids, blue
//! sketches and references, green "add" and red "remove" arrows. Coordinates are in a unit square.

use std::f32::consts::{PI, TAU};

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui,
};
use threebcad_core::FeatureKind;

use crate::ui::theme;

const SOLID: Color32 = Color32::from_rgb(236, 196, 74);
const SOLID_TOP: Color32 = Color32::from_rgb(250, 222, 130);
const SOLID_SIDE: Color32 = Color32::from_rgb(200, 158, 40);
const INK: Color32 = Color32::from_rgb(55, 55, 65);
const BLUE: Color32 = Color32::from_rgb(35, 105, 200);
const PLANE: Color32 = Color32::from_rgb(120, 170, 230);
const GREEN: Color32 = Color32::from_rgb(40, 150, 60);
const RED: Color32 = Color32::from_rgb(205, 50, 40);

struct Pen<'a> {
    p: &'a Painter,
    r: Rect,
    /// Multiplies every colour (greyed out when disabled).
    fade: f32,
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.r.min + vec2(x * self.r.width(), y * self.r.height())
    }

    fn c(&self, c: Color32) -> Color32 {
        if self.fade >= 1.0 {
            c
        } else {
            let g = ((c.r() as f32 + c.g() as f32 + c.b() as f32) / 3.0) as u8;
            Color32::from_rgba_unmultiplied(g, g, g, (255.0 * self.fade) as u8)
        }
    }

    fn w(&self, px: f32) -> f32 {
        px * self.r.width() / 24.0
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

    fn poly(&self, pts: &[(f32, f32)], fill: Color32) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p.add(Shape::convex_polygon(
            pts,
            self.c(fill),
            Stroke::new(self.w(1.0), self.c(INK)),
        ));
    }

    fn circle(&self, x: f32, y: f32, r: f32, fill: Option<Color32>, color: Color32) {
        let c = self.at(x, y);
        let rr = r * self.r.width();
        if let Some(f) = fill {
            self.p.circle_filled(c, rr, self.c(f));
        }
        self.p.circle_stroke(c, rr, Stroke::new(self.w(1.3), self.c(color)));
    }

    /// Arc around `(cx, cy)` from angle `a0` to `a1`.
    fn arc(&self, (cx, cy): (f32, f32), r: f32, (a0, a1): (f32, f32), color: Color32, width: f32) {
        let pts: Vec<(f32, f32)> = (0..=16)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / 16.0;
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        self.line(&pts, color, width);
    }

    /// Arrow from `a` to `b` with a filled head.
    fn arrow(&self, a: (f32, f32), b: (f32, f32), color: Color32) {
        self.line(&[a, b], color, 2.2);
        let d = vec2(b.0 - a.0, b.1 - a.1).normalized();
        let n = vec2(-d.y, d.x);
        let s = 0.13;
        let base = (b.0 - d.x * s, b.1 - d.y * s);
        self.poly(
            &[
                b,
                (base.0 + n.x * s * 0.6, base.1 + n.y * s * 0.6),
                (base.0 - n.x * s * 0.6, base.1 - n.y * s * 0.6),
            ],
            color,
        );
    }

    /// An isometric block: front face from (x, y) of size (w, h), depth d up and to the right.
    fn block(&self, x: f32, y: f32, w: f32, h: f32, d: f32) {
        let (dx, dy) = (d * 0.8, -d * 0.6);
        self.poly(&[(x, y), (x + dx, y + dy), (x + w + dx, y + dy), (x + w, y)], SOLID_TOP);
        self.poly(
            &[
                (x + w, y),
                (x + w + dx, y + dy),
                (x + w + dx, y + h + dy),
                (x + w, y + h),
            ],
            SOLID_SIDE,
        );
        self.poly(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], SOLID);
    }

    fn plane(&self, x: f32, y: f32, w: f32, h: f32) {
        let s = h * 0.5;
        self.poly(&[(x + s, y), (x + w, y), (x + w - s, y + h), (x, y + h)], PLANE);
    }

    fn sketch_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.line(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)], BLUE, 1.6);
    }
}

/// Draws icon `name` into `r`.
pub fn paint(p: &Painter, r: Rect, name: &str, enabled: bool) {
    let k = Pen {
        p,
        r,
        fade: if enabled { 1.0 } else { 0.35 },
    };
    match name {
        "extrude" => {
            k.block(0.12, 0.5, 0.5, 0.36, 0.3);
            k.arrow((0.5, 0.38), (0.5, 0.04), GREEN);
        }
        "cut" => {
            k.block(0.08, 0.42, 0.62, 0.44, 0.3);
            k.poly(&[(0.3, 0.42), (0.48, 0.42), (0.6, 0.33), (0.42, 0.33)], INK);
            k.arrow((0.47, 0.02), (0.47, 0.36), RED);
        }
        "revolve" | "revolve_cut" => {
            k.poly(&[(0.42, 0.22), (0.62, 0.22), (0.62, 0.8), (0.42, 0.8)], SOLID);
            k.dashed((0.32, 0.08), (0.32, 0.95), INK);
            k.arc(
                (0.32, 0.5),
                0.28,
                (PI * 0.15, PI * 1.6),
                if name == "revolve" { GREEN } else { RED },
                2.0,
            );
            k.arrow((0.42, 0.24), (0.52, 0.32), if name == "revolve" { GREEN } else { RED });
        }
        "fillet" => {
            k.poly(&[(0.1, 0.9), (0.1, 0.3), (0.42, 0.3), (0.42, 0.9)], SOLID);
            k.poly(&[(0.1, 0.9), (0.42, 0.62), (0.9, 0.62), (0.9, 0.9)], SOLID);
            k.arc((0.62, 0.3), 0.2, (PI * 0.5, PI), BLUE, 2.4);
            k.arc((0.62, 0.3), 0.32, (PI * 0.5, PI), BLUE, 1.0);
        }
        "chamfer" => {
            k.poly(&[(0.1, 0.9), (0.1, 0.35), (0.55, 0.35), (0.9, 0.62), (0.9, 0.9)], SOLID);
            k.line(&[(0.55, 0.35), (0.9, 0.62)], BLUE, 2.4);
        }
        "shell" => {
            k.block(0.1, 0.42, 0.6, 0.46, 0.32);
            k.poly(
                &[(0.2, 0.39), (0.31, 0.31), (0.73, 0.31), (0.62, 0.39)],
                Color32::from_rgb(150, 115, 25),
            );
        }
        "hole" => {
            k.block(0.08, 0.36, 0.66, 0.5, 0.28);
            k.circle(0.41, 0.61, 0.14, Some(Color32::from_rgb(120, 95, 30)), INK);
            k.circle(0.41, 0.61, 0.22, None, INK);
        }
        "draft" => {
            k.poly(&[(0.25, 0.25), (0.75, 0.25), (0.9, 0.85), (0.1, 0.85)], SOLID);
            k.dashed((0.25, 0.25), (0.25, 0.85), BLUE);
            k.arc((0.25, 0.25), 0.4, (PI * 0.42, PI * 0.5), BLUE, 1.6);
        }
        "rib" => {
            k.poly(&[(0.1, 0.9), (0.1, 0.1), (0.3, 0.1), (0.3, 0.9)], SOLID);
            k.poly(&[(0.1, 0.9), (0.3, 0.7), (0.9, 0.7), (0.9, 0.9)], SOLID);
            k.poly(
                &[(0.3, 0.32), (0.3, 0.7), (0.68, 0.7)],
                Color32::from_rgb(120, 190, 120),
            );
        }
        "lpattern" => {
            for i in 0..3 {
                k.block(0.06 + i as f32 * 0.3, 0.55, 0.2, 0.25, 0.12);
            }
            k.arrow((0.1, 0.3), (0.92, 0.3), BLUE);
        }
        "cpattern" => {
            for i in 0..6 {
                let a = TAU * i as f32 / 6.0;
                k.circle(0.5 + 0.34 * a.cos(), 0.5 + 0.34 * a.sin(), 0.1, Some(SOLID), INK);
            }
            k.circle(0.5, 0.5, 0.04, Some(BLUE), BLUE);
        }
        "mirror" => {
            k.dashed((0.5, 0.05), (0.5, 0.95), BLUE);
            k.poly(&[(0.08, 0.8), (0.4, 0.8), (0.4, 0.25)], SOLID);
            k.poly(&[(0.6, 0.8), (0.92, 0.8), (0.6, 0.25)], SOLID_TOP);
        }
        "plane" | "refgeom" => {
            k.plane(0.05, 0.3, 0.9, 0.45);
            if name == "refgeom" {
                k.line(&[(0.5, 0.05), (0.5, 0.95)], RED, 1.6);
            }
        }
        "axis" => {
            k.dashed((0.15, 0.9), (0.85, 0.1), BLUE);
            k.line(&[(0.15, 0.9), (0.85, 0.1)], BLUE, 1.0);
        }
        "point" => k.circle(0.5, 0.5, 0.12, Some(BLUE), INK),
        "csys" | "origin" => {
            k.arrow((0.25, 0.75), (0.9, 0.75), RED);
            k.arrow((0.25, 0.75), (0.25, 0.1), GREEN);
            k.arrow((0.25, 0.75), (0.6, 0.45), BLUE);
        }
        "box" => k.block(0.1, 0.4, 0.55, 0.48, 0.32),
        "cylinder" => {
            k.poly(&[(0.2, 0.25), (0.8, 0.25), (0.8, 0.8), (0.2, 0.8)], SOLID);
            k.p.add(Shape::ellipse_filled(
                k.at(0.5, 0.25),
                vec2(k.r.width() * 0.3, k.r.height() * 0.1),
                k.c(SOLID_TOP),
            ));
            k.p.add(Shape::ellipse_stroke(
                k.at(0.5, 0.25),
                vec2(k.r.width() * 0.3, k.r.height() * 0.1),
                Stroke::new(k.w(1.0), k.c(INK)),
            ));
            k.arc((0.5, 0.8), 0.3, (0.0, PI), INK, 1.0);
        }
        "sketch" => {
            k.sketch_rect(0.1, 0.35, 0.7, 0.88);
            k.line(&[(0.45, 0.6), (0.9, 0.12)], INK, 2.2);
            k.poly(&[(0.45, 0.6), (0.4, 0.68), (0.5, 0.64)], INK);
        }
        "exit_sketch" => {
            k.sketch_rect(0.1, 0.35, 0.6, 0.85);
            k.line(&[(0.45, 0.55), (0.62, 0.75), (0.95, 0.2)], GREEN, 3.0);
        }
        "convert" => {
            k.block(0.05, 0.5, 0.45, 0.38, 0.2);
            k.arrow((0.5, 0.45), (0.85, 0.2), BLUE);
            k.line(&[(0.62, 0.05), (0.95, 0.05)], BLUE, 1.6);
        }
        "measure" => {
            k.poly(&[(0.05, 0.45), (0.95, 0.45), (0.95, 0.72), (0.05, 0.72)], SOLID_TOP);
            for i in 1..9 {
                let x = 0.05 + i as f32 * 0.1;
                k.line(&[(x, 0.45), (x, if i % 2 == 0 { 0.6 } else { 0.53 })], INK, 1.0);
            }
        }
        "mass" => {
            k.poly(
                &[(0.3, 0.35), (0.7, 0.35), (0.9, 0.9), (0.1, 0.9)],
                Color32::from_rgb(110, 115, 125),
            );
            k.circle(0.5, 0.22, 0.12, None, INK);
        }
        "whatswrong" => {
            k.poly(
                &[(0.5, 0.06), (0.95, 0.9), (0.05, 0.9)],
                Color32::from_rgb(250, 200, 40),
            );
            k.line(&[(0.5, 0.35), (0.5, 0.65)], INK, 2.6);
            k.circle(0.5, 0.78, 0.03, Some(INK), INK);
        }
        "rebuild" => {
            k.poly(
                &[(0.32, 0.05), (0.68, 0.05), (0.68, 0.95), (0.32, 0.95)],
                Color32::from_rgb(70, 70, 75),
            );
            k.circle(0.5, 0.22, 0.1, Some(RED), INK);
            k.circle(0.5, 0.5, 0.1, Some(Color32::from_rgb(250, 200, 40)), INK);
            k.circle(0.5, 0.78, 0.1, Some(GREEN), INK);
        }
        "export" => {
            k.poly(&[(0.1, 0.4), (0.7, 0.4), (0.7, 0.9), (0.1, 0.9)], Color32::WHITE);
            k.arrow((0.4, 0.65), (0.92, 0.12), GREEN);
        }
        "slicer" => {
            k.poly(
                &[(0.3, 0.05), (0.7, 0.05), (0.7, 0.4), (0.3, 0.4)],
                Color32::from_rgb(110, 115, 125),
            );
            k.poly(&[(0.38, 0.4), (0.62, 0.4), (0.5, 0.58)], INK);
            k.poly(&[(0.1, 0.78), (0.9, 0.78), (0.9, 0.9), (0.1, 0.9)], SOLID);
            k.line(&[(0.3, 0.7), (0.7, 0.7)], SOLID_SIDE, 2.0);
        }
        "help" => {
            k.circle(0.5, 0.5, 0.42, Some(Color32::WHITE), BLUE);
            k.p.text(
                k.at(0.5, 0.52),
                Align2::CENTER_CENTER,
                "?",
                FontId::proportional(r.height() * 0.7),
                k.c(BLUE),
            );
        }
        // Sketch tools.
        "select" => k.poly(
            &[
                (0.25, 0.1),
                (0.25, 0.85),
                (0.43, 0.65),
                (0.58, 0.95),
                (0.68, 0.9),
                (0.53, 0.6),
                (0.78, 0.6),
            ],
            Color32::WHITE,
        ),
        "dimension" => {
            k.line(&[(0.1, 0.3), (0.1, 0.9)], INK, 1.0);
            k.line(&[(0.9, 0.3), (0.9, 0.9)], INK, 1.0);
            k.arrow((0.5, 0.45), (0.12, 0.45), BLUE);
            k.arrow((0.5, 0.45), (0.88, 0.45), BLUE);
            k.line(&[(0.1, 0.85), (0.9, 0.85)], INK, 1.6);
        }
        "line" => k.line(&[(0.12, 0.85), (0.88, 0.15)], BLUE, 2.0),
        "centerline" => k.dashed((0.12, 0.85), (0.88, 0.15), INK),
        "rect" | "rect_center" | "rect_3pt" => {
            if name == "rect_3pt" {
                k.line(
                    &[(0.1, 0.55), (0.45, 0.15), (0.9, 0.5), (0.55, 0.9), (0.1, 0.55)],
                    BLUE,
                    1.8,
                );
            } else {
                k.sketch_rect(0.1, 0.22, 0.9, 0.8);
            }
            if name == "rect_center" {
                k.circle(0.5, 0.51, 0.04, Some(INK), INK);
            }
        }
        "circle" | "circle_perim" => {
            k.circle(0.5, 0.5, 0.38, None, BLUE);
            if name == "circle" {
                k.circle(0.5, 0.5, 0.04, Some(INK), INK);
            } else {
                k.circle(0.85, 0.36, 0.05, Some(INK), INK);
            }
        }
        "arc" | "arc_tangent" | "arc_3pt" => {
            k.arc((0.5, 0.65), 0.38, (PI, TAU), BLUE, 1.8);
            if name == "arc" {
                k.circle(0.5, 0.65, 0.04, Some(INK), INK);
            }
            if name == "arc_tangent" {
                k.line(&[(0.0, 0.85), (0.12, 0.65)], INK, 1.4);
            }
        }
        "spline" => k.line(
            &(0..=20)
                .map(|i| {
                    let t = i as f32 / 20.0;
                    (0.1 + 0.8 * t, 0.5 - 0.3 * (t * TAU).sin())
                })
                .collect::<Vec<_>>(),
            BLUE,
            1.8,
        ),
        "slot" => {
            k.line(&[(0.3, 0.3), (0.7, 0.3)], BLUE, 1.8);
            k.line(&[(0.3, 0.7), (0.7, 0.7)], BLUE, 1.8);
            k.arc((0.3, 0.5), 0.2, (PI * 0.5, PI * 1.5), BLUE, 1.8);
            k.arc((0.7, 0.5), 0.2, (-PI * 0.5, PI * 0.5), BLUE, 1.8);
        }
        "polygon" => {
            let pts: Vec<(f32, f32)> = (0..=6)
                .map(|i| {
                    let a = TAU * i as f32 / 6.0;
                    (0.5 + 0.4 * a.cos(), 0.5 + 0.4 * a.sin())
                })
                .collect();
            k.line(&pts, BLUE, 1.8);
        }
        "sketch_point" => k.circle(0.5, 0.5, 0.08, Some(BLUE), BLUE),
        "sketch_fillet" => {
            k.line(&[(0.15, 0.9), (0.15, 0.5)], BLUE, 1.8);
            k.arc((0.5, 0.5), 0.35, (PI, PI * 1.5), BLUE, 1.8);
            k.line(&[(0.5, 0.15), (0.9, 0.15)], BLUE, 1.8);
        }
        "sketch_chamfer" => k.line(&[(0.15, 0.9), (0.15, 0.5), (0.5, 0.15), (0.9, 0.15)], BLUE, 1.8),
        "trim" => {
            k.line(&[(0.1, 0.5), (0.9, 0.5)], BLUE, 1.8);
            k.line(&[(0.5, 0.1), (0.5, 0.9)], BLUE, 1.8);
            k.line(&[(0.62, 0.38), (0.85, 0.62)], RED, 2.0);
            k.line(&[(0.85, 0.38), (0.62, 0.62)], RED, 2.0);
        }
        "extend" => {
            k.line(&[(0.85, 0.1), (0.85, 0.9)], BLUE, 1.8);
            k.line(&[(0.1, 0.5), (0.45, 0.5)], BLUE, 1.8);
            k.arrow((0.45, 0.5), (0.8, 0.5), GREEN);
        }
        "offset" => {
            k.sketch_rect(0.1, 0.1, 0.6, 0.6);
            k.line(
                &[(0.25, 0.25), (0.9, 0.25), (0.9, 0.9), (0.25, 0.9), (0.25, 0.25)],
                INK,
                1.0,
            );
        }
        "sketch_mirror" => {
            k.dashed((0.5, 0.05), (0.5, 0.95), INK);
            k.line(&[(0.1, 0.8), (0.4, 0.25)], BLUE, 1.8);
            k.line(&[(0.9, 0.8), (0.6, 0.25)], BLUE, 1.8);
        }
        "sketch_lpattern" => {
            for i in 0..3 {
                k.circle(0.18 + 0.32 * i as f32, 0.55, 0.12, None, BLUE);
            }
        }
        "sketch_cpattern" => {
            for i in 0..5 {
                let a = TAU * i as f32 / 5.0;
                k.circle(0.5 + 0.33 * a.cos(), 0.5 + 0.33 * a.sin(), 0.09, None, BLUE);
            }
        }
        _ => k.circle(0.5, 0.5, 0.12, Some(INK), INK),
    }
}

/// Icon for a feature in the FeatureManager.
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

/// CommandManager button: a large icon over its label.
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
