//! The desktop file workflow: New / Open / Save, recent files, the unsaved-changes prompt,
//! autosave with crash recovery, and importing other formats.

use crate::i18n::tr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Key};
use threebcad_core::assembly::EXT as ASSEMBLY_EXT;
use threebcad_core::drawing::EXT as DRAWING_EXT;
use threebcad_core::{
    AssemblyDocument, DrawingDocument, FeatureKind, PartDocument, PlaneRef, Shared, UnitSystem, Units,
};
use threebcad_io::printer::PRINTERS;
use threebcad_io::Slicer;
use threebcad_kernel::Plane;

use crate::app::{App, VERSION};
use crate::assembly::Session;
use crate::ui::ribbon::RibbonTab;

const MAX_RECENT: usize = 8;
pub const EXT: &str = "3bcad";
/// Other apps' formats: File > Open reads them into a new part, File > Import adds them to this one.
pub const IMPORT_EXTS: [&str; 6] = ["step", "stp", "stl", "3mf", "obj", "dxf"];

/// Preferences kept between runs, in `settings.txt` in the user's config folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// 0 turns autosave off.
    pub autosave_minutes: u32,
    pub recent: Vec<PathBuf>,
    /// The slicer Send to Slicer opens, and where the user located each one (`Slicer as usize`).
    pub slicer: Slicer,
    pub slicer_paths: [Option<PathBuf>; 3],
    /// The printer parts are laid out for, and its build volume (mm).
    pub printer: String,
    pub bed: [f64; 3],
    /// The steepest overhang printed without support, in degrees from vertical.
    pub overhang_deg: f64,
    pub nozzle: f64,
    /// The thinnest wall worth printing (mm).
    pub min_wall: f64,
    /// Export meshing: chord (mm) and angle (degrees) tolerance.
    pub chord: f64,
    pub angle_deg: f64,
    /// Ask GitHub for a newer release at start.
    pub check_updates: bool,
    /// A release the user chose to skip: not offered at start.
    pub skip_update: String,
    pub language: crate::i18n::Language,
    pub theme: crate::ui::theme::Theme,
    /// Size of the whole interface (1 = 100 %).
    pub ui_scale: f32,
    /// How the model is drawn, and with perspective, when 3BCAD starts.
    pub display_style: threebcad_render::DisplayStyle,
    pub perspective: bool,
    pub view_cube: bool,
    /// Units of new parts.
    pub units: UnitSystem,
    /// Draw a coarse copy of large models while the view moves.
    pub lod: bool,
    pub show_fps: bool,
    /// Right-drag shows the mouse gesture wheel instead of panning (pan with Shift+right-drag).
    pub mouse_gestures: bool,
    /// Keyboard shortcuts that differ from the defaults.
    pub keys: crate::commands::Overrides,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            autosave_minutes: 10,
            recent: Vec::new(),
            slicer: Slicer::OrcaSlicer,
            slicer_paths: Default::default(),
            printer: PRINTERS[0].name.into(),
            bed: PRINTERS[0].bed,
            overhang_deg: 45.0,
            nozzle: 0.4,
            min_wall: 0.8,
            chord: 0.01,
            angle_deg: 10.0,
            check_updates: true,
            skip_update: String::new(),
            language: Default::default(),
            theme: Default::default(),
            ui_scale: 1.0,
            display_style: Default::default(),
            perspective: true,
            view_cube: true,
            units: UnitSystem::Mmgs,
            lod: true,
            show_fps: false,
            mouse_gestures: false,
            keys: Default::default(),
        }
    }
}

