//! Sketch mode: drawing tools with live previews, inferencing, Smart Dimension, relations,
//! definition-status colours and dragging with live solving.

use std::collections::HashMap;

use eframe::egui::{self, Align2, Color32, FontId, Painter, PointerButton, Pos2, Rect, Response, Shape, Stroke};
use glam::{Vec2 as GVec2, Vec3};
use threebcad_core::FeatureId;
use threebcad_kernel::Plane;
use threebcad_render::OrbitCamera;
use threebcad_sketch::geom::{self, P2};
use threebcad_sketch::inference::{self, Snap};
use threebcad_sketch::tools::Item;
use threebcad_sketch::{Analysis, Constraint, EntityId, PointId, Sketch, Status};

const UNDER: Color32 = Color32::from_rgb(20, 70, 230);
const FULL: Color32 = Color32::from_rgb(15, 15, 20);
const OVER: Color32 = Color32::from_rgb(220, 30, 30);
const HOVER: Color32 = Color32::from_rgb(255, 140, 20);
const SELECTED: Color32 = Color32::from_rgb(20, 160, 255);
const PREVIEW: Color32 = Color32::from_rgb(230, 120, 0);
const INFER: Color32 = Color32::from_rgb(230, 170, 0);
const DIM: Color32 = Color32::from_rgb(40, 40, 50);
const RELATION: Color32 = Color32::from_rgb(20, 120, 60);
/// Snap radius and pick tolerance, screen pixels.
const SNAP_PX: f64 = 8.0;
const PICK_PX: f32 = 7.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Line,
    Centerline,
    CornerRect,
    CenterRect,
    Rect3Pt,
    Circle,
    PerimeterCircle,
    CenterArc,
    TangentArc,
    Arc3Pt,
    Spline,
    Slot,
    CenterSlot,
    Polygon,
    Point,
    Fillet,
    Chamfer,
    Trim,
    Extend,
    Offset,
    Mirror,
    LinearPattern,
    CircularPattern,
    SmartDimension,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Line => "Line",
            Tool::Centerline => "Centerline",
            Tool::CornerRect => "Corner Rectangle",
            Tool::CenterRect => "Center Rectangle",
            Tool::Rect3Pt => "3 Point Rectangle",
            Tool::Circle => "Circle",
            Tool::PerimeterCircle => "Perimeter Circle",
            Tool::CenterArc => "Centerpoint Arc",
            Tool::TangentArc => "Tangent Arc",
            Tool::Arc3Pt => "3 Point Arc",
            Tool::Spline => "Spline",
            Tool::Slot => "Straight Slot",
            Tool::CenterSlot => "Centerpoint Slot",
            Tool::Polygon => "Polygon",
            Tool::Point => "Point",
            Tool::Fillet => "Sketch Fillet",
            Tool::Chamfer => "Sketch Chamfer",
            Tool::Trim => "Trim Entities",
            Tool::Extend => "Extend Entities",
            Tool::Offset => "Offset Entities",
            Tool::Mirror => "Mirror Entities",
            Tool::LinearPattern => "Linear Pattern",
            Tool::CircularPattern => "Circular Pattern",
            Tool::SmartDimension => "Smart Dimension",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Select => "Click to select, drag geometry to move it, double-click a dimension to edit it",
            Tool::Line => "Click points to chain lines; double-click, right-click or Esc ends the chain",
            Tool::Centerline => "Click two points",
            Tool::CornerRect => "Click two opposite corners",
            Tool::CenterRect => "Click the center, then a corner",
            Tool::Rect3Pt => "Click two corners of one side, then the opposite side",
            Tool::Circle => "Click the center, then the radius (or type it in the PropertyManager)",
            Tool::PerimeterCircle => "Click three points on the circle",
            Tool::CenterArc => "Click the center, the start, then the end",
            Tool::TangentArc => "Click the end of a line or arc, then the arc end",
            Tool::Arc3Pt => "Click the start, the end, then a point on the arc",
            Tool::Spline => "Click points; double-click or Enter finishes",
            Tool::Slot => "Click both arc centers, then the width",
            Tool::CenterSlot => "Click the slot center, an arc center, then the width",
            Tool::Polygon => "Click the center, then a vertex (or edge midpoint)",
            Tool::Point => "Click to place a point",
            Tool::Fillet | Tool::Chamfer => "Click a corner where two lines meet",
            Tool::Trim => "Click the piece of an entity to remove",
            Tool::Extend => "Click near the end of a line to extend",
            Tool::Offset | Tool::LinearPattern => "Select entities, set the options, then Apply",
            Tool::Mirror => "Select entities and a centerline (picked last), then Apply",
            Tool::CircularPattern => "Select entities and the center point (picked last), then Apply",
            Tool::SmartDimension => "Click one or two items, then click where the dimension goes",
        }
    }

    /// Clicks that complete a drawing tool; `None` for tools that do not draw by clicking points.
    fn clicks(self) -> Option<usize> {
        match self {
            Tool::Point => Some(1),
            Tool::Line
            | Tool::Centerline
            | Tool::CornerRect
            | Tool::CenterRect
            | Tool::Circle
            | Tool::TangentArc
            | Tool::Polygon => Some(2),
            Tool::Rect3Pt | Tool::PerimeterCircle | Tool::CenterArc | Tool::Arc3Pt | Tool::Slot | Tool::CenterSlot => {
                Some(3)
            }
            Tool::Spline => Some(usize::MAX),
            _ => None,
        }
    }

    pub fn uses_selection(self) -> bool {
        matches!(
            self,
            Tool::Offset | Tool::Mirror | Tool::LinearPattern | Tool::CircularPattern
        )
    }
}

/// Settings shown in the PropertyManager for the active tool.
#[derive(Clone, Debug)]
pub struct ToolOptions {
    pub polygon_sides: usize,
    pub inscribed: bool,
    pub fillet_radius: f64,
    pub chamfer_angle_mode: bool,
    pub chamfer_d1: f64,
    pub chamfer_d2: f64,
    pub chamfer_angle: f64,
    pub offset: f64,
    pub offset_reverse: bool,
    pub pattern_count: usize,
    pub pattern_spacing: f64,
    pub pattern_angle: f64,
    pub circular_count: usize,
    pub circular_total: f64,
    /// Radius typed for the next circle.
    pub radius: Option<f64>,
    pub show_relations: bool,
}

impl Default for ToolOptions {
    fn default() -> Self {
        Self {
            polygon_sides: 6,
            inscribed: true,
            fillet_radius: 2.0,
            chamfer_angle_mode: false,
            chamfer_d1: 2.0,
            chamfer_d2: 2.0,
            chamfer_angle: 45.0,
            offset: 2.0,
            offset_reverse: false,
            pattern_count: 3,
            pattern_spacing: 10.0,
            pattern_angle: 0.0,
            circular_count: 6,
            circular_total: 360.0,
            radius: None,
            show_relations: true,
        }
    }
}

