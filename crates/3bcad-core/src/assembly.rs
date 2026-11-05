//! Assemblies: parts (and other assemblies) placed as components and held together by mates,
//! with exploded views and a bill of materials.
//!
//! An assembly file only says which files it uses and where they sit; the parts stay in their own
//! files and are read again whenever they change on disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use anyhow::{anyhow, bail, ensure, Context};
use serde::{Deserialize, Serialize};
use threebcad_kernel::interference::{self, Interference};
use threebcad_kernel::mate::{self, Drag, Geom, MateKind, Pose};
use threebcad_kernel::mesher::print_mesh;
use threebcad_kernel::print::Tri;
use threebcad_kernel::topology::{dot, P3};

use crate::document::property;
use crate::{FaceRef, PartDocument, Units};

const FORMAT: &str = "3bcad-assembly";
pub const FORMAT_VERSION: u64 = 1;
/// Assembly files sit next to the `.3bcad` parts they use.
pub const EXT: &str = "3bcasm";
const UNDO_LIMIT: usize = 100;
/// Meshing for interference checks (mm): finer than `interference::TOUCH`.
const CHECK_TOL: f64 = 0.005;
/// Room left between a new component and the ones already placed (mm).
const GAP: f64 = 20.0;

/// A part or sub-assembly placed in the assembly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Component {
    /// Kept when other components are removed, so mates keep pointing at the right one.
    pub id: u32,
    /// "Bracket<1>": the file's name and which copy.
    pub name: String,
    /// The part or assembly file: absolute while open, saved relative to the assembly.
    pub file: PathBuf,
    pub pose: Pose,
    /// Stays where it is: mates move the other components to it.
    pub fixed: bool,
}

/// A face of a component, as a mate holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MateFace {
    pub component: u32,
    /// Which part of a sub-assembly (always 0 for a part).
    pub leaf: usize,
    pub face: FaceRef,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mate {
    pub name: String,
    pub kind: MateKind,
    pub a: MateFace,
    pub b: MateFace,
    /// Distance (mm) or angle (degrees).
    #[serde(default)]
    pub value: f64,
    #[serde(default)]
    pub flip: bool,
    /// Lock: where b's component sits relative to a's.
    #[serde(default)]
    pub lock: Pose,
}

/// One step of the exploded view: these components move `distance` along `direction`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExplodeStep {
    pub components: Vec<u32>,
    /// A unit vector, in assembly coordinates.
    pub direction: P3,
    pub distance: f64,
}

/// What an edit changes, and what undo brings back.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Contents {
    pub components: Vec<Component>,
    pub mates: Vec<Mate>,
    pub explode: Vec<ExplodeStep>,
    pub next_id: u32,
}

#[derive(Serialize, Deserialize)]
struct AssemblyFile {
    format: String,
    version: u64,
    name: String,
    units: Units,
    #[serde(default)]
    properties: Vec<(String, String)>,
    contents: Contents,
}

enum Source {
    Part(Box<PartDocument>),
    Assembly(Box<AssemblyDocument>),
    /// The file could not be read; why.
    Missing(String),
}

struct Loaded {
    source: Source,
    modified: Option<SystemTime>,
    /// Different for every read of every file, so a viewer can tell a re-read part from the old one.
    generation: u64,
}

static GENERATION: AtomicU64 = AtomicU64::new(1);

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn is_assembly(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(EXT))
}

/// Reads a component file; `within` are the assemblies it is being read for, which it must not use.
fn load(path: &Path, within: &[PathBuf]) -> Loaded {
    let source = if within.iter().any(|p| p == path) {
        Source::Missing("it contains the assembly it is in".into())
    } else if is_assembly(path) {
        let mut chain = within.to_vec();
        chain.push(path.to_path_buf());
        match AssemblyDocument::open_within(path, &chain) {
            Ok(a) => Source::Assembly(Box::new(a)),
            Err(e) => Source::Missing(format!("{e:#}")),
        }
    } else {
        match PartDocument::open(path) {
            Ok(mut p) => {
                p.name = stem(path);
                Source::Part(Box::new(p))
            }
            Err(e) => Source::Missing(format!("{e:#}")),
        }
    };
    Loaded {
        source,
        modified: modified(path),
        generation: GENERATION.fetch_add(1, Ordering::Relaxed),
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "Part".into(), |s| s.to_string_lossy().into_owned())
}

/// `path` written relative to the folder `base`, with `/` between names, so the files can move
/// together to another folder or computer; absolute when it is on another drive.
fn relative(path: &Path, base: &Path) -> PathBuf {
    let (p, b): (Vec<_>, Vec<_>) = (path.components().collect(), base.components().collect());
    let same = p.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if same == 0 {
        return path.to_path_buf();
    }
    let up = std::iter::repeat_n("..".to_string(), b.len() - same);
    let down = p[same..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned());
    PathBuf::from(up.chain(down).collect::<Vec<_>>().join("/"))
}

/// A part as placed in the assembly.
pub struct Leaf<'a> {
    pub component: u32,
    /// Its number within the component: 0 for a part, one per part in a sub-assembly.
    pub leaf: usize,
    /// "Bracket<1>", or "Arm<1> / Pin<2>" inside a sub-assembly.
    pub name: String,
    pub file: &'a Path,
    pub part: &'a PartDocument,
    /// Changes whenever the part's file is read again.
    pub generation: u64,
    /// Where the part sits in the assembly.
    pub pose: Pose,
}

