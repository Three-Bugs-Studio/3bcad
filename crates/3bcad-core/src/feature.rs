use threebcad_kernel::Plane;
use threebcad_sketch::{EntityId, Sketch};

use crate::naming::{EdgeRef, FaceRef, VertexRef};
use crate::FeatureId;

#[derive(Clone, Debug, PartialEq)]
pub struct Feature {
    pub name: String,
    pub kind: FeatureKind,
    /// Left out of the rebuild without being deleted; children follow.
    pub suppressed: bool,
}

/// Front / Top / Right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
#[derive(Clone, Debug, PartialEq)]
pub enum PlaneRef {
    Std(StdPlane),
    /// A planar model face.
    Face(FaceRef),
    /// A reference plane feature.
    Feature(FeatureId),
}

/// A direction or line: pattern directions, revolve and circular pattern axes.
#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub enum PointRef {
    Origin,
    Vertex(VertexRef),
    /// A reference point feature.
    Feature(FeatureId),
}

/// How far an extrusion goes in one direction.
#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThinKind {
    OneDirection,
    MidPlane,
    TwoDirection,
}

/// Thin feature: a wall of thickness `t1` (and `t2` for two directions) along the profile.
#[derive(Clone, Debug, PartialEq)]
pub struct Thin {
    pub kind: ThinKind,
    pub t1: f64,
    pub t2: f64,
    /// One direction: put the wall on the other side of the profile.
    pub reverse: bool,
}

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub enum RevolveAxis {
    /// A line (usually a centerline) of the profile sketch.
    SketchLine(EntityId),
    Axis(AxisRef),
}

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChamferKind {
    EqualDistance,
    DistanceDistance,
    AngleDistance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoleStandard {
    Iso,
    Ansi,
}

#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
pub enum Seeds {
    Features(Vec<FeatureId>),
    /// Every body that exists before the pattern.
    Bodies,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PatternDir {
    pub axis: AxisRef,
    pub spacing: f64,
    /// Instances including the seed.
    pub count: usize,
    pub reverse: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinearPattern {
    pub seeds: Seeds,
    pub dir1: PatternDir,
    pub dir2: Option<PatternDir>,
    /// (index along direction 1, index along direction 2) of instances to leave out.
    pub skip: Vec<(usize, usize)>,
}

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub struct Mirror {
    pub seeds: Seeds,
    pub plane: PlaneRef,
    /// Mirrored bodies: merge with the bodies they touch.
    pub merge: bool,
}

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub enum RefAxisDef {
    TwoPoints(PointRef, PointRef),
    Edge(EdgeRef),
    Cylinder(FaceRef),
    TwoPlanes(PlaneRef, PlaneRef),
}

#[derive(Clone, Debug, PartialEq)]
pub enum RefPointDef {
    Vertex(VertexRef),
    FaceCenter(FaceRef),
    /// Center of a circular edge.
    ArcCenter(EdgeRef),
    Coords([f64; 3]),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CoordSysDef {
    pub origin: PointRef,
    /// X axis direction (default world X).
    pub x: Option<AxisRef>,
    /// Z axis direction (default world Z).
    pub z: Option<AxisRef>,
}

#[derive(Clone, Debug, PartialEq)]
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
}

impl FeatureKind {
    /// Name prefix in the FeatureManager ("Boss-Extrude" + number).
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
        }
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
            FeatureKind::Shell { faces, .. } => faces.iter().for_each(|f| face(f, &mut out)),
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
        }
        out.sort();
        out.dedup();
        out
    }
}
