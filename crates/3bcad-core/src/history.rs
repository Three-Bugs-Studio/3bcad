use anyhow::{bail, ensure, Result};
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use petgraph::visit::{Dfs, EdgeRef};
use petgraph::Direction;
use threebcad_kernel::Plane;
use threebcad_sketch::Sketch;

use crate::feature::{Extrude, PlaneRef};
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

impl History {
    pub fn add_sketch(&mut self, sketch: Sketch, plane: Plane) -> FeatureId {
        self.insert(FeatureKind::Sketch {
            sketch,
            plane,
            on: None,
        })
    }

    /// A sketch that follows a face or reference plane when the model changes.
    pub fn add_sketch_on(&mut self, sketch: Sketch, plane: Plane, on: PlaneRef) -> Result<FeatureId> {
        self.add(FeatureKind::Sketch {
            sketch,
            plane,
            on: Some(on),
        })
    }

    /// Blind Boss-Extrude, merged with what it touches.
    pub fn add_extrude(&mut self, sketch: FeatureId, depth: f64) -> Result<FeatureId> {
        self.add(FeatureKind::Extrude(Extrude::boss(sketch, depth)))
    }

    /// Adds any feature at the rollback bar after checking that what it is built from exists and
    /// is above the bar.
    pub fn add(&mut self, kind: FeatureKind) -> Result<FeatureId> {
        self.check_refs(&kind, self.rollback)?;
        let id = self.insert(kind);
        self.relink();
        Ok(id)
    }

    fn check_refs(&self, kind: &FeatureKind, pos: usize) -> Result<()> {
        for r in kind.references() {
            ensure!(self.get(r).is_some(), "a feature this one uses was deleted");
            let at = self.position(r).unwrap_or(usize::MAX);
            ensure!(
                at < pos,
                "{} is below the rollback bar or after this feature",
                self.graph[r].name
            );
        }
        let needs_sketch = match kind {
            FeatureKind::Extrude(e) => Some(e.sketch),
            FeatureKind::Revolve(r) => Some(r.sketch),
            FeatureKind::Hole(h) => Some(h.sketch),
            FeatureKind::Rib { sketch, .. } => Some(*sketch),
            _ => None,
        };
        if let Some(s) = needs_sketch {
            ensure!(
                self.get(s).is_some_and(|f| f.kind.is_sketch()),
                "{} needs a sketch",
                kind.prefix()
            );
        }
        Ok(())
    }

