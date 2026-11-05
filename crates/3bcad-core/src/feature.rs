use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use threebcad_kernel::mesher::PrintMesh;
use threebcad_kernel::{CompressedSolid, Plane};
use threebcad_sketch::{EntityId, Sketch};

use crate::naming::{EdgeRef, FaceRef, VertexRef};
use crate::FeatureId;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Feature {
    pub name: String,
    pub kind: FeatureKind,
    /// Left out of the rebuild without being deleted; children follow.
    pub suppressed: bool,
}

/// Front / Top / Right.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StdPlane {
    Front,
    Top,
    Right,
}

impl StdPlane {
    pub const ALL: [StdPlane; 3] = [StdPlane::Front, StdPlane::Top, StdPlane::Right];

    pub fn name(self) -> &'static str {
        match self {
            StdPlane::Front => "Front Plane",
            StdPlane::Top => "Top Plane",
            StdPlane::Right => "Right Plane",
        }
    }

    pub fn plane(self) -> Plane {
        match self {
            StdPlane::Front => Plane::FRONT,
            StdPlane::Top => Plane::TOP,
            StdPlane::Right => Plane::RIGHT,
        }
    }
}

/// A plane to sketch on, mirror about or measure from.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PlaneRef {
    Std(StdPlane),
    /// A planar model face.
    Face(FaceRef),
    /// A reference plane feature.
    Feature(FeatureId),
}

/// A direction or line: pattern directions, revolve and circular pattern axes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum AxisRef {
    /// World X (0), Y (1) or Z (2) through the origin.
    Std(u8),
    /// A straight edge (its line) or a circular edge (its axis).
    Edge(EdgeRef),
    /// A cylindrical face's axis, or a planar face's normal through its center.
    Face(FaceRef),
    /// A reference axis feature.
    Feature(FeatureId),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PointRef {
    Origin,
    Vertex(VertexRef),
    /// A reference point feature.
    Feature(FeatureId),
}

/// How far an extrusion goes in one direction.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum End {
    Blind(f64),
    ThroughAll,
    UpToNext,
    UpToVertex(VertexRef),
    UpToSurface(FaceRef),
    OffsetFromSurface(FaceRef, f64),
    /// Symmetric about the sketch plane, total depth.
    MidPlane(f64),
}