/// Maps between the sketch plane and the viewport.
pub struct View<'a> {
    pub cam: &'a OrbitCamera,
    pub rect: Rect,
    pub plane: Plane,
}

impl View<'_> {
    fn size(&self) -> GVec2 {
        GVec2::new(self.rect.width(), self.rect.height())
    }

    pub fn to_screen(&self, p: P2) -> Option<Pos2> {
        let w = Vec3::from(self.plane.to_world(p).map(|v| v as f32));
        let s = self.cam.project(w, self.size())?;
        Some(self.rect.min + egui::vec2(s.x, s.y))
    }

    /// Where the ray under `pos` meets the sketch plane, in sketch coordinates.
    pub fn to_sketch(&self, pos: Pos2) -> Option<P2> {
        let rel = pos - self.rect.min;
        let (o, d) = self.cam.ray(GVec2::new(rel.x, rel.y), self.size());
        let (o, d) = (o.as_dvec3(), d.as_dvec3());
        let n = glam::DVec3::from(self.plane.normal());
        let origin = glam::DVec3::from(self.plane.origin);
        let denom = d.dot(n);
        if denom.abs() < 1e-9 {
            return None;
        }
        let t = (origin - o).dot(n) / denom;
        (t > 0.0).then(|| self.plane.to_local((o + d * t).to_array()))
    }

    /// Sketch units per screen pixel around `p`.
    pub fn per_pixel(&self, p: P2) -> f64 {
        let w = Vec3::from(self.plane.to_world(p).map(|v| v as f32));
        self.cam.world_per_pixel(w, self.rect.height()) as f64
    }

    fn polyline(&self, pts: &[P2]) -> Vec<Pos2> {
        pts.iter().filter_map(|p| self.to_screen(*p)).collect()
    }
}

struct Drag {
    /// Points being dragged with their offset from the grab point.
    points: Vec<(PointId, P2)>,
    /// Dragging a dimension label instead.
    label: Option<usize>,
}

pub struct SketchSession {
    pub feature: Option<FeatureId>,
    pub plane: Plane,
    pub plane_name: String,
    pub sketch: Sketch,
    original: Sketch,
    undo: Vec<Sketch>,
    redo: Vec<Sketch>,
    pub tool: Tool,
    clicks: Vec<Snap>,
    pub selection: Vec<Item>,
    pub hover: Option<Item>,
    dim_items: Vec<Item>,
    drag: Option<Drag>,
    /// Dimension being edited and its text.
    pub editing: Option<(usize, String)>,
    pub opts: ToolOptions,
    pub analysis: Analysis,
    pub message: Option<String>,
    pub diagnosis: Option<Vec<usize>>,
}

impl SketchSession {
    pub fn new(feature: Option<FeatureId>, sketch: Sketch, plane: Plane, plane_name: String) -> Self {
        let mut s = Self {
            feature,
            plane,
            plane_name,
            original: sketch.clone(),
            sketch,
            undo: Vec::new(),
            redo: Vec::new(),
            tool: Tool::Select,
            clicks: Vec::new(),
            selection: Vec::new(),
            hover: None,
            dim_items: Vec::new(),
            drag: None,
            editing: None,
            opts: ToolOptions::default(),
            analysis: Analysis::default(),
            message: None,
            diagnosis: None,
        };
        s.sketch.solve();
        s.refresh();
        s
    }

    pub fn changed(&self) -> bool {
        self.sketch != self.original
    }