impl Settings {
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for (k, v) in text.lines().filter_map(|l| l.split_once('=')) {
            // A hand-edited file may hold anything: keep the default for what does not parse.
            let num = |old: f64| {
                v.trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|x| x.is_finite() && *x > 0.0)
                    .unwrap_or(old)
            };
            match k {
                "autosave_minutes" => s.autosave_minutes = v.trim().parse().unwrap_or(s.autosave_minutes),
                "recent" if s.recent.len() < MAX_RECENT => s.recent.push(PathBuf::from(v)),
                "slicer" => s.slicer = Slicer::from_name(v).unwrap_or(s.slicer),
                "printer" => s.printer = v.into(),
                "bed" => {
                    let n: Vec<f64> = v
                        .split(',')
                        .filter_map(|x| x.trim().parse().ok())
                        .filter(|x: &f64| x.is_finite() && *x > 0.0)
                        .collect();
                    if let [x, y, z] = n[..] {
                        s.bed = [x, y, z];
                    }
                }
                "overhang_deg" => s.overhang_deg = num(s.overhang_deg).min(90.0),
                "nozzle" => s.nozzle = num(s.nozzle),
                "min_wall" => s.min_wall = num(s.min_wall),
                "chord" => s.chord = num(s.chord),
                "angle_deg" => s.angle_deg = num(s.angle_deg).min(90.0),
                "check_updates" => s.check_updates = v.trim() != "false",
                "skip_update" => s.skip_update = v.trim().into(),
                "language" => s.language = crate::i18n::Language::from_id(v).unwrap_or(s.language),
                "theme" => s.theme = crate::ui::theme::Theme::from_id(v).unwrap_or(s.theme),
                "ui_scale" => s.ui_scale = (num(f64::from(s.ui_scale)) as f32).clamp(0.5, 3.0),
                "display_style" => {
                    s.display_style = threebcad_render::DisplayStyle::from_id(v).unwrap_or(s.display_style)
                }
                "perspective" => s.perspective = v.trim() != "false",
                "view_cube" => s.view_cube = v.trim() != "false",
                "units" => s.units = UnitSystem::ALL.into_iter().find(|u| u.short() == v.trim()).unwrap_or(s.units),
                "lod" => s.lod = v.trim() != "false",
                "show_fps" => s.show_fps = v.trim() == "true",
                "mouse_gestures" => s.mouse_gestures = v.trim() == "true",
                _ => {
                    if let Some(sl) = k.strip_prefix("slicer_path.").and_then(Slicer::from_name) {
                        s.slicer_paths[sl as usize] = Some(PathBuf::from(v));
                    }
                    if let Some(c) = k.strip_prefix("key.").and_then(crate::commands::Command::from_id) {
                        match v.trim() {
                            "none" => {
                                s.keys.insert(c, None);
                            }
                            key => {
                                if let Some(key) = crate::commands::parse(key) {
                                    s.keys.insert(c, Some(key));
                                }
                            }
                        }
                    }
                }
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        let [x, y, z] = self.bed;
        let mut out = format!(
            "autosave_minutes={}\nslicer={}\nprinter={}\nbed={x},{y},{z}\noverhang_deg={}\nnozzle={}\nmin_wall={}\nchord={}\nangle_deg={}\n",
            self.autosave_minutes,
            self.slicer.name(),
            self.printer,
            self.overhang_deg,
            self.nozzle,
            self.min_wall,
            self.chord,
            self.angle_deg
        );
        out += &format!("check_updates={}\n", self.check_updates);
        out += &format!(
            "language={}\ntheme={}\nui_scale={}\ndisplay_style={}\nperspective={}\nview_cube={}\nunits={}\nlod={}\nshow_fps={}\nmouse_gestures={}\n",
            self.language.id(),
            self.theme.id(),
            self.ui_scale,
            self.display_style.id(),
            self.perspective,
            self.view_cube,
            self.units.short(),
            self.lod,
            self.show_fps,
            self.mouse_gestures
        );
        for c in crate::commands::Command::ALL {
            if let Some(k) = self.keys.get(&c) {
                let k = k.as_ref().map_or_else(|| "none".to_string(), crate::commands::text);
                out += &format!("key.{}={k}\n", c.id());
            }
        }
        if !self.skip_update.is_empty() {
            out += &format!("skip_update={}\n", self.skip_update);
        }
        for s in Slicer::ALL {
            if let Some(p) = &self.slicer_paths[s as usize] {
                out += &format!("slicer_path.{}={}\n", s.name(), p.display());
            }
        }
        for p in &self.recent {
            out += &format!("recent={}\n", p.display());
        }
        out
    }

    pub fn load() -> Self {
        config_dir()
            .and_then(|d| std::fs::read_to_string(d.join("settings.txt")).ok())
            .map_or_else(Self::default, |t| Self::parse(&t))
    }

    pub fn save(&self) {
        if let Some(d) = config_dir() {
            // Written aside and moved into place, so a crash mid-write keeps the old file (with the
            // recent files and located slicers). Losing one change is not worth an error message.
            let tmp = d.join("settings.txt.tmp");
            let _ = std::fs::create_dir_all(&d)
                .and_then(|()| std::fs::write(&tmp, self.to_text()))
                .and_then(|()| std::fs::rename(&tmp, d.join("settings.txt")));
        }
    }

    /// Settings > Reset to defaults: every preference, but not the recent files or where the
    /// slicers were found.
    pub fn reset(&mut self) {
        *self = Self {
            recent: std::mem::take(&mut self.recent),
            slicer_paths: std::mem::take(&mut self.slicer_paths),
            ..Self::default()
        };
    }

    /// Puts `path` first in the recent list.
    pub fn add_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }
}

/// `%APPDATA%\3BCAD`, `~/Library/Application Support/3BCAD` or `~/.config/3bcad`.
pub fn config_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if cfg!(windows) {
        env("APPDATA").map(|d| d.join("3BCAD"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|d| d.join("Library/Application Support/3BCAD"))
    } else {
        env("XDG_CONFIG_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".config")))
            .map(|d| d.join("3bcad"))
    }
}

/// Where this run autosaves a part (or, with `ASSEMBLY_EXT`, an assembly): one file per process,
/// so two open copies of 3BCAD do not collide.
pub fn recovery_file(dir: &Path, ext: &str) -> PathBuf {
    dir.join(format!("recovery-{}.{ext}", std::process::id()))
}

fn is_assembly(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ASSEMBLY_EXT))
}

fn is_drawing(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(DRAWING_EXT))
}

