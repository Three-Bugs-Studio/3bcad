use std::collections::HashMap;

use threebcad_kernel::Solid;
use threebcad_sketch::Sketch;

use crate::{FeatureId, History, Rebuild};

const UNDO_LIMIT: usize = 100;

/// A part: its feature history, the bodies it builds, and undo/redo.
// shortcut: assemblies and save/load are not modelled yet.
pub struct PartDocument {
    pub name: String,
    history: History,
    undo: Vec<History>,
    redo: Vec<History>,
    merge_key: Option<u64>,
    built: Rebuild,
    revision: u64,
}

impl PartDocument {
    pub fn new(name: impl Into<String>) -> Self {
        let mut doc = Self {
            name: name.into(),
            history: History::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            merge_key: None,
            built: Rebuild::default(),
            revision: 0,
        };
        doc.rebuild();
        doc
    }

    /// Starter part: a 30 mm cube and a 20 mm diameter cylinder.
    pub fn demo() -> Self {
        let mut doc = Self::new("Part1");
        doc.edit(|h| -> anyhow::Result<()> {
            let s = h.add_sketch(Sketch::rectangle(30.0, 30.0));
            h.add_extrude(s, 30.0)?;
            let c = h.add_sketch(Sketch::circle([55.0, 15.0], 10.0));
            h.add_extrude(c, 40.0)?;
            Ok(())
        })
        .expect("demo part builds");
        doc.undo.clear();
        doc
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn bodies(&self) -> &[(FeatureId, Solid)] {
        &self.built.bodies
    }

    pub fn errors(&self) -> &HashMap<FeatureId, String> {
        &self.built.errors
    }

    /// Bumps on every rebuild; viewers compare it to know when to re-mesh.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn edit<R>(&mut self, f: impl FnOnce(&mut History) -> R) -> R {
        self.edit_merging(None, f)
    }

    /// Consecutive edits with the same `key` collapse into one undo step,
    /// so dragging a dimension does not flood the undo stack.
    pub fn edit_merging<R>(&mut self, key: Option<u64>, f: impl FnOnce(&mut History) -> R) -> R {
        if key.is_none() || key != self.merge_key {
            self.undo.push(self.history.clone());
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.merge_key = key;
        let r = f(&mut self.history);
        self.rebuild();
        r
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.history, prev));
            self.merge_key = None;
            self.rebuild();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.history, next));
            self.merge_key = None;
            self.rebuild();
        }
    }

    fn rebuild(&mut self) {
        self.built = self.history.rebuild();
        self.revision += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FeatureKind;

    #[test]
    fn demo_builds_two_bodies() {
        let doc = PartDocument::demo();
        assert_eq!(doc.bodies().len(), 2);
        assert!(doc.errors().is_empty());
        let names: Vec<_> = doc.history().iter().map(|(_, f)| f.name.as_str()).collect();
        assert_eq!(names, ["Sketch1", "Boss-Extrude1", "Sketch2", "Boss-Extrude2"]);
    }

    #[test]
    fn rollback_suppresses_later_features_and_inserts_at_bar() {
        let mut doc = PartDocument::demo();
        doc.edit(|h| h.set_rollback(2));
        assert_eq!(doc.bodies().len(), 1);
        let s = doc.edit(|h| h.add_sketch(Sketch::circle([0.0, 0.0], 3.0)));
        assert_eq!(doc.history().iter().position(|(id, _)| id == s), Some(2));
        assert!(doc.edit(|h| h.add_extrude(s, 5.0)).is_ok());
        assert_eq!(doc.bodies().len(), 2);
        // A sketch below the bar cannot be extruded.
        let below = doc.history().iter().nth(4).unwrap().0;
        assert!(doc.edit(|h| h.add_extrude(below, 5.0)).is_err());
    }

    #[test]
    fn deleting_a_sketch_cascades_and_undo_restores() {
        let mut doc = PartDocument::demo();
        let sketch1 = doc.history().iter().next().unwrap().0;
        assert_eq!(doc.edit(|h| h.remove(sketch1)), 2);
        assert_eq!(
            (doc.history().len(), doc.history().rollback(), doc.bodies().len()),
            (2, 2, 1)
        );
        doc.undo();
        assert_eq!((doc.history().len(), doc.bodies().len()), (4, 2));
        doc.redo();
        assert_eq!(doc.history().len(), 2);
    }

    #[test]
    fn dimension_drag_is_one_undo_step_and_rebuilds() {
        let mut doc = PartDocument::demo();
        let extrude = doc.history().iter().nth(1).unwrap().0;
        for depth in [31.0, 32.0, 33.0] {
            doc.edit_merging(Some(1), |h| {
                if let Some(FeatureKind::ExtrudeBoss { depth: d, .. }) = h.get_mut(extrude).map(|f| &mut f.kind) {
                    *d = depth;
                }
            });
        }
        doc.undo();
        assert!(!doc.can_undo());
        assert!(
            matches!(doc.history().get(extrude).unwrap().kind, FeatureKind::ExtrudeBoss { depth, .. } if depth == 30.0)
        );
    }

    #[test]
    fn bad_dimension_reports_error_on_feature() {
        let mut doc = PartDocument::demo();
        let extrude = doc.history().iter().nth(1).unwrap().0;
        doc.edit(|h| {
            if let Some(FeatureKind::ExtrudeBoss { depth, .. }) = h.get_mut(extrude).map(|f| &mut f.kind) {
                *depth = 0.0;
            }
        });
        assert!(doc.errors().contains_key(&extrude));
        assert_eq!(doc.bodies().len(), 1);
    }
}
