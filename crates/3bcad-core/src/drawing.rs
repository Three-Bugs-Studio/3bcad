//! Drawings: sheets with a title block, views of a part with hidden lines removed, section and
//! detail views, dimensions and notes, laid out as pages for the screen and for PDF, SVG and DXF.
//!
//! A drawing file only says which part it shows and how; the part stays in its own file and is
//! read again whenever it changes on disk, so the views always show the part as it is.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{bail, ensure, Context};
use serde::{Deserialize, Serialize};
use threebcad_io::drawing::{text_width, Align, Layer, Mark, Page, Shape, P2};
use threebcad_kernel::hlr::{self, Projection};
use threebcad_kernel::topology::{cross, dot, EdgeCurve, P3};
use threebcad_kernel::Plane;
use threebcad_sketch::{Constraint, EntityId, PointId};

use crate::assembly::{modified, relative, stem};
use crate::document::property;
use crate::naming::{self, EdgeRef, VertexRef};
use crate::{End, FeatureId, FeatureKind, PartDocument, Units};

const FORMAT: &str = "3bcad-drawing";
pub const FORMAT_VERSION: u64 = 1;
/// Drawing files sit next to the `.3bcad` part they show.
pub const EXT: &str = "3bcdrw";
const UNDO_LIMIT: usize = 100;
/// Sheet edge to border (mm).
const MARGIN: f64 = 10.0;
/// Text capital heights (mm): dimensions, notes and captions; title block labels.
pub const TEXT: f64 = 3.5;
const SMALL: f64 = 2.0;
const ARROW: f64 = 3.0;
/// How far section lines and centre marks run past what they mark (paper mm).
const OVERRUN: f64 = 3.0;
/// Title block cells (mm).
const CELL: P2 = [45.0, 9.0];
const COLUMNS: usize = 4;
/// Room between a new view and the one it comes from (paper mm).
const GAP: f64 = 20.0;

/// ISO A sizes and ANSI sizes, landscape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SheetSize {
    A4,
    A3,
    A2,
    A1,
    A0,
    AnsiA,
    AnsiB,
    AnsiC,
    AnsiD,
    AnsiE,
}

impl SheetSize {
    pub const ALL: [SheetSize; 10] = [
        SheetSize::A4,
        SheetSize::A3,
        SheetSize::A2,
        SheetSize::A1,
        SheetSize::A0,
        SheetSize::AnsiA,
        SheetSize::AnsiB,
        SheetSize::AnsiC,
        SheetSize::AnsiD,
        SheetSize::AnsiE,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SheetSize::A4 => "A4",
            SheetSize::A3 => "A3",
            SheetSize::A2 => "A2",
            SheetSize::A1 => "A1",
            SheetSize::A0 => "A0",
            SheetSize::AnsiA => "ANSI A",
            SheetSize::AnsiB => "ANSI B",
            SheetSize::AnsiC => "ANSI C",
            SheetSize::AnsiD => "ANSI D",
            SheetSize::AnsiE => "ANSI E",
        }
    }

    /// Width and height in mm, landscape (ISO 216; ANSI Y14.1 in inches).
    pub fn size(self) -> P2 {
        let inch = |w: f64, h: f64| [w * 25.4, h * 25.4];
        match self {
            SheetSize::A4 => [297.0, 210.0],
            SheetSize::A3 => [420.0, 297.0],
            SheetSize::A2 => [594.0, 420.0],
            SheetSize::A1 => [841.0, 594.0],
            SheetSize::A0 => [1189.0, 841.0],
            SheetSize::AnsiA => inch(11.0, 8.5),
            SheetSize::AnsiB => inch(17.0, 11.0),
            SheetSize::AnsiC => inch(22.0, 17.0),
            SheetSize::AnsiD => inch(34.0, 22.0),
            SheetSize::AnsiE => inch(44.0, 34.0),
        }
    }
}

/// The side a model view looks at the part from (Z up: Front looks along +Y).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    Front,
    Back,
    Top,
    Bottom,
    Left,
    Right,
    Isometric,
}