/// A line of the bill of materials.
#[derive(Clone, Debug, PartialEq)]
pub struct BomRow {
    /// 0 for the assembly's own components, one more for each sub-assembly level.
    pub level: usize,
    /// "2", or "2.1" for the first item inside item 2.
    pub item: String,
    pub part_number: String,
    pub name: String,
    pub description: String,
    pub material: String,
    pub quantity: usize,
    pub properties: Vec<(String, String)>,
}

pub struct AssemblyDocument {
    pub name: String,
    /// The file it was opened from or saved to.
    path: Option<PathBuf>,
    units: Units,
    properties: Vec<(String, String)>,
    contents: Contents,
    undo: Vec<Contents>,
    redo: Vec<Contents>,
    merge_key: Option<u64>,
    dirty: bool,
    files: HashMap<PathBuf, Loaded>,
    /// Mates that do not hold (index into the mates) and why.
    problems: Vec<(usize, String)>,
    revision: u64,
}

impl AssemblyDocument {
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
            files: HashMap::new(),
            problems: Vec::new(),
            revision: 0,
        }
    }

    /// The assembly as JSON, with component files relative to the folder `base` (absolute when
    /// there is none: autosave copies live elsewhere).
    pub fn to_json(&self, base: Option<&Path>) -> String {
        let mut contents = self.contents.clone();
        if let Some(base) = base {
            for c in &mut contents.components {
                c.file = relative(&c.file, base);
            }
        }
        let file = AssemblyFile {
            format: FORMAT.into(),
            version: FORMAT_VERSION,
            name: self.name.clone(),
            units: self.units,
            properties: self.properties.clone(),
            contents,
        };
        serde_json::to_string_pretty(&file).expect("an assembly always serialises")
    }

    /// Reads assembly JSON whose relative component files start at the folder `base`, and the
    /// files it uses.
    pub fn from_json(text: &str, base: Option<&Path>) -> anyhow::Result<Self> {
        Self::from_json_within(text, base, &[])
    }

    fn from_json_within(text: &str, base: Option<&Path>, within: &[PathBuf]) -> anyhow::Result<Self> {
        let value: serde_json::Value = serde_json::from_str(text).context("the file is not valid JSON")?;
        ensure!(
            value.get("format").and_then(|f| f.as_str()) == Some(FORMAT),
            "this is not a 3BCAD assembly file"
        );
        let version = value
            .get("version")
            .and_then(|v| v.as_u64())
            .context("the file has no format version")?;
        ensure!(
            version <= FORMAT_VERSION,
            "this assembly was saved by a newer 3BCAD (format version {version}); this version reads up to {FORMAT_VERSION}. Update 3BCAD to open it"
        );
        let file: AssemblyFile = serde_json::from_value(value).context("the assembly file is damaged")?;
        let mut doc = Self::new(file.name);
        doc.units = file.units;
        doc.properties = file.properties;
        doc.contents = file.contents;
        let ids: Vec<u32> = doc.contents.components.iter().map(|c| c.id).collect();
        ensure!(
            ids.iter().all(|&id| id < doc.contents.next_id)
                && ids.iter().enumerate().all(|(i, id)| !ids[..i].contains(id))
                && doc
                    .contents
                    .mates
                    .iter()
                    .all(|m| ids.contains(&m.a.component) && ids.contains(&m.b.component)),
            "the assembly file is damaged: its components and mates do not match"
        );
        for c in &mut doc.contents.components {
            if let Some(base) = base.filter(|_| c.file.is_relative()) {
                c.file = base.join(&c.file);
            }
            if !doc.files.contains_key(&c.file) {
                doc.files.insert(c.file.clone(), load(&c.file, within));
            }
        }
        doc.solve(None);
        Ok(doc)
    }

    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let path = std::path::absolute(path)?;
        Self::open_within(&path, std::slice::from_ref(&path))
    }

    fn open_within(path: &Path, within: &[PathBuf]) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        let mut doc = Self::from_json_within(&text, path.parent(), within)?;
        doc.path = Some(path.to_path_buf());
        Ok(doc)
    }

    /// Writes the assembly next to `path` first and then moves it into place, like a part.
    pub fn save(&mut self, path: &Path) -> anyhow::Result<()> {
        let path = std::path::absolute(path)?;
        self.write(&path, path.parent())?;
        self.path = Some(path);
        self.dirty = false;
        Ok(())
    }

    /// Saves a copy without marking the assembly saved (autosave), with absolute file names.
    pub fn write_to(&self, path: &Path) -> anyhow::Result<()> {
        self.write(path, None)
    }

    fn write(&self, path: &Path, base: Option<&Path>) -> anyhow::Result<()> {
        let tmp = path.with_extension("3bcasm.tmp");
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
        self.dirty |= units != self.units;
        self.units = units;
    }

    pub fn properties(&self) -> &[(String, String)] {
        &self.properties
    }

    pub fn set_properties(&mut self, properties: Vec<(String, String)>) {
        self.dirty |= properties != self.properties;
        self.properties = properties;
    }

    pub fn contents(&self) -> &Contents {
        &self.contents
    }

    pub fn component(&self, id: u32) -> Option<&Component> {
        self.contents.components.iter().find(|c| c.id == id)
    }

    fn index(&self, id: u32) -> Option<usize> {
        self.contents.components.iter().position(|c| c.id == id)
    }

    /// Mates that do not hold, and why.
    pub fn problems(&self) -> &[(usize, String)] {
        &self.problems
    }

    pub fn problem(&self, mate: usize) -> Option<&str> {
        self.problems
            .iter()
            .find(|(m, _)| *m == mate)
            .map(|(_, why)| why.as_str())
    }

    /// Bumps whenever something moves or a file is read again; viewers re-mesh on it.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Why a component's file could not be read.
    pub fn file_error(&self, id: u32) -> Option<&str> {
        match &self.files.get(&self.component(id)?.file)?.source {
            Source::Missing(why) => Some(why),
            _ => None,
        }
    }

    /// The sub-assembly a component is, if it is one.
    pub fn sub_assembly(&self, id: u32) -> Option<&AssemblyDocument> {
        match &self.files.get(&self.component(id)?.file)?.source {
            Source::Assembly(a) => Some(a),
            _ => None,
        }
    }

    /// The parts of a component, placed relative to the component.
    fn inner(&self, c: &Component) -> Vec<Leaf<'_>> {
        let Some((file, loaded)) = self.files.get_key_value(&c.file) else {
            return Vec::new();
        };
        match &loaded.source {
            Source::Part(part) => vec![Leaf {
                component: c.id,
                leaf: 0,
                name: c.name.clone(),
                file,
                part,
                generation: loaded.generation,
                pose: Pose::IDENTITY,
            }],
            Source::Assembly(a) => a
                .leaves()
                .into_iter()
                .map(|l| Leaf {
                    name: format!("{} / {}", c.name, l.name),
                    ..l
                })
                .collect(),
            Source::Missing(_) => Vec::new(),
        }
    }

    /// Every part in the assembly, sub-assemblies opened up, where it sits.
    pub fn leaves(&self) -> Vec<Leaf<'_>> {
        self.contents
            .components
            .iter()
            .flat_map(|c| {
                self.inner(c).into_iter().enumerate().map(move |(leaf, l)| Leaf {
                    component: c.id,
                    leaf,
                    pose: c.pose.compose(&l.pose),
                    ..l
                })
            })
            .collect()
    }

    /// The face a mate holds, in its component's coordinates, and the component's index.
    fn geom(&self, f: &MateFace) -> Result<(usize, Geom), String> {
        let ci = self.index(f.component).ok_or("its component was removed")?;
        let c = &self.contents.components[ci];
        let inner = self.inner(c);
        let Leaf { part, pose, .. } = inner
            .get(f.leaf)
            .ok_or_else(|| format!("{} could not be read", c.name))?;
        let (body, face) = part
            .resolve_face(&f.face)
            .ok_or_else(|| format!("its face on {} is gone", c.name))?;
        let t = &part.bodies()[body].topo.faces[face];
        let g = match (t.cylinder, t.plane) {
            (Some((p, d, r)), _) => Geom::Cylinder { p, d, r },
            (None, Some(pl)) => Geom::Plane {
                p: pl.origin,
                n: pl.normal(),
            },
            _ => return Err("only flat and cylindrical faces can be mated".into()),
        };
        Ok((ci, g.moved(pose)))
    }

    fn poses(&self) -> Vec<Pose> {
        self.contents.components.iter().map(|c| c.pose).collect()
    }

    /// Moves the free components until their mates hold (pulling one towards `drag`'s target),
    /// and notes the mates that cannot.
    fn solve(&mut self, drag: Option<(u32, Pose)>) {
        let mut poses = self.poses();
        let fixed: Vec<bool> = self.contents.components.iter().map(|c| c.fixed).collect();
        let (mut mates, mut which, mut problems) = (Vec::new(), Vec::new(), Vec::new());
        for (i, m) in self.contents.mates.iter().enumerate() {
            match (self.geom(&m.a), self.geom(&m.b)) {
                (Ok((a, ga)), Ok((b, gb))) => {
                    mates.push(mate::Mate {
                        kind: m.kind,
                        a,
                        ga,
                        b,
                        gb,
                        value: m.value,
                        flip: m.flip,
                        lock: m.lock,
                    });
                    which.push(i);
                }
                (Err(why), _) | (_, Err(why)) => problems.push((i, why)),
            }
        }
        let drag = drag.and_then(|(id, target)| {
            Some(Drag {
                component: self.index(id)?,
                target,
            })
        });
        for k in mate::solve(&mut poses, &fixed, &mates, drag) {
            problems.push((which[k], "it contradicts the mates before it (over-defined)".into()));
        }
        problems.sort_by_key(|p| p.0);
        for (c, p) in self.contents.components.iter_mut().zip(poses) {
            c.pose = p;
        }
        self.problems = problems;
        self.revision += 1;
    }

    /// Makes an edit one undo step (or part of the last one, for the same `key`) and solves again.
    fn edit<R>(&mut self, key: Option<u64>, f: impl FnOnce(&mut Contents) -> R) -> R {
        if key.is_none() || key != self.merge_key {
            self.undo.push(self.contents.clone());
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.merge_key = key;
        self.dirty = true;
        let r = f(&mut self.contents);
        self.solve(None);
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
            self.redo.push(std::mem::replace(&mut self.contents, prev));
            self.after_history();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.contents, next));
            self.after_history();
        }
    }

    fn after_history(&mut self) {
        self.merge_key = None;
        self.dirty = true;
        for c in &self.contents.components {
            if !self.files.contains_key(&c.file) {
                let within: Vec<PathBuf> = self.path.iter().cloned().collect();
                self.files.insert(c.file.clone(), load(&c.file, &within));
            }
        }
        self.solve(None);
    }

    /// Whether this assembly, or one inside it, uses `file`.
    fn uses(&self, file: &Path) -> bool {
        self.files.iter().any(|(p, l)| {
            p == file
                || match &l.source {
                    Source::Assembly(a) => a.uses(file),
                    _ => false,
                }
        })
    }

    /// Adds a part or assembly file as a new component and returns its id. The first component is
    /// fixed at the origin; later ones are set down beside the others.
    pub fn insert(&mut self, file: &Path) -> anyhow::Result<u32> {
        let file = std::path::absolute(file)?;
        let within: Vec<PathBuf> = self.path.iter().cloned().collect();
        if !self.files.contains_key(&file) {
            let loaded = load(&file, &within);
            if let Source::Missing(why) = &loaded.source {
                bail!("cannot use {}: {why}", file.display());
            }
            self.files.insert(file.clone(), loaded);
        }
        if let (Some(me), Source::Assembly(a)) = (&self.path, &self.files[&file].source) {
            ensure!(!a.uses(me), "{} already contains this assembly", file.display());
        }
        let copies = self.contents.components.iter().filter(|c| c.file == file).count();
        let name = format!("{}<{}>", stem(&file), copies + 1);
        let first = self.contents.components.is_empty();
        let pose = if first { Pose::IDENTITY } else { self.beside(&file) };
        let id = self.edit(None, |c| {
            let id = c.next_id;
            c.next_id += 1;
            c.components.push(Component {
                id,
                name,
                file,
                pose,
                fixed: first,
            });
            id
        });
        Ok(id)
    }

    /// Where a new component goes: past the +X side of the ones already there.
    fn beside(&self, file: &Path) -> Pose {
        let placed = bounds(self.leaves().iter().flat_map(|l| corners(l.part, &l.pose)));
        let new = bounds(match &self.files[file].source {
            Source::Part(p) => corners(p, &Pose::IDENTITY),
            Source::Assembly(a) => a.leaves().iter().flat_map(|l| corners(l.part, &l.pose)).collect(),
            Source::Missing(_) => Vec::new(),
        });
        match (placed, new) {
            (Some((_, hi)), Some((lo, _))) => Pose::at([hi[0] + GAP - lo[0], 0.0, 0.0]),
            _ => Pose::IDENTITY,
        }
    }

    /// Removes a component and the mates and explode steps that use it.
    pub fn remove(&mut self, id: u32) {
        self.edit(None, |c| {
            c.components.retain(|x| x.id != id);
            c.mates.retain(|m| m.a.component != id && m.b.component != id);
            for s in &mut c.explode {
                s.components.retain(|&x| x != id);
            }
            c.explode.retain(|s| !s.components.is_empty());
        });
    }

    pub fn set_fixed(&mut self, id: u32, fixed: bool) {
        self.edit(None, |c| {
            if let Some(x) = c.components.iter_mut().find(|x| x.id == id) {
                x.fixed = fixed;
            }
        });
    }

    pub fn rename(&mut self, id: u32, name: String) {
        self.edit(None, |c| {
            if let Some(x) = c.components.iter_mut().find(|x| x.id == id) {
                x.name = name;
            }
        });
    }

    /// Puts a component somewhere (typed in, or dropped after a drag); its mates still apply.
    pub fn set_pose(&mut self, id: u32, pose: Pose, key: Option<u64>) {
        self.edit(key, |c| {
            if let Some(x) = c.components.iter_mut().find(|x| x.id == id) {
                x.pose = pose;
            }
        });
    }

    /// Pulls a component towards `target` as far as its mates let it (one frame of a mouse drag;
    /// frames with the same `key` are one undo step).
    pub fn drag(&mut self, id: u32, target: Pose, key: u64) {
        if self.merge_key != Some(key) {
            self.undo.push(self.contents.clone());
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.merge_key = Some(key);
        self.dirty = true;
        self.solve(Some((id, target)));
    }

    /// Adds a mate between two faces of different components, keeping the distance or angle they
    /// have now and the alignment closest to how they sit, so the parts move as little as possible.
    pub fn add_mate(&mut self, kind: MateKind, a: MateFace, b: MateFace) -> anyhow::Result<usize> {
        ensure!(a.component != b.component, "pick faces of two different components");
        let (ia, ga) = self.geom(&a).map_err(|e| anyhow!(e))?;
        let (ib, gb) = self.geom(&b).map_err(|e| anyhow!(e))?;
        ensure!(
            kind.fits(&ga, &gb),
            "{} needs {}",
            kind.name(),
            if kind == MateKind::Concentric {
                "two cylindrical faces"
            } else {
                "a cylindrical face"
            }
        );
        let poses = self.poses();
        let mut m = mate::Mate {
            kind,
            a: ia,
            ga,
            b: ib,
            gb,
            value: 0.0,
            flip: false,
            lock: poses[ia].inverse().compose(&poses[ib]),
        };
        let len = mate::length_scale(&poses, &[m]);
        match kind {
            MateKind::Distance => {
                // With no distance asked for, the last residual is the distance there is now.
                let mut r = Vec::new();
                m.residuals(&poses[ia], &poses[ib], len, &mut r);
                let now = r.last().copied().unwrap_or(0.0);
                m.value = if matches!((ga, gb), (Geom::Plane { .. }, Geom::Plane { .. })) {
                    now
                } else {
                    now.abs()
                };
            }
            MateKind::Angle => {
                let (ua, ub) = (direction(&ga.moved(&poses[ia])), direction(&gb.moved(&poses[ib])));
                m.value = dot(ua, ub).clamp(-1.0, 1.0).acos().to_degrees();
            }
            _ => {}
        }
        if m.value.abs() < 1e-9 {
            // Not -0, which would show as "-0.00".
            m.value = 0.0;
        }
        let flipped = mate::Mate { flip: true, ..m };
        m.flip = flipped.error(&poses, len) < m.error(&poses, len);
        let n = self.contents.mates.iter().filter(|x| x.kind == kind).count() + 1;
        let mate = Mate {
            name: format!("{}{n}", kind.name()),
            kind,
            a,
            b,
            value: m.value,
            flip: m.flip,
            lock: m.lock,
        };
        Ok(self.edit(None, |c| {
            c.mates.push(mate);
            c.mates.len() - 1
        }))
    }

    /// Changes a mate's value or alignment (`key` merges a dragged value into one undo step).
    pub fn set_mate(&mut self, i: usize, value: f64, flip: bool, key: Option<u64>) {
        self.edit(key, |c| {
            if let Some(m) = c.mates.get_mut(i) {
                m.value = value;
                m.flip = flip;
            }
        });
    }

    pub fn remove_mate(&mut self, i: usize) {
        self.edit(None, |c| {
            if i < c.mates.len() {
                c.mates.remove(i);
            }
        });
    }

    pub fn add_explode_step(&mut self, components: Vec<u32>, direction: P3, distance: f64) {
        let direction = threebcad_kernel::topology::unit(direction);
        self.edit(None, |c| {
            c.explode.push(ExplodeStep {
                components,
                direction,
                distance,
            })
        });
    }

    pub fn set_explode_step(&mut self, i: usize, direction: P3, distance: f64, key: Option<u64>) {
        let direction = threebcad_kernel::topology::unit(direction);
        self.edit(key, |c| {
            if let Some(s) = c.explode.get_mut(i) {
                s.direction = direction;
                s.distance = distance;
            }
        });
    }

    pub fn remove_explode_step(&mut self, i: usize) {
        self.edit(None, |c| {
            if i < c.explode.len() {
                c.explode.remove(i);
            }
        });
    }

    /// How far a component has moved when the exploded view is `t` steps along: 0 is collapsed,
    /// the number of steps fully exploded, and in between each step slides in turn.
    pub fn explode_offset(&self, id: u32, t: f64) -> P3 {
        let mut off = [0.0; 3];
        for (k, s) in self.contents.explode.iter().enumerate() {
            let f = (t - k as f64).clamp(0.0, 1.0);
            if f > 0.0 && s.components.contains(&id) {
                for (o, d) in off.iter_mut().zip(s.direction) {
                    *o += d * s.distance * f;
                }
            }
        }
        off
    }

    /// Re-reads component files changed on disk since they were read; true when one was, so the
    /// assembly shows the parts as they are now.
    pub fn refresh(&mut self) -> bool {
        let within: Vec<PathBuf> = self.path.iter().cloned().collect();
        let mut changed = false;
        for (path, l) in &mut self.files {
            if let Source::Assembly(a) = &mut l.source {
                changed |= a.refresh();
            }
            if modified(path) != l.modified {
                *l = load(path, &within);
                changed = true;
            }
        }
        if changed {
            self.solve(None);
        }
        changed
    }

    /// Every pair of parts that share volume: their leaf indices (as in `leaves`) and what they
    /// share. Parts that only touch are left out.
    pub fn interferences(&self) -> Vec<(usize, usize, Interference)> {
        let leaves = self.leaves();
        let meshes: Vec<Vec<Tri>> = leaves
            .iter()
            .map(|l| {
                l.part
                    .bodies()
                    .iter()
                    .flat_map(|b| {
                        let m = print_mesh(&b.solid, CHECK_TOL);
                        m.triangles
                            .iter()
                            .map(|t| t.map(|i| l.pose.apply(m.vertices[i])))
                            .collect::<Vec<_>>()
                    })
                    .collect()
            })
            .collect();
        let mut out = Vec::new();
        for i in 0..meshes.len() {
            for j in i + 1..meshes.len() {
                if let Some(x) = interference::interference(&meshes[i], &meshes[j]) {
                    out.push((i, j, x));
                }
            }
        }
        out
    }

    /// The bill of materials: one row per different file, with how many there are. Indented, the
    /// parts of each sub-assembly follow it, one level deeper and counted per sub-assembly.
    pub fn bom(&self, indented: bool) -> Vec<BomRow> {
        let mut rows = Vec::new();
        self.bom_rows(indented, 0, "", &mut rows);
        rows
    }

    fn bom_rows(&self, indented: bool, level: usize, prefix: &str, rows: &mut Vec<BomRow>) {
        let mut files: Vec<(&Path, usize)> = Vec::new();
        for c in &self.contents.components {
            match files.iter_mut().find(|(f, _)| *f == c.file) {
                Some(f) => f.1 += 1,
                None => files.push((&c.file, 1)),
            }
        }
        for (k, (file, quantity)) in files.into_iter().enumerate() {
            let item = format!("{prefix}{}", k + 1);
            let (properties, material, missing) = match self.files.get(file).map(|l| &l.source) {
                Some(Source::Part(p)) => (p.properties(), p.material().name.clone(), None),
                Some(Source::Assembly(a)) => (a.properties(), String::new(), None),
                Some(Source::Missing(why)) => (&[][..], String::new(), Some(why.clone())),
                None => (&[][..], String::new(), Some("not read".into())),
            };
            let name = stem(file);
            rows.push(BomRow {
                level,
                item: item.clone(),
                part_number: property(properties, "Part number").unwrap_or(&name).to_string(),
                description: missing
                    .map(|why| format!("missing: {why}"))
                    .or_else(|| property(properties, "Description").map(str::to_string))
                    .unwrap_or_default(),
                name,
                material,
                quantity,
                properties: properties.to_vec(),
            });
            if let (true, Some(Source::Assembly(a))) = (indented, self.files.get(file).map(|l| &l.source)) {
                a.bom_rows(true, level + 1, &format!("{item}."), rows);
            }
        }
    }
}