    pub fn checkpoint(&mut self) {
        self.undo.push(self.sketch.clone());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Drops the last checkpoint and restores it (a failed operation).
    fn rollback(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.sketch = prev;
        }
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.sketch, prev));
            self.clicks.clear();
            self.refresh();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.sketch, next));
            self.refresh();
        }
    }

    /// Re-analyzes after any change and drops selections that no longer exist.
    pub fn refresh(&mut self) {
        self.analysis = self.sketch.analyze();
        self.diagnosis = None;
        let sk = &self.sketch;
        let exists = |i: &Item| match *i {
            Item::Point(p) => p.0 < sk.point_count(),
            Item::Entity(e) => e.0 < sk.entities().len(),
            Item::Constraint(c) => c < sk.constraints().len(),
        };
        self.selection.retain(exists);
        self.dim_items.retain(exists);
        if self.hover.is_some_and(|h| !exists(&h)) {
            self.hover = None;
        }
        if self.editing.as_ref().is_some_and(|(i, _)| *i >= sk.constraints().len()) {
            self.editing = None;
        }
    }

    fn solve_and_refresh(&mut self) {
        let report = self.sketch.solve();
        if !report.converged {
            self.message = Some("The relations conflict: the sketch is over-defined".into());
        }
        self.refresh();
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.clicks.clear();
        self.dim_items.clear();
        self.message = None;
        if !tool.uses_selection() && tool != Tool::Select {
            self.selection.clear();
        }
    }

    pub fn escape(&mut self) {
        if self.editing.take().is_some() {
            return;
        }
        if !self.clicks.is_empty() || !self.dim_items.is_empty() {
            self.clicks.clear();
            self.dim_items.clear();
        } else if self.tool != Tool::Select {
            self.set_tool(Tool::Select);
        } else {
            self.selection.clear();
        }
    }

    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let mut points = Vec::new();
        let mut entities = Vec::new();
        let mut constraints = Vec::new();
        for i in &self.selection {
            match *i {
                Item::Point(p) => points.push(p),
                Item::Entity(e) => entities.push(e),
                Item::Constraint(c) => constraints.push(c),
            }
        }
        self.checkpoint();
        self.sketch.remove(&points, &entities, &constraints);
        self.selection.clear();
        self.solve_and_refresh();
    }

    pub fn add_relation(&mut self, c: Constraint) {
        if !self.sketch.accepts(&c) {
            return;
        }
        self.checkpoint();
        self.sketch.constrain(c);
        self.solve_and_refresh();
    }

    pub fn remove_relation(&mut self, i: usize) {
        self.checkpoint();
        self.sketch.remove_constraint(i);
        self.selection.clear();
        self.solve_and_refresh();
    }

    /// Sets a dimension; refuses values that make the sketch unsolvable.
    pub fn set_dimension(&mut self, i: usize, value: f64) {
        self.checkpoint();
        self.sketch.set_dimension(i, value);
        if self.sketch.solve().converged {
            self.message = None;
            self.refresh();
        } else {
            self.rollback();
            self.message = Some("That value cannot be built; the dimension was not changed".into());
        }
    }

    pub fn set_construction(&mut self, on: bool) {
        self.checkpoint();
        for i in &self.selection {
            if let Item::Entity(e) = *i {
                self.sketch.set_construction(e, on);
            }
        }
        self.refresh();
    }

    pub fn run_diagnosis(&mut self) {
        self.diagnosis = Some(self.sketch.diagnose());
    }

    /// Offset / Mirror / Patterns on the current selection.
    pub fn apply_selection_tool(&mut self) {
        let entities: Vec<EntityId> = self
            .selection
            .iter()
            .filter_map(|i| match *i {
                Item::Entity(e) => Some(e),
                _ => None,
            })
            .collect();
        let o = self.opts.clone();
        self.checkpoint();
        let result = match self.tool {
            Tool::Offset => {
                let d = if o.offset_reverse { -o.offset } else { o.offset };
                self.sketch.offset(&entities, d).map(drop)
            }
            Tool::Mirror => match entities.split_last() {
                Some((&axis, rest)) if self.sketch.entity(axis).is_line() => self.sketch.mirror(rest, axis).map(drop),
                _ => Err("Select the entities, then the line to mirror about".to_string()),
            },
            Tool::LinearPattern => self
                .sketch
                .linear_pattern(
                    &entities,
                    o.pattern_count,
                    o.pattern_spacing,
                    o.pattern_angle.to_radians(),
                )
                .map(drop),
            Tool::CircularPattern => match self.selection.last() {
                Some(Item::Point(c)) => self
                    .sketch
                    .circular_pattern(&entities, *c, o.circular_count, o.circular_total.to_radians())
                    .map(drop),
                _ => Err("Select the entities, then the center point".to_string()),
            },
            _ => Ok(()),
        };
        match result {
            Ok(()) => {
                self.selection.clear();
                self.message = None;
                self.solve_and_refresh();
            }
            Err(e) => {
                self.rollback();
                self.message = Some(e);
            }
        }
    }

    pub fn status_text(&self) -> (String, Color32) {
        match self.analysis.status {
            Status::UnderDefined => (format!("Under Defined ({} DOF)", self.analysis.dof), UNDER),
            Status::FullyDefined => ("Fully Defined".into(), FULL),
            Status::OverDefined => ("Over Defined".into(), OVER),
        }
    }

    pub fn describe(&self, item: Item) -> String {
        match item {
            Item::Point(p) => format!("Point{}", p.0 + 1),
            Item::Entity(e) => format!("{}{}", self.sketch.entity(e).kind_name(), e.0 + 1),
            Item::Constraint(i) => self.describe_relation(i),
        }
    }

    pub fn describe_relation(&self, i: usize) -> String {
        let c = &self.sketch.constraints()[i];
        let items: Vec<String> = c
            .points()
            .into_iter()
            .map(|p| self.describe(Item::Point(p)))
            .chain(c.entities().into_iter().map(|e| self.describe(Item::Entity(e))))
            .collect();
        match c.dimension() {
            Some(v) => format!("{} {}  ({})", c.name(), fmt_value(c, v), items.join(", ")),
            None => format!("{}  ({})", c.name(), items.join(", ")),
        }
    }

    /// Relations (not dimensions-only) that involve any selected item.
    pub fn relations_of_selection(&self) -> Vec<usize> {
        let sk = &self.sketch;
        (0..sk.constraints().len())
            .filter(|&i| {
                let c = &sk.constraints()[i];
                !c.is_internal()
                    && self.selection.iter().any(|s| match *s {
                        Item::Point(p) => c.points().contains(&p),
                        Item::Entity(e) => c.entities().contains(&e),
                        Item::Constraint(k) => k == i,
                    })
            })
            .collect()
    }

    // ---------- viewport interaction ----------

    /// What the cursor snaps to, plus dotted inference lines (sketch coordinates).
    fn current_snap(&self, view: &View, cursor: Pos2, ctrl: bool) -> Option<(Snap, Vec<(P2, P2)>)> {
        let c = view.to_sketch(cursor)?;
        let tol = SNAP_PX * view.per_pixel(c);
        if ctrl {
            return Some((Snap::Free(c), Vec::new()));
        }
        let snap = inference::snap(&self.sketch, c, tol, &[]);
        let Snap::Free(mut p) = snap else {
            return Some((snap, Vec::new()));
        };
        let mut lines = Vec::new();
        let segment_tool = matches!(self.tool, Tool::Line | Tool::Centerline | Tool::Slot | Tool::CenterSlot);
        if let (true, Some(last)) = (segment_tool, self.clicks.last()) {
            let from = last.pos(&self.sketch);
            match inference::align(from, p, 3f64.to_radians()) {
                Some(inference::Align::Horizontal) => p[1] = from[1],
                Some(inference::Align::Vertical) => p[0] = from[0],
                None => {}
            }
        }
        let (same_y, same_x) = inference::aligned_with(&self.sketch, p, tol, &[]);
        if let Some(q) = same_y {
            p[1] = self.sketch.point(q)[1];
            lines.push((self.sketch.point(q), p));
        }
        if let Some(q) = same_x {
            p[0] = self.sketch.point(q)[0];
            lines.push((self.sketch.point(q), p));
        }
        Some((Snap::Free(p), lines))
    }

    /// The point, dimension label or entity under the cursor.
    fn hit(&self, view: &View, pos: Pos2) -> Option<Item> {
        let sk = &self.sketch;
        let point = sk
            .point_ids()
            .filter_map(|p| Some((view.to_screen(sk.point(p))?.distance(pos), p)))
            .filter(|(d, _)| *d <= PICK_PX)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, p)) = point {
            return Some(Item::Point(p));
        }
        for i in 0..sk.constraints().len() {
            if let Some(at) = self.label_pos(view, i).and_then(|l| view.to_screen(l)) {
                if Rect::from_center_size(at, egui::vec2(52.0, 18.0)).contains(pos) {
                    return Some(Item::Constraint(i));
                }
            }
        }
        sk.entity_ids()
            .filter_map(|e| Some((screen_distance(&view.polyline(&sk.polyline(e)), pos)?, e)))
            .filter(|(d, _)| *d <= PICK_PX)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, e)| Item::Entity(e))
    }

    /// Handles mouse input; returns true when it consumed a drag (so the camera should not orbit).
    pub fn interact(&mut self, view: &View, resp: &Response, ctx: &egui::Context) -> bool {
        let (ctrl, enter) = ctx.input(|i| (i.modifiers.command, i.key_pressed(egui::Key::Enter)));
        if let Some(pos) = resp.hover_pos() {
            self.hover = self.hit(view, pos);
        } else {
            self.hover = None;
        }
        if self.tool == Tool::Spline && enter && self.clicks.len() >= 2 && self.editing.is_none() {
            self.finish_spline();
        }

        // Dragging geometry or labels with the Select tool.
        if self.tool == Tool::Select && resp.drag_started_by(PointerButton::Primary) {
            let origin = ctx.input(|i| i.pointer.press_origin()).unwrap_or_default();
            let grab = view.to_sketch(origin);
            let target = self.hit(view, origin);
            if let (Some(grab), Some(item)) = (grab, target) {
                let points: Vec<PointId> = match item {
                    Item::Point(p) => vec![p],
                    Item::Entity(e) => self.sketch.entity(e).points(),
                    Item::Constraint(_) => Vec::new(),
                };
                let offsets = points
                    .into_iter()
                    .map(|p| (p, geom::sub(self.sketch.point(p), grab)))
                    .collect();
                let label = match item {
                    Item::Constraint(i) if self.sketch.constraints()[i].dimension().is_some() => Some(i),
                    _ => None,
                };
                self.checkpoint();
                self.drag = Some(Drag { points: offsets, label });
            }
        }
        if let Some(drag) = &self.drag {
            if resp.dragged_by(PointerButton::Primary) {
                if let Some(to) = resp.interact_pointer_pos().and_then(|p| view.to_sketch(p)) {
                    if let Some(i) = drag.label {
                        self.sketch.set_label(i, to);
                    } else {
                        let moves: Vec<(PointId, P2)> =
                            drag.points.iter().map(|&(p, off)| (p, geom::add(to, off))).collect();
                        self.sketch.drag(&moves);
                        self.analysis = self.sketch.analyze();
                    }
                }
            }
            if resp.drag_stopped() {
                self.drag = None;
                self.refresh();
            }
            return true;
        }

        if resp.secondary_clicked() {
            self.clicks.clear();
            self.dim_items.clear();
        }
        if resp.double_clicked() {
            self.double_click(view, resp.interact_pointer_pos());
        } else if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                self.click(view, pos, ctrl);
            }
        }
        false
    }

    fn double_click(&mut self, view: &View, pos: Option<Pos2>) {
        match self.tool {
            Tool::Line => self.clicks.clear(),
            Tool::Spline if self.clicks.len() >= 2 => self.finish_spline(),
            Tool::Select | Tool::SmartDimension => {
                if let Some(Item::Constraint(i)) = pos.and_then(|p| self.hit(view, p)) {
                    self.start_editing(i);
                }
            }
            _ => {}
        }
    }

    pub fn start_editing(&mut self, i: usize) {
        let c = &self.sketch.constraints()[i];
        if let Some(v) = c.dimension() {
            let shown = if c.is_angular() { v.to_degrees() } else { v };
            self.editing = Some((i, num(shown)));
        }
    }

    fn click(&mut self, view: &View, pos: Pos2, ctrl: bool) {
        let hit = self.hit(view, pos);
        let at = view.to_sketch(pos);
        match self.tool {
            Tool::Select | Tool::Offset | Tool::Mirror | Tool::LinearPattern | Tool::CircularPattern => match hit {
                Some(item) if ctrl || self.tool.uses_selection() => {
                    if let Some(k) = self.selection.iter().position(|s| *s == item) {
                        self.selection.remove(k);
                    } else {
                        self.selection.push(item);
                    }
                }
                Some(item) => self.selection = vec![item],
                None if !ctrl => self.selection.clear(),
                None => {}
            },
            Tool::SmartDimension => match hit {
                Some(item @ (Item::Point(_) | Item::Entity(_)))
                    if self.dim_items.len() < 2 && !self.dim_items.contains(&item) =>
                {
                    self.dim_items.push(item);
                }
                _ if !self.dim_items.is_empty() => {
                    if let Some(place) = at {
                        self.place_dimension(place);
                    }
                }
                Some(Item::Constraint(i)) => self.start_editing(i),
                _ => {}
            },
            Tool::Fillet | Tool::Chamfer => {
                let Some(Item::Point(corner)) = hit else { return };
                let o = self.opts.clone();
                self.checkpoint();
                let r = match self.tool {
                    Tool::Fillet => self.sketch.fillet(corner, o.fillet_radius),
                    _ if o.chamfer_angle_mode => {
                        self.sketch
                            .chamfer_angle(corner, o.chamfer_d1, o.chamfer_angle.to_radians())
                    }
                    _ => self.sketch.chamfer(corner, o.chamfer_d1, o.chamfer_d2),
                };
                self.finish_op(r.map(drop));
            }
            Tool::Trim | Tool::Extend => {
                let (Some(Item::Entity(e)), Some(at)) = (hit, at) else {
                    return;
                };
                self.checkpoint();
                let r = if self.tool == Tool::Trim {
                    self.sketch.trim(e, at)
                } else {
                    self.sketch.extend(e, at)
                };
                self.finish_op(r);
            }
            _ => self.click_drawing_tool(view, pos, ctrl),
        }
    }

    fn finish_op(&mut self, r: Result<(), String>) {
        match r {
            Ok(()) => {
                self.message = None;
                self.selection.clear();
                self.solve_and_refresh();
            }
            Err(e) => {
                self.rollback();
                self.message = Some(e);
            }
        }
    }

    fn place_dimension(&mut self, place: P2) {
        let items = std::mem::take(&mut self.dim_items);
        match self.sketch.smart_dimension(&items, place) {
            Some(c) => {
                self.checkpoint();
                let i = self.sketch.constrain(c);
                self.sketch.set_label(i, place);
                self.refresh();
                if self.analysis.status == Status::OverDefined {
                    self.message = Some("This dimension over-defines the sketch; delete it or another relation".into());
                }
                self.start_editing(i);
            }
            None => self.message = Some("Those items cannot be dimensioned together".into()),
        }
    }

    fn click_drawing_tool(&mut self, view: &View, pos: Pos2, ctrl: bool) {
        let Some(needed) = self.tool.clicks() else { return };
        let Some((snap, _)) = self.current_snap(view, pos, ctrl) else {
            return;
        };
        // A double-click's first click lands where the last one did; ignore the duplicate.
        if self
            .clicks
            .last()
            .is_some_and(|l| geom::dist(l.pos(&self.sketch), snap.pos(&self.sketch)) < 1e-9)
        {
            return;
        }
        if self.tool == Tool::TangentArc && self.clicks.is_empty() && !matches!(snap, Snap::Point(_)) {
            self.message = Some("Start a tangent arc on the end of a line or arc".into());
            return;
        }
        self.clicks.push(snap);
        if self.clicks.len() < needed {
            return;
        }
        let snaps = std::mem::take(&mut self.clicks);
        self.checkpoint();
        match build(&mut self.sketch, self.tool, &snaps, &self.opts) {
            Ok(created) => {
                self.opts.radius = None;
                self.message = None;
                self.solve_and_refresh();
                // Lines chain from their end until the chain closes on its first point.
                if self.tool == Tool::Line {
                    if let Some(&e) = created.first() {
                        let (_, end) = self.sketch.entity(e).endpoints().unwrap();
                        let closed = matches!(snaps[1], Snap::Point(_)) && self.sketch.entities_at(end).len() > 1;
                        if !closed {
                            self.clicks = vec![Snap::Point(end)];
                        }
                    }
                }
            }
            Err(e) => {
                self.rollback();
                self.message = Some(e);
            }
        }
    }

    fn finish_spline(&mut self) {
        let snaps = std::mem::take(&mut self.clicks);
        self.checkpoint();
        match build(&mut self.sketch, Tool::Spline, &snaps, &self.opts) {
            Ok(_) => self.solve_and_refresh(),
            Err(e) => {
                self.rollback();
                self.message = Some(e);
            }
        }
    }

    // ---------- drawing ----------

    fn entity_color(&self, e: EntityId) -> Color32 {
        if self.selection.contains(&Item::Entity(e)) {
            SELECTED
        } else if self.hover == Some(Item::Entity(e)) {
            HOVER
        } else if self.analysis.over_entities.get(e.0).copied().unwrap_or(false) {
            OVER
        } else if self.analysis.free_entities.get(e.0).copied().unwrap_or(true) {
            UNDER
        } else {
            FULL
        }
    }

    pub fn paint(&self, painter: &Painter, view: &View, cursor: Option<Pos2>, ctrl: bool) {
        let sk = &self.sketch;
        for e in sk.entity_ids() {
            let color = self.entity_color(e);
            draw_entity(painter, view, sk, e, color, sk.entity(e).construction);
        }
        for p in sk.point_ids() {
            let Some(s) = view.to_screen(sk.point(p)) else { continue };
            let item = Item::Point(p);
            let (color, r) = if self.selection.contains(&item) {
                (SELECTED, 4.5)
            } else if self.hover == Some(item) {
                (HOVER, 4.5)
            } else if self.analysis.free_points.get(p.0).copied().unwrap_or(true) {
                (UNDER, 2.5)
            } else {
                (FULL, 2.5)
            };
            painter.circle_filled(s, r, color);
        }
        if self.opts.show_relations {
            self.paint_relations(painter, view);
        }
        let mut seen_groups = Vec::new();
        for i in 0..sk.constraints().len() {
            let c = &sk.constraints()[i];
            if c.dimension().is_none() {
                continue;
            }
            if let Some(g) = pattern_group(c) {
                if seen_groups.contains(&g) {
                    continue;
                }
                seen_groups.push(g);
            }
            let item = Item::Constraint(i);
            let color = if self.selection.contains(&item) {
                SELECTED
            } else if self.hover == Some(item) {
                HOVER
            } else if self.analysis.over_constraints.get(i).copied().unwrap_or(false) {
                OVER
            } else {
                DIM
            };
            self.paint_dimension(painter, view, i, color);
        }
        for item in &self.dim_items {
            self.highlight(painter, view, *item, SELECTED);
        }
        if let Some(cursor) = cursor {
            self.paint_preview(painter, view, cursor, ctrl);
        }
    }

    fn highlight(&self, painter: &Painter, view: &View, item: Item, color: Color32) {
        match item {
            Item::Point(p) => {
                if let Some(s) = view.to_screen(self.sketch.point(p)) {
                    painter.circle_stroke(s, 6.0, Stroke::new(2.0_f32, color));
                }
            }
            Item::Entity(e) => draw_entity(painter, view, &self.sketch, e, color, false),
            Item::Constraint(_) => {}
        }
    }

    fn paint_preview(&self, painter: &Painter, view: &View, cursor: Pos2, ctrl: bool) {
        let Some((snap, inference_lines)) = self.current_snap(view, cursor, ctrl) else {
            return;
        };
        let needs_points = self.tool.clicks().is_some();
        if needs_points {
            let pos = snap.pos(&self.sketch);
            if let Some(s) = view.to_screen(pos) {
                match snap {
                    Snap::Point(_) => {
                        painter.circle_stroke(s, 7.0, Stroke::new(2.0_f32, INFER));
                    }
                    Snap::Midpoint(_) => {
                        painter.text(
                            s + egui::vec2(9.0, -9.0),
                            Align2::LEFT_BOTTOM,
                            "M",
                            FontId::proportional(12.0),
                            INFER,
                        );
                    }
                    Snap::OnCurve(..) => {
                        painter.text(
                            s + egui::vec2(9.0, -9.0),
                            Align2::LEFT_BOTTOM,
                            "◉",
                            FontId::proportional(12.0),
                            INFER,
                        );
                    }
                    Snap::Free(_) => {}
                }
                for (a, b) in &inference_lines {
                    dashed(painter, &view.polyline(&[*a, *b]), Stroke::new(1.0_f32, INFER));
                }
            }
        }
        let Some(needed) = self.tool.clicks() else { return };
        if self.clicks.is_empty() {
            return;
        }
        let mut snaps = self.clicks.clone();
        snaps.push(snap);
        let stroke = Stroke::new(1.5_f32, PREVIEW);
        let complete = snaps.len() == needed || (self.tool == Tool::Spline && snaps.len() >= 2);
        if complete {
            let mut scratch = self.sketch.clone();
            let before = scratch.entities().len();
            if build(&mut scratch, self.tool, &snaps, &self.opts).is_ok() {
                for e in (before..scratch.entities().len()).map(EntityId) {
                    let pts = view.polyline(&scratch.polyline(e));
                    if scratch.entity(e).construction {
                        dashed(painter, &pts, Stroke::new(1.0_f32, PREVIEW));
                    } else {
                        painter.add(Shape::line(pts, stroke));
                    }
                }
                if self.tool == Tool::Line {
                    if let Some(al) = inference::align(snaps[0].pos(&self.sketch), snap.pos(&self.sketch), 1e-9) {
                        let tag = if al == inference::Align::Horizontal { "—" } else { "|" };
                        painter.text(
                            cursor + egui::vec2(12.0, 12.0),
                            Align2::LEFT_TOP,
                            tag,
                            FontId::proportional(14.0),
                            INFER,
                        );
                    }
                }
                return;
            }
        }
        // Not enough clicks for a shape yet: rubber-band through the clicks.
        let pts: Vec<P2> = snaps.iter().map(|s| s.pos(&self.sketch)).collect();
        dashed(painter, &view.polyline(&pts), stroke);
    }

    fn paint_relations(&self, painter: &Painter, view: &View) {
        let sk = &self.sketch;
        let mut stacked: HashMap<(i32, i32), f32> = HashMap::new();
        for (i, c) in sk.constraints().iter().enumerate() {
            let glyph = c.glyph();
            if glyph.is_empty() || c.dimension().is_some() {
                continue;
            }
            let anchor = match c.entities().first() {
                Some(&e) => {
                    let poly = sk.polyline(e);
                    poly[poly.len() / 2]
                }
                None => sk.point(c.points()[0]),
            };
            let Some(s) = view.to_screen(anchor) else { continue };
            let key = ((s.x / 4.0) as i32, (s.y / 4.0) as i32);
            let k = stacked.entry(key).or_insert(0.0);
            let at = s + egui::vec2(10.0 + *k, -12.0);
            *k += 16.0;
            let color = if self.analysis.over_constraints.get(i).copied().unwrap_or(false) {
                OVER
            } else {
                RELATION
            };
            let rect = Rect::from_center_size(at, egui::vec2(14.0, 14.0));
            painter.rect_filled(rect, 2.0, Color32::from_rgba_unmultiplied(235, 245, 235, 220));
            painter.text(at, Align2::CENTER_CENTER, glyph, FontId::proportional(11.0), color);
        }
    }

    /// The label position of a dimension: stored, or a sensible default.
    fn label_pos(&self, view: &View, i: usize) -> Option<P2> {
        let sk = &self.sketch;
        let c = &sk.constraints()[i];
        c.dimension()?;
        if let Some(l) = sk.label(i) {
            return Some(l);
        }
        let mid = |a: P2, b: P2| geom::lerp(a, b, 0.5);
        Some(match *c {
            Constraint::Distance(p, q, _)
            | Constraint::HorizontalDistance(p, q, _)
            | Constraint::VerticalDistance(p, q, _) => {
                let (a, b) = (sk.point(p), sk.point(q));
                let n = geom::perp(geom::normalize(geom::sub(b, a)));
                geom::add(mid(a, b), geom::scale(n, 18.0 * view.per_pixel(a)))
            }
            Constraint::PointLineDistance(p, ..) => sk.point(p),
            Constraint::Angle(a, ..) => {
                let poly = sk.polyline(a);
                poly[poly.len() / 2]
            }
            Constraint::Radius(e, _) | Constraint::Diameter(e, _) => {
                let c = sk.point(sk.entity(e).center()?);
                geom::add(c, [sk.radius(e) * 1.3, sk.radius(e) * 0.4])
            }
            Constraint::LinearCopy { src, dst, .. } | Constraint::CircularCopy { src, dst, .. } => {
                mid(sk.point(src), sk.point(dst))
            }
            _ => return None,
        })
    }

    fn paint_dimension(&self, painter: &Painter, view: &View, i: usize, color: Color32) {
        let sk = &self.sketch;
        let c = &sk.constraints()[i];
        let (Some(value), Some(label)) = (c.dimension(), self.label_pos(view, i)) else {
            return;
        };
        let thin = Stroke::new(1.0_f32, color);
        let seg = |a: P2, b: P2| {
            if let (Some(a), Some(b)) = (view.to_screen(a), view.to_screen(b)) {
                painter.line_segment([a, b], thin);
            }
        };
        let arrows = |a: P2, b: P2| {
            if let (Some(a), Some(b)) = (view.to_screen(a), view.to_screen(b)) {
                painter.line_segment([a, b], thin);
                arrow(painter, b, a, color);
                arrow(painter, a, b, color);
            }
        };
        match *c {
            Constraint::Distance(p, q, _) => {
                let (a, b) = (sk.point(p), sk.point(q));
                let n = geom::perp(geom::normalize(geom::sub(b, a)));
                let off = geom::dot(geom::sub(label, a), n);
                let (a2, b2) = (geom::add(a, geom::scale(n, off)), geom::add(b, geom::scale(n, off)));
                seg(a, a2);
                seg(b, b2);
                arrows(a2, b2);
            }
            Constraint::HorizontalDistance(p, q, _) => {
                let (a, b) = (sk.point(p), sk.point(q));
                let (a2, b2) = ([a[0], label[1]], [b[0], label[1]]);
                seg(a, a2);
                seg(b, b2);
                arrows(a2, b2);
            }
            Constraint::VerticalDistance(p, q, _) => {
                let (a, b) = (sk.point(p), sk.point(q));
                let (a2, b2) = ([label[0], a[1]], [label[0], b[1]]);
                seg(a, a2);
                seg(b, b2);
                arrows(a2, b2);
            }
            Constraint::PointLineDistance(p, l, _) => {
                let (a, b) = sk.entity(l).endpoints().unwrap();
                let (pa, pb, pp) = (sk.point(a), sk.point(b), sk.point(p));
                let d = geom::sub(pb, pa);
                let t = geom::dot(geom::sub(pp, pa), d) / geom::dot(d, d);
                arrows(pp, geom::lerp(pa, pb, t));
                seg(geom::lerp(pp, geom::lerp(pa, pb, t), 0.5), label);
            }
            Constraint::Angle(l1, l2, _) => {
                let ends = |e: EntityId| {
                    let (a, b) = sk.entity(e).endpoints().unwrap();
                    (sk.point(a), sk.point(b))
                };
                let ((a1, b1), (a2, b2)) = (ends(l1), ends(l2));
                if let Some((t, _)) = geom::line_line(a1, b1, a2, b2) {
                    let v = geom::lerp(a1, b1, t);
                    let r = geom::dist(label, v);
                    let start = geom::angle_of(geom::sub(b1, a1));
                    let pts = geom::arc_points(v, r, start, value, 24);
                    painter.add(Shape::line(view.polyline(&pts), thin));
                }
            }
            Constraint::Radius(e, _) | Constraint::Diameter(e, _) => {
                if let Some(center) = sk.entity(e).center() {
                    let cpt = sk.point(center);
                    let r = sk.radius(e);
                    let dir = geom::normalize(geom::sub(label, cpt));
                    let rim = geom::add(cpt, geom::scale(dir, r));
                    let from = if matches!(c, Constraint::Diameter(..)) {
                        geom::sub(cpt, geom::scale(dir, r))
                    } else {
                        cpt
                    };
                    seg(from, label);
                    if let (Some(a), Some(b)) = (view.to_screen(from), view.to_screen(rim)) {
                        arrow(painter, a, b, color);
                    }
                }
            }
            _ => {}
        }
        if let Some(at) = view.to_screen(label) {
            let text = fmt_value(c, value);
            let galley_rect = painter.text(at, Align2::CENTER_CENTER, &text, FontId::proportional(13.0), color);
            painter.rect_stroke(
                galley_rect.expand(2.0),
                2.0,
                Stroke::new(0.5_f32, color.gamma_multiply(0.4)),
                egui::StrokeKind::Outside,
            );
        }
    }

    /// Inline Modify box for the dimension being edited.
    pub fn editor(&mut self, ctx: &egui::Context, view: &View) {
        let Some((i, mut text)) = self.editing.clone() else {
            return;
        };
        let Some(at) = self.label_pos(view, i).and_then(|l| view.to_screen(l)) else {
            self.editing = None;
            return;
        };
        let mut commit = None;
        egui::Area::new(egui::Id::new("dimension_editor"))
            .fixed_pos(at + egui::vec2(-40.0, 12.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(70.0));
                        if !r.has_focus() && !r.lost_focus() {
                            r.request_focus();
                        }
                        let enter = r.lost_focus() && ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                        if ui.small_button("✔").clicked() || enter {
                            commit = Some(true);
                        }
                        if ui.small_button("✖").clicked() {
                            commit = Some(false);
                        }
                    });
                });
            });
        match commit {
            Some(true) => {
                self.editing = None;
                match text.trim().parse::<f64>() {
                    Ok(v) => {
                        let c = &self.sketch.constraints()[i];
                        let value = if c.is_angular() { v.to_radians() } else { v };
                        if !c.is_angular() && value <= 0.0 {
                            self.message = Some("Lengths must be positive".into());
                        } else {
                            self.set_dimension(i, value);
                        }
                    }
                    Err(_) => self.message = Some(format!("\"{text}\" is not a number")),
                }
            }
            Some(false) => self.editing = None,
            None => self.editing = Some((i, text)),
        }
    }
}

