//! Persistent names for faces, edges and vertices, so a fillet or a sketch-on-face still finds its
//! geometry after the part rebuilds with new dimensions.
//!
//! Every face a feature's tool creates is tagged (feature, pattern instance, profile region, which
//! tool face). Booleans split and merge faces but never invent new surfaces, so after each
//! operation a result face inherits the tag of the source face it lies on. Edges are named by the
//! two faces they separate and vertices by the faces around them; when several share a name they
//! are told apart by position.

use threebcad_kernel::topology::{Topology, P3};
use threebcad_kernel::{Tool, ToolFace};

use crate::FeatureId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FaceTag {
    pub feature: FeatureId,
    /// Pattern / mirror instance (0 for the feature itself).
    pub instance: u32,
    /// Profile region of the tool.
    pub region: u32,
    pub face: ToolFace,
}

impl FaceTag {
    /// A face whose origin could not be traced.
    pub fn unknown(feature: FeatureId) -> Self {
        Self {
            feature,
            instance: 0,
            region: 0,
            face: ToolFace::Side(usize::MAX),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FaceRef {
    pub tag: FaceTag,
    /// Which one, when a face was split into several with the same tag.
    pub index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeRef {
    /// Tags of the two faces, sorted.
    pub faces: [FaceTag; 2],
    pub index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VertexRef {
    /// Tags of the faces meeting at the vertex, sorted.
    pub faces: Vec<FaceTag>,
    pub index: usize,
}

/// What naming needs to know about a body.
pub trait Named {
    fn topo(&self) -> &Topology;
    fn tags(&self) -> &[FaceTag];
}

fn order_key(p: P3) -> [i64; 3] {
    // Round so tiny rebuild noise does not reorder twins.
    p.map(|v| (v * 1e4).round() as i64)
}

/// Tags of a tool's faces.
pub fn tool_tags(topo: &Topology, tool: &Tool, feature: FeatureId, instance: u32, region: u32) -> Vec<FaceTag> {
    topo.faces
        .iter()
        .map(|f| FaceTag {
            feature,
            instance,
            region,
            face: tool.face_at(f.sample),
        })
        .collect()
}

/// Tags for `topo`'s faces, each copied from the nearest face among `sources`.
pub fn inherit_tags(topo: &Topology, sources: &[(&Topology, &[FaceTag])], fallback: FeatureId) -> Vec<FaceTag> {
    topo.faces
        .iter()
        .map(|f| {
            let mut best = (f64::INFINITY, FaceTag::unknown(fallback));
            for (src, tags) in sources {
                for (sf, tag) in src.faces.iter().zip(tags.iter()) {
                    let d = sf.distance(f.sample);
                    if d < best.0 {
                        best = (d, *tag);
                    }
                }
            }
            best.1
        })
        .collect()
}

fn edge_key<B: Named>(b: &B, e: usize) -> [FaceTag; 2] {
    let edge = &b.topo().edges[e];
    let t = |i: usize| b.tags()[edge.sides[i].face];
    let mut k = match edge.sides.len() {
        0 => [FaceTag::unknown(FeatureId::end()); 2],
        1 => [t(0), t(0)],
        _ => [t(0), t(1)],
    };
    k.sort();
    k
}

fn vertex_key<B: Named>(b: &B, v: usize) -> Vec<FaceTag> {
    let mut k: Vec<FaceTag> = b.topo().vertex_faces[v].iter().map(|&f| b.tags()[f]).collect();
    k.sort();
    k.dedup();
    k
}

/// Every (body, element) with `matches`, in a stable order: by position, because the order of the
/// bodies changes when an edit merges one of them.
fn candidates<B: Named>(
    bodies: &[B],
    count: impl Fn(&B) -> usize,
    matches: impl Fn(&B, usize) -> bool,
    at: impl Fn(&B, usize) -> P3,
) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize, [i64; 3])> = Vec::new();
    for (bi, b) in bodies.iter().enumerate() {
        for i in (0..count(b)).filter(|&i| matches(b, i)) {
            out.push((bi, i, order_key(at(b, i))));
        }
    }
    out.sort_by_key(|&(_, _, k)| k);
    out.into_iter().map(|(b, i, _)| (b, i)).collect()
}

fn face_candidates<B: Named>(bodies: &[B], tag: FaceTag) -> Vec<(usize, usize)> {
    candidates(
        bodies,
        |b| b.tags().len(),
        |b, i| b.tags()[i] == tag,
        |b, i| b.topo().faces[i].sample,
    )
}

fn edge_candidates<B: Named>(bodies: &[B], key: [FaceTag; 2]) -> Vec<(usize, usize)> {
    candidates(
        bodies,
        |b| b.topo().edges.len(),
        |b, i| edge_key(b, i) == key,
        |b, i| b.topo().edges[i].mid,
    )
}

fn vertex_candidates<B: Named>(bodies: &[B], key: &[FaceTag]) -> Vec<(usize, usize)> {
    candidates(
        bodies,
        |b| b.topo().vertices.len(),
        |b, i| vertex_key(b, i) == key,
        |b, i| b.topo().vertices[i],
    )
}

pub fn face_ref<B: Named>(bodies: &[B], body: usize, face: usize) -> Option<FaceRef> {
    let tag = *bodies.get(body)?.tags().get(face)?;
    let index = face_candidates(bodies, tag).iter().position(|&c| c == (body, face))?;
    Some(FaceRef { tag, index })
}

pub fn edge_ref<B: Named>(bodies: &[B], body: usize, edge: usize) -> Option<EdgeRef> {
    let b = bodies.get(body)?;
    (edge < b.topo().edges.len()).then_some(())?;
    let faces = edge_key(b, edge);
    let index = edge_candidates(bodies, faces).iter().position(|&c| c == (body, edge))?;
    Some(EdgeRef { faces, index })
}

pub fn vertex_ref<B: Named>(bodies: &[B], body: usize, vertex: usize) -> Option<VertexRef> {
    let b = bodies.get(body)?;
    (vertex < b.topo().vertices.len()).then_some(())?;
    let faces = vertex_key(b, vertex);
    let index = vertex_candidates(bodies, &faces)
        .iter()
        .position(|&c| c == (body, vertex))?;
    Some(VertexRef { faces, index })
}

/// (body, face) a reference names now; an out-of-range index falls back to the last twin.
pub fn resolve_face<B: Named>(bodies: &[B], r: &FaceRef) -> Option<(usize, usize)> {
    let c = face_candidates(bodies, r.tag);
    c.get(r.index).or(c.last()).copied()
}

pub fn resolve_edge<B: Named>(bodies: &[B], r: &EdgeRef) -> Option<(usize, usize)> {
    let c = edge_candidates(bodies, r.faces);
    c.get(r.index).or(c.last()).copied()
}

pub fn resolve_vertex<B: Named>(bodies: &[B], r: &VertexRef) -> Option<(usize, usize)> {
    let c = vertex_candidates(bodies, &r.faces);
    c.get(r.index).or(c.last()).copied()
}