fn is_import(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| IMPORT_EXTS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Adds a STEP, mesh or DXF file to `doc` as one feature: STEP as solid bodies, meshes as
/// reference, DXF as a sketch on `on` (the Top plane when `None`). Returns it and a status line.
fn import_into(
    doc: &mut PartDocument,
    path: &Path,
    on: Option<(Plane, PlaneRef)>,
) -> anyhow::Result<(threebcad_core::FeatureId, String)> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let file = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let mut msg = format!("Imported {file}");
    let id = match ext.as_str() {
        "step" | "stp" => {
            let import = threebcad_io::read_step(&std::fs::read(path)?)?;
            if !import.skipped.is_empty() {
                msg = format!("Imported {file}; {} shells could not be read", import.skipped.len());
                log::warn!("{file}: skipped {:?}", import.skipped);
            }
            doc.edit(|h| {
                h.add(FeatureKind::Imported {
                    file: file.clone(),
                    solids: Shared::new(import.solids),
                })
            })?
        }
        "stl" | "3mf" | "obj" => {
            let mesh = threebcad_io::read_mesh(path)?;
            doc.edit(|h| {
                h.add(FeatureKind::ImportedMesh {
                    file: file.clone(),
                    mesh: Shared::new(mesh),
                })
            })?
        }
        "dxf" => {
            let sketch = threebcad_io::read_dxf(&std::fs::read(path)?, None)?;
            doc.edit(|h| match on {
                Some((plane, on @ (PlaneRef::Face(_) | PlaneRef::Feature(_)))) => h.add_sketch_on(sketch, plane, on),
                Some((plane, _)) => Ok(h.add_sketch(sketch, plane)),
                None => Ok(h.add_sketch(sketch, Plane::TOP)),
            })?
        }
        _ => anyhow::bail!("3BCAD cannot import .{ext} files"),
    };
    Ok((id, msg))
}

/// The newest recovery file left behind by a run that did not close normally.
// shortcut: a second copy of 3BCAD that is still running also leaves a file here and would be
// offered too; lock the file if people run several copies at once.
pub fn leftover_recovery(dir: &Path, own: &[PathBuf]) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e == EXT || e == ASSEMBLY_EXT || e == DRAWING_EXT)
                && !own.contains(p)
        })
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

/// Something that discards the open part, waiting for the unsaved-changes answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    New,
    NewAssembly,
    NewDrawing,
    Open(PathBuf),
    Close,
}

/// File state the app keeps between frames.
pub struct Files {
    pub settings: Settings,
    /// Settings changed since they were saved; written once the mouse is up, not every drag frame.
    pub settings_changed: bool,
    pub path: Option<PathBuf>,
    pub pending: Option<Pending>,
    /// A recovery file to offer on startup.
    pub offer: Option<PathBuf>,
    /// The assembly a part was opened from with Edit Part, to go back to.
    pub return_to: Option<PathBuf>,
    /// The File > Properties window, and the rows being edited in it.
    pub properties: Option<Vec<(String, String)>>,
    /// The program to start once this one has closed (an update was installed).
    pub restart: Option<PathBuf>,
    recovery_dir: Option<PathBuf>,
    last_autosave: Instant,
    close_ok: bool,
    title: String,
}

impl Files {
    pub fn new() -> Self {
        let recovery_dir = config_dir().map(|d| d.join("recovery"));
        let offer = recovery_dir.as_ref().and_then(|d| {
            let own = [EXT, ASSEMBLY_EXT, DRAWING_EXT].map(|e| recovery_file(d, e));
            leftover_recovery(d, &own)
        });
        Self {
            settings: Settings::load(),
            settings_changed: false,
            path: None,
            pending: None,
            offer,
            return_to: None,
            properties: None,
            restart: None,
            recovery_dir,
            last_autosave: Instant::now(),
            close_ok: false,
            title: String::new(),
        }
    }

    fn own_recovery(&self, ext: &str) -> Option<PathBuf> {
        self.recovery_dir.as_deref().map(|d| recovery_file(d, ext))
    }