/// Creates the geometry of a drawing tool from its clicks. Used for real and for previews.
fn build(sk: &mut Sketch, tool: Tool, snaps: &[Snap], opts: &ToolOptions) -> Result<Vec<EntityId>, String> {
    let pos: Vec<P2> = snaps.iter().map(|s| s.pos(sk)).collect();
    let too_small = || "Too small: click further apart".to_string();
    let lines = |r: Option<[EntityId; 4]>| r.map(|l| l.to_vec()).ok_or_else(too_small);
    match tool {
        Tool::Point => {
            snaps[0].place(sk);
            Ok(Vec::new())
        }
        Tool::Line | Tool::Centerline => {
            if geom::dist(pos[0], pos[1]) < 1e-9 {
                return Err(too_small());
            }
            let a = snaps[0].place(sk);
            let b = snaps[1].place(sk);
            let l = sk.add_line(a, b);
            sk.set_construction(l, tool == Tool::Centerline);
            match inference::align(pos[0], pos[1], 1e-9) {
                Some(inference::Align::Horizontal) if sk.accepts(&Constraint::Horizontal(l)) => {
                    sk.constrain(Constraint::Horizontal(l));
                }
                Some(inference::Align::Vertical) if sk.accepts(&Constraint::Vertical(l)) => {
                    sk.constrain(Constraint::Vertical(l));
                }
                _ => {}
            }
            Ok(vec![l])
        }
        Tool::CornerRect => {
            let (a, b) = (snaps[0].place(sk), snaps[1].place(sk));
            lines(sk.add_rectangle(a, b))
        }
        Tool::CenterRect => {
            let (c, k) = (snaps[0].place(sk), snaps[1].place(sk));
            lines(sk.add_center_rectangle(c, k))
        }
        Tool::Rect3Pt => {
            let (a, b) = (snaps[0].place(sk), snaps[1].place(sk));
            lines(sk.add_rectangle_3pt(a, b, pos[2]))
        }
        Tool::Circle => {
            let r = opts.radius.unwrap_or_else(|| geom::dist(pos[0], pos[1]));
            if r < 1e-9 {
                return Err(too_small());
            }
            let c = snaps[0].place(sk);
            let e = sk.add_circle(c, r);
            if let Snap::Point(p) = snaps[1] {
                if sk.accepts(&Constraint::PointOnCurve(p, e)) {
                    sk.constrain(Constraint::PointOnCurve(p, e));
                }
            }
            Ok(vec![e])
        }
        Tool::PerimeterCircle => sk
            .add_circle_3pt(pos[0], pos[1], pos[2])
            .map(|e| vec![e])
            .ok_or_else(|| "The three points are in a line".into()),
        Tool::CenterArc => {
            let ccw = geom::cross(geom::sub(pos[1], pos[0]), geom::sub(pos[2], pos[0])) >= 0.0;
            let (c, s) = (snaps[0].place(sk), snaps[1].place(sk));
            sk.add_center_arc(c, s, pos[2], ccw)
                .map(|e| vec![e])
                .ok_or_else(too_small)
        }
        Tool::TangentArc => {
            let Snap::Point(from) = snaps[0] else {
                return Err("Start a tangent arc on the end of a line or arc".into());
            };
            let end = snaps[1].place(sk);
            sk.add_tangent_arc(from, end)
                .map(|e| vec![e])
                .ok_or_else(|| "Start on the end of a line or arc, and end off its tangent".into())
        }
        Tool::Arc3Pt => {
            let (a, b) = (snaps[0].place(sk), snaps[1].place(sk));
            sk.add_arc_3pt(a, b, pos[2])
                .map(|e| vec![e])
                .ok_or_else(|| "The three points are in a line".into())
        }
        Tool::Spline => {
            if snaps.len() < 2 {
                return Err("A spline needs two points".into());
            }
            let pts: Vec<PointId> = snaps.iter().map(|s| s.place(sk)).collect();
            Ok(vec![sk.add_spline(pts)])
        }
        Tool::Slot | Tool::CenterSlot => {
            let (a, b) = (snaps[0].place(sk), snaps[1].place(sk));
            let r = if tool == Tool::Slot {
                sk.add_slot(a, b, pos[2])
            } else {
                sk.add_center_slot(a, b, pos[2])
            };
            lines(r)
        }
        Tool::Polygon => {
            let c = snaps[0].place(sk);
            sk.add_polygon(c, pos[1], opts.polygon_sides, opts.inscribed)
                .ok_or_else(too_small)
        }
        _ => Ok(Vec::new()),
    }
}

