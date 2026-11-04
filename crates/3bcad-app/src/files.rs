//! The desktop file workflow: New / Open / Save, recent files, the unsaved-changes prompt,
//! autosave with crash recovery, and importing other formats.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use threebcad_core::{FeatureKind, PartDocument, PlaneRef, Shared, UnitSystem, Units};
use threebcad_kernel::Plane;

use crate::app::{App, VERSION};

const MAX_RECENT: usize = 8;
pub const EXT: &str = "3bcad";

/// Preferences kept between runs, in `settings.txt` in the user's config folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// 0 turns autosave off.
    pub autosave_minutes: u32,
    pub recent: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            autosave_minutes: 10,
            recent: Vec::new(),
        }
    }
}

impl Settings {
    pub fn parse(text: &str) -> Self {
        let mut s = Self {
            recent: Vec::new(),
            ..Self::default()
        };
        for line in text.lines() {
            match line.split_once('=') {
                Some(("autosave_minutes", v)) => s.autosave_minutes = v.trim().parse().unwrap_or(s.autosave_minutes),
                Some(("recent", v)) if s.recent.len() < MAX_RECENT => s.recent.push(PathBuf::from(v)),
                _ => {}
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        let mut out = format!("autosave_minutes={}\n", self.autosave_minutes);
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
            // Losing a preference is not worth an error message.
            let _ = std::fs::create_dir_all(&d).and_then(|()| std::fs::write(d.join("settings.txt"), self.to_text()));
        }
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

/// Where this run autosaves: one file per process, so two open copies of 3BCAD do not collide.
pub fn recovery_file(dir: &Path) -> PathBuf {
    dir.join(format!("recovery-{}.{EXT}", std::process::id()))
}

/// The newest recovery file left behind by a run that did not close normally.
// shortcut: a second copy of 3BCAD that is still running also leaves a file here and would be
// offered too; lock the file if people run several copies at once.
pub fn leftover_recovery(dir: &Path, own: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == EXT) && p != own)
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

/// Something that discards the open part, waiting for the unsaved-changes answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    New,
    Open(PathBuf),
    Close,
}

/// File state the app keeps between frames.
pub struct Files {
    pub settings: Settings,
    pub path: Option<PathBuf>,
    pub pending: Option<Pending>,
    /// A recovery file to offer on startup.
    pub offer: Option<PathBuf>,
    recovery_dir: Option<PathBuf>,
    last_autosave: Instant,
    close_ok: bool,
    title: String,
}

impl Files {
    pub fn new() -> Self {
        let recovery_dir = config_dir().map(|d| d.join("recovery"));
        let offer = recovery_dir
            .as_ref()
            .and_then(|d| leftover_recovery(d, &recovery_file(d)));
        Self {
            settings: Settings::load(),
            path: None,
            pending: None,
            offer,
            recovery_dir,
            last_autosave: Instant::now(),
            close_ok: false,
            title: String::new(),
        }
    }

    fn own_recovery(&self) -> Option<PathBuf> {
        self.recovery_dir.as_deref().map(recovery_file)
    }

    pub fn clear_recovery(&self) {
        if let Some(p) = self.own_recovery() {
            let _ = std::fs::remove_file(p);
        }
    }
}

pub fn title(doc: &PartDocument) -> String {
    format!(
        "{}{} - 3BCAD {VERSION}",
        doc.name,
        if doc.is_dirty() { " *" } else { "" }
    )
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "Part".into(), |s| s.to_string_lossy().into_owned())
}