fn direction(g: &Geom) -> P3 {
    match *g {
        Geom::Plane { n, .. } => n,
        Geom::Cylinder { d, .. } => d,
    }
}

/// The corners of a part's bounding box where it sits.
fn corners(part: &PartDocument, pose: &Pose) -> Vec<P3> {
    let Some((lo, hi)) = bounds(part.bodies().iter().flat_map(|b| b.topo.vertices.iter().copied())) else {
        return Vec::new();
    };
    (0..8)
        .map(|k| {
            pose.apply([
                [lo[0], hi[0]][k & 1],
                [lo[1], hi[1]][(k >> 1) & 1],
                [lo[2], hi[2]][k >> 2],
            ])
        })
        .collect()
}

fn bounds(points: impl IntoIterator<Item = P3>) -> Option<(P3, P3)> {
    points.into_iter().fold(None, |acc, p| {
        let (lo, hi) = acc.unwrap_or((p, p));
        Some((
            std::array::from_fn(|k| lo[k].min(p[k])),
            std::array::from_fn(|k| hi[k].max(p[k])),
        ))
    })
}

/// The bill of materials as CSV: the usual columns, then one per other custom property.
pub fn bom_csv(rows: &[BomRow]) -> String {
    let standard = |k: &str| k.eq_ignore_ascii_case("Part number") || k.eq_ignore_ascii_case("Description");
    let mut extra: Vec<&str> = Vec::new();
    for (k, _) in rows.iter().flat_map(|r| &r.properties) {
        if !standard(k) && !extra.iter().any(|e| e.eq_ignore_ascii_case(k)) {
            extra.push(k);
        }
    }
    let cell = |s: &str| {
        if s.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    let mut out = String::from("Item,Level,Part number,Name,Description,Material,Quantity");
    for e in &extra {
        out += &format!(",{}", cell(e));
    }
    out += "\n";
    for r in rows {
        out += &[
            cell(&r.item),
            r.level.to_string(),
            cell(&r.part_number),
            cell(&r.name),
            cell(&r.description),
            cell(&r.material),
            r.quantity.to_string(),
        ]
        .join(",");
        for e in &extra {
            out += &format!(",{}", cell(property(&r.properties, e).unwrap_or("")));
        }
        out += "\n";
    }
    out
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use threebcad_kernel::topology::norm;
    use threebcad_kernel::Plane;
    use threebcad_sketch::Sketch;

    use super::*;

    /// A fresh folder for one test's files.
    fn folder(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("3bcad-asm-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A 40 x 40 x 5 plate with a Ø10 hole through its middle.
    fn plate(path: &Path) {
        let mut doc = PartDocument::new("Plate");
        doc.edit(|h| {
            let mut s = Sketch::rectangle(40.0, 40.0);
            let c = s.add_point(20.0, 20.0);
            s.add_circle(c, 5.0);
            let s = h.add_sketch(s, Plane::TOP);
            h.add_extrude(s, 5.0).unwrap();
        });
        doc.set_properties(vec![
            ("Part number".into(), "PL-40".into()),
            ("Description".into(), "Base plate, 5 mm".into()),
            ("Finish".into(), "Black, matte".into()),
        ]);
        doc.save(path).unwrap();
    }

    /// A pin of radius `r`, 20 long, standing on its base at the origin.
    fn pin(path: &Path, r: f64) {
        let mut doc = PartDocument::new("Pin");
        doc.edit(|h| {
            let s = h.add_sketch(Sketch::circle([0.0, 0.0], r), Plane::TOP);
            h.add_extrude(s, 20.0).unwrap();
        });
        doc.save(path).unwrap();
    }

    /// A face of a component's part picked by a test on its shape.
    fn face(asm: &AssemblyDocument, id: u32, pick: impl Fn(&threebcad_kernel::topology::TopoFace) -> bool) -> MateFace {
        let leaf = asm.leaves().into_iter().find(|l| l.component == id).unwrap();
        let body = &leaf.part.bodies()[0];
        let f = body.topo.faces.iter().position(pick).expect("the face");
        MateFace {
            component: id,
            leaf: 0,
            face: leaf.part.face_ref(0, f).unwrap(),
        }
    }

    fn flat(z: f64, up: bool) -> impl Fn(&threebcad_kernel::topology::TopoFace) -> bool {
        move |f| {
            f.plane.is_some_and(|p| {
                (p.origin[2] - z).abs() < 1e-6 && (p.normal()[2] > 0.5) == up && p.normal()[2].abs() > 0.5
            })
        }
    }

    fn round(r: f64) -> impl Fn(&threebcad_kernel::topology::TopoFace) -> bool {
        move |f| f.cylinder.is_some_and(|c| (c.2 - r).abs() < 1e-6)
    }

    /// The plate fixed at the origin and a pin of radius `r` mated into its hole, flush with the
    /// plate's bottom.
    fn pinned(dir: &Path, r: f64) -> (AssemblyDocument, u32, u32) {
        plate(&dir.join("plate.3bcad"));
        pin(&dir.join("pin.3bcad"), r);
        let mut asm = AssemblyDocument::new("Fixture");
        let p = asm.insert(&dir.join("plate.3bcad")).unwrap();
        let q = asm.insert(&dir.join("pin.3bcad")).unwrap();
        let hole = face(&asm, p, round(5.0));
        let side = face(&asm, q, round(r));
        asm.add_mate(MateKind::Concentric, hole, side).unwrap();
        let bottom = face(&asm, p, flat(0.0, false));
        let base = face(&asm, q, flat(0.0, false));
        asm.add_mate(MateKind::Coincident, bottom, base).unwrap();
        (asm, p, q)
    }

    #[test]
    fn a_pin_mated_into_a_plate_goes_through_its_hole() {
        let dir = folder("pin");
        let (asm, p, q) = pinned(&dir, 5.0);
        assert!(asm.problems().is_empty(), "{:?}", asm.problems());
        let plate = asm.component(p).unwrap();
        assert!(
            plate.fixed && plate.pose == Pose::IDENTITY,
            "the first component is fixed at the origin"
        );
        let pin = asm.component(q).unwrap();
        assert!(!pin.fixed);
        // The pin's axis is the hole's, and its base is flush with the plate's bottom.
        let base = pin.pose.apply([0.0; 3]);
        assert!(norm([base[0] - 20.0, base[1] - 20.0, base[2]]) < 1e-5, "{base:?}");
        assert!((pin.pose.turn([0.0, 0.0, 1.0])[2] - 1.0).abs() < 1e-6);
        // A pin the size of the hole touches it; a bigger one shares a ring with the plate.
        assert!(asm.interferences().is_empty());
        let (big, ..) = pinned(&folder("pin-big"), 5.2);
        let found = big.interferences();
        assert_eq!(found.len(), 1);
        let ring = PI * (5.2 * 5.2 - 25.0) * 5.0;
        assert!(
            (found[0].2.volume - ring).abs() / ring < 0.03,
            "{} vs {ring}",
            found[0].2.volume
        );
    }

    #[test]
    fn an_assembly_saves_relative_paths_and_opens_where_it_was() {
        let dir = folder("save");
        let (mut asm, _, q) = pinned(&dir, 5.0);
        let file = dir.join("fixture.3bcasm");
        asm.save(&file).unwrap();
        let json = std::fs::read_to_string(&file).unwrap();
        assert!(json.contains("\"file\": \"pin.3bcad\""), "{json}");
        // Moved to another folder together, it still finds its parts.
        let moved = folder("save-moved");
        for f in ["plate.3bcad", "pin.3bcad", "fixture.3bcasm"] {
            std::fs::copy(dir.join(f), moved.join(f)).unwrap();
        }
        let back = AssemblyDocument::open(&moved.join("fixture.3bcasm")).unwrap();
        assert!(back.problems().is_empty());
        assert_eq!(back.contents().mates.len(), 2);
        let (a, b) = (asm.component(q).unwrap().pose, back.component(q).unwrap().pose);
        assert!(norm([a.t[0] - b.t[0], a.t[1] - b.t[1], a.t[2] - b.t[2]]) < 1e-9);
        assert_eq!(back.leaves()[1].file, moved.join("pin.3bcad"));
        assert!(!back.is_dirty());
        // A newer or a broken file is refused.
        let newer = json.replacen("\"version\": 1", "\"version\": 9", 1);
        assert!(format!("{:#}", AssemblyDocument::from_json(&newer, None).err().unwrap()).contains("newer"));
        let broken = json.replacen("\"component\": 1", "\"component\": 7", 1);
        assert!(AssemblyDocument::from_json(&broken, Some(&dir)).is_err());
        assert_eq!(
            relative(Path::new("/a/b/c.3bcad"), Path::new("/a/d")),
            Path::new("../b/c.3bcad")
        );
    }

    #[test]
    fn components_follow_their_part_when_it_changes() {
        let dir = folder("follow");
        let (mut asm, _, q) = pinned(&dir, 5.0);
        let volume = |asm: &AssemblyDocument| {
            let l = asm.leaves().into_iter().find(|l| l.component == q).unwrap();
            threebcad_kernel::mass::mass_properties(&[print_mesh(&l.part.bodies()[0].solid, 0.01)]).volume
        };
        let before = volume(&asm);
        assert!(!asm.refresh(), "nothing changed yet");
        // The pin is made thicker in its own file (a second later, so the time stamp changes).
        std::thread::sleep(std::time::Duration::from_millis(1100));
        pin(&dir.join("pin.3bcad"), 5.2);
        assert!(asm.refresh());
        assert!(volume(&asm) > before * 1.07);
        // Its mates still find the side face (now bigger) and the base.
        assert!(asm.problems().is_empty(), "{:?}", asm.problems());
        assert_eq!(asm.interferences().len(), 1);
    }

    #[test]
    fn a_contradicting_mate_is_flagged_and_undo_takes_it_back() {
        let dir = folder("over");
        let (mut asm, p, q) = pinned(&dir, 5.0);
        let before = asm.component(q).unwrap().pose;
        // The pin's base is flush with the plate's bottom; also 3 mm from it cannot be.
        let bottom = face(&asm, p, flat(0.0, false));
        let base = face(&asm, q, flat(0.0, false));
        let m = asm.add_mate(MateKind::Distance, bottom, base).unwrap();
        asm.set_mate(m, 3.0, asm.contents().mates[m].flip, None);
        assert!(asm.problem(m).is_some_and(|w| w.contains("over-defined")));
        let pose = asm.component(q).unwrap().pose;
        assert!(
            norm([
                pose.t[0] - before.t[0],
                pose.t[1] - before.t[1],
                pose.t[2] - before.t[2]
            ]) < 1e-6
        );
        asm.undo();
        asm.undo();
        assert_eq!(asm.contents().mates.len(), 2);
        assert!(asm.problems().is_empty());
        // Concentric needs two cylinders.
        let top = face(&asm, p, flat(5.0, true));
        let base = face(&asm, q, flat(0.0, false));
        assert!(asm.add_mate(MateKind::Concentric, top, base).is_err());
    }

    #[test]
    fn a_dragged_pin_only_slides_along_its_hole() {
        let dir = folder("drag");
        let (mut asm, p, q) = pinned(&dir, 5.0);
        // Without the coincident mate, the pin can slide and turn in the hole.
        asm.remove_mate(1);
        let start = asm.component(q).unwrap().pose;
        asm.drag(q, start.moved([15.0, 0.0, 6.0]), 1);
        let now = asm.component(q).unwrap().pose.apply([0.0; 3]);
        assert!(norm([now[0] - 20.0, now[1] - 20.0, 0.0]) < 1e-5, "{now:?}");
        assert!((now[2] - start.t[2] - 6.0).abs() < 1e-3);
        asm.drag(q, start.moved([15.0, 0.0, 9.0]), 1);
        asm.end_merge();
        asm.undo();
        assert_eq!(asm.component(q).unwrap().pose, start, "a whole drag is one undo step");
        // The plate is fixed: removing it takes its mates with it.
        asm.remove(p);
        assert!(asm.contents().mates.is_empty());
    }

    #[test]
    fn exploded_views_move_components_step_by_step() {
        let dir = folder("explode");
        let (mut asm, p, q) = pinned(&dir, 5.0);
        asm.add_explode_step(vec![q], [0.0, 0.0, 2.0], 30.0);
        asm.add_explode_step(vec![p, q], [1.0, 0.0, 0.0], 10.0);
        assert_eq!(asm.explode_offset(q, 0.0), [0.0; 3]);
        assert_eq!(asm.explode_offset(q, 0.5), [0.0, 0.0, 15.0]);
        assert_eq!(asm.explode_offset(q, 2.0), [10.0, 0.0, 30.0]);
        assert_eq!(asm.explode_offset(p, 1.5), [5.0, 0.0, 0.0]);
        // Exploding never moves the components themselves.
        assert!(asm.problems().is_empty());
        asm.remove(q);
        assert_eq!(asm.contents().explode.len(), 1);
    }

    #[test]
    fn the_bill_of_materials_counts_parts_and_opens_sub_assemblies() {
        let dir = folder("bom");
        let (mut sub, ..) = pinned(&dir, 5.0);
        sub.insert(&dir.join("pin.3bcad")).unwrap();
        sub.set_properties(vec![("Part number".into(), "FX-1".into())]);
        sub.save(&dir.join("fixture.3bcasm")).unwrap();
        let mut top = AssemblyDocument::new("Station");
        top.insert(&dir.join("fixture.3bcasm")).unwrap();
        top.insert(&dir.join("fixture.3bcasm")).unwrap();
        top.insert(&dir.join("plate.3bcad")).unwrap();
        top.save(&dir.join("station.3bcasm")).unwrap();
        assert_eq!(top.leaves().len(), 2 * 3 + 1);
        assert_eq!(top.leaves()[1].name, "fixture<1> / pin<1>");
        let flat: Vec<_> = top
            .bom(false)
            .into_iter()
            .map(|r| (r.item, r.part_number, r.quantity))
            .collect();
        assert_eq!(flat, [("1".into(), "FX-1".into(), 2), ("2".into(), "PL-40".into(), 1)]);
        let indented = top.bom(true);
        let rows: Vec<_> = indented
            .iter()
            .map(|r| (r.item.as_str(), r.level, r.name.as_str(), r.quantity))
            .collect();
        assert_eq!(
            rows,
            [
                ("1", 0, "fixture", 2),
                ("1.1", 1, "plate", 1),
                ("1.2", 1, "pin", 2),
                ("2", 0, "plate", 1)
            ]
        );
        let csv = bom_csv(&indented);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[0],
            "Item,Level,Part number,Name,Description,Material,Quantity,Finish"
        );
        assert_eq!(
            lines[2],
            "1.1,1,PL-40,plate,\"Base plate, 5 mm\",PLA,1,\"Black, matte\""
        );
        // An assembly cannot hold itself, even through another one.
        assert!(top.insert(&dir.join("station.3bcasm")).is_err());
        let mut fixture = AssemblyDocument::open(&dir.join("fixture.3bcasm")).unwrap();
        assert!(fixture.insert(&dir.join("station.3bcasm")).is_err());
    }
}