fn pattern_group(c: &Constraint) -> Option<u32> {
    match *c {
        Constraint::LinearCopy { group, .. } | Constraint::CircularCopy { group, .. } => Some(group),
        _ => None,
    }
}

/// Number without trailing zeros, at most 3 decimals.
pub fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn fmt_value(c: &Constraint, v: f64) -> String {
    match c {
        Constraint::Angle(..) | Constraint::CircularCopy { .. } => format!("{}°", num(v.to_degrees())),
        Constraint::Radius(..) => format!("R{}", num(v)),
        Constraint::Diameter(..) => format!("⌀{}", num(v)),
        _ => num(v),
    }
}

fn screen_distance(poly: &[Pos2], p: Pos2) -> Option<f32> {
    poly.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let t = if ab.length_sq() > 0.0 {
                ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (a + ab * t).distance(p)
        })
        .min_by(|a, b| a.total_cmp(b))
}

fn dashed(painter: &Painter, pts: &[Pos2], stroke: Stroke) {
    if pts.len() >= 2 {
        painter.extend(Shape::dashed_line(pts, stroke, 6.0, 4.0));
    }
}

fn arrow(painter: &Painter, from: Pos2, tip: Pos2, color: Color32) {
    let dir = (tip - from).normalized();
    if !dir.x.is_finite() {
        return;
    }
    let side = egui::vec2(-dir.y, dir.x);
    let base = tip - dir * 8.0;
    painter.add(Shape::convex_polygon(
        vec![tip, base + side * 3.0, base - side * 3.0],
        color,
        Stroke::NONE,
    ));
}