    pub fn clear_recovery(&self) {
        for ext in [EXT, ASSEMBLY_EXT, DRAWING_EXT] {
            if let Some(p) = self.own_recovery(ext) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

pub fn title(name: &str, dirty: bool) -> String {
    format!("{name}{} - 3BCAD {VERSION}", if dirty { " *" } else { "" })
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "Part".into(), |s| s.to_string_lossy().into_owned())
}

impl App {
    /// The open document's name: the assembly's while one is open, else the part's.
    pub fn doc_name(&self) -> String {
        if let Some(d) = &self.drw {
            return d.doc.name.clone();
        }
        self.asm
            .as_ref()
            .map_or_else(|| self.doc.name.clone(), |s| s.doc.name.clone())
    }

    pub fn is_dirty(&self) -> bool {
        if let Some(d) = &self.drw {
            return d.doc.is_dirty();
        }
        self.asm
            .as_ref()
            .map_or_else(|| self.doc.is_dirty(), |s| s.doc.is_dirty())
    }

    pub fn units(&self) -> Units {
        if let Some(d) = &self.drw {
            return d.doc.units();
        }
        self.asm.as_ref().map_or_else(|| self.doc.units(), |s| s.doc.units())
    }

    fn set_units(&mut self, u: Units) {
        if let Some(d) = &mut self.drw {
            return d.doc.set_units(u);
        }
        match &mut self.asm {
            Some(s) => s.doc.set_units(u),
            None => self.doc.set_units(u),
        }
    }

    pub fn can_undo(&self) -> bool {
        if let Some(d) = &self.drw {
            return d.doc.can_undo();
        }
        self.asm
            .as_ref()
            .map_or_else(|| self.doc.can_undo() || self.sketch.is_some(), |s| s.doc.can_undo())
    }

    pub fn can_redo(&self) -> bool {
        if let Some(d) = &self.drw {
            return d.doc.can_redo();
        }
        self.asm
            .as_ref()
            .map_or_else(|| self.doc.can_redo() || self.sketch.is_some(), |s| s.doc.can_redo())
    }

    pub fn undo(&mut self) {
        if let Some(d) = &mut self.drw {
            d.doc.undo();
            self.status = tr("Undone").into();
            return;
        }
        match (&mut self.asm, &mut self.sketch) {
            (Some(s), _) => s.doc.undo(),
            (None, Some(sk)) => return sk.undo(),
            (None, None) => self.doc.undo(),
        }
        self.status = tr("Undone").into();
    }

    pub fn redo(&mut self) {
        if let Some(d) = &mut self.drw {
            d.doc.redo();
            self.status = tr("Redone").into();
            return;
        }
        match (&mut self.asm, &mut self.sketch) {
            (Some(s), _) => s.doc.redo(),
            (None, Some(sk)) => return sk.redo(),
            (None, None) => self.doc.redo(),
        }
        self.status = tr("Redone").into();
    }

    /// Shortcuts, title, autosave, the close button and the file dialogs; once per frame.
    pub fn files_frame(&mut self, ctx: &egui::Context) {
        crate::ui::properties::set_units(self.units());
        self.dropped_files(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.files.close_ok && self.is_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.files.pending = Some(Pending::Close);
        }
        self.autosave(ctx);
        if self.files.settings_changed && !ctx.input(|i| i.pointer.any_down()) {
            self.files.settings.save();
            self.files.settings_changed = false;
        }
        let title = title(&self.doc_name(), self.is_dirty());
        if title != self.files.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.files.title = title;
        }
        self.unsaved_prompt(ctx);
        self.recovery_prompt(ctx);
    }

    /// Files dragged onto the window: the first one is opened like File > Open.
    fn dropped_files(&mut self, ctx: &egui::Context) {
        let (hovering, dropped) = ctx.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw
                    .dropped_files
                    .iter()
                    .filter_map(|f| f.path.clone())
                    .collect::<Vec<_>>(),
            )
        });
        if hovering {
            let rect = ctx.screen_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
            painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(120));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Drop to open (3BCAD, STEP, STL, 3MF, OBJ, DXF)",
                egui::FontId::proportional(22.0),
                egui::Color32::WHITE,
            );
        }
        if let Some(path) = dropped.first() {
            self.request(Pending::Open(path.clone()));
            if dropped.len() > 1 {
                self.status = format!("Opened the first of {} files; drop one file at a time", dropped.len());
            }
        }
    }

    fn autosave(&mut self, ctx: &egui::Context) {
        let every = self.files.settings.autosave_minutes;
        if every == 0 || !self.is_dirty() {
            self.files.last_autosave = Instant::now();
            return;
        }
        let due = Duration::from_secs(u64::from(every) * 60);
        if let Some(left) = due
            .checked_sub(self.files.last_autosave.elapsed())
            .filter(|d| !d.is_zero())
        {
            // egui only runs frames on input, so wake up when the autosave is due.
            ctx.request_repaint_after(left);
            return;
        }
        self.files.last_autosave = Instant::now();
        let ext = match (&self.asm, &self.drw) {
            (_, Some(_)) => DRAWING_EXT,
            (Some(_), _) => ASSEMBLY_EXT,
            _ => EXT,
        };
        let Some(path) = self.files.own_recovery(ext) else {
            return;
        };
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .map_err(anyhow::Error::from)
            .and_then(|()| match (&self.asm, &self.drw) {
                (_, Some(d)) => d.doc.write_to(&path),
                (Some(s), _) => s.doc.write_to(&path),
                _ => self.doc.write_to(&path),
            });
        if let Err(e) = written {
            self.status = format!("Autosave failed: {e:#}");
        }
    }

    /// Closes 3BCAD (asking about unsaved changes) and starts the program again: an update that was
    /// just installed.
    pub fn restart(&mut self, ctx: &egui::Context) {
        self.files.restart = std::env::current_exe().ok();
        if self.is_dirty() {
            self.files.pending = Some(Pending::Close);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Runs `p` now, or after asking about unsaved changes.
    pub fn request(&mut self, p: Pending) {
        if self.is_dirty() {
            self.files.pending = Some(p);
        } else {
            self.perform(p);
        }
    }

    fn perform(&mut self, p: Pending) {
        match p {
            Pending::New => {
                self.files.return_to = None;
                let units = Units {
                    system: self.files.settings.units,
                    ..Units::default()
                };
                self.set_doc(PartDocument::new_in("Part1", units), None);
                self.status = tr("New part").into();
            }
            Pending::NewAssembly => {
                self.set_assembly(AssemblyDocument::new("Assembly1"), None);
                self.status = tr("New assembly: insert a part to start").into();
            }
            Pending::NewDrawing => {
                self.set_drawing(DrawingDocument::new("Drawing1"), None);
                self.status = tr("New drawing: choose the part it shows (Drawing tab > Model)").into();
            }
            Pending::Open(path) if is_drawing(&path) => match DrawingDocument::open(&path) {
                Ok(mut doc) => {
                    doc.name = stem(&path);
                    self.status = match doc.part_error() {
                        None => format!("Opened {}", path.display()),
                        Some(why) => format!("Opened {}; its part could not be read: {why}", path.display()),
                    };
                    self.files.settings.add_recent(&path);
                    self.files.settings.save();
                    self.set_drawing(doc, Some(path));
                }
                Err(e) => {
                    self.status = format!("Could not open {}: {e:#}", path.display());
                    self.files.settings.recent.retain(|p| p != &path || p.exists());
                }
            },
            Pending::Open(path) if is_assembly(&path) => match AssemblyDocument::open(&path) {
                Ok(mut doc) => {
                    doc.name = stem(&path);
                    let missing = doc
                        .contents()
                        .components
                        .iter()
                        .filter(|c| doc.file_error(c.id).is_some())
                        .count();
                    self.status = match missing {
                        0 => format!("Opened {}", path.display()),
                        n => format!("Opened {}; {n} components could not be read", path.display()),
                    };
                    self.files.settings.add_recent(&path);
                    self.files.settings.save();
                    self.set_assembly(doc, Some(path));
                }
                Err(e) => {
                    self.status = format!("Could not open {}: {e:#}", path.display());
                    self.files.settings.recent.retain(|p| p != &path || p.exists());
                }
            },
            Pending::Open(path) if is_import(&path) => {
                // A new part, so the open one survives a file that cannot be read.
                let mut doc = PartDocument::new(stem(&path));
                match import_into(&mut doc, &path, None) {
                    Ok((id, msg)) => {
                        self.files.settings.add_recent(&path);
                        self.files.settings.save();
                        self.set_doc(doc, None);
                        self.selected = Some(id);
                        self.status = msg.replacen("Imported", "Opened", 1);
                    }
                    Err(e) => {
                        self.status = format!("Could not open {}: {e:#}", path.display());
                        self.files.settings.recent.retain(|p| p != &path || p.exists());
                    }
                }
            }
            Pending::Open(path) => match PartDocument::open(&path) {
                Ok(mut doc) => {
                    doc.name = stem(&path);
                    self.status = format!("Opened {}", path.display());
                    self.files.settings.add_recent(&path);
                    self.files.settings.save();
                    self.set_doc(doc, Some(path));
                }
                Err(e) => {
                    self.status = format!("Could not open {}: {e:#}", path.display());
                    self.files.settings.recent.retain(|p| p != &path || p.exists());
                }
            },
            Pending::Close => self.files.close_ok = true,
        }
    }

    fn set_doc(&mut self, doc: PartDocument, path: Option<PathBuf>) {
        self.doc = doc;
        self.asm = None;
        self.drw = None;
        if matches!(self.tab, RibbonTab::Assembly | RibbonTab::Drawing) {
            self.tab = RibbonTab::Features;
        }
        self.files.path = path;
        self.files.clear_recovery();
        self.sketch = None;
        self.sketch_on = None;
        self.selected = None;
        self.selection.clear();
        self.renaming = None;
        self.forget_caches();
        self.sync_geometry();
        self.zoom_to_fit();
    }

    /// Opens an assembly; the part that was open stays in memory, out of sight.
    fn set_assembly(&mut self, doc: AssemblyDocument, path: Option<PathBuf>) {
        self.asm = Some(Session::new(doc));
        self.drw = None;
        self.files.path = path;
        self.files.return_to = None;
        self.files.clear_recovery();
        self.tab = RibbonTab::Assembly;
        self.sketch = None;
        self.sketch_on = None;
        self.selected = None;
        self.selection.clear();
        self.renaming = None;
        self.forget_caches();
        self.sync_geometry();
        self.zoom_to_fit();
    }

    /// Opens a drawing; the part that was open stays in memory, out of sight.
    fn set_drawing(&mut self, doc: DrawingDocument, path: Option<PathBuf>) {
        self.drw = Some(crate::drawing::Session::new(doc));
        self.asm = None;
        self.files.path = path;
        self.files.return_to = None;
        self.files.clear_recovery();
        self.tab = RibbonTab::Drawing;
        self.sketch = None;
        self.sketch_on = None;
        self.selected = None;
        self.selection.clear();
        self.renaming = None;
    }

    pub fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "All supported",
                &[&[EXT, ASSEMBLY_EXT, DRAWING_EXT][..], &IMPORT_EXTS[..]].concat(),
            )
            .add_filter("3BCAD part, assembly or drawing", &[EXT, ASSEMBLY_EXT, DRAWING_EXT])
            .add_filter("STEP", &["step", "stp"])
            .add_filter("Mesh (STL, 3MF, OBJ)", &["stl", "3mf", "obj"])
            .add_filter("DXF drawing", &["dxf"])
            .add_filter("3BCAD part", &[EXT])
            .add_filter("3BCAD assembly", &[ASSEMBLY_EXT])
            .add_filter("3BCAD drawing", &[DRAWING_EXT])
            .pick_file()
        {
            self.request(Pending::Open(path));
        }
    }

    /// Saves to the part's file, asking for one the first time; false if nothing was saved.
    pub fn save(&mut self) -> bool {
        match self.files.path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    pub fn save_as(&mut self) -> bool {
        let (what, ext) = match (&self.asm, &self.drw) {
            (_, Some(_)) => ("3BCAD drawing", DRAWING_EXT),
            (Some(_), _) => ("3BCAD assembly", ASSEMBLY_EXT),
            _ => ("3BCAD part", EXT),
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter(what, &[ext])
            .set_file_name(format!("{}.{ext}", self.doc_name()))
            .save_file()
        else {
            return false;
        };
        self.save_to(&path)
    }

    fn save_to(&mut self, path: &Path) -> bool {
        // An open sketch is part of what the user sees, so it is saved too.
        if self.sketch.is_some() {
            self.exit_sketch(true);
        }
        let saved = match (&mut self.asm, &mut self.drw) {
            (_, Some(d)) => {
                d.doc.name = stem(path);
                d.doc.save(path)
            }
            (Some(s), _) => {
                s.doc.name = stem(path);
                s.doc.save(path)
            }
            _ => {
                self.doc.name = stem(path);
                self.doc.save(path)
            }
        };
        match saved {
            Ok(()) => {
                self.files.path = Some(path.to_path_buf());
                self.files.settings.add_recent(path);
                self.files.settings.save();
                self.files.clear_recovery();
                self.status = format!("Saved {}", path.display());
                true
            }
            Err(e) => {
                self.status = format!("Save failed: {e:#}");
                false
            }
        }
    }

    fn unsaved_prompt(&mut self, ctx: &egui::Context) {
        let Some(p) = self.files.pending.clone() else { return };
        let mut answer = None;
        egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading(tr("Save changes?"));
            ui.label(format!("{} has changes that are not saved.", self.doc_name()));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Save")).clicked() {
                    answer = Some(true);
                }
                if ui.button(tr("Don't Save")).clicked() {
                    answer = Some(false);
                }
                if ui.button(tr("Cancel")).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.files.pending = None;
                    self.files.restart = None;
                }
            });
        });
        let Some(save) = answer else { return };
        self.files.pending = None;
        if save && !self.save() {
            return;
        }
        let close = p == Pending::Close;
        self.perform(p);
        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn recovery_prompt(&mut self, ctx: &egui::Context) {
        let Some(path) = self.files.offer.clone() else { return };
        let when = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map_or(String::new(), |d| format!(" {} min ago", d.as_secs() / 60));
        let mut answer = None;
        egui::Modal::new(egui::Id::new("recover")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading(tr("Recover your work?"));
            ui.label(format!(
                "3BCAD did not close normally last time. A copy of the {} was autosaved{when}.",
                if is_assembly(&path) {
                    "assembly"
                } else if is_drawing(&path) {
                    "drawing"
                } else {
                    "part"
                }
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Recover")).clicked() {
                    answer = Some(true);
                }
                if ui.button(tr("Discard")).clicked() {
                    answer = Some(false);
                }
            });
        });
        let Some(recover) = answer else { return };
        self.files.offer = None;
        if recover {
            // Unsaved until the user saves it somewhere.
            let recovered = if is_assembly(&path) {
                AssemblyDocument::open(&path).map(|mut doc| {
                    doc.mark_dirty();
                    self.set_assembly(doc, None);
                })
            } else if is_drawing(&path) {
                DrawingDocument::open(&path).map(|mut doc| {
                    doc.mark_dirty();
                    self.set_drawing(doc, None);
                })
            } else {
                PartDocument::open(&path).map(|mut doc| {
                    doc.mark_dirty();
                    self.set_doc(doc, None);
                })
            };
            self.status = match recovered {
                Ok(()) => "Recovered the autosaved file: save it to keep it".into(),
                Err(e) => format!("Could not recover: {e:#}"),
            };
        }
        let _ = std::fs::remove_file(path);
    }

    pub fn import_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("All supported", &["step", "stp", "stl", "3mf", "obj", "dxf"])
            .add_filter("STEP", &["step", "stp"])
            .add_filter("Mesh (STL, 3MF, OBJ)", &["stl", "3mf", "obj"])
            .add_filter("DXF drawing", &["dxf"])
            .pick_file()
        else {
            return;
        };
        self.status = match self.import(&path) {
            Ok(msg) => msg,
            Err(e) => format!("Import failed: {e:#}"),
        };
    }

    /// Adds a file to the part: STEP as solid bodies, meshes as reference, DXF as a sketch.
    pub fn import(&mut self, path: &Path) -> anyhow::Result<String> {
        let on = self.sketch_target().map(|(plane, _, on)| (plane, on));
        let (id, msg) = import_into(&mut self.doc, path, on)?;
        self.selected = Some(id);
        self.sync_geometry();
        self.zoom_to_fit();
        Ok(msg)
    }

    /// Exports the selected sketch, or the selected planar face's outline, as DXF.
    pub fn export_dxf(&mut self) {
        let sketch = match self.selected.and_then(|id| self.doc.history().get(id)) {
            Some(f) => match &f.kind {
                FeatureKind::Sketch { sketch, .. } => Some(sketch.clone()),
                _ => None,
            },
            None => None,
        };
        let sketch = sketch.or_else(|| {
            self.selection.iter().find_map(|r| match *r {
                crate::app::ModelRef::Face { body, face } => self.face_sketch(body, face),
                _ => None,
            })
        });
        let Some(sketch) = sketch else {
            self.status = tr("Select a sketch or a planar face to export as DXF").into();
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("DXF", &["dxf"])
            .set_file_name(format!("{}.dxf", self.doc.name))
            .save_file()
        else {
            return;
        };
        self.status = match threebcad_io::write_dxf(&path, &sketch) {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => format!("Export failed: {e}"),
        };
    }
}