impl End {
    pub fn name(&self) -> &'static str {
        match self {
            End::Blind(_) => "Blind",
            End::ThroughAll => "Through All",
            End::UpToNext => "Up To Next",
            End::UpToVertex(_) => "Up To Vertex",
            End::UpToSurface(_) => "Up To Surface",
            End::OffsetFromSurface(..) => "Offset From Surface",
            End::MidPlane(_) => "Mid Plane",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThinKind {
    OneDirection,
    MidPlane,
    TwoDirection,
}

/// Thin feature: a wall of thickness `t1` (and `t2` for two directions) along the profile.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Thin {
    pub kind: ThinKind,
    pub t1: f64,
    pub t2: f64,
    /// One direction: put the wall on the other side of the profile.
    pub reverse: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Extrude {
    pub sketch: FeatureId,
    pub cut: bool,
    pub end: End,
    /// Direction 2, extruding the other way from the sketch plane.
    pub end2: Option<End>,
    pub reverse: bool,
    /// Boss: combine with the bodies it touches.
    pub merge: bool,
    /// Cut: keep the material inside the profile instead of removing it.
    pub flip_side: bool,
    /// Draft angle in radians, positive tapers inwards.
    pub draft: f64,
    pub thin: Option<Thin>,
}

impl Extrude {
    pub fn boss(sketch: FeatureId, depth: f64) -> Self {
        Self {
            sketch,
            cut: false,
            end: End::Blind(depth),
            end2: None,
            reverse: false,
            merge: true,
            flip_side: false,
            draft: 0.0,
            thin: None,
        }
    }

    pub fn cut(sketch: FeatureId, depth: f64) -> Self {
        Self {
            cut: true,
            ..Self::boss(sketch, depth)
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum RevolveAxis {
    /// A line (usually a centerline) of the profile sketch.
    SketchLine(EntityId),
    Axis(AxisRef),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Revolve {
    pub sketch: FeatureId,
    pub cut: bool,
    /// `None`: the first centerline of the sketch.
    pub axis: Option<RevolveAxis>,
    /// Radians, direction 1.
    pub angle: f64,
    /// Radians the other way (direction 2), 0 for none.
    pub angle2: f64,
    pub reverse: bool,
    pub merge: bool,
}

impl Revolve {
    pub fn new(sketch: FeatureId, cut: bool) -> Self {
        Self {
            sketch,
            cut,
            axis: None,
            angle: std::f64::consts::TAU,
            angle2: 0.0,
            reverse: false,
            merge: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChamferKind {
    EqualDistance,
    DistanceDistance,
    AngleDistance,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoleKind {
    Simple,
    Counterbore,
    Countersink,
    Tapped,
}

impl HoleKind {
    pub const ALL: [HoleKind; 4] = [
        HoleKind::Simple,
        HoleKind::Counterbore,
        HoleKind::Countersink,
        HoleKind::Tapped,
    ];

    pub fn name(self) -> &'static str {
        match self {
            HoleKind::Simple => "Hole",
            HoleKind::Counterbore => "Counterbore",
            HoleKind::Countersink => "Countersink",
            HoleKind::Tapped => "Tapped Hole",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoleStandard {
    Iso,
    Ansi,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hole {
    /// A sketch whose free points place the holes.
    pub sketch: FeatureId,
    pub kind: HoleKind,
    pub standard: HoleStandard,
    /// Row of the size table, e.g. "M6" or "1/4".
    pub size: String,
    /// `End::Blind` or `End::ThroughAll`.
    pub end: End,
}

/// What a pattern or mirror repeats.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Seeds {
    Features(Vec<FeatureId>),
    /// Every body that exists before the pattern.
    Bodies,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PatternDir {
    pub axis: AxisRef,
    pub spacing: f64,
    /// Instances including the seed.
    pub count: usize,
    pub reverse: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LinearPattern {
    pub seeds: Seeds,
    pub dir1: PatternDir,
    pub dir2: Option<PatternDir>,
    /// (index along direction 1, index along direction 2) of instances to leave out.
    pub skip: Vec<(usize, usize)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CircularPattern {
    pub seeds: Seeds,
    pub axis: AxisRef,
    pub count: usize,
    /// Radians: the total angle when `equal`, else the angle between instances.
    pub angle: f64,
    pub equal: bool,
    pub reverse: bool,
    pub skip: Vec<usize>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Mirror {
    pub seeds: Seeds,
    pub plane: PlaneRef,
    /// Mirrored bodies: merge with the bodies they touch.
    pub merge: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum RefPlaneDef {
    Offset {
        base: PlaneRef,
        distance: f64,
        flip: bool,
    },
    /// Through `axis`, turned by `angle` (radians) from `base`.
    Angle {
        base: PlaneRef,
        axis: AxisRef,
        angle: f64,
    },
    ThreePoint([PointRef; 3]),
    Mid(PlaneRef, PlaneRef),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum RefAxisDef {
    TwoPoints(PointRef, PointRef),
    Edge(EdgeRef),
    Cylinder(FaceRef),
    TwoPlanes(PlaneRef, PlaneRef),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum RefPointDef {
    Vertex(VertexRef),
    FaceCenter(FaceRef),
    /// Center of a circular edge.
    ArcCenter(EdgeRef),
    Coords([f64; 3]),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CoordSysDef {
    pub origin: PointRef,
    /// X axis direction (default world X).
    pub x: Option<AxisRef>,
    /// Z axis direction (default world Z).
    pub z: Option<AxisRef>,
}

/// A curve a sweep follows or a curve-driven pattern runs along.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveRef {
    /// The open chain (lines, arcs, splines) of a sketch.
    Sketch(FeatureId),
    /// A helix or spiral feature.
    Helix(FeatureId),
}

/// What a sweep carries along its path.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum SweepProfile {
    /// The closed regions of a sketch.
    Sketch(FeatureId),
    /// A circle of this diameter square to the start of the path.
    Circle(f64),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Sweep {
    pub profile: SweepProfile,
    pub path: CurveRef,
    /// Radians the profile turns about the path from start to end.
    pub twist: f64,
    pub cut: bool,
    /// Boss: combine with the bodies it touches.
    pub merge: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Loft {
    /// Sketches with one closed region each, in order.
    pub profiles: Vec<FeatureId>,
    /// Sketches with an open chain running from the first profile to the last.
    pub guides: Vec<FeatureId>,
    /// Tangency length square to the first / last profile (0: none).
    pub start_tangent: f64,
    pub end_tangent: f64,
    pub cut: bool,
    pub merge: bool,
}

/// How big a helix is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum HelixSize {
    PitchTurns {
        pitch: f64,
        turns: f64,
    },
    HeightPitch {
        height: f64,
        pitch: f64,
    },
    HeightTurns {
        height: f64,
        turns: f64,
    },
    /// A flat spiral: `pitch` is the gap between turns.
    Spiral {
        pitch: f64,
        turns: f64,
    },
}

impl HelixSize {
    pub fn name(&self) -> &'static str {
        match self {
            HelixSize::PitchTurns { .. } => "Pitch and Revolution",
            HelixSize::HeightPitch { .. } => "Height and Pitch",
            HelixSize::HeightTurns { .. } => "Height and Revolution",
            HelixSize::Spiral { .. } => "Spiral",
        }
    }

    /// (pitch, turns), the pitch being the rise per turn (0 for a spiral).
    pub fn pitch_turns(&self) -> (f64, f64) {
        match *self {
            HelixSize::PitchTurns { pitch, turns } => (pitch, turns),
            HelixSize::HeightPitch { height, pitch } => (pitch, if pitch.abs() > 1e-12 { height / pitch } else { 0.0 }),
            HelixSize::HeightTurns { height, turns } => (if turns.abs() > 1e-12 { height / turns } else { 0.0 }, turns),
            HelixSize::Spiral { turns, .. } => (0.0, turns),
        }
    }
}

/// A helix or spiral starting on a sketch circle, rising along the sketch normal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HelixDef {
    pub sketch: FeatureId,
    pub size: HelixSize,
    /// Radians; positive widens the helix as it rises.
    pub taper: f64,
    /// Counter-clockwise seen from the end of the axis (right-handed rising).
    pub ccw: bool,
    /// Radians from the sketch's x direction.
    pub start: f64,
    /// Rise the other way (below the sketch).
    pub reverse: bool,
}

/// A modelled thread on a cylindrical face.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ThreadDef {
    pub face: FaceRef,
    pub standard: HoleStandard,
    /// Size from the hole tables ("M6", "1/4").
    pub size: String,
    /// Threaded length from the start end (`None`: the whole face).
    pub length: Option<f64>,
    /// Distance from the start end to where the thread begins.
    pub offset: f64,
    /// Start from the other end of the face.
    pub flip: bool,
    pub right: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombineOp {
    Add,
    Subtract,
    Common,
}

impl CombineOp {
    pub fn name(self) -> &'static str {
        match self {
            CombineOp::Add => "Add",
            CombineOp::Subtract => "Subtract",
            CombineOp::Common => "Common",
        }
    }
}

/// What cuts bodies apart.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum SplitTool {
    Plane(PlaneRef),
    /// The closed regions of a sketch, through everything both ways.
    Sketch(FeatureId),
}

/// Moves (or copies) bodies: a translation, then a rotation, or a face mated onto a plane.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MoveBody {
    /// A face of each body to move (empty: every body).
    pub bodies: Vec<FaceRef>,
    pub translate: [f64; 3],
    /// About an axis, radians.
    pub rotate: Option<(AxisRef, f64)>,
    /// Mate a planar face of the body onto a plane (coincident, facing away from it).
    pub mate: Option<(FaceRef, PlaneRef)>,
    pub copy: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrapKind {
    Emboss,
    Deboss,
    /// A shallow groove that marks the outline only.
    Scribe,
}

impl WrapKind {
    pub fn name(self) -> &'static str {
        match self {
            WrapKind::Emboss => "Emboss",
            WrapKind::Deboss => "Deboss",
            WrapKind::Scribe => "Scribe",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum FeatureKind {
    /// A sketch on a plane; `on` makes it follow a face or reference plane when the model changes.
    Sketch {
        sketch: Sketch,
        plane: Plane,
        on: Option<PlaneRef>,
    },
    Extrude(Extrude),
    Revolve(Revolve),
    Fillet {
        edges: Vec<EdgeRef>,
        radius: f64,
        /// Also round edges that continue a selected edge smoothly.
        propagate: bool,
    },
    Chamfer {
        edges: Vec<EdgeRef>,
        kind: ChamferKind,
        d1: f64,
        d2: f64,
        /// Radians, for angle-distance.
        angle: f64,
        flip: bool,
    },
    Shell {
        faces: Vec<FaceRef>,
        thickness: f64,
        outward: bool,
    },
    Hole(Hole),
    LinearPattern(LinearPattern),
    CircularPattern(CircularPattern),
    Mirror(Mirror),
    RefPlane(RefPlaneDef),
    RefAxis(RefAxisDef),
    RefPoint(RefPointDef),
    CoordSys(CoordSysDef),
    /// Tilts `faces` by `angle` (radians) about where they cross `neutral`.
    Draft {
        neutral: PlaneRef,
        faces: Vec<FaceRef>,
        angle: f64,
    },
    /// A wall grown from an open sketch line into the part.
    Rib {
        sketch: FeatureId,
        thickness: f64,
        /// Thickness on both sides of the sketch plane, else on the normal side.
        both_sides: bool,
        /// Grow towards the other side of the line.
        flip: bool,
    },
    Sweep(Sweep),
    Loft(Loft),
    /// A reference curve (no material).
    Helix(HelixDef),
    Thread(ThreadDef),
    /// A fillet on one straight edge whose radius changes along it: `(position 0..1, radius)`.
    VariableFillet {
        edge: EdgeRef,
        radii: Vec<[f64; 2]>,
    },
    /// Rounds where face set `a` meets face set `b`.
    FaceFillet {
        a: Vec<FaceRef>,
        b: Vec<FaceRef>,
        radius: f64,
    },
    /// Merges, subtracts or intersects bodies; each body is named by one of its faces.
    Combine {
        op: CombineOp,
        main: FaceRef,
        tools: Vec<FaceRef>,
    },
    Split {
        tool: SplitTool,
        /// Bodies to split (empty: all).
        bodies: Vec<FaceRef>,
    },
    /// Removes the bodies, or with `keep` removes every other body.
    DeleteBody {
        bodies: Vec<FaceRef>,
        keep: bool,
    },
    MoveBody(MoveBody),
    /// Scales bodies (empty: all) about a point; non-uniform with different factors.
    ScaleBody {
        bodies: Vec<FaceRef>,
        about: PointRef,
        factors: [f64; 3],
    },
    /// Repeats the seeds at every point of a sketch, relative to the seeds' center.
    SketchPattern {
        seeds: Seeds,
        sketch: FeatureId,
    },
    /// Repeats the seeds along a curve.
    CurvePattern {
        seeds: Seeds,
        curve: CurveRef,
        /// Instances including the seed.
        count: usize,
        /// Distance along the curve between instances (`None`: spread over the whole curve).
        spacing: Option<f64>,
        /// Turn the instances with the curve.
        align: bool,
    },
    /// Raises (or with `reverse` sinks) a planar face into a smooth cap.
    Dome {
        face: FaceRef,
        height: f64,
        reverse: bool,
    },
    /// Sketch outlines wrapped onto a cylindrical face.
    Wrap {
        sketch: FeatureId,
        face: FaceRef,
        kind: WrapKind,
        depth: f64,
    },
    /// Moves faces `clearance` into the part (flat faces along their normal, cylinders radially)
    /// so printed parts slide together: holes grow, pins and tabs shrink.
    Fit {
        faces: Vec<FaceRef>,
        clearance: f64,
    },
    /// Solids read from a STEP file.
    Imported {
        file: String,
        solids: Shared<Vec<CompressedSolid>>,
    },
    /// A triangle mesh kept for reference: drawn, measured and exported, but not a solid.
    ImportedMesh {
        file: String,
        mesh: Shared<PrintMesh>,
    },
}

/// Big imported data shared by every copy of the tree (undo steps, the rebuild cache); copies are
/// equal when they share the data.
#[derive(Clone, Debug)]
pub struct Shared<T>(pub Arc<T>);

impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(value))
    }
}

impl<T> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: Serialize> Serialize for Shared<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Shared<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(d).map(Self::new)
    }
}

impl FeatureKind {
    /// Name prefix in the feature tree ("Boss-Extrude" + number).
    pub fn prefix(&self) -> &'static str {
        match self {
            FeatureKind::Sketch { .. } => "Sketch",
            FeatureKind::Extrude(e) if e.cut => "Cut-Extrude",
            FeatureKind::Extrude(_) => "Boss-Extrude",
            FeatureKind::Revolve(r) if r.cut => "Cut-Revolve",
            FeatureKind::Revolve(_) => "Revolve",
            FeatureKind::Fillet { .. } => "Fillet",
            FeatureKind::Chamfer { .. } => "Chamfer",
            FeatureKind::Shell { .. } => "Shell",
            FeatureKind::Hole(h) => match h.kind {
                HoleKind::Simple => "Hole",
                HoleKind::Counterbore => "CBORE Hole",
                HoleKind::Countersink => "CSK Hole",
                HoleKind::Tapped => "Tapped Hole",
            },
            FeatureKind::LinearPattern(_) => "LPattern",
            FeatureKind::CircularPattern(_) => "CirPattern",
            FeatureKind::Mirror(_) => "Mirror",
            FeatureKind::RefPlane(_) => "Plane",
            FeatureKind::RefAxis(_) => "Axis",
            FeatureKind::RefPoint(_) => "Point",
            FeatureKind::CoordSys(_) => "Coordinate System",
            FeatureKind::Draft { .. } => "Draft",
            FeatureKind::Rib { .. } => "Rib",
            FeatureKind::Sweep(w) if w.cut => "Cut-Sweep",
            FeatureKind::Sweep(_) => "Sweep",
            FeatureKind::Loft(l) if l.cut => "Cut-Loft",
            FeatureKind::Loft(_) => "Loft",
            FeatureKind::Helix(h) if matches!(h.size, HelixSize::Spiral { .. }) => "Spiral",
            FeatureKind::Helix(_) => "Helix",
            FeatureKind::Thread(_) => "Thread",
            FeatureKind::VariableFillet { .. } => "VarFillet",
            FeatureKind::FaceFillet { .. } => "FaceFillet",
            FeatureKind::Combine { .. } => "Combine",
            FeatureKind::Split { .. } => "Split",
            FeatureKind::DeleteBody { keep: true, .. } => "Body-Keep",
            FeatureKind::DeleteBody { .. } => "Body-Delete",
            FeatureKind::MoveBody(m) if m.copy => "Body-Copy",
            FeatureKind::MoveBody(_) => "Body-Move",
            FeatureKind::ScaleBody { .. } => "Scale",
            FeatureKind::SketchPattern { .. } => "SketchPattern",
            FeatureKind::CurvePattern { .. } => "CrvPattern",
            FeatureKind::Dome { .. } => "Dome",
            FeatureKind::Wrap { .. } => "Wrap",
            FeatureKind::Fit { .. } => "Fit",
            FeatureKind::Imported { .. } => "Imported",
            FeatureKind::ImportedMesh { .. } => "Imported Mesh",
        }
    }

    /// Makes a curve, not material.
    pub fn is_curve(&self) -> bool {
        matches!(self, FeatureKind::Helix(_))
    }

    pub fn is_sketch(&self) -> bool {
        matches!(self, FeatureKind::Sketch { .. })
    }

    /// Every feature this one is built from: sketches, referenced faces, planes, axes, seeds.
    pub fn references(&self) -> Vec<FeatureId> {
        let mut out = Vec::new();
        let face = |f: &FaceRef, out: &mut Vec<FeatureId>| out.push(f.tag.feature);
        let edge = |e: &EdgeRef, out: &mut Vec<FeatureId>| out.extend(e.faces.iter().map(|t| t.feature));
        let vertex = |v: &VertexRef, out: &mut Vec<FeatureId>| out.extend(v.faces.iter().map(|t| t.feature));
        let plane = |p: &PlaneRef, out: &mut Vec<FeatureId>| match p {
            PlaneRef::Std(_) => {}
            PlaneRef::Face(f) => face(f, out),
            PlaneRef::Feature(id) => out.push(*id),
        };
        let axis = |a: &AxisRef, out: &mut Vec<FeatureId>| match a {
            AxisRef::Std(_) => {}
            AxisRef::Edge(e) => edge(e, out),
            AxisRef::Face(f) => face(f, out),
            AxisRef::Feature(id) => out.push(*id),
        };
        let point = |p: &PointRef, out: &mut Vec<FeatureId>| match p {
            PointRef::Origin => {}
            PointRef::Vertex(v) => vertex(v, out),
            PointRef::Feature(id) => out.push(*id),
        };
        let end = |e: &End, out: &mut Vec<FeatureId>| match e {
            End::UpToVertex(v) => vertex(v, out),
            End::UpToSurface(f) | End::OffsetFromSurface(f, _) => face(f, out),
            _ => {}
        };
        let curve = |c: &CurveRef, out: &mut Vec<FeatureId>| match c {
            CurveRef::Sketch(id) | CurveRef::Helix(id) => out.push(*id),
        };
        let seeds = |s: &Seeds, out: &mut Vec<FeatureId>| {
            if let Seeds::Features(f) = s {
                out.extend(f);
            }
        };
        match self {
            FeatureKind::Sketch { on, .. } => {
                if let Some(p) = on {
                    plane(p, &mut out);
                }
            }
            FeatureKind::Extrude(e) => {
                out.push(e.sketch);
                end(&e.end, &mut out);
                if let Some(e2) = &e.end2 {
                    end(e2, &mut out);
                }
            }
            FeatureKind::Revolve(r) => {
                out.push(r.sketch);
                if let Some(RevolveAxis::Axis(a)) = &r.axis {
                    axis(a, &mut out);
                }
            }
            FeatureKind::Fillet { edges, .. } | FeatureKind::Chamfer { edges, .. } => {
                edges.iter().for_each(|e| edge(e, &mut out))
            }
            FeatureKind::Shell { faces, .. } | FeatureKind::Fit { faces, .. } => {
                faces.iter().for_each(|f| face(f, &mut out))
            }
            FeatureKind::Hole(h) => {
                out.push(h.sketch);
                end(&h.end, &mut out);
            }
            FeatureKind::LinearPattern(p) => {
                seeds(&p.seeds, &mut out);
                axis(&p.dir1.axis, &mut out);
                if let Some(d) = &p.dir2 {
                    axis(&d.axis, &mut out);
                }
            }
            FeatureKind::CircularPattern(p) => {
                seeds(&p.seeds, &mut out);
                axis(&p.axis, &mut out);
            }
            FeatureKind::Mirror(m) => {
                seeds(&m.seeds, &mut out);
                plane(&m.plane, &mut out);
            }
            FeatureKind::RefPlane(d) => match d {
                RefPlaneDef::Offset { base, .. } => plane(base, &mut out),
                RefPlaneDef::Angle { base, axis: a, .. } => {
                    plane(base, &mut out);
                    axis(a, &mut out);
                }
                RefPlaneDef::ThreePoint(p) => p.iter().for_each(|p| point(p, &mut out)),
                RefPlaneDef::Mid(a, b) => {
                    plane(a, &mut out);
                    plane(b, &mut out);
                }
            },
            FeatureKind::RefAxis(d) => match d {
                RefAxisDef::TwoPoints(a, b) => {
                    point(a, &mut out);
                    point(b, &mut out);
                }
                RefAxisDef::Edge(e) => edge(e, &mut out),
                RefAxisDef::Cylinder(f) => face(f, &mut out),
                RefAxisDef::TwoPlanes(a, b) => {
                    plane(a, &mut out);
                    plane(b, &mut out);
                }
            },
            FeatureKind::RefPoint(d) => match d {
                RefPointDef::Vertex(v) => vertex(v, &mut out),
                RefPointDef::FaceCenter(f) => face(f, &mut out),
                RefPointDef::ArcCenter(e) => edge(e, &mut out),
                RefPointDef::Coords(_) => {}
            },
            FeatureKind::CoordSys(c) => {
                point(&c.origin, &mut out);
                for a in [&c.x, &c.z].into_iter().flatten() {
                    axis(a, &mut out);
                }
            }
            FeatureKind::Draft { neutral, faces, .. } => {
                plane(neutral, &mut out);
                faces.iter().for_each(|f| face(f, &mut out));
            }
            FeatureKind::Rib { sketch, .. } => out.push(*sketch),
            FeatureKind::Sweep(w) => {
                if let SweepProfile::Sketch(id) = w.profile {
                    out.push(id);
                }
                curve(&w.path, &mut out);
            }
            FeatureKind::Loft(l) => {
                out.extend(&l.profiles);
                out.extend(&l.guides);
            }
            FeatureKind::Helix(h) => out.push(h.sketch),
            FeatureKind::Thread(t) => face(&t.face, &mut out),
            FeatureKind::VariableFillet { edge: e, .. } => edge(e, &mut out),
            FeatureKind::FaceFillet { a, b, .. } => a.iter().chain(b).for_each(|f| face(f, &mut out)),
            FeatureKind::Combine { main, tools, .. } => {
                std::iter::once(main).chain(tools).for_each(|f| face(f, &mut out))
            }
            FeatureKind::Split { tool, bodies } => {
                match tool {
                    SplitTool::Plane(p) => plane(p, &mut out),
                    SplitTool::Sketch(id) => out.push(*id),
                }
                bodies.iter().for_each(|f| face(f, &mut out));
            }
            FeatureKind::DeleteBody { bodies, .. } => bodies.iter().for_each(|f| face(f, &mut out)),
            FeatureKind::MoveBody(m) => {
                m.bodies.iter().for_each(|f| face(f, &mut out));
                if let Some((a, _)) = &m.rotate {
                    axis(a, &mut out);
                }
                if let Some((f, p)) = &m.mate {
                    face(f, &mut out);
                    plane(p, &mut out);
                }
            }
            FeatureKind::ScaleBody { bodies, about, .. } => {
                bodies.iter().for_each(|f| face(f, &mut out));
                point(about, &mut out);
            }
            FeatureKind::SketchPattern { seeds: sd, sketch } => {
                seeds(sd, &mut out);
                out.push(*sketch);
            }
            FeatureKind::CurvePattern {
                seeds: sd, curve: c, ..
            } => {
                seeds(sd, &mut out);
                curve(c, &mut out);
            }
            FeatureKind::Dome { face: f, .. } => face(f, &mut out),
            FeatureKind::Wrap { sketch, face: f, .. } => {
                out.push(*sketch);
                face(f, &mut out);
            }
            FeatureKind::Imported { .. } | FeatureKind::ImportedMesh { .. } => {}
        }
        out.sort();
        out.dedup();
        out
    }
}