impl Orientation {
    pub const ALL: [Orientation; 7] = [
        Orientation::Front,
        Orientation::Back,
        Orientation::Top,
        Orientation::Bottom,
        Orientation::Left,
        Orientation::Right,
        Orientation::Isometric,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Orientation::Front => "Front",
            Orientation::Back => "Back",
            Orientation::Top => "Top",
            Orientation::Bottom => "Bottom",
            Orientation::Left => "Left",
            Orientation::Right => "Right",
            Orientation::Isometric => "Isometric",
        }
    }

    /// x is right on paper, y up, the normal points at the viewer.
    pub fn plane(self) -> Plane {
        let frame = |x: P3, y: P3| Plane { origin: [0.0; 3], x, y };
        match self {
            Orientation::Front => frame([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            Orientation::Back => frame([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            Orientation::Top => frame([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            Orientation::Bottom => frame([1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
            Orientation::Left => frame([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
            Orientation::Right => frame([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            // From front-right-top: the front face to the left, the right face to the right.
            Orientation::Isometric => {
                let s2 = std::f64::consts::FRAC_1_SQRT_2;
                let s6 = 1.0 / 6f64.sqrt();
                frame([s2, s2, 0.0], [-s6, s6, 2.0 * s6])
            }
        }
    }
}

/// Where a projected view sits from its parent on the sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::Left, Dir::Right, Dir::Up, Dir::Down];

    pub fn name(self) -> &'static str {
        match self {
            Dir::Left => "Left",
            Dir::Right => "Right",
            Dir::Up => "Above",
            Dir::Down => "Below",
        }
    }

    fn offset(self) -> P2 {
        match self {
            Dir::Left => [-1.0, 0.0],
            Dir::Right => [1.0, 0.0],
            Dir::Up => [0.0, 1.0],
            Dir::Down => [0.0, -1.0],
        }
    }

    fn horizontal(self) -> bool {
        matches!(self, Dir::Left | Dir::Right)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ViewKind {
    /// The part seen from one side.
    Model(Orientation),
    /// The parent seen from the side it is placed on (third angle) or from the opposite side
    /// (first angle), lined up with it.
    Projected { parent: u32, dir: Dir },
    /// The parent cut along a vertical or horizontal line `at` (model mm, across the parent),
    /// seen from the right (or above), or from the other side with `flip`.
    Section {
        parent: u32,
        label: String,
        vertical: bool,
        at: f64,
        flip: bool,
    },
    /// A circle of the parent (model mm, in the parent's coordinates), drawn larger.
    Detail {
        parent: u32,
        label: String,
        center: P2,
        radius: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    /// Kept when other views are removed, so children and dimensions keep pointing at it.
    pub id: u32,
    pub kind: ViewKind,
    /// Where the part's origin (a detail's centre) lands on the sheet (mm). Projected and section
    /// views take the coordinate that lines them up from their parent.
    pub at: P2,
    /// Paper mm per model mm; `None` uses the sheet's (projected and section views always use
    /// their parent's, a detail twice its parent's).
    pub scale: Option<f64>,
    /// Hidden lines drawn dashed (section views never show them).
    pub hidden: bool,
    /// Centre marks on circles and centrelines on cylinders.
    pub marks: bool,
}

/// A point of the part a dimension or leader holds on to, so it follows the part's changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Anchor {
    Vertex(VertexRef),
    /// A circular edge: its centre, and its radius for diameter and radius dimensions.
    Circle(EdgeRef),
    /// A sketch point and a sketch circle or arc, for dimensions brought in from the model.
    SketchPoint {
        sketch: FeatureId,
        point: usize,
    },
    SketchCircle {
        sketch: FeatureId,
        entity: usize,
    },
    /// The start (`end` false) or end of an extrusion, on its sketch's first point.
    Extrude {
        feature: FeatureId,
        end: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DimKind {
    Horizontal,
    Vertical,
    Aligned,
    Diameter,
    Radius,
}

impl DimKind {
    pub const LINEAR: [DimKind; 3] = [DimKind::Horizontal, DimKind::Vertical, DimKind::Aligned];

    pub fn name(self) -> &'static str {
        match self {
            DimKind::Horizontal => "Horizontal",
            DimKind::Vertical => "Vertical",
            DimKind::Aligned => "Aligned",
            DimKind::Diameter => "Diameter",
            DimKind::Radius => "Radius",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    pub view: u32,
    pub kind: DimKind,
    pub a: Anchor,
    /// The second point of a linear dimension.
    #[serde(default)]
    pub b: Option<Anchor>,
    /// Where its text sits, relative to the view's origin on the sheet (mm).
    pub at: P2,
    /// Brought in from the part's sketches and features, not measured off the view.
    #[serde(default)]
    pub model: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub text: String,
    /// Where the text starts on the sheet (mm): the first line's baseline.
    pub at: P2,
    /// What the leader points at: a view and a point in its coordinates (model mm).
    #[serde(default)]
    pub leader: Option<(u32, P2)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    pub name: String,
    pub size: SheetSize,
    /// Paper mm per model mm (0.5 is 1:2).
    pub scale: f64,
    /// Third-angle projection (ANSI), else first angle (ISO).
    pub third_angle: bool,
    pub views: Vec<View>,
    pub dims: Vec<Dimension>,
    pub notes: Vec<Note>,
}

impl Sheet {
    fn new(name: String) -> Self {
        Self {
            name,
            size: SheetSize::A3,
            scale: 1.0,
            third_angle: false,
            views: Vec::new(),
            dims: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn view(&self, id: u32) -> Option<&View> {
        self.views.iter().find(|v| v.id == id)
    }
}

/// What an edit changes, and what undo brings back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Contents {
    /// The part the views show: absolute while open, saved relative to the drawing.
    pub model: Option<PathBuf>,
    pub sheets: Vec<Sheet>,
    /// Title block cells: a label and its text, where `{Name}` is filled in from the drawing's or
    /// the part's properties, or `{Sheet}`, `{Scale}`, `{Projection}`, `{Model}`, `{Material}`.
    pub title_block: Vec<(String, String)>,
    pub next_id: u32,
}

impl Default for Contents {
    fn default() -> Self {
        let cell = |l: &str, t: &str| (l.to_string(), t.to_string());
        Self {
            model: None,
            sheets: vec![Sheet::new("Sheet1".into())],
            title_block: vec![
                cell("Title", "{Model}"),
                cell("Part number", "{Part number}"),
                cell("Material", "{Material}"),
                cell("Drawn by", "{Drawn by}"),
                cell("Date", "{Date}"),
                cell("Scale", "{Scale}"),
                cell("Projection", "{Projection}"),
                cell("Sheet", "{Sheet}"),
            ],
            next_id: 0,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct DrawingFile {
    format: String,
    version: u64,
    name: String,
    units: Units,
    #[serde(default)]
    properties: Vec<(String, String)>,
    contents: Contents,
}

struct Loaded {
    part: Result<Box<PartDocument>, String>,
    modified: Option<SystemTime>,
    /// Bumps on every read, so projections of the old part are not reused.
    generation: u64,
}

/// Section hatching after ANSI Y14.2, chosen from the part's material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hatch {
    /// ANSI31: iron, aluminium, wood and anything else.
    General,
    /// ANSI32: steel.
    Steel,
    /// ANSI33: brass, bronze, copper.
    Copper,
    /// ANSI37: plastics and rubber.
    Plastic,
}

impl Hatch {
    pub fn of(material: &str) -> Hatch {
        let m = material.to_lowercase();
        let has = |words: &[&str]| words.iter().any(|w| m.contains(w));
        if has(&["steel", "titanium"]) {
            Hatch::Steel
        } else if has(&["brass", "bronze", "copper"]) {
            Hatch::Copper
        } else if has(&["alumin", "iron", "magnesium", "zinc", "metal", "oak", "wood"]) {
            Hatch::General
        } else {
            Hatch::Plastic
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Hatch::General => "ANSI31 (iron, general)",
            Hatch::Steel => "ANSI32 (steel)",
            Hatch::Copper => "ANSI33 (brass, bronze, copper)",
            Hatch::Plastic => "ANSI37 (plastic, rubber)",
        }
    }

    /// Line families: angle (degrees), spacing (paper mm), offset, dashed.
    fn families(self) -> &'static [(f64, f64, f64, bool)] {
        match self {
            Hatch::General => &[(45.0, 2.5, 0.0, false)],
            Hatch::Steel => &[(45.0, 3.0, 0.0, false), (45.0, 3.0, 0.6, false)],
            Hatch::Copper => &[(45.0, 3.0, 0.0, false), (45.0, 3.0, 1.5, true)],
            Hatch::Plastic => &[(45.0, 2.5, 0.0, false), (135.0, 2.5, 0.0, false)],
        }
    }
}

/// A view as drawn now: geometry in view coordinates (model mm) and where it lands on paper.
#[derive(Clone)]
struct Drawn {
    frame: Plane,
    scale: f64,
    /// Paper position of `pivot`.
    at: P2,
    pivot: P2,
    lines: Vec<hlr::Line>,
    cut: Vec<[P2; 2]>,
    /// A detail's circle: only what is inside shows.
    clip: Option<(P2, f64)>,
    lo: P2,
    hi: P2,
}

impl Drawn {
    fn paper(&self, p: P2) -> P2 {
        [
            self.at[0] + self.scale * (p[0] - self.pivot[0]),
            self.at[1] + self.scale * (p[1] - self.pivot[1]),
        ]
    }

    fn model(&self, q: P2) -> P2 {
        [
            self.pivot[0] + (q[0] - self.at[0]) / self.scale,
            self.pivot[1] + (q[1] - self.at[1]) / self.scale,
        ]
    }

    fn shows(&self, p: P2) -> bool {
        self.clip.is_none_or(|(c, r)| dist(p, c) <= r + 1e-9)
    }

    fn to_view(&self, p: P3) -> P2 {
        let q = hlr::to_view(&self.frame, p);
        [q[0], q[1]]
    }
}

/// Something picked on a view.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Vertex(VertexRef),
    Circle(EdgeRef),
    /// A straight edge, by its end points.
    Edge(VertexRef, VertexRef),
}

pub struct DrawingDocument {
    pub name: String,
    path: Option<PathBuf>,
    units: Units,
    properties: Vec<(String, String)>,
    contents: Contents,
    undo: Vec<Contents>,
    redo: Vec<Contents>,
    merge_key: Option<u64>,
    dirty: bool,
    part: Option<Loaded>,
    generation: u64,
    /// Hidden-line projections by what they were made from.
    cache: HashMap<String, Projection>,
    drawn: HashMap<u32, Drawn>,
    revision: u64,
}

fn add(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub2(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn mul(a: P2, s: f64) -> P2 {
    [a[0] * s, a[1] * s]
}

fn dist(a: P2, b: P2) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn unit2(a: P2) -> P2 {
    let l = a[0].hypot(a[1]).max(1e-12);
    [a[0] / l, a[1] / l]
}

/// "1:2", "2:1", "1:2.5".
pub fn scale_text(s: f64) -> String {
    let t = |v: f64| {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    if s >= 1.0 {
        format!("{}:1", t(s))
    } else {
        format!("1:{}", t(1.0 / s))
    }
}

/// The view of a parent from the side `dir` (third angle) or the opposite side (first angle).
fn projected(parent: &Plane, dir: Dir, third: bool) -> Plane {
    let [dx, dy] = dir.offset();
    let toward: P3 = std::array::from_fn(|i| parent.x[i] * dx + parent.y[i] * dy);
    let out = if third { toward } else { toward.map(|v| -v) };
    if dir.horizontal() {
        Plane {
            origin: [0.0; 3],
            x: cross(parent.y, out),
            y: parent.y,
        }
    } else {
        Plane {
            origin: [0.0; 3],
            x: parent.x,
            y: cross(out, parent.x),
        }
    }
}

fn section_dir(vertical: bool, flip: bool) -> Dir {
    match (vertical, flip) {
        (true, false) => Dir::Right,
        (true, true) => Dir::Left,
        (false, false) => Dir::Up,
        (false, true) => Dir::Down,
    }
}

/// The parts of a polyline inside a circle.
fn clip_circle(pts: &[P2], c: P2, r: f64) -> Vec<Vec<P2>> {
    let mut runs: Vec<Vec<P2>> = Vec::new();
    let mut open = false;
    for w in pts.windows(2) {
        let (a, d) = (w[0], sub2(w[1], w[0]));
        let f = sub2(a, c);
        let (qa, qb, qc) = (dot2(d, d), 2.0 * dot2(f, d), dot2(f, f) - r * r);
        let disc = qb * qb - 4.0 * qa * qc;
        if qa < 1e-18 || disc <= 0.0 {
            open = false;
            continue;
        }
        let s = disc.sqrt();
        let (t0, t1) = (((-qb - s) / (2.0 * qa)).max(0.0), ((-qb + s) / (2.0 * qa)).min(1.0));
        if t0 >= t1 {
            open = false;
            continue;
        }
        let (p0, p1) = (add(a, mul(d, t0)), add(a, mul(d, t1)));
        if !(open && t0 == 0.0) {
            runs.push(vec![p0]);
        }
        runs.last_mut().unwrap().push(p1);
        open = t1 == 1.0;
    }
    runs
}

fn dot2(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// Hatch lines (paper mm) filling the region the segments outline (even-odd), optionally only
/// inside a circle.
fn hatch(segments: &[[P2; 2]], pattern: Hatch, clip: Option<(P2, f64)>) -> Vec<Vec<P2>> {
    let mut out = Vec::new();
    for &(deg, spacing, offset, dashed) in pattern.families() {
        let (s, c) = deg.to_radians().sin_cos();
        // Turn the region so this family's lines run along x.
        let segs: Vec<[P2; 2]> = segments
            .iter()
            .map(|seg| seg.map(|p| [p[0] * c + p[1] * s, -p[0] * s + p[1] * c]))
            .collect();
        let (lo, hi) = segs
            .iter()
            .flatten()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p[1]), hi.max(p[1]))
            });
        if lo > hi {
            continue;
        }
        let mut y = ((lo - offset) / spacing).ceil() * spacing + offset;
        while y <= hi {
            let mut xs: Vec<f64> = segs
                .iter()
                .filter(|[a, b]| (a[1] > y) != (b[1] > y))
                .map(|[a, b]| a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]))
                .collect();
            xs.sort_by(f64::total_cmp);
            for pair in xs.chunks_exact(2) {
                let back = |x: f64| [x * c - y * s, x * s + y * c];
                let line = vec![back(pair[0]), back(pair[1])];
                let pieces = match clip {
                    Some((cc, r)) => clip_circle(&line, cc, r),
                    None => vec![line],
                };
                for p in pieces {
                    if dashed {
                        out.extend(dashes(p[0], p[1], 2.0, 1.0));
                    } else {
                        out.push(p);
                    }
                }
            }
            y += spacing;
        }
    }
    out
}

fn dashes(a: P2, b: P2, on: f64, off: f64) -> Vec<Vec<P2>> {
    let len = dist(a, b);
    let d = unit2(sub2(b, a));
    let mut out = Vec::new();
    let mut t = 0.0;
    while t < len {
        out.push(vec![add(a, mul(d, t)), add(a, mul(d, (t + on).min(len)))]);
        t += on + off;
    }
    out
}

/// A filled arrowhead with its tip at `tip`, pointing along `d`.
fn arrow(tip: P2, d: P2) -> Shape {
    let d = unit2(d);
    let n = [-d[1], d[0]];
    let base = sub2(tip, mul(d, ARROW));
    Shape::Fill(vec![
        tip,
        add(base, mul(n, ARROW / 6.0)),
        sub2(base, mul(n, ARROW / 6.0)),
    ])
}

struct Marks(Vec<Mark>);

impl Marks {
    fn line(&mut self, layer: Layer, pts: Vec<P2>) {
        if pts.len() >= 2 {
            self.0.push(Mark {
                layer,
                shape: Shape::Line(pts),
            });
        }
    }

    fn shape(&mut self, layer: Layer, shape: Shape) {
        self.0.push(Mark { layer, shape });
    }

    fn text(&mut self, layer: Layer, at: P2, height: f64, text: impl Into<String>, align: Align) {
        let text = text.into();
        if !text.is_empty() {
            self.shape(
                layer,
                Shape::Text {
                    at,
                    height,
                    text,
                    align,
                },
            );
        }
    }

    fn circle(&mut self, layer: Layer, c: P2, r: f64) {
        let pts = (0..=64)
            .map(|k| {
                let t = k as f64 / 64.0 * std::f64::consts::TAU;
                [c[0] + r * t.cos(), c[1] + r * t.sin()]
            })
            .collect();
        self.line(layer, pts);
    }
}

impl DrawingDocument {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: None,
            units: Units::default(),
            properties: Vec::new(),
            contents: Contents::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            merge_key: None,
            dirty: false,
            part: None,
            generation: 0,
            cache: HashMap::new(),
            drawn: HashMap::new(),
            revision: 0,
        }
    }

    /// The drawing as JSON, with the part's file relative to the folder `base` (absolute when
    /// there is none: autosave copies live elsewhere).
    pub fn to_json(&self, base: Option<&Path>) -> String {
        let mut contents = self.contents.clone();
        if let (Some(base), Some(m)) = (base, &mut contents.model) {
            *m = relative(m, base);
        }
        let file = DrawingFile {
            format: FORMAT.into(),
            version: FORMAT_VERSION,
            name: self.name.clone(),
            units: self.units,
            properties: self.properties.clone(),
            contents,
        };
        serde_json::to_string_pretty(&file).expect("a drawing always serialises")
    }

    /// Reads drawing JSON whose relative part file starts at the folder `base`, and the part.
    pub fn from_json(text: &str, base: Option<&Path>) -> anyhow::Result<Self> {
        let value: serde_json::Value = serde_json::from_str(text).context("the file is not valid JSON")?;
        ensure!(
            value.get("format").and_then(|f| f.as_str()) == Some(FORMAT),
            "this is not a 3BCAD drawing file"
        );
        let version = value
            .get("version")
            .and_then(|v| v.as_u64())
            .context("the file has no format version")?;
        ensure!(
            version <= FORMAT_VERSION,
            "this drawing was saved by a newer 3BCAD (format version {version}); this version reads up to {FORMAT_VERSION}. Update 3BCAD to open it"
        );
        let file: DrawingFile = serde_json::from_value(value).context("the drawing file is damaged")?;
        validate(&file.contents)?;
        let mut doc = Self::new(file.name);
        doc.units = file.units;
        doc.properties = file.properties;
        doc.contents = file.contents;
        if let (Some(base), Some(m)) = (base, &mut doc.contents.model) {
            if m.is_relative() {
                *m = base.join(&*m);
            }
        }
        doc.load();
        doc.update();
        Ok(doc)
    }

    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let path = std::path::absolute(path)?;
        let text = std::fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let mut doc = Self::from_json(&text, path.parent())?;
        doc.path = Some(path);
        Ok(doc)
    }

    /// Writes the drawing next to `path` first and then moves it into place, like a part.
    pub fn save(&mut self, path: &Path) -> anyhow::Result<()> {
        let path = std::path::absolute(path)?;
        self.write(&path, path.parent())?;
        self.path = Some(path);
        self.dirty = false;
        Ok(())
    }

    /// Saves a copy without marking the drawing saved (autosave), with an absolute part file.
    pub fn write_to(&self, path: &Path) -> anyhow::Result<()> {
        self.write(path, None)
    }

    fn write(&self, path: &Path, base: Option<&Path>) -> anyhow::Result<()> {
        let tmp = path.with_extension("3bcdrw.tmp");
        std::fs::write(&tmp, self.to_json(base)).with_context(|| format!("cannot write {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn units(&self) -> Units {
        self.units
    }

    pub fn set_units(&mut self, units: Units) {
        if units != self.units {
            self.dirty = true;
            self.units = units;
            self.revision += 1;
        }
    }

    pub fn properties(&self) -> &[(String, String)] {
        &self.properties
    }

    pub fn set_properties(&mut self, properties: Vec<(String, String)>) {
        if properties != self.properties {
            self.dirty = true;
            self.properties = properties;
            self.revision += 1;
        }
    }

    pub fn contents(&self) -> &Contents {
        &self.contents
    }

    /// Bumps whenever what the sheets show changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The part the views show, once it has been read.
    pub fn part(&self) -> Option<&PartDocument> {
        self.part.as_ref()?.part.as_deref().ok()
    }

    /// Why the part could not be read.
    pub fn part_error(&self) -> Option<&str> {
        self.part.as_ref()?.part.as_ref().err().map(String::as_str)
    }

    fn load(&mut self) {
        self.part = self.contents.model.as_ref().map(|path| {
            self.generation += 1;
            Loaded {
                part: PartDocument::open(path)
                    .map(|mut p| {
                        p.name = stem(path);
                        Box::new(p)
                    })
                    .map_err(|e| format!("{e:#}")),
                modified: modified(path),
                generation: self.generation,
            }
        });
    }

    /// Reads the part again if its file changed on disk; true when it did.
    pub fn refresh(&mut self) -> bool {
        let (Some(path), Some(l)) = (&self.contents.model, &self.part) else {
            return false;
        };
        if modified(path) == l.modified {
            return false;
        }
        self.load();
        self.update();
        true
    }

    /// Shows the part in `file`; the views of the old one now show the new one.
    pub fn set_model(&mut self, file: &Path) -> anyhow::Result<()> {
        let file = std::path::absolute(file)?;
        let part = PartDocument::open(&file).with_context(|| format!("cannot use {}", file.display()))?;
        drop(part);
        self.edit(None, |c| c.model = Some(file));
        Ok(())
    }

    /// Makes an edit one undo step (or part of the last one, for the same `key`) and draws again.
    pub fn edit<R>(&mut self, key: Option<u64>, f: impl FnOnce(&mut Contents) -> R) -> R {
        if key.is_none() || key != self.merge_key {
            self.undo.push(self.contents.clone());
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.merge_key = key;
        self.dirty = true;
        let model = self.contents.model.clone();
        let r = f(&mut self.contents);
        if self.contents.model != model {
            self.load();
        }
        self.update();
        r
    }

    /// Ends a run of merged edits (a drag), so the next one is its own undo step.
    pub fn end_merge(&mut self) {
        self.merge_key = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            let now = std::mem::replace(&mut self.contents, prev);
            self.after_history(now.model.clone());
            self.redo.push(now);
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            let now = std::mem::replace(&mut self.contents, next);
            self.after_history(now.model.clone());
            self.undo.push(now);
        }
    }

    fn after_history(&mut self, model_before: Option<PathBuf>) {
        self.merge_key = None;
        self.dirty = true;
        if self.contents.model != model_before {
            self.load();
        }
        self.update();
    }

    fn new_id(c: &mut Contents) -> u32 {
        c.next_id += 1;
        c.next_id - 1
    }

    pub fn add_sheet(&mut self) -> usize {
        self.edit(None, |c| {
            let n = (1..)
                .map(|i| format!("Sheet{i}"))
                .find(|n| c.sheets.iter().all(|s| &s.name != n))
                .unwrap();
            let mut s = Sheet::new(n);
            if let Some(first) = c.sheets.first() {
                (s.size, s.scale, s.third_angle) = (first.size, first.scale, first.third_angle);
            }
            c.sheets.push(s);
            c.sheets.len() - 1
        })
    }

    /// Removes a sheet with its views; the last one stays.
    pub fn remove_sheet(&mut self, sheet: usize) {
        if self.contents.sheets.len() > 1 && sheet < self.contents.sheets.len() {
            self.edit(None, |c| {
                c.sheets.remove(sheet);
            });
        }
    }

    /// Adds a view of the part from one side, its middle at `center` on the sheet.
    pub fn add_model_view(&mut self, sheet: usize, o: Orientation, center: P2) -> Option<u32> {
        let hidden = o != Orientation::Isometric;
        let id = self.add_view(sheet, ViewKind::Model(o), hidden, None)?;
        let (lo, hi) = self.view_rect(id)?;
        self.shift(sheet, id, sub2(center, mul(add(lo, hi), 0.5)));
        Some(id)
    }

    fn add_view(&mut self, sheet: usize, kind: ViewKind, hidden: bool, scale: Option<f64>) -> Option<u32> {
        (sheet < self.contents.sheets.len()).then_some(())?;
        Some(self.edit(None, |c| {
            let id = Self::new_id(c);
            c.sheets[sheet].views.push(View {
                id,
                kind,
                at: [0.0, 0.0],
                scale,
                hidden,
                marks: true,
            });
            id
        }))
    }

    /// Moves a view on the sheet, as part of the edit that made it.
    fn shift(&mut self, sheet: usize, id: u32, delta: P2) {
        if let Some(v) = self.contents.sheets[sheet].views.iter_mut().find(|v| v.id == id) {
            v.at = add(v.at, delta);
        }
        self.update();
    }

    /// Moves a new view out from `parent` towards `dir`, `GAP` clear of it.
    fn set_beside(&mut self, sheet: usize, id: u32, parent: u32, dir: Dir) {
        let (Some((plo, phi)), Some((lo, hi))) = (self.view_rect(parent), self.view_rect(id)) else {
            return;
        };
        let (pmid, mid) = (mul(add(plo, phi), 0.5), mul(add(lo, hi), 0.5));
        let delta = match dir {
            Dir::Right => [phi[0] + GAP - lo[0], pmid[1] - mid[1]],
            Dir::Left => [plo[0] - GAP - hi[0], pmid[1] - mid[1]],
            Dir::Up => [pmid[0] - mid[0], phi[1] + GAP - lo[1]],
            Dir::Down => [pmid[0] - mid[0], plo[1] - GAP - hi[1]],
        };
        self.shift(sheet, id, delta);
    }

    /// The front view and two views projected from it, set out on the sheet.
    pub fn standard_views(&mut self, sheet: usize) -> Option<[u32; 3]> {
        let s = self.contents.sheets.get(sheet)?;
        let [w, h] = s.size.size();
        let third = s.third_angle;
        let (front_at, below) = if third {
            ([w * 0.3, h * 0.35], false)
        } else {
            ([w * 0.3, h * 0.65], true)
        };
        let front = self.add_model_view(sheet, Orientation::Front, front_at)?;
        let side = self.add_projected(sheet, front, Dir::Right)?;
        let top = self.add_projected(sheet, front, if below { Dir::Down } else { Dir::Up })?;
        Some([front, top, side])
    }

    /// A view projected from `parent` and placed beside it towards `dir`.
    pub fn add_projected(&mut self, sheet: usize, parent: u32, dir: Dir) -> Option<u32> {
        let p = self.contents.sheets.get(sheet)?.view(parent)?;
        if matches!(p.kind, ViewKind::Detail { .. }) {
            return None;
        }
        let hidden = p.hidden;
        let id = self.add_view(sheet, ViewKind::Projected { parent, dir }, hidden, None)?;
        self.set_beside(sheet, id, parent, dir);
        Some(id)
    }

    /// A section of `parent` along a line through `through` (view coordinates).
    pub fn add_section(&mut self, sheet: usize, parent: u32, vertical: bool, through: P2) -> Option<u32> {
        let s = self.contents.sheets.get(sheet)?;
        let used: Vec<String> = s
            .views
            .iter()
            .filter_map(|v| match &v.kind {
                ViewKind::Section { label, .. } | ViewKind::Detail { label, .. } => Some(label.clone()),
                _ => None,
            })
            .collect();
        if matches!(s.view(parent)?.kind, ViewKind::Detail { .. }) {
            return None;
        }
        let label = next_label(&used);
        let kind = ViewKind::Section {
            parent,
            label,
            vertical,
            at: if vertical { through[0] } else { through[1] },
            flip: false,
        };
        let id = self.add_view(sheet, kind, false, None)?;
        self.set_beside(sheet, id, parent, section_dir(vertical, false));
        Some(id)
    }

    /// A detail of the circle at `center` (view coordinates) on `parent`, twice as large, placed
    /// to the right of the parent.
    pub fn add_detail(&mut self, sheet: usize, parent: u32, center: P2, radius: f64) -> Option<u32> {
        let s = self.contents.sheets.get(sheet)?;
        let used: Vec<String> = s
            .views
            .iter()
            .filter_map(|v| match &v.kind {
                ViewKind::Section { label, .. } | ViewKind::Detail { label, .. } => Some(label.clone()),
                _ => None,
            })
            .collect();
        let p = s.view(parent)?;
        if matches!(p.kind, ViewKind::Detail { .. }) || !(radius.is_finite() && radius > 0.0) {
            return None;
        }
        let hidden = p.hidden;
        let label = next_label(&used);
        let scale = self.drawn.get(&parent)?.scale * 2.0;
        let kind = ViewKind::Detail {
            parent,
            label,
            center,
            radius,
        };
        let id = self.add_view(sheet, kind, hidden, Some(scale))?;
        self.set_beside(sheet, id, parent, Dir::Right);
        Some(id)
    }

    /// Removes a view, the views made from it, their dimensions and the leaders on them.
    pub fn remove_view(&mut self, sheet: usize, id: u32) {
        let Some(s) = self.contents.sheets.get(sheet) else {
            return;
        };
        let mut gone = vec![id];
        for v in &s.views {
            if parent_of(&v.kind).is_some_and(|p| gone.contains(&p)) {
                gone.push(v.id);
            }
        }
        self.edit(None, |c| {
            let s = &mut c.sheets[sheet];
            s.views.retain(|v| !gone.contains(&v.id));
            s.dims.retain(|d| !gone.contains(&d.view));
            for n in &mut s.notes {
                if n.leader.is_some_and(|(v, _)| gone.contains(&v)) {
                    n.leader = None;
                }
            }
        });
    }

    /// The view's frame, scale and paper placement, from its parent's.
    fn place(sheet: &Sheet, v: &View, drawn: &HashMap<u32, Drawn>) -> Option<(Plane, f64, P2, P2, Option<f64>)> {
        let parent = parent_of(&v.kind).map(|p| drawn.get(&p));
        Some(match &v.kind {
            ViewKind::Model(o) => (o.plane(), v.scale.unwrap_or(sheet.scale), v.at, [0.0; 2], None),
            ViewKind::Projected { dir, .. } => {
                let p = parent??;
                let frame = projected(&p.frame, *dir, sheet.third_angle);
                (frame, p.scale, lined_up(v.at, p, dir.horizontal()), [0.0; 2], None)
            }
            ViewKind::Section { vertical, at, flip, .. } => {
                let p = parent??;
                let dir = section_dir(*vertical, *flip);
                let frame = projected(&p.frame, dir, sheet.third_angle);
                let axis = if *vertical { p.frame.x } else { p.frame.y };
                let depth = dot(axis.map(|a| a * at), frame.normal());
                (
                    frame,
                    p.scale,
                    lined_up(v.at, p, dir.horizontal()),
                    [0.0; 2],
                    Some(depth),
                )
            }
            ViewKind::Detail { center, .. } => {
                let p = parent??;
                (p.frame, v.scale.unwrap_or(p.scale * 2.0), v.at, *center, None)
            }
        })
    }

    /// Projects every view again (reusing projections that did not change) and bumps the revision.
    fn update(&mut self) {
        let part = self.part.as_ref().and_then(|l| l.part.as_deref().ok());
        let generation = self.part.as_ref().map_or(0, |l| l.generation);
        let topos: Vec<&threebcad_kernel::topology::Topology> = part
            .map(|p| p.bodies().iter().map(|b| b.topo.as_ref()).collect())
            .unwrap_or_default();
        let mut drawn: HashMap<u32, Drawn> = HashMap::new();
        let mut used = HashSet::new();
        for sheet in &self.contents.sheets {
            for v in &sheet.views {
                let Some((frame, scale, at, pivot, cut)) = Self::place(sheet, v, &drawn) else {
                    continue;
                };
                let (lines, cut_segs, clip) = if let ViewKind::Detail {
                    parent, center, radius, ..
                } = &v.kind
                {
                    let p = &drawn[parent];
                    let lines = p
                        .lines
                        .iter()
                        .flat_map(|l| {
                            clip_circle(&l.points, *center, *radius)
                                .into_iter()
                                .map(|points| hlr::Line { points, ..l.clone() })
                        })
                        .collect();
                    (lines, p.cut.clone(), Some((*center, *radius)))
                } else {
                    let key = format!("{generation} {:?} {:?} {:?}", frame.x, frame.y, cut);
                    let proj = self
                        .cache
                        .entry(key.clone())
                        .or_insert_with(|| hlr::project(&topos, &frame, cut));
                    used.insert(key);
                    (proj.lines.clone(), proj.cut.clone(), None)
                };
                let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
                for p in lines.iter().flat_map(|l| &l.points) {
                    for i in 0..2 {
                        lo[i] = lo[i].min(p[i]);
                        hi[i] = hi[i].max(p[i]);
                    }
                }
                if let Some((c, r)) = clip {
                    (lo, hi) = (sub2(c, [r, r]), add(c, [r, r]));
                }
                if lo[0] > hi[0] {
                    (lo, hi) = ([-10.0, -10.0], [10.0, 10.0]);
                }
                drawn.insert(
                    v.id,
                    Drawn {
                        frame,
                        scale,
                        at,
                        pivot,
                        lines,
                        cut: cut_segs,
                        clip,
                        lo,
                        hi,
                    },
                );
            }
        }
        self.cache.retain(|k, _| used.contains(k));
        self.drawn = drawn;
        self.revision += 1;
    }

    /// A view's corners on the sheet (mm).
    pub fn view_rect(&self, id: u32) -> Option<(P2, P2)> {
        let d = self.drawn.get(&id)?;
        Some((d.paper(d.lo), d.paper(d.hi)))
    }

    /// The paper scale a view is drawn at.
    pub fn view_scale(&self, id: u32) -> Option<f64> {
        self.drawn.get(&id).map(|d| d.scale)
    }

    /// Where the view's origin (a detail's centre) is on the sheet: dimensions sit relative to it.
    pub fn view_origin(&self, id: u32) -> Option<P2> {
        self.drawn.get(&id).map(|d| d.at)
    }

    /// A sheet point in a view's coordinates (model mm), and back.
    pub fn to_view(&self, id: u32, p: P2) -> Option<P2> {
        self.drawn.get(&id).map(|d| d.model(p))
    }

    pub fn to_sheet(&self, id: u32, p: P2) -> Option<P2> {
        self.drawn.get(&id).map(|d| d.paper(p))
    }

    /// The topmost view under a sheet point.
    pub fn view_at(&self, sheet: usize, p: P2) -> Option<u32> {
        let s = self.contents.sheets.get(sheet)?;
        s.views.iter().rev().map(|v| v.id).find(|&id| {
            self.view_rect(id).is_some_and(|(lo, hi)| {
                (lo[0] - 2.0..=hi[0] + 2.0).contains(&p[0]) && (lo[1] - 2.0..=hi[1] + 2.0).contains(&p[1])
            })
        })
    }

    /// The vertex, circle or straight edge of the part nearest a sheet point (within `tol` mm on
    /// paper), on the view under it.
    pub fn pick(&self, sheet: usize, p: P2, tol: f64) -> Option<(u32, Pick)> {
        let view = self.view_at(sheet, p)?;
        let d = self.drawn.get(&view)?;
        let part = self.part()?;
        let q = d.model(p);
        let tol = tol / d.scale;
        let bodies = part.bodies();
        let mut best: Option<(f64, Pick)> = None;
        let take = |dd: f64, pick: Option<Pick>, best: &mut Option<(f64, Pick)>| {
            if dd <= tol && best.as_ref().is_none_or(|b| dd < b.0) {
                if let Some(pick) = pick {
                    *best = Some((dd, pick));
                }
            }
        };
        for (bi, b) in bodies.iter().enumerate() {
            for (vi, &v) in b.topo.vertices.iter().enumerate() {
                let at = d.to_view(v);
                if d.shows(at) {
                    take(dist(at, q), part.vertex_ref(bi, vi).map(Pick::Vertex), &mut best);
                }
            }
        }
        if best.is_some() {
            return Some((view, best?.1));
        }
        for (bi, b) in bodies.iter().enumerate() {
            for (ei, e) in b.topo.edges.iter().enumerate() {
                if let Some((c, r)) = hlr::circle_in_view(&e.curve, &d.frame) {
                    let on = add(c, mul(unit2(sub2(q, c)), r));
                    if d.shows(on) {
                        take(
                            (dist(q, c) - r).abs(),
                            part.edge_ref(bi, ei).map(Pick::Circle),
                            &mut best,
                        );
                    }
                } else if let EdgeCurve::Line { .. } = e.curve {
                    let [a, bb] = e.vertices.map(|v| d.to_view(b.topo.vertices[v]));
                    let ab = sub2(bb, a);
                    let t = (dot2(sub2(q, a), ab) / dot2(ab, ab).max(1e-18)).clamp(0.0, 1.0);
                    let on = add(a, mul(ab, t));
                    if dist(a, bb) > 1e-6 && d.shows(on) {
                        let refs = (part.vertex_ref(bi, e.vertices[0]), part.vertex_ref(bi, e.vertices[1]));
                        let pick = match refs {
                            (Some(x), Some(y)) => Some(Pick::Edge(x, y)),
                            _ => None,
                        };
                        take(dist(q, on), pick, &mut best);
                    }
                }
            }
        }
        best.map(|(_, p)| (view, p))
    }

    /// Adds a dimension measured off a view: between two points, an edge's length, or a circle's
    /// diameter (radius for an arc); its text goes at the sheet point `text`.
    pub fn add_dimension(&mut self, sheet: usize, view: u32, picks: &[Pick], text: P2) -> Option<usize> {
        let origin = self.view_origin(view)?;
        let (kind, a, b) = match picks {
            [Pick::Circle(e)] => {
                let kind = if self.part().and_then(|p| full_circle(p, e)) == Some(false) {
                    DimKind::Radius
                } else {
                    DimKind::Diameter
                };
                (kind, Anchor::Circle(e.clone()), None)
            }
            [Pick::Edge(x, y)] => (
                DimKind::Aligned,
                Anchor::Vertex(x.clone()),
                Some(Anchor::Vertex(y.clone())),
            ),
            [x, y] => {
                let anchor = |p: &Pick| match p {
                    Pick::Vertex(v) => Some(Anchor::Vertex(v.clone())),
                    Pick::Circle(e) => Some(Anchor::Circle(e.clone())),
                    Pick::Edge(..) => None,
                };
                (DimKind::Aligned, anchor(x)?, Some(anchor(y)?))
            }
            _ => return None,
        };
        let mut dim = Dimension {
            view,
            kind,
            a,
            b,
            at: sub2(text, origin),
            model: false,
        };
        self.level(&mut dim);
        Some(self.edit(None, |c| {
            c.sheets[sheet].dims.push(dim);
            c.sheets[sheet].dims.len() - 1
        }))
    }

    /// An aligned dimension between points level with each other becomes horizontal or vertical.
    fn level(&self, dim: &mut Dimension) {
        if dim.kind != DimKind::Aligned {
            return;
        }
        if let (Some(pa), Some(pb)) = (self.anchor_point(dim, false), self.anchor_point(dim, true)) {
            if (pa[1] - pb[1]).abs() < 1e-6 {
                dim.kind = DimKind::Horizontal;
            } else if (pa[0] - pb[0]).abs() < 1e-6 {
                dim.kind = DimKind::Vertical;
            }
        }
    }

    /// Brings the part's sketch dimensions (onto views that see their sketch face on) and
    /// extrusion depths (onto views that see them side on) to the sheet; returns how many.
    pub fn import_model_dimensions(&mut self, sheet: usize) -> usize {
        let Some(part) = self.part() else { return 0 };
        let Some(s) = self.contents.sheets.get(sheet) else {
            return 0;
        };
        // Views that can carry dimensions, in sheet order.
        let views: Vec<(u32, Plane)> = s
            .views
            .iter()
            .filter(|v| match &v.kind {
                ViewKind::Model(o) => *o != Orientation::Isometric,
                ViewKind::Projected { .. } => true,
                _ => false,
            })
            .filter_map(|v| Some((v.id, self.drawn.get(&v.id)?.frame)))
            .collect();
        let mut new: Vec<Dimension> = Vec::new();
        for (id, f) in part.history().iter() {
            if f.suppressed || !part.history().is_active(id) {
                continue;
            }
            match &f.kind {
                FeatureKind::Sketch { sketch, plane, .. } => {
                    let n = plane.normal();
                    let Some(&(view, frame)) = views.iter().find(|(_, fr)| dot(fr.normal(), n).abs() > 0.9999) else {
                        continue;
                    };
                    let along = |axis: P3| {
                        if dot(axis, frame.x).abs() > 0.9999 {
                            DimKind::Horizontal
                        } else if dot(axis, frame.y).abs() > 0.9999 {
                            DimKind::Vertical
                        } else {
                            DimKind::Aligned
                        }
                    };
                    let point = |p: &PointId| Anchor::SketchPoint { sketch: id, point: p.0 };
                    let circle = |e: &EntityId| Anchor::SketchCircle {
                        sketch: id,
                        entity: e.0,
                    };
                    for c in sketch.constraints() {
                        let (kind, a, b) = match c {
                            Constraint::Distance(p, q, _) => (DimKind::Aligned, point(p), Some(point(q))),
                            Constraint::HorizontalDistance(p, q, _) => (along(plane.x), point(p), Some(point(q))),
                            Constraint::VerticalDistance(p, q, _) => (along(plane.y), point(p), Some(point(q))),
                            Constraint::Radius(e, _) => (DimKind::Radius, circle(e), None),
                            Constraint::Diameter(e, _) => (DimKind::Diameter, circle(e), None),
                            _ => continue,
                        };
                        new.push(Dimension {
                            view,
                            kind,
                            a,
                            b,
                            at: [0.0; 2],
                            model: true,
                        });
                    }
                }
                FeatureKind::Extrude(e) if matches!(e.end, End::Blind(_) | End::MidPlane(_)) => {
                    let Some(FeatureKind::Sketch { plane, .. }) = part.history().get(e.sketch).map(|f| &f.kind) else {
                        continue;
                    };
                    let n = plane.normal();
                    let Some(&(view, frame)) = views.iter().find(|(_, fr)| dot(fr.normal(), n).abs() < 1e-6) else {
                        continue;
                    };
                    let kind = if dot(n, frame.x).abs() > 0.9999 {
                        DimKind::Horizontal
                    } else if dot(n, frame.y).abs() > 0.9999 {
                        DimKind::Vertical
                    } else {
                        DimKind::Aligned
                    };
                    let at = |end| Anchor::Extrude { feature: id, end };
                    new.push(Dimension {
                        view,
                        kind,
                        a: at(false),
                        b: Some(at(true)),
                        at: [0.0; 2],
                        model: true,
                    });
                }
                _ => {}
            }
        }
        let have = &s.dims;
        new.retain(|d| !have.iter().any(|h| h.view == d.view && h.a == d.a && h.b == d.b));
        let mut placed = Vec::new();
        let mut stack: HashMap<(u32, u8), usize> = HashMap::new();
        for mut d in new {
            let (Some(pa), Some(dw)) = (self.anchor_point(&d, false), self.drawn.get(&d.view)) else {
                continue;
            };
            self.level(&mut d);
            let pb = self.anchor_point(&d, true).unwrap_or(pa);
            let hi = dw.paper(dw.hi);
            let (pa, pb) = (dw.paper(pa), dw.paper(pb));
            let mid = mul(add(pa, pb), 0.5);
            let side = match d.kind {
                DimKind::Horizontal => 0,
                DimKind::Vertical => 1,
                _ => 2,
            };
            let k = stack.entry((d.view, side)).or_default();
            let step = 8.0 * (*k as f64 + 1.0);
            *k += 1;
            let text = match d.kind {
                DimKind::Horizontal => [mid[0], hi[1] + step],
                DimKind::Vertical => [hi[0] + step, mid[1]],
                DimKind::Aligned => [mid[0] + step, mid[1] + step],
                DimKind::Diameter | DimKind::Radius => {
                    let r = self.anchor_radius(&d).unwrap_or(0.0) * dw.scale;
                    let s2 = std::f64::consts::FRAC_1_SQRT_2;
                    add(pa, mul([s2, s2], r + step))
                }
            };
            d.at = sub2(text, dw.at);
            placed.push(d);
        }
        let count = placed.len();
        if count > 0 {
            self.edit(None, |c| c.sheets[sheet].dims.extend(placed));
        }
        count
    }

    /// Where an anchor of a dimension is, in its view's coordinates (`second`: its `b`).
    fn anchor_point(&self, d: &Dimension, second: bool) -> Option<P2> {
        let a = if second { d.b.as_ref()? } else { &d.a };
        self.anchor(d.view, a).map(|(p, _)| p)
    }

    fn anchor_radius(&self, d: &Dimension) -> Option<f64> {
        self.anchor(d.view, &d.a)?.1
    }

    /// An anchor in a view's coordinates, with the radius of a circle.
    fn anchor(&self, view: u32, a: &Anchor) -> Option<(P2, Option<f64>)> {
        let part = self.part()?;
        let dw = self.drawn.get(&view)?;
        let sketch_of = |id: FeatureId| match &part.history().get(id)?.kind {
            FeatureKind::Sketch { sketch, plane, .. } => Some((sketch, plane)),
            _ => None,
        };
        match a {
            Anchor::Vertex(r) => {
                let ((b, v), _) = naming::find_vertex(part.bodies(), r)?;
                Some((dw.to_view(part.bodies()[b].topo.vertices[v]), None))
            }
            Anchor::Circle(r) => {
                let (b, e) = part.resolve_edge(r)?;
                let (c, rad) = hlr::circle_in_view(&part.bodies()[b].topo.edges[e].curve, &dw.frame)?;
                Some((c, Some(rad)))
            }
            Anchor::SketchPoint { sketch, point } => {
                let (s, plane) = sketch_of(*sketch)?;
                (*point < s.point_count()).then_some(())?;
                Some((dw.to_view(plane.to_world(s.point(PointId(*point)))), None))
            }
            Anchor::SketchCircle { sketch, entity } => {
                let (s, plane) = sketch_of(*sketch)?;
                let e = s.entities().get(*entity)?;
                let c = e.center()?;
                let r = s.radius(EntityId(*entity));
                Some((dw.to_view(plane.to_world(s.point(c))), Some(r)))
            }
            Anchor::Extrude { feature, end } => {
                let FeatureKind::Extrude(e) = &part.history().get(*feature)?.kind else {
                    return None;
                };
                let (s, plane) = sketch_of(e.sketch)?;
                let base = plane.to_world(s.point(s.point_ids().next()?));
                let sign = if e.reverse { -1.0 } else { 1.0 };
                let (from, to) = match e.end {
                    End::Blind(d) => (0.0, d),
                    End::MidPlane(d) => (-d / 2.0, d / 2.0),
                    _ => return None,
                };
                let n = plane.normal();
                let w = sign * if *end { to } else { from };
                Some((dw.to_view(std::array::from_fn(|i| base[i] + n[i] * w)), None))
            }
        }
    }

    /// A dimension's value in model mm, if its anchors can still be found.
    pub fn dimension_value(&self, sheet: usize, i: usize) -> Option<f64> {
        let d = self.contents.sheets.get(sheet)?.dims.get(i)?;
        self.measure(d)
    }

    fn measure(&self, d: &Dimension) -> Option<f64> {
        match d.kind {
            DimKind::Diameter => Some(2.0 * self.anchor_radius(d)?),
            DimKind::Radius => self.anchor_radius(d),
            kind => {
                let (a, b) = (self.anchor_point(d, false)?, self.anchor_point(d, true)?);
                Some(match kind {
                    DimKind::Horizontal => (b[0] - a[0]).abs(),
                    DimKind::Vertical => (b[1] - a[1]).abs(),
                    _ => dist(a, b),
                })
            }
        }
    }

    /// A dimension's text: "30.00", "Ø10.00", "R5.00".
    pub fn dimension_text(&self, d: &Dimension) -> Option<String> {
        let v = self.measure(d)? / self.units.system.mm_per_unit();
        let n = format!("{:.*}", self.units.decimals, v);
        Some(match d.kind {
            DimKind::Diameter => format!("Ø{n}"),
            DimKind::Radius => format!("R{n}"),
            _ => n,
        })
    }

    /// What `{Name}` stands for in the title block on a sheet.
    fn field(&self, sheet: usize, name: &str) -> String {
        let s = &self.contents.sheets[sheet];
        let built_in = match name.to_lowercase().as_str() {
            "sheet" => Some(format!("{} of {}", sheet + 1, self.contents.sheets.len())),
            "sheet name" => Some(s.name.clone()),
            "scale" => Some(scale_text(s.scale)),
            "projection" => Some(if s.third_angle { "Third angle" } else { "First angle" }.into()),
            "model" => self.part().map(|p| p.name.clone()),
            "material" => self.part().map(|p| p.material().name.clone()),
            "units" => Some(self.units.system.length_unit().into()),
            "drawing" => Some(self.name.clone()),
            _ => None,
        };
        built_in
            .or_else(|| property(&self.properties, name).map(str::to_string))
            .or_else(|| self.part().and_then(|p| p.property(name)).map(str::to_string))
            .unwrap_or_default()
    }

    /// A title block cell's text with its `{Name}` fields filled in.
    pub fn fill(&self, sheet: usize, text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            let Some(len) = rest[start..].find('}') else { break };
            out += &rest[..start];
            out += &self.field(sheet, &rest[start + 1..start + len]);
            rest = &rest[start + len + 1..];
        }
        out + rest
    }

    /// The hatch used for cut faces, from the part's material.
    pub fn hatch(&self) -> Hatch {
        Hatch::of(&self.part().map_or_else(String::new, |p| p.material().name.clone()))
    }

    /// Everything on a sheet, in paper mm.
    pub fn page(&self, sheet: usize) -> Page {
        let Some(s) = self.contents.sheets.get(sheet) else {
            return Page::default();
        };
        let [w, h] = s.size.size();
        let mut m = Marks(Vec::new());
        let (x0, y0, x1, y1) = (MARGIN, MARGIN, w - MARGIN, h - MARGIN);
        m.line(Layer::Format, vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]]);
        self.title_block(sheet, &mut m, [x1, y0]);
        let hatch = self.hatch();
        for v in &s.views {
            self.draw_view(s, v, hatch, &mut m);
        }
        for d in &s.dims {
            self.draw_dimension(d, &mut m);
        }
        for n in &s.notes {
            self.draw_note(n, &mut m);
        }
        Page {
            name: s.name.clone(),
            size: [w, h],
            marks: m.0,
        }
    }

    /// Every sheet, for export.
    pub fn pages(&self) -> Vec<Page> {
        (0..self.contents.sheets.len()).map(|i| self.page(i)).collect()
    }

    fn title_block(&self, sheet: usize, m: &mut Marks, corner: P2) {
        let cells = &self.contents.title_block;
        if cells.is_empty() {
            return;
        }
        let rows = cells.len().div_ceil(COLUMNS);
        let left = corner[0] - CELL[0] * COLUMNS as f64;
        let top = corner[1] + CELL[1] * rows as f64;
        m.line(Layer::Format, vec![[left, corner[1]], [left, top], [corner[0], top]]);
        for (i, (label, text)) in cells.iter().enumerate() {
            let (row, col) = (i / COLUMNS, i % COLUMNS);
            let x = left + CELL[0] * col as f64;
            let y = top - CELL[1] * (row + 1) as f64;
            m.line(
                Layer::Format,
                vec![[x, y], [x + CELL[0], y], [x + CELL[0], y + CELL[1]]],
            );
            m.text(
                Layer::Format,
                [x + 1.5, y + CELL[1] - SMALL - 1.0],
                SMALL,
                label.clone(),
                Align::Left,
            );
            let value = self.fill(sheet, text);
            // Long values shrink to fit their cell.
            let room = CELL[0] - 3.0;
            let height = (room / text_width(&value, TEXT).max(1e-9) * TEXT).min(TEXT);
            m.text(Layer::Format, [x + 1.5, y + 1.8], height, value, Align::Left);
        }
    }

    fn draw_view(&self, s: &Sheet, v: &View, hatch_pattern: Hatch, m: &mut Marks) {
        let Some(d) = self.drawn.get(&v.id) else { return };
        let section = matches!(v.kind, ViewKind::Section { .. });
        for l in &d.lines {
            let layer = match (l.visible, v.hidden && !section) {
                (true, _) => Layer::Visible,
                (false, true) => Layer::Hidden,
                (false, false) => continue,
            };
            m.line(layer, l.points.iter().map(|&p| d.paper(p)).collect());
        }
        let parent_is_section = parent_of(&v.kind)
            .and_then(|p| s.view(p))
            .is_some_and(|p| matches!(p.kind, ViewKind::Section { .. }));
        if section || (parent_is_section && d.clip.is_some()) {
            let segs: Vec<[P2; 2]> = d.cut.iter().map(|seg| seg.map(|p| d.paper(p))).collect();
            let clip = d.clip.map(|(c, r)| (d.paper(c), r * d.scale));
            for l in hatch(&segs, hatch_pattern, clip) {
                m.line(Layer::Hatch, l);
            }
        }
        if v.marks {
            self.centre_marks(d, m);
        }
        let bottom = d.paper(d.lo)[1];
        let mid = 0.5 * (d.paper(d.lo)[0] + d.paper(d.hi)[0]);
        match &v.kind {
            ViewKind::Section {
                parent,
                label,
                vertical,
                at,
                ..
            } => {
                m.text(
                    Layer::Notes,
                    [mid, bottom - 8.0],
                    TEXT * 1.4,
                    format!("SECTION {label}-{label}"),
                    Align::Center,
                );
                if let Some(p) = self.drawn.get(parent) {
                    let ext = OVERRUN / p.scale + 2.0;
                    let (a, b) = if *vertical {
                        ([*at, p.lo[1] - ext], [*at, p.hi[1] + ext])
                    } else {
                        ([p.lo[0] - ext, *at], [p.hi[0] + ext, *at])
                    };
                    let (a, b) = (p.paper(a), p.paper(b));
                    m.line(Layer::Center, vec![a, b]);
                    let along = unit2(sub2(b, a));
                    let sight = d.frame.normal().map(|x| -x);
                    let s2 = [dot(sight, p.frame.x), dot(sight, p.frame.y)];
                    for (end, inward) in [(a, along), (b, mul(along, -1.0))] {
                        m.line(Layer::Visible, vec![end, add(end, mul(inward, 5.0))]);
                        let tail = add(end, mul(s2, -ARROW * 2.5));
                        m.line(Layer::Notes, vec![tail, end]);
                        m.shape(Layer::Notes, arrow(end, s2));
                        let at = add(tail, mul(along, if end == a { -2.0 } else { 2.0 }));
                        m.text(
                            Layer::Notes,
                            add(at, [0.0, -TEXT * 0.5]),
                            TEXT * 1.4,
                            label.clone(),
                            Align::Center,
                        );
                    }
                }
            }
            ViewKind::Detail {
                parent,
                label,
                center,
                radius,
            } => {
                let scale = d.scale;
                m.text(
                    Layer::Notes,
                    [d.at[0], d.at[1] - radius * scale - 8.0],
                    TEXT * 1.4,
                    format!("DETAIL {label} ({})", scale_text(scale)),
                    Align::Center,
                );
                if let Some(p) = self.drawn.get(parent) {
                    let c = p.paper(*center);
                    let r = radius * p.scale;
                    m.circle(Layer::Notes, c, r);
                    m.text(
                        Layer::Notes,
                        add(c, [r * 0.75 + 1.5, r * 0.75 + 1.5]),
                        TEXT * 1.4,
                        label.clone(),
                        Align::Left,
                    );
                }
            }
            _ => {}
        }
    }

    /// Crosses on circles seen face on, and lines along cylinders seen side on.
    fn centre_marks(&self, d: &Drawn, m: &mut Marks) {
        let Some(part) = self.part() else { return };
        let over = OVERRUN / d.scale;
        let mut seen: Vec<(P2, f64)> = Vec::new();
        let n = d.frame.normal();
        for b in part.bodies() {
            for e in &b.topo.edges {
                let Some((c, r)) = hlr::circle_in_view(&e.curve, &d.frame) else {
                    continue;
                };
                if !d.shows(c) || seen.iter().any(|(sc, sr)| dist(*sc, c) < 1e-3 && (sr - r).abs() < 1e-3) {
                    continue;
                }
                seen.push((c, r));
                for axis in [[1.0, 0.0], [0.0, 1.0]] {
                    let reach = mul(axis, r + over);
                    m.line(Layer::Center, vec![d.paper(sub2(c, reach)), d.paper(add(c, reach))]);
                }
            }
            for f in &b.topo.faces {
                let Some((p, dir, _)) = f.cylinder else { continue };
                if dot(dir, n).abs() > 1e-6 {
                    continue;
                }
                let along: Vec<f64> = f
                    .tris
                    .iter()
                    .flatten()
                    .map(|v| dot(threebcad_kernel::topology::sub(*v, p), dir))
                    .collect();
                let (t0, t1) = along
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &t| (a.min(t), b.max(t)));
                if t0 > t1 {
                    continue;
                }
                let at = |t: f64| d.to_view(std::array::from_fn(|i| p[i] + dir[i] * t));
                let (a, b) = (at(t0 - over), at(t1 + over));
                let key = (mul(add(a, b), 0.5), dist(a, b));
                if seen
                    .iter()
                    .any(|(c, l)| dist(*c, key.0) < 1e-3 && (l - key.1).abs() < 1e-3)
                {
                    continue;
                }
                seen.push(key);
                for piece in d
                    .clip
                    .map_or_else(|| vec![vec![a, b]], |(c, r)| clip_circle(&[a, b], c, r))
                {
                    m.line(Layer::Center, piece.into_iter().map(|q| d.paper(q)).collect());
                }
            }
        }
    }

    fn draw_dimension(&self, dim: &Dimension, m: &mut Marks) {
        let (Some(d), Some(text)) = (self.drawn.get(&dim.view), self.dimension_text(dim)) else {
            return;
        };
        let t = add(d.at, dim.at);
        let l = Layer::Dimensions;
        match dim.kind {
            DimKind::Diameter | DimKind::Radius => {
                let (Some(c), Some(r)) = (self.anchor_point(dim, false), self.anchor_radius(dim)) else {
                    return;
                };
                let (c, r) = (d.paper(c), r * d.scale);
                let v = unit2(sub2(t, c));
                let rim = add(c, mul(v, r));
                let start = if dim.kind == DimKind::Diameter {
                    sub2(c, mul(v, r))
                } else {
                    c
                };
                m.line(l, vec![start, t]);
                m.shape(l, arrow(rim, v));
                if dim.kind == DimKind::Diameter {
                    m.shape(l, arrow(start, mul(v, -1.0)));
                }
                let right = t[0] >= c[0];
                let shoulder = add(t, [if right { 3.0 } else { -3.0 }, 0.0]);
                m.line(l, vec![t, shoulder]);
                let align = if right { Align::Left } else { Align::Right };
                m.text(
                    l,
                    add(shoulder, [if right { 1.0 } else { -1.0 }, -TEXT / 2.0]),
                    TEXT,
                    text,
                    align,
                );
            }
            kind => {
                let (Some(a), Some(b)) = (self.anchor_point(dim, false), self.anchor_point(dim, true)) else {
                    return;
                };
                let (a, b) = (d.paper(a), d.paper(b));
                let u = match kind {
                    DimKind::Horizontal => [1.0, 0.0],
                    DimKind::Vertical => [0.0, 1.0],
                    _ => unit2(sub2(b, a)),
                };
                let w = [-u[1], u[0]];
                let line = dot2(t, w);
                let foot = |p: P2| add(p, mul(w, line - dot2(p, w)));
                let (fa, fb) = (foot(a), foot(b));
                for (p, f) in [(a, fa), (b, fb)] {
                    let out = unit2(sub2(f, p));
                    if dist(p, f) > 1.0 {
                        m.line(l, vec![add(p, mul(out, 1.0)), add(f, mul(out, 2.0))]);
                    }
                }
                // The dimension line runs out to the text if the text sits past an end.
                let (ta, tb) = (dot2(fa, u), dot2(fb, u));
                let tt = dot2(t, u);
                let (lo, hi) = (ta.min(tb).min(tt), ta.max(tb).max(tt));
                let base = sub2(fa, mul(u, ta));
                m.line(l, vec![add(base, mul(u, lo)), add(base, mul(u, hi))]);
                let dir = if tb >= ta { u } else { mul(u, -1.0) };
                m.shape(l, arrow(fa, mul(dir, -1.0)));
                m.shape(l, arrow(fb, dir));
                let on = add(base, mul(u, tt));
                if u[0].abs() >= u[1].abs() {
                    m.text(
                        l,
                        add(on, mul(w, if w[1] >= 0.0 { 1.0 } else { -1.0 - TEXT })),
                        TEXT,
                        text,
                        Align::Center,
                    );
                } else if on[0] < (a[0] + b[0]) / 2.0 {
                    // Left of what it measures: the text goes on the outside of the line.
                    m.text(l, add(on, [-1.5, -TEXT / 2.0]), TEXT, text, Align::Right);
                } else {
                    m.text(l, add(on, [1.5, -TEXT / 2.0]), TEXT, text, Align::Left);
                }
            }
        }
    }

    fn draw_note(&self, n: &Note, m: &mut Marks) {
        let lines: Vec<&str> = n.text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            m.text(
                Layer::Notes,
                [n.at[0], n.at[1] - i as f64 * TEXT * 1.6],
                TEXT,
                *line,
                Align::Left,
            );
        }
        let Some((view, p)) = n.leader else { return };
        let Some(target) = self.to_sheet(view, p) else { return };
        let width = lines.iter().map(|l| text_width(l, TEXT)).fold(0.0, f64::max);
        let left = target[0] < n.at[0] + width / 2.0;
        let start = [
            if left { n.at[0] - 1.0 } else { n.at[0] + width + 1.0 },
            n.at[1] + TEXT / 2.0,
        ];
        let knee = add(start, [if left { -3.0 } else { 3.0 }, 0.0]);
        m.line(Layer::Notes, vec![start, knee, target]);
        m.shape(Layer::Notes, arrow(target, sub2(target, knee)));
    }

    /// Where a dimension's text is on the sheet.
    pub fn dimension_at(&self, d: &Dimension) -> Option<P2> {
        Some(add(self.view_origin(d.view)?, d.at))
    }

    /// A picked item on the sheet, to highlight it: a vertex's point, a circle, an edge's ends.
    pub fn outline(&self, view: u32, pick: &Pick) -> Vec<P2> {
        let Some(d) = self.drawn.get(&view) else {
            return Vec::new();
        };
        let at = |a: Anchor| self.anchor(view, &a);
        let pts = match pick {
            Pick::Vertex(v) => at(Anchor::Vertex(v.clone())).map(|(p, _)| vec![p]),
            Pick::Edge(a, b) => at(Anchor::Vertex(a.clone()))
                .zip(at(Anchor::Vertex(b.clone())))
                .map(|(a, b)| vec![a.0, b.0]),
            Pick::Circle(e) => at(Anchor::Circle(e.clone())).map(|(c, r)| {
                let r = r.unwrap_or(0.0);
                (0..=48)
                    .map(|k| {
                        let t = k as f64 / 48.0 * std::f64::consts::TAU;
                        [c[0] + r * t.cos(), c[1] + r * t.sin()]
                    })
                    .collect()
            }),
        };
        pts.unwrap_or_default().into_iter().map(|p| d.paper(p)).collect()
    }

    /// The box a note's text covers on the sheet.
    pub fn note_rect(n: &Note) -> (P2, P2) {
        let lines = n.text.lines().count().max(1);
        let width = n.text.lines().map(|l| text_width(l, TEXT)).fold(0.0, f64::max);
        (
            [n.at[0] - 1.0, n.at[1] - (lines - 1) as f64 * TEXT * 1.6 - 1.0],
            [n.at[0] + width + 1.0, n.at[1] + TEXT + 1.0],
        )
    }
}

/// Whether a circular edge's circle is closed by the edges on it (a hole or boss), not an arc.
fn full_circle(part: &PartDocument, e: &EdgeRef) -> Option<bool> {
    use threebcad_kernel::topology::{norm, sub};
    let (b, e) = part.resolve_edge(e)?;
    let edges = &part.bodies().get(b)?.topo.edges;
    let EdgeCurve::Circle { center, axis, radius } = edges.get(e)?.curve else {
        return None;
    };
    let same = |o: &&threebcad_kernel::topology::TopoEdge| match o.curve {
        EdgeCurve::Circle {
            center: c,
            axis: a,
            radius: r,
        } => norm(sub(c, center)) < 1e-4 && dot(a, axis).abs() > 0.9999 && (r - radius).abs() < 1e-4,
        _ => false,
    };
    let around: f64 = edges.iter().filter(same).map(|o| o.length).sum();
    Some(around > 0.99 * std::f64::consts::TAU * radius)
}

fn parent_of(kind: &ViewKind) -> Option<u32> {
    match kind {
        ViewKind::Model(_) => None,
        ViewKind::Projected { parent, .. } | ViewKind::Section { parent, .. } | ViewKind::Detail { parent, .. } => {
            Some(*parent)
        }
    }
}

/// A child's placement with the coordinate that lines it up taken from its parent.
fn lined_up(at: P2, parent: &Drawn, horizontal: bool) -> P2 {
    let o = parent.paper([0.0, 0.0]);
    if horizontal {
        [at[0], o[1]]
    } else {
        [o[0], at[1]]
    }
}

/// The first letter not used yet for a section or detail: A, B, ..., Z, AA, ...
fn next_label(used: &[String]) -> String {
    (0..)
        .map(|i: usize| {
            let letter = |k: usize| char::from(b'A' + (k % 26) as u8);
            if i < 26 {
                letter(i).to_string()
            } else {
                format!("{}{}", letter(i / 26 - 1), letter(i))
            }
        })
        .find(|l| !used.contains(l))
        .unwrap()
}

/// Views point at views before them on the same sheet, so drawing them in order always has the
/// parent ready; dimensions and leaders point at views that exist.
fn validate(c: &Contents) -> anyhow::Result<()> {
    ensure!(!c.sheets.is_empty(), "the drawing file is damaged: it has no sheets");
    let mut ids = HashSet::new();
    for s in &c.sheets {
        ensure!(
            s.scale.is_finite() && s.scale > 0.0,
            "the drawing file is damaged: a sheet has no scale"
        );
        let mut here = HashSet::new();
        for v in &s.views {
            if !(v.id < c.next_id && ids.insert(v.id)) {
                bail!("the drawing file is damaged: its views do not match");
            }
            if let Some(p) = parent_of(&v.kind) {
                let parent = s.view(p).filter(|_| here.contains(&p));
                ensure!(
                    parent.is_some_and(|p| !matches!(p.kind, ViewKind::Detail { .. })),
                    "the drawing file is damaged: a view's parent is missing"
                );
            }
            if let ViewKind::Detail { radius, .. } = v.kind {
                ensure!(
                    radius.is_finite() && radius > 0.0,
                    "the drawing file is damaged: a detail has no size"
                );
            }
            ensure!(
                v.scale.is_none_or(|s| s.is_finite() && s > 0.0) && v.at.iter().all(|x| x.is_finite()),
                "the drawing file is damaged: a view has no place"
            );
            here.insert(v.id);
        }
        ensure!(
            s.dims.iter().all(|d| here.contains(&d.view))
                && s.notes.iter().all(|n| n.leader.is_none_or(|(v, _)| here.contains(&v))),
            "the drawing file is damaged: a dimension or note points at a missing view"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Extrude;
    use threebcad_kernel::topology::norm;
    use threebcad_sketch::Sketch;

    fn folder(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("3bcad-drawing-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A `w` x 20 x 10 block on the top plane with a Ø10 hole through it at (15, 10).
    fn block(path: &Path, w: f64) {
        let mut p = PartDocument::new("Block");
        p.edit(|h| -> anyhow::Result<()> {
            let s = h.add_sketch(Sketch::rectangle(w, 20.0), Plane::TOP);
            h.add_extrude(s, 10.0)?;
            let c = h.add_sketch(Sketch::circle([15.0, 10.0], 5.0), Plane::TOP);
            // Cuts go against the sketch normal unless reversed: up through the block.
            let mut e = Extrude::cut(c, 10.0);
            (e.end, e.reverse) = (End::ThroughAll, true);
            h.add(FeatureKind::Extrude(e))?;
            Ok(())
        })
        .unwrap();
        assert!(p.errors().is_empty(), "{:?}", p.errors());
        p.set_properties(vec![("Part number".into(), "BLK-7".into())]);
        p.save(path).unwrap();
    }

    fn texts(page: &Page) -> Vec<String> {
        page.marks
            .iter()
            .filter_map(|m| match &m.shape {
                Shape::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn size(d: &DrawingDocument, id: u32) -> P2 {
        let (lo, hi) = d.view_rect(id).unwrap();
        [hi[0] - lo[0], hi[1] - lo[1]]
    }

    fn drawing(dir: &Path, w: f64) -> DrawingDocument {
        let part = dir.join("block.3bcad");
        block(&part, w);
        let mut d = DrawingDocument::new("Block");
        d.set_model(&part).unwrap();
        d
    }

    #[test]
    fn sheets_come_in_iso_and_ansi_sizes() {
        assert_eq!(SheetSize::A4.size(), [297.0, 210.0]);
        assert_eq!(SheetSize::A0.size(), [1189.0, 841.0]);
        let b = SheetSize::AnsiB.size();
        assert!((b[0] - 431.8).abs() < 1e-9 && (b[1] - 279.4).abs() < 1e-9);
        // Each ISO size is half the next one up.
        for w in SheetSize::ALL[..5].windows(2) {
            let (small, big) = (w[0].size(), w[1].size());
            assert!((big[1] - small[0]).abs() <= 1.0 && (big[0] / 2.0 - small[1]).abs() <= 1.0);
        }
        let mut d = DrawingDocument::new("D");
        d.edit(None, |c| c.sheets[0].size = SheetSize::AnsiE);
        assert_eq!(d.page(0).size, SheetSize::AnsiE.size());
        assert_eq!(d.add_sheet(), 1);
        assert_eq!(d.contents().sheets[1].name, "Sheet2");
        assert_eq!(d.contents().sheets[1].size, SheetSize::AnsiE);
        d.remove_sheet(0);
        d.remove_sheet(0);
        assert_eq!(d.contents().sheets.len(), 1, "the last sheet stays");
    }

    #[test]
    fn standard_views_follow_the_projection_angle_and_line_up() {
        let dir = folder("angle");
        let mut d = drawing(&dir, 30.0);
        let [front, top, side] = d.standard_views(0).unwrap();
        // First angle (ISO): the top view goes below the front, the left view to its right.
        let normal = |d: &DrawingDocument, id| d.drawn[&id].frame.normal();
        assert!(norm(sub3(normal(&d, side), [-1.0, 0.0, 0.0])) < 1e-9);
        assert!(norm(sub3(normal(&d, top), [0.0, 0.0, 1.0])) < 1e-9);
        let (f, t, s) = (
            d.view_rect(front).unwrap(),
            d.view_rect(top).unwrap(),
            d.view_rect(side).unwrap(),
        );
        assert!(t.1[1] < f.0[1] && s.0[0] > f.1[0]);
        // Same scale, lined up: the top view shares the front's x range, the side its z range.
        assert!((t.0[0] - f.0[0]).abs() < 1e-6 && (t.1[0] - f.1[0]).abs() < 1e-6);
        assert!((s.0[1] - f.0[1]).abs() < 1e-6 && (s.1[1] - f.1[1]).abs() < 1e-6);
        // The front shows 30 x 10, the top 30 x 20, the side 20 x 10 (1:1).
        assert!(norm2(sub2(size(&d, front), [30.0, 10.0])) < 1e-6);
        assert!(norm2(sub2(size(&d, top), [30.0, 20.0])) < 1e-6);
        assert!(norm2(sub2(size(&d, side), [20.0, 10.0])) < 1e-6);
        // Moving the front drags the projected views along their line.
        d.edit(None, |c| c.sheets[0].views[0].at[1] += 7.0);
        assert!((d.view_rect(side).unwrap().0[1] - s.0[1] - 7.0).abs() < 1e-6);
        assert!(
            (d.view_rect(top).unwrap().0[1] - t.0[1]).abs() < 1e-6,
            "the top view only lines up in x"
        );
        // Third angle (ANSI): the view on the right is the right view.
        d.edit(None, |c| c.sheets[0].third_angle = true);
        assert!(norm(sub3(normal(&d, side), [1.0, 0.0, 0.0])) < 1e-9);
        assert!(
            norm(sub3(normal(&d, top), [0.0, 0.0, -1.0])) < 1e-9,
            "below the front is now the bottom"
        );
        // The hole shows dashed in the front view.
        let page = d.page(0);
        assert!(page.marks.iter().any(|m| m.layer == Layer::Hidden));
        assert!(
            page.marks.iter().any(|m| m.layer == Layer::Center),
            "centre marks on the hole"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn views_follow_the_part_and_the_file_keeps_relative_paths() {
        let dir = folder("follow");
        let mut d = drawing(&dir, 30.0);
        let [front, ..] = d.standard_views(0).unwrap();
        let path = dir.join("Block.3bcdrw");
        d.save(&path).unwrap();
        assert!(!d.is_dirty());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"model\": \"block.3bcad\""), "{text}");
        // Moved with its part to another folder, it opens with the same views.
        let moved = folder("follow-moved");
        std::fs::copy(&path, moved.join("Block.3bcdrw")).unwrap();
        std::fs::copy(dir.join("block.3bcad"), moved.join("block.3bcad")).unwrap();
        let mut back = DrawingDocument::open(&moved.join("Block.3bcdrw")).unwrap();
        assert_eq!(back.contents().sheets, d.contents().sheets);
        assert!(norm2(sub2(size(&back, front), [30.0, 10.0])) < 1e-6);
        // The part gets wider: the views follow once the file changes.
        std::thread::sleep(std::time::Duration::from_millis(20));
        block(&moved.join("block.3bcad"), 45.0);
        assert!(back.refresh());
        assert!(norm2(sub2(size(&back, front), [45.0, 10.0])) < 1e-6);
        assert!(!back.refresh(), "nothing changed since");
        // A newer format and a damaged file are refused.
        let newer = text.replace("\"version\": 1", "\"version\": 99");
        let err = DrawingDocument::from_json(&newer, Some(&dir)).err().unwrap();
        assert!(err.to_string().contains("newer 3BCAD"), "{err}");
        let damaged = text.replace("\"next_id\": 3", "\"next_id\": 1");
        assert!(DrawingDocument::from_json(&damaged, Some(&dir)).is_err());
        std::fs::remove_dir_all(dir).ok();
        std::fs::remove_dir_all(moved).ok();
    }

    #[test]
    fn a_section_is_hatched_for_the_material_and_labelled() {
        let dir = folder("section");
        let mut d = drawing(&dir, 30.0);
        let top = d.add_model_view(0, Orientation::Top, [100.0, 200.0]).unwrap();
        // Cut along y = 10 (through the hole's centre), seen from above: the front of the block.
        let sec = d.add_section(0, top, false, [15.0, 10.0]).unwrap();
        let page = d.page(0);
        let t = texts(&page);
        assert!(t.contains(&"SECTION A-A".to_string()) && t.iter().filter(|x| *x == "A").count() == 2);
        // The cut face is two 10 x 10 squares beside the Ø10 hole; hatching stays inside them.
        let (lo, hi) = d.view_rect(sec).unwrap();
        assert!((hi[0] - lo[0] - 30.0).abs() < 1e-6 && (hi[1] - lo[1] - 10.0).abs() < 1e-6);
        let hatch: Vec<&Vec<P2>> = page
            .marks
            .iter()
            .filter(|m| m.layer == Layer::Hatch)
            .filter_map(|m| match &m.shape {
                Shape::Line(p) => Some(p),
                _ => None,
            })
            .collect();
        assert!(hatch.len() > 10);
        for l in &hatch {
            let mid = d.to_view(sec, mul(add(l[0], l[1]), 0.5)).unwrap();
            let x = mid[0];
            assert!(
                (0.0..=10.0 + 1e-6).contains(&x) || (20.0 - 1e-6..=30.0).contains(&x),
                "hatch at x = {x}"
            );
        }
        // PLA is a plastic: cross-hatched. Steel gets pairs of lines.
        assert_eq!(d.hatch(), Hatch::Plastic);
        assert_eq!(Hatch::of("Steel (mild)"), Hatch::Steel);
        assert_eq!(Hatch::of("Brass"), Hatch::Copper);
        assert_eq!(Hatch::of("Aluminium 6061"), Hatch::General);
        let angles: HashSet<i64> = hatch
            .iter()
            .map(|l| {
                (sub2(l[1], l[0])[1]
                    .atan2(sub2(l[1], l[0])[0])
                    .to_degrees()
                    .rem_euclid(180.0))
                .round() as i64
            })
            .collect();
        assert_eq!(angles, HashSet::from([45, 135]));
        // No hidden lines in a section view.
        assert!(!d.contents().sheets[0].views[1].hidden);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_detail_shows_a_circle_of_its_parent_larger() {
        let dir = folder("detail");
        let mut d = drawing(&dir, 30.0);
        let top = d.add_model_view(0, Orientation::Top, [100.0, 200.0]).unwrap();
        let det = d.add_detail(0, top, [15.0, 10.0], 6.0).unwrap();
        assert_eq!(d.view_scale(det), Some(2.0));
        let page = d.page(0);
        assert!(texts(&page).contains(&"DETAIL A (2:1)".to_string()));
        // Its lines lie within the circle, 12 mm on paper; the hole's rim (r 5) shows at r 10.
        let at = d.view_origin(det).unwrap();
        let (lo, hi) = d.view_rect(det).unwrap();
        assert!((hi[0] - lo[0] - 24.0).abs() < 1e-6);
        let inside: Vec<P2> = page
            .marks
            .iter()
            .filter(|m| m.layer == Layer::Visible)
            .filter_map(|m| match &m.shape {
                Shape::Line(p) => Some(p.clone()),
                _ => None,
            })
            .flatten()
            .filter(|p| dist(*p, at) < 12.5)
            .collect();
        assert!(inside.iter().all(|p| dist(*p, at) <= 12.0 + 1e-6));
        assert!(inside.iter().any(|p| (dist(*p, at) - 10.0).abs() < 0.05));
        // Removing the parent removes the detail.
        d.remove_view(0, top);
        assert!(d.contents().sheets[0].views.is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn dimensions_come_from_the_model_or_the_view_and_follow_the_part() {
        let dir = folder("dims");
        let mut d = drawing(&dir, 30.0);
        let [front, top, _] = d.standard_views(0).unwrap();
        // The sketches' 30, 20 and R5 on the top view, the 10 deep extrusions on the front.
        let n = d.import_model_dimensions(0);
        let dims = d.contents().sheets[0].dims.clone();
        let values: Vec<(u32, DimKind, f64)> = (0..n)
            .map(|i| (dims[i].view, dims[i].kind, d.dimension_value(0, i).unwrap()))
            .collect();
        assert!(values.contains(&(top, DimKind::Horizontal, 30.0)), "{values:?}");
        assert!(values
            .iter()
            .any(|&(v, k, x)| v == top && k == DimKind::Vertical && (x - 20.0).abs() < 1e-9));
        assert!(values
            .iter()
            .any(|&(v, k, x)| v == top && k == DimKind::Radius && (x - 5.0).abs() < 1e-9));
        assert!(values
            .iter()
            .any(|&(v, k, x)| v == front && k == DimKind::Vertical && (x - 10.0).abs() < 1e-9));
        assert_eq!(d.import_model_dimensions(0), 0, "already there");
        // A reference dimension across the block's front edge, picked on the front view.
        let (lo, hi) = d.view_rect(front).unwrap();
        let (_, a) = d.pick(0, lo, 1.0).unwrap();
        let (_, b) = d.pick(0, [hi[0], lo[1]], 1.0).unwrap();
        assert!(matches!((&a, &b), (Pick::Vertex(_), Pick::Vertex(_))));
        let i = d
            .add_dimension(0, front, &[a, b], [lo[0] + 15.0, lo[1] - 10.0])
            .unwrap();
        assert_eq!(d.contents().sheets[0].dims[i].kind, DimKind::Horizontal);
        assert_eq!(d.dimension_value(0, i), Some(30.0));
        // The hole's rim on the top view gives a diameter.
        let (tlo, _) = d.view_rect(top).unwrap();
        // On the rim at 53°, away from where truck splits circles into arcs.
        let rim = add(tlo, [15.0 + 5.0 * 0.6, 10.0 + 5.0 * 0.8]);
        let (view, circle) = d.pick(0, rim, 1.0).unwrap();
        assert!(matches!(circle, Pick::Circle(_)) && view == top);
        let j = d.add_dimension(0, top, &[circle], add(rim, [10.0, 10.0])).unwrap();
        assert_eq!(
            d.dimension_text(&d.contents().sheets[0].dims[j].clone()).unwrap(),
            "Ø10.00"
        );
        // Widen the part: the reference dimension follows its vertices.
        let path = dir.join("Block.3bcdrw");
        d.save(&path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        block(&dir.join("block.3bcad"), 45.0);
        assert!(d.refresh());
        assert_eq!(d.dimension_value(0, i), Some(45.0));
        assert_eq!(d.dimension_value(0, 0), Some(45.0), "the imported sketch width");
        let page = d.page(0);
        assert!(texts(&page).contains(&"45.00".to_string()));
        assert!(page.marks.iter().filter(|m| m.layer == Layer::Dimensions).count() > 20);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn notes_have_leaders_and_the_title_block_fills_in() {
        let dir = folder("notes");
        let mut d = drawing(&dir, 30.0);
        let front = d.add_model_view(0, Orientation::Front, [150.0, 150.0]).unwrap();
        let target = [15.0, 10.0];
        d.edit(None, |c| {
            c.sheets[0].notes.push(Note {
                text: "BREAK SHARP EDGES\nDEBURR".into(),
                at: [200.0, 190.0],
                leader: Some((front, target)),
            })
        });
        d.set_properties(vec![("Drawn by".into(), "DD".into())]);
        d.add_sheet();
        let page = d.page(0);
        let t = texts(&page);
        for want in [
            "BREAK SHARP EDGES",
            "DEBURR",
            "block",
            "BLK-7",
            "PLA",
            "DD",
            "1:1",
            "First angle",
            "1 of 2",
        ] {
            assert!(t.contains(&want.to_string()), "{want} in {t:?}");
        }
        // The leader ends at the point on the view, with an arrowhead there.
        let tip = d.to_sheet(front, target).unwrap();
        assert!(page
            .marks
            .iter()
            .any(|m| m.layer == Layer::Notes && matches!(&m.shape, Shape::Fill(p) if dist(p[0], tip) < 1e-9)));
        assert!(page
            .marks
            .iter()
            .any(|m| matches!(&m.shape, Shape::Line(p) if p.last().is_some_and(|q| dist(*q, tip) < 1e-9))));
        assert_eq!(d.fill(1, "{Sheet} / {Nope}"), "2 of 2 / ");
        // Removing the view drops the leader, keeps the note.
        d.remove_view(0, front);
        assert_eq!(d.contents().sheets[0].notes[0].leader, None);
        // Undo brings the view back with its leader.
        d.undo();
        assert_eq!(d.contents().sheets[0].notes[0].leader, Some((front, target)));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn labels_run_through_the_alphabet() {
        assert_eq!(next_label(&[]), "A");
        assert_eq!(next_label(&["A".into(), "B".into()]), "C");
        let all: Vec<String> = (b'A'..=b'Z').map(|c| char::from(c).to_string()).collect();
        assert_eq!(next_label(&all), "AA");
        assert_eq!(scale_text(0.5), "1:2");
        assert_eq!(scale_text(2.0), "2:1");
        assert_eq!(scale_text(0.4), "1:2.5");
    }

    fn sub3(a: P3, b: P3) -> P3 {
        threebcad_kernel::topology::sub(a, b)
    }

    fn norm2(a: P2) -> f64 {
        a[0].hypot(a[1])
    }
}