/// The File menu in the title row.
pub fn menu(app: &mut App, ui: &mut egui::Ui) {
    use crate::commands::Command;
    use crate::i18n::tr;
    let part = app.asm.is_none() && app.drw.is_none();
    let keys = app.files.settings.keys.clone();
    let key = |c: Command| crate::commands::key_of(&keys, c).map_or_else(String::new, |k| crate::commands::text(&k));
    ui.menu_button(tr("File"), |ui| {
        if ui.add(egui::Button::new(tr("New Part")).shortcut_text(key(Command::New))).clicked() {
            ui.close_menu();
            app.request(Pending::New);
        }
        if ui
            .button(tr("New Assembly"))
            .on_hover_text(tr("Put parts together with mates"))
            .clicked()
        {
            ui.close_menu();
            app.request(Pending::NewAssembly);
        }
        if ui
            .button(tr("New Drawing"))
            .on_hover_text(tr("Engineering drawing of a part: views, sections, dimensions, PDF / DXF"))
            .clicked()
        {
            ui.close_menu();
            app.request(Pending::NewDrawing);
        }
        if ui
            .add(egui::Button::new(tr("Open…")).shortcut_text(key(Command::Open)))
            .on_hover_text(tr("3BCAD files, or STEP, STL, 3MF, OBJ and DXF as a new part"))
            .clicked()
        {
            ui.close_menu();
            app.open_dialog();
        }
        ui.menu_button(tr("Open Recent"), |ui| {
            if app.files.settings.recent.is_empty() {
                ui.weak(tr("No recent parts"));
            }
            for p in app.files.settings.recent.clone() {
                if ui.button(p.display().to_string()).clicked() {
                    ui.close_menu();
                    app.request(Pending::Open(p));
                }
            }
        });
        if ui.add(egui::Button::new(tr("Save")).shortcut_text(key(Command::Save))).clicked() {
            ui.close_menu();
            app.save();
        }
        if ui
            .add(egui::Button::new(tr("Save As…")).shortcut_text(key(Command::SaveAs)))
            .clicked()
        {
            ui.close_menu();
            app.save_as();
        }
        if ui
            .button(tr("Properties…"))
            .on_hover_text(tr("Part number, description and other properties, for the bill of materials"))
            .clicked()
        {
            ui.close_menu();
            app.files.properties = Some(match (&app.asm, &app.drw) {
                (_, Some(d)) => d.doc.properties().to_vec(),
                (Some(s), _) => s.doc.properties().to_vec(),
                _ => app.doc.properties().to_vec(),
            });
        }
        ui.separator();
        if ui
            .add_enabled(part, egui::Button::new(tr("Import…")))
            .on_hover_text(tr("STEP solids, STL / 3MF / OBJ meshes, DXF sketches"))
            .clicked()
        {
            ui.close_menu();
            app.import_dialog();
        }
        if ui
            .add_enabled(part, egui::Button::new(tr("Export DXF…")))
            .on_hover_text(tr("The selected sketch or planar face"))
            .clicked()
        {
            ui.close_menu();
            app.export_dxf();
        }
        ui.separator();
        ui.menu_button(tr("Units"), |ui| units_ui(app, ui));
        if part {
            ui.menu_button(tr("Material"), |ui| crate::ui::evaluate::material_editor(app, ui));
        }
        ui.separator();
        if ui
            .add(egui::Button::new(tr("Settings…")).shortcut_text(key(Command::Settings)))
            .on_hover_text(tr("Theme, language, interface size, shortcuts, mouse, autosave, updates"))
            .clicked()
        {
            ui.close_menu();
            app.run(ui.ctx(), Command::Settings);
        }
    });
}