impl App {
    /// Shortcuts, title, autosave, the close button and the file dialogs; once per frame.
    pub fn files_frame(&mut self, ctx: &egui::Context) {
        crate::ui::properties::set_units(self.doc.units());
        // Ctrl+Shift+S first: the Ctrl+S shortcut would also match it.
        let (new, open, save_as, save) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::N)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::O)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::S)),
            )
        });
        if new {
            self.request(Pending::New);
        }
        if open {
            self.open_dialog();
        }
        if save_as {
            self.save_as();
        } else if save {
            self.save();
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.files.close_ok && self.doc.is_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.files.pending = Some(Pending::Close);
        }
        self.autosave(ctx);
        let title = title(&self.doc);
        if title != self.files.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.files.title = title;
        }
        self.unsaved_prompt(ctx);
        self.recovery_prompt(ctx);
    }

    fn autosave(&mut self, ctx: &egui::Context) {
        let every = self.files.settings.autosave_minutes;
        if every == 0 || !self.doc.is_dirty() {
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
        let Some(path) = self.files.own_recovery() else { return };
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .map_err(anyhow::Error::from)
            .and_then(|()| self.doc.write_to(&path));
        if let Err(e) = written {
            self.status = format!("Autosave failed: {e:#}");
        }
    }

    /// Runs `p` now, or after asking about unsaved changes.
    pub fn request(&mut self, p: Pending) {
        if self.doc.is_dirty() {
            self.files.pending = Some(p);
        } else {
            self.perform(p);
        }
    }

    fn perform(&mut self, p: Pending) {
        match p {
            Pending::New => {
                self.set_doc(PartDocument::new("Part1"), None);
                self.status = "New part".into();
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
        self.files.path = path;
        self.files.clear_recovery();
        self.sketch = None;
        self.sketch_on = None;
        self.selected = None;
        self.selection.clear();
        self.renaming = None;
        self.sync_geometry();
        self.zoom_to_fit();
    }

    pub fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter("3BCAD part", &[EXT]).pick_file() {
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
        let Some(path) = rfd::FileDialog::new()
            .add_filter("3BCAD part", &[EXT])
            .set_file_name(format!("{}.{EXT}", self.doc.name))
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
        self.doc.name = stem(path);
        match self.doc.save(path) {
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
            ui.heading("Save changes?");
            ui.label(format!("{} has changes that are not saved.", self.doc.name));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    answer = Some(true);
                }
                if ui.button("Don't Save").clicked() {
                    answer = Some(false);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.files.pending = None;
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
            ui.heading("Recover your work?");
            ui.label(format!(
                "3BCAD did not close normally last time. A copy of the part was autosaved{when}."
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Recover").clicked() {
                    answer = Some(true);
                }
                if ui.button("Discard").clicked() {
                    answer = Some(false);
                }
            });
        });
        let Some(recover) = answer else { return };
        self.files.offer = None;
        if recover {
            match PartDocument::open(&path) {
                Ok(mut doc) => {
                    // Unsaved until the user saves it somewhere.
                    doc.mark_dirty();
                    self.set_doc(doc, None);
                    self.status = "Recovered the autosaved part: save it to keep it".into();
                }
                Err(e) => self.status = format!("Could not recover: {e:#}"),
            }
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
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let file = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let id = match ext.as_str() {
            "step" | "stp" => {
                let import = threebcad_io::read_step(&std::fs::read(path)?)?;
                let skipped = import.skipped.len();
                let id = self.doc.edit(|h| {
                    h.add(FeatureKind::Imported {
                        file: file.clone(),
                        solids: Shared::new(import.solids),
                    })
                })?;
                self.selected = Some(id);
                if skipped > 0 {
                    return Ok(format!("Imported {file}; {skipped} shells could not be read"));
                }
                id
            }
            "stl" | "3mf" | "obj" => {
                let mesh = threebcad_io::read_mesh(path)?;
                self.doc.edit(|h| {
                    h.add(FeatureKind::ImportedMesh {
                        file: file.clone(),
                        mesh: Shared::new(mesh),
                    })
                })?
            }
            "dxf" => {
                let sketch = threebcad_io::read_dxf(&std::fs::read(path)?, None)?;
                let target = self.sketch_target();
                self.doc.edit(|h| match target {
                    Some((plane, _, on @ (PlaneRef::Face(_) | PlaneRef::Feature(_)))) => {
                        h.add_sketch_on(sketch, plane, on)
                    }
                    Some((plane, ..)) => Ok(h.add_sketch(sketch, plane)),
                    None => Ok(h.add_sketch(sketch, Plane::TOP)),
                })?
            }
            _ => anyhow::bail!("3BCAD cannot import .{ext} files"),
        };
        self.selected = Some(id);
        self.sync_geometry();
        self.zoom_to_fit();
        Ok(format!("Imported {file}"))
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
            self.status = "Select a sketch or a planar face to export as DXF".into();
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
    ui.menu_button("File", |ui| {
        if ui.add(egui::Button::new("New").shortcut_text("Ctrl+N")).clicked() {
            ui.close_menu();
            app.request(Pending::New);
        }
        if ui.add(egui::Button::new("Open…").shortcut_text("Ctrl+O")).clicked() {
            ui.close_menu();
            app.open_dialog();
        }
        ui.menu_button("Open Recent", |ui| {
            if app.files.settings.recent.is_empty() {
                ui.weak("No recent parts");
            }
            for p in app.files.settings.recent.clone() {
                if ui.button(p.display().to_string()).clicked() {
                    ui.close_menu();
                    app.request(Pending::Open(p));
                }
            }
        });
        if ui.add(egui::Button::new("Save").shortcut_text("Ctrl+S")).clicked() {
            ui.close_menu();
            app.save();
        }
        if ui
            .add(egui::Button::new("Save As…").shortcut_text("Ctrl+Shift+S"))
            .clicked()
        {
            ui.close_menu();
            app.save_as();
        }
        ui.separator();
        if ui
            .button("Import…")
            .on_hover_text("STEP solids, STL / 3MF / OBJ meshes, DXF sketches")
            .clicked()
        {
            ui.close_menu();
            app.import_dialog();
        }
        if ui
            .button("Export DXF…")
            .on_hover_text("The selected sketch or planar face")
            .clicked()
        {
            ui.close_menu();
            app.export_dxf();
        }
        ui.separator();
        ui.menu_button("Units", |ui| units_ui(app, ui));
        ui.menu_button("Material", |ui| crate::ui::evaluate::material_editor(app, ui));
        ui.horizontal(|ui| {
            ui.label("Autosave every");
            let mut m = app.files.settings.autosave_minutes;
            if ui
                .add(egui::DragValue::new(&mut m).range(0..=120).suffix(" min"))
                .on_hover_text("0 turns autosave off")
                .changed()
            {
                app.files.settings.autosave_minutes = m;
                app.files.settings.save();
            }
        });
    });
}

/// Unit system and precision of the open part.
pub fn units_ui(app: &mut App, ui: &mut egui::Ui) {
    let mut u: Units = app.doc.units();
    for s in UnitSystem::ALL {
        ui.radio_value(&mut u.system, s, s.name());
    }
    ui.horizontal(|ui| {
        ui.label("Decimals");
        ui.add(egui::DragValue::new(&mut u.decimals).range(0..=6));
    });
    app.doc.set_units(u);
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
        assert_eq!(Settings::parse(&s.to_text()), s);
        assert_eq!(Settings::parse("junk\nautosave_minutes=x").autosave_minutes, 10);
    }

    #[test]
    fn leftover_recovery_skips_this_run() {
        let dir = std::env::temp_dir().join(format!("3bcad-recovery-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let own = recovery_file(&dir);
        std::fs::write(&own, "{}").unwrap();
        assert_eq!(leftover_recovery(&dir, &own), None);
        let old = dir.join("recovery-1.3bcad");
        std::fs::write(&old, "{}").unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        assert_eq!(leftover_recovery(&dir, &own), Some(old));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn title_marks_unsaved_changes() {
        let mut doc = PartDocument::demo();
        assert_eq!(title(&doc), format!("Part1 - 3BCAD {VERSION}"));
        doc.edit(|_| ());
        assert!(title(&doc).starts_with("Part1 * - "));
    }
}