    /// New features go in at the rollback bar, like SolidWorks.
    fn insert(&mut self, kind: FeatureKind) -> FeatureId {
        let name = self.next_name(kind.prefix());
        let id = self.graph.add_node(Feature {
            name,
            kind,
            suppressed: false,
        });
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

    /// Rebuilds the dependency edges from what every feature references.
    pub fn relink(&mut self) {
        self.graph.clear_edges();
        let links: Vec<(FeatureId, FeatureId)> = self
            .order
            .iter()
            .flat_map(|&id| {
                self.graph[id]
                    .kind
                    .references()
                    .into_iter()
                    .filter(|r| self.graph.contains_node(*r))
                    .map(move |r| (r, id))
            })
            .collect();
        for (a, b) in links {
            self.graph.add_edge(a, b, ());
        }
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

    pub fn position(&self, id: FeatureId) -> Option<usize> {
        self.order.iter().position(|&o| o == id)
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

    /// Features this one is built from.
    pub fn parents(&self, id: FeatureId) -> Vec<FeatureId> {
        self.graph
            .edges_directed(id, Direction::Incoming)
            .map(|e| e.source())
            .collect()
    }

    /// Features built from this one (directly).
    pub fn children(&self, id: FeatureId) -> Vec<FeatureId> {
        self.graph
            .edges_directed(id, Direction::Outgoing)
            .map(|e| e.target())
            .collect()
    }

    /// This feature and everything built from it, directly or not.
    pub fn descendants(&self, id: FeatureId) -> Vec<FeatureId> {
        let mut out = Vec::new();
        let mut dfs = Dfs::new(&self.graph, id);
        while let Some(n) = dfs.next(&self.graph) {
            out.push(n);
        }
        out
    }

    /// Deletes a feature and everything built from it. Returns how many went.
    pub fn remove(&mut self, id: FeatureId) -> usize {
        let doomed = self.descendants(id);
        for &n in &doomed {
            if let Some(pos) = self.position(n) {
                if pos < self.rollback {
                    self.rollback -= 1;
                }
                self.order.remove(pos);
            }
            self.graph.remove_node(n);
        }
        doomed.len()
    }

    /// Moves a feature to `to` (index in build order), refusing moves that would put it above
    /// something it is built from or below something built from it.
    pub fn move_to(&mut self, id: FeatureId, to: usize) -> Result<()> {
        let Some(from) = self.position(id) else {
            bail!("no such feature")
        };
        let to = to.min(self.order.len() - 1);
        if from == to {
            return Ok(());
        }
        let mut order = self.order.clone();
        order.remove(from);
        order.insert(to, id);
        let pos = |f: FeatureId| order.iter().position(|&o| o == f).unwrap_or(0);
        let name = |f: FeatureId| self.graph[f].name.clone();
        for p in self.parents(id) {
            ensure!(pos(p) < to, "{} must stay below {}, its parent", name(id), name(p));
        }
        for c in self.children(id) {
            ensure!(pos(c) > to, "{} must stay above {}, its child", name(id), name(c));
        }
        let was_active = from < self.rollback;
        self.order = order;
        if was_active && to >= self.rollback {
            self.rollback = to + 1;
        }
        Ok(())
    }

    pub fn rename(&mut self, id: FeatureId, name: &str) -> Result<()> {
        let name = name.trim();
        ensure!(!name.is_empty(), "a name cannot be empty");
        ensure!(
            !self.iter().any(|(o, f)| o != id && f.name == name),
            "another feature is already called {name}"
        );
        if let Some(f) = self.get_mut(id) {
            f.name = name.to_string();
        }
        Ok(())
    }

    pub fn set_suppressed(&mut self, id: FeatureId, suppressed: bool) {
        if let Some(f) = self.get_mut(id) {
            f.suppressed = suppressed;
        }
    }

    /// Suppressed itself or built from something suppressed.
    pub fn is_suppressed(&self, id: FeatureId) -> bool {
        self.order
            .iter()
            .filter(|&&o| self.graph[o].suppressed)
            .any(|&o| self.descendants(o).contains(&id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> (History, [FeatureId; 4]) {
        let mut h = History::default();
        let s1 = h.add_sketch(Sketch::rectangle(10.0, 10.0), Plane::TOP);
        let e1 = h.add_extrude(s1, 5.0).unwrap();
        let s2 = h.add_sketch(Sketch::circle([0.0, 0.0], 2.0), Plane::TOP);
        let e2 = h.add_extrude(s2, 5.0).unwrap();
        (h, [s1, e1, s2, e2])
    }

    #[test]
    fn reorder_keeps_parents_above_children() {
        let (mut h, [s1, e1, s2, e2]) = demo();
        // The second sketch can move above the first extrude (it does not use it).
        h.move_to(s2, 1).unwrap();
        assert_eq!(h.iter().map(|(i, _)| i).collect::<Vec<_>>(), [s1, s2, e1, e2]);
        // An extrude cannot go above its sketch; a sketch cannot go below its extrude.
        assert!(h.move_to(e2, 0).is_err());
        assert!(h.move_to(s1, 3).is_err());
        assert_eq!(h.rollback(), 4);
    }

    #[test]
    fn suppression_spreads_to_children_and_rename_checks_names() {
        let (mut h, [s1, e1, s2, e2]) = demo();
        h.set_suppressed(s1, true);
        assert!(h.is_suppressed(e1) && h.is_suppressed(s1));
        assert!(!h.is_suppressed(s2) && !h.is_suppressed(e2));
        assert!(h.rename(e1, "Base").is_ok());
        assert!(h.rename(e2, "Base").is_err());
        assert!(h.rename(e2, "  ").is_err());
    }
}