/// Unit system and precision of the open part.
pub fn units_ui(app: &mut App, ui: &mut egui::Ui) {
    let mut u: Units = app.units();
    for s in UnitSystem::ALL {
        ui.radio_value(&mut u.system, s, s.name());
    }
    ui.horizontal(|ui| {
        ui.label(tr("Decimals"));
        ui.add(egui::DragValue::new(&mut u.decimals).range(0..=6));
    });
    app.set_units(u);
}

/// File > Properties: name and value rows for the open part or assembly.
pub fn properties_window(app: &mut App, ctx: &egui::Context) {
    let Some(mut rows) = app.files.properties.take() else {
        return;
    };
    let mut open = true;
    let mut apply = false;
    egui::Window::new(format!("Properties of {}", app.doc_name()))
        .open(&mut open)
        .default_width(460.0)
        .show(ctx, |ui| {
            ui.weak(
                "Part number and Description go into the bill of materials; other rows become extra columns. \
                 Drawings fill in {Name} in title blocks and notes.",
            );
            egui::Grid::new("custom_properties").num_columns(3).show(ui, |ui| {
                ui.strong(tr("Name"));
                ui.strong(tr("Value"));
                ui.end_row();
                let mut gone = None;
                for (i, (k, v)) in rows.iter_mut().enumerate() {
                    // A grid cell is only as wide as its column was: size the fields outright.
                    let h = ui.spacing().interact_size.y;
                    ui.add_sized([150.0, h], egui::TextEdit::singleline(k));
                    ui.add_sized([260.0, h], egui::TextEdit::singleline(v));
                    if ui.small_button("✖").on_hover_text(tr("Remove")).clicked() {
                        gone = Some(i);
                    }
                    ui.end_row();
                }
                if let Some(i) = gone {
                    rows.remove(i);
                }
            });
            ui.horizontal(|ui| {
                for name in ["Part number", "Description", "Author"] {
                    let missing = !rows.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
                    if missing && ui.button(format!("+ {name}")).clicked() {
                        rows.push((name.into(), String::new()));
                    }
                }
                if ui.button(tr("+ Other")).clicked() {
                    rows.push((String::new(), String::new()));
                }
            });
            ui.separator();
            apply = ui.button(tr("Apply")).clicked();
        });
    if apply {
        let clean: Vec<(String, String)> = rows
            .iter()
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .filter(|(k, _)| !k.is_empty())
            .collect();
        match (&mut app.asm, &mut app.drw) {
            (_, Some(d)) => d.doc.set_properties(clean),
            (Some(s), _) => s.doc.set_properties(clean),
            _ => app.doc.set_properties(clean),
        }
        app.status = tr("Properties saved with the file").into();
        return;
    }
    if open {
        app.files.properties = Some(rows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_cap_recent_files() {
        let mut s = Settings::default();
        for i in 0..12 {
            s.add_recent(Path::new(&format!("part{i}.3bcad")));
        }
        s.add_recent(Path::new("part5.3bcad"));
        assert_eq!(s.recent.len(), MAX_RECENT);
        assert_eq!(s.recent[0], Path::new("part5.3bcad"));
        assert_eq!(s.recent.iter().filter(|p| p.ends_with("part5.3bcad")).count(), 1);
        s.autosave_minutes = 3;
        s.slicer = Slicer::PrusaSlicer;
        s.slicer_paths[Slicer::BambuStudio as usize] = Some(PathBuf::from("D:\\Apps\\Bambu Studio\\bambu-studio.exe"));
        s.printer = "Custom".into();
        s.bed = [300.0, 250.5, 400.0];
        s.min_wall = 1.2;
        s.angle_deg = 5.0;
        s.check_updates = false;
        s.skip_update = "0.9.0".into();
        s.language = crate::i18n::Language::Vietnamese;
        s.theme = crate::ui::theme::Theme::HighContrast;
        s.ui_scale = 1.5;
        s.display_style = threebcad_render::DisplayStyle::HiddenLinesVisible;
        s.perspective = false;
        s.view_cube = false;
        s.units = UnitSystem::Ips;
        s.lod = false;
        s.show_fps = true;
        s.mouse_gestures = true;
        use crate::commands::Command;
        s.keys.insert(Command::ZoomToFit, None);
        s.keys.insert(Command::SectionView, crate::commands::parse("Ctrl+Shift+X"));
        assert_eq!(Settings::parse(&s.to_text()), s);
        let junk = Settings::parse(
            "junk\nautosave_minutes=x\nbed=1,2\nnozzle=-1\nchord=NaN\nslicer=Cura\ntheme=pink\nui_scale=0\nkey.save=Hyper+S\nkey.nothing=F",
        );
        assert_eq!(junk, Settings::default());
    }

    #[test]
    fn reset_restores_every_preference_but_keeps_recent_files_and_slicers() {
        let mut s = Settings::default();
        s.add_recent(Path::new("part.3bcad"));
        s.slicer_paths[0] = Some(PathBuf::from("orca.exe"));
        s.theme = crate::ui::theme::Theme::Dark;
        s.ui_scale = 2.0;
        s.keys.insert(crate::commands::Command::Save, None);
        s.reset();
        assert_eq!(
            s,
            Settings {
                recent: vec![PathBuf::from("part.3bcad")],
                slicer_paths: [Some(PathBuf::from("orca.exe")), None, None],
                ..Settings::default()
            }
        );
    }

    #[test]
    fn leftover_recovery_skips_this_run() {
        let dir = std::env::temp_dir().join(format!("3bcad-recovery-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let own = [recovery_file(&dir, EXT), recovery_file(&dir, ASSEMBLY_EXT)];
        std::fs::write(&own[0], "{}").unwrap();
        std::fs::write(&own[1], "{}").unwrap();
        assert_eq!(leftover_recovery(&dir, &own), None);
        let old = dir.join("recovery-1.3bcasm");
        std::fs::write(&old, "{}").unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        assert_eq!(leftover_recovery(&dir, &own), Some(old));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn other_apps_files_open_as_a_new_part() {
        let dir = std::env::temp_dir().join(format!("3bcad-open-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let square = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let solid = threebcad_kernel::operations::extrude_polygon(&square, 5.0).unwrap();
        let stl = dir.join("cube.STL");
        let mesh = threebcad_kernel::mesher::print_mesh(&solid, 0.01);
        threebcad_io::write_stl(&stl, &[("cube", &mesh)]).unwrap();
        let step = dir.join("cube.stp");
        threebcad_io::write_step(&step, "cube", [&solid], &[]).unwrap();
        assert!(is_import(&stl) && is_import(&step) && !is_import(&dir.join("cube.3bcad")));

        let mut doc = PartDocument::new(stem(&step));
        let (_, msg) = import_into(&mut doc, &step, None).unwrap();
        assert_eq!((doc.bodies().len(), msg.as_str()), (1, "Imported cube.stp"));
        let mut doc = PartDocument::new(stem(&stl));
        import_into(&mut doc, &stl, None).unwrap();
        assert_eq!(doc.built().meshes.len(), 1);
        assert!(import_into(&mut doc, &dir.join("missing.obj"), None).is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn title_marks_unsaved_changes() {
        let mut doc = PartDocument::demo();
        assert_eq!(title(&doc.name, doc.is_dirty()), format!("Part1 - 3BCAD {VERSION}"));
        doc.edit(|_| ());
        assert!(title(&doc.name, doc.is_dirty()).starts_with("Part1 * - "));
    }
}