/// Draws one sketch entity (construction geometry dashed).
pub fn draw_entity(painter: &Painter, view: &View, sk: &Sketch, e: EntityId, color: Color32, construction: bool) {
    let pts = view.polyline(&sk.polyline(e));
    if construction {
        dashed(painter, &pts, Stroke::new(1.2_f32, color));
    } else {
        painter.add(Shape::line(pts, Stroke::new(2.0_f32, color)));
    }
}

/// Lines of a sketch shown outside sketch mode (not yet used by a feature).
pub fn draw_sketch(painter: &Painter, view: &View, sk: &Sketch, color: Color32) {
    for e in sk.entity_ids() {
        let construction = sk.entity(e).construction;
        draw_entity(painter, view, sk, e, color, construction);
    }
}

/// Screen distance from `pos` to the nearest curve of a sketch.
pub fn distance_to_sketch(view: &View, sk: &Sketch, pos: Pos2) -> Option<f32> {
    sk.entity_ids()
        .filter_map(|e| screen_distance(&view.polyline(&sk.polyline(e)), pos))
        .min_by(|a, b| a.total_cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(p: P2) -> Snap {
        Snap::Free(p)
    }

    #[test]
    fn build_creates_each_tool_shape() {
        let opts = ToolOptions::default();
        let cases: &[(Tool, &[P2], usize)] = &[
            (Tool::Line, &[[0.0, 0.0], [5.0, 0.0]], 1),
            (Tool::Centerline, &[[0.0, 0.0], [5.0, 5.0]], 1),
            (Tool::CornerRect, &[[0.0, 0.0], [5.0, 3.0]], 4),
            (Tool::CenterRect, &[[0.0, 0.0], [5.0, 3.0]], 6),
            (Tool::Rect3Pt, &[[0.0, 0.0], [5.0, 1.0], [4.0, 5.0]], 4),
            (Tool::Circle, &[[0.0, 0.0], [5.0, 0.0]], 1),
            (Tool::PerimeterCircle, &[[1.0, 0.0], [0.0, 1.0], [-1.0, 0.0]], 1),
            (Tool::CenterArc, &[[0.0, 0.0], [2.0, 0.0], [0.0, 3.0]], 1),
            (Tool::Arc3Pt, &[[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0]], 1),
            (Tool::Spline, &[[0.0, 0.0], [1.0, 1.0], [2.0, 0.0]], 1),
            (Tool::Slot, &[[0.0, 0.0], [5.0, 0.0], [2.0, 1.0]], 5),
            (Tool::CenterSlot, &[[0.0, 0.0], [5.0, 0.0], [2.0, 1.0]], 5),
            (Tool::Polygon, &[[0.0, 0.0], [5.0, 0.0]], 7),
        ];
        for &(tool, pts, entities) in cases {
            let mut sk = Sketch::default();
            let snaps: Vec<Snap> = pts.iter().map(|&p| snap(p)).collect();
            build(&mut sk, tool, &snaps, &opts).unwrap_or_else(|e| panic!("{tool:?}: {e}"));
            assert_eq!(sk.entities().len(), entities, "{tool:?}");
            assert!(sk.solve().converged, "{tool:?}");
        }
    }

    #[test]
    fn horizontal_line_gets_a_relation_and_degenerate_input_errors() {
        let mut sk = Sketch::default();
        build(
            &mut sk,
            Tool::Line,
            &[snap([0.0, 0.0]), snap([5.0, 0.0])],
            &ToolOptions::default(),
        )
        .unwrap();
        assert_eq!(sk.constraints(), &[Constraint::Horizontal(EntityId(0))]);
        assert!(build(
            &mut sk,
            Tool::Line,
            &[snap([1.0, 1.0]), snap([1.0, 1.0])],
            &ToolOptions::default()
        )
        .is_err());
        assert!(build(
            &mut sk,
            Tool::PerimeterCircle,
            &[snap([0.0, 0.0]), snap([1.0, 1.0]), snap([2.0, 2.0])],
            &ToolOptions::default()
        )
        .is_err());
    }

    #[test]
    fn session_undo_dimension_edit_and_delete() {
        let mut s = SketchSession::new(None, Sketch::rectangle(10.0, 5.0), Plane::TOP, "Top Plane".into());
        assert_eq!(s.analysis.status, Status::FullyDefined);
        s.set_dimension(5, 20.0);
        assert!((s.sketch.point(PointId(2))[0] - 20.0).abs() < 1e-6);
        s.undo();
        assert!((s.sketch.point(PointId(2))[0] - 10.0).abs() < 1e-6);
        s.redo();
        assert!(s.changed());
        s.selection = vec![Item::Constraint(6)];
        s.delete_selection();
        assert_eq!(s.analysis.status, Status::UnderDefined);
    }

    #[test]
    fn impossible_dimension_is_refused() {
        // A 3-4-5 triangle cannot get a 20 mm side.
        let mut sk = Sketch::default();
        let p: Vec<PointId> = [[0.0, 0.0], [3.0, 0.0], [3.0, 4.0]]
            .iter()
            .map(|q| sk.add_point(q[0], q[1]))
            .collect();
        let l = sk.add_line(p[0], p[1]);
        sk.add_line(p[1], p[2]);
        sk.add_line(p[2], p[0]);
        sk.constrain(Constraint::Fix(p[0], [0.0, 0.0]));
        sk.constrain(Constraint::Horizontal(l));
        let ab = sk.constrain(Constraint::Distance(p[0], p[1], 3.0));
        sk.constrain(Constraint::Distance(p[1], p[2], 4.0));
        sk.constrain(Constraint::Distance(p[2], p[0], 5.0));
        let mut s = SketchSession::new(None, sk, Plane::TOP, "Top Plane".into());
        let before = s.sketch.clone();
        s.set_dimension(ab, 20.0);
        assert_eq!(s.sketch, before);
        assert!(s.message.is_some());
    }

    #[test]
    fn numbers_format_cleanly() {
        assert_eq!(num(10.0), "10");
        assert_eq!(num(2.5), "2.5");
        assert_eq!(num(1.23456), "1.235");
        assert_eq!(fmt_value(&Constraint::Diameter(EntityId(0), 0.0), 5.0), "⌀5");
    }
}
