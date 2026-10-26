use std::collections::HashMap;

use anyhow::{bail, ensure, Result};
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use petgraph::visit::Dfs;
use threebcad_kernel::operations::{self, ProfileRegion, ProfileSegment};
use threebcad_kernel::{Plane, Solid};
use threebcad_sketch::{Segment, Sketch, Status};

use crate::{Feature, FeatureKind};

pub type FeatureId = NodeIndex;

/// The FeatureManager tree: features in build order, a dependency DAG
/// (edge = "is built from") and a SolidWorks-style rollback bar.
#[derive(Clone, Debug, Default)]
pub struct History {
    graph: StableDiGraph<Feature, ()>,
    order: Vec<FeatureId>,
    /// Number of features above the rollback bar (the ones that get built).
    rollback: usize,
}

/// Result of evaluating the tree up to the rollback bar.
#[derive(Default)]
pub struct Rebuild {
    pub bodies: Vec<(FeatureId, Solid)>,
    pub errors: HashMap<FeatureId, String>,
    /// Definition status and remaining degrees of freedom of each built sketch.
    pub sketch_status: HashMap<FeatureId, (Status, usize)>,
}

impl History {
    pub fn add_sketch(&mut self, sketch: Sketch, plane: Plane) -> FeatureId {
        let name = self.next_name("Sketch");
        self.insert(Feature {
            name,
            kind: FeatureKind::Sketch { sketch, plane },
        })
    }

    pub fn add_extrude(&mut self, sketch: FeatureId, depth: f64) -> Result<FeatureId> {
        let is_sketch = matches!(self.get(sketch).map(|f| &f.kind), Some(FeatureKind::Sketch { .. }));
        ensure!(is_sketch && self.is_active(sketch), "Extrude needs an active sketch");
        let name = self.next_name("Boss-Extrude");
        let id = self.insert(Feature {
            name,
            kind: FeatureKind::ExtrudeBoss { sketch, depth },
        });
        self.graph.add_edge(sketch, id, ());
        Ok(id)
    }

    /// New features go in at the rollback bar, like SolidWorks.
    fn insert(&mut self, feature: Feature) -> FeatureId {
        let id = self.graph.add_node(feature);
        self.order.insert(self.rollback, id);
        self.rollback += 1;
        id
    }

    fn next_name(&self, prefix: &str) -> String {
        let n = self
            .graph
            .node_weights()
            .filter_map(|f| f.name.strip_prefix(prefix)?.parse::<usize>().ok())
            .max();
        format!("{prefix}{}", n.unwrap_or(0) + 1)
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Features in build order.
    pub fn iter(&self) -> impl Iterator<Item = (FeatureId, &Feature)> {
        self.order.iter().map(|&id| (id, &self.graph[id]))
    }

    pub fn get(&self, id: FeatureId) -> Option<&Feature> {
        self.graph.node_weight(id)
    }

    pub fn get_mut(&mut self, id: FeatureId) -> Option<&mut Feature> {
        self.graph.node_weight_mut(id)
    }

    pub fn rollback(&self) -> usize {
        self.rollback
    }

    pub fn set_rollback(&mut self, n: usize) {
        self.rollback = n.min(self.order.len());
    }

    pub fn is_active(&self, id: FeatureId) -> bool {
        self.order[..self.rollback].contains(&id)
    }

    /// Deletes a feature and everything built from it. Returns how many went.
    pub fn remove(&mut self, id: FeatureId) -> usize {
        let mut doomed = Vec::new();
        let mut dfs = Dfs::new(&self.graph, id);
        while let Some(n) = dfs.next(&self.graph) {
            doomed.push(n);
        }
        for &n in &doomed {
            if let Some(pos) = self.order.iter().position(|&o| o == n) {
                if pos < self.rollback {
                    self.rollback -= 1;
                }
                self.order.remove(pos);
            }
            self.graph.remove_node(n);
        }
        doomed.len()
    }

    /// Evaluates every feature above the rollback bar.
    // shortcut: full rebuild on every edit; propagate dirty flags along graph edges once parts get big.
    pub fn rebuild(&mut self) -> Rebuild {
        let mut out = Rebuild::default();
        for i in 0..self.rollback {
            let id = self.order[i];
            let extrude = match self.graph[id].kind {
                FeatureKind::ExtrudeBoss { sketch, depth } => Some((sketch, depth)),
                FeatureKind::Sketch { .. } => None,
            };
            let result = match extrude {
                Some((sketch, depth)) => self
                    .extrude(sketch, depth)
                    .map(|solids| out.bodies.extend(solids.into_iter().map(|s| (id, s)))),
                None => self.solve_sketch(id, &mut out),
            };
            if let Err(e) = result {
                out.errors.insert(id, e.to_string());
            }
        }
        out
    }

    fn solve_sketch(&mut self, id: FeatureId, out: &mut Rebuild) -> Result<()> {
        let FeatureKind::Sketch { sketch, .. } = &mut self.graph[id].kind else {
            unreachable!()
        };
        let report = sketch.solve();
        let analysis = sketch.analyze();
        out.sketch_status.insert(id, (analysis.status, analysis.dof));
        ensure!(
            report.converged,
            "Sketch cannot be solved: over-defined or conflicting (residual {:.1e})",
            report.residual
        );
        Ok(())
    }

    fn extrude(&self, sketch: FeatureId, depth: f64) -> Result<Vec<Solid>> {
        let Some(FeatureKind::Sketch { sketch, plane }) = self.get(sketch).map(|f| &f.kind) else {
            bail!("Referenced sketch is missing")
        };
        let regions: Vec<ProfileRegion> = sketch
            .regions()
            .map_err(anyhow::Error::msg)?
            .iter()
            .map(|r| ProfileRegion {
                outer: r.outer.iter().map(to_kernel).collect(),
                holes: r.holes.iter().map(|h| h.iter().map(to_kernel).collect()).collect(),
            })
            .collect();
        operations::extrude(&regions, depth, plane)
    }
}

fn to_kernel(s: &Segment) -> ProfileSegment {
    match *s {
        Segment::Line { from, to } => ProfileSegment::Line { from, to },
        Segment::Arc { center, from, to, ccw } => ProfileSegment::Arc { center, from, to, ccw },
        Segment::Circle { center, radius, ccw } => ProfileSegment::Circle { center, radius, ccw },
        Segment::Bezier(b) => ProfileSegment::Bezier(b),
    }
}
