//! File > Settings (Ctrl+,): every preference in one window, saved for this user.

use eframe::egui::{self, KeyboardShortcut, Modifiers, RichText};
use threebcad_core::UnitSystem;
use threebcad_io::Slicer;
use threebcad_render::DisplayStyle;

use crate::app::App;
use crate::commands::{self, Command};
use crate::i18n::{tr, trf, Language};
use crate::ui::theme::Theme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    General,
    View,
    Mouse,
    Shortcuts,
    Printing,
    Performance,
}

impl Page {
    const ALL: [Self; 6] = [
        Self::General,
        Self::View,
        Self::Mouse,
        Self::Shortcuts,
        Self::Printing,
        Self::Performance,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::View => "View",
            Self::Mouse => "Mouse",
            Self::Shortcuts => "Shortcuts",
            Self::Printing => "3D printing",
            Self::Performance => "Performance",
        }
    }
}

/// The open Settings window.
#[derive(Debug, Default)]
pub struct SettingsWindow {
    pub page: Page,
    /// The command waiting for its new shortcut to be pressed.
    pub capture: Option<Command>,
    message: Option<String>,
    confirm_reset: bool,
}

/// The shortcut a key press makes, from this frame's input (Esc cancels: `Some(None)`).
fn pressed(ctx: &egui::Context) -> Option<Option<KeyboardShortcut>> {
    ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key {
                key, pressed: true, modifiers, ..
            } => {
                if *key == egui::Key::Escape {
                    return Some(None);
                }
                let mut m = Modifiers::NONE;
                if modifiers.command || modifiers.ctrl {
                    m = m.plus(Modifiers::COMMAND);
                }
                if modifiers.alt {
                    m = m.plus(Modifiers::ALT);
                }
                if modifiers.shift {
                    m = m.plus(Modifiers::SHIFT);
                }
                Some(Some(KeyboardShortcut::new(m, *key)))
            }
            _ => None,
        })
    })
}

pub fn window(app: &mut App, ctx: &egui::Context) {
    let Some(mut w) = app.settings_window.take() else {
        return;
    };
    // A key press while waiting for a shortcut is the shortcut, not a command.
    if let Some(cmd) = w.capture {
        if let Some(key) = pressed(ctx) {
            w.capture = None;
            match key {
                None => {}
                Some(key) => match commands::conflict(&app.files.settings.keys, cmd, key) {
                    Some(other) => {
                        w.message = Some(trf("{} is already the shortcut for {}", &[&commands::text(&key), &tr(other.name())]));
                    }
                    None => {
                        let keys = &mut app.files.settings.keys;
                        if Some(key) == cmd.default_key() {
                            keys.remove(&cmd);
                        } else {
                            keys.insert(cmd, Some(key));
                        }
                        w.message = None;
                        app.files.settings_changed = true;
                    }
                },
            }
        }
    }
    let mut open = true;
    let mut changed = false;
    egui::Window::new(tr("Settings"))
        .open(&mut open)
        .default_width(560.0)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(130.0);
                    for p in Page::ALL {
                        if ui.selectable_label(w.page == p, tr(p.name())).clicked() {
                            w.page = p;
                            w.capture = None;
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_min_width(380.0);
                    let s = &mut app.files.settings;
                    match w.page {
                        Page::General => {
                            ui.horizontal(|ui| {
                                ui.label(tr("Language"));
                                egui::ComboBox::from_id_salt("language")
                                    .selected_text(s.language.name())
                                    .show_ui(ui, |ui| {
                                        for l in Language::ALL {
                                            changed |= ui.selectable_value(&mut s.language, l, l.name()).changed();
                                        }
                                    });
                            });
                            ui.add_space(4.0);
                            ui.label(tr("Theme"));
                            for t in Theme::ALL {
                                changed |= ui.radio_value(&mut s.theme, t, tr(t.name())).changed();
                            }
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label(tr("Interface size"));
                                let mut pct = (s.ui_scale * 100.0).round();
                                let r = ui.add(
                                    egui::Slider::new(&mut pct, 50.0..=300.0)
                                        .step_by(10.0)
                                        .suffix(" %"),
                                );
                                if r.changed() {
                                    s.ui_scale = pct / 100.0;
                                    changed = true;
                                }
                            });
                            ui.weak(tr("Ctrl + plus and Ctrl + minus change it too; Ctrl+0 resets it."));
                            ui.add_space(4.0);
                            ui.label(tr("Units of new parts"));
                            ui.horizontal(|ui| {
                                for u in UnitSystem::ALL {
                                    changed |= ui.radio_value(&mut s.units, u, tr(u.name())).changed();
                                }
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label(tr("Autosave every"));
                                changed |= ui
                                    .add(egui::DragValue::new(&mut s.autosave_minutes).range(0..=120).suffix(" min"))
                                    .on_hover_text(tr("0 turns autosave off"))
                                    .changed();
                            });
                            changed |= ui
                                .checkbox(&mut s.check_updates, tr("Check for updates at start"))
                                .on_hover_text(tr(
                                    "Asks GitHub for a newer release when 3BCAD starts; nothing else is sent",
                                ))
                                .changed();
                        }
                        Page::View => {
                            ui.horizontal(|ui| {
                                ui.label(tr("Display style"));
                                egui::ComboBox::from_id_salt("style")
                                    .selected_text(tr(s.display_style.name()))
                                    .show_ui(ui, |ui| {
                                        for d in DisplayStyle::ALL {
                                            changed |= ui.selectable_value(&mut s.display_style, d, tr(d.name())).changed();
                                        }
                                    });
                            });
                            changed |= ui.checkbox(&mut s.perspective, tr("Perspective (off: orthographic)")).changed();
                            changed |= ui.checkbox(&mut s.view_cube, tr("Show the view cube")).changed();
                            ui.weak(tr("These are how 3BCAD starts; the View toolbar changes them for now."));
                        }
                        Page::Mouse => {
                            ui.label(tr("Right mouse drag"));
                            changed |= ui.radio_value(&mut s.mouse_gestures, false, tr("Pans the view")).changed();
                            changed |= ui
                                .radio_value(&mut s.mouse_gestures, true, tr("Shows the mouse gesture wheel"))
                                .changed();
                            ui.weak(tr(
                                "Gestures: drag up for the Top view, down for Front, left for Isometric, right to zoom to fit. Shift + right drag still pans.",
                            ));
                            ui.weak(tr("Left drag orbits, the wheel zooms, Ctrl + middle drag pans."));
                        }
                        Page::Shortcuts => shortcuts(ui, &mut w, s, &mut changed),
                        Page::Printing => {
                            ui.horizontal(|ui| {
                                ui.label(tr("Send to Slicer opens"));
                                egui::ComboBox::from_id_salt("slicer")
                                    .selected_text(s.slicer.name())
                                    .show_ui(ui, |ui| {
                                        for sl in Slicer::ALL {
                                            changed |= ui.selectable_value(&mut s.slicer, sl, sl.name()).changed();
                                        }
                                    });
                            });
                            egui::Grid::new("slicers").num_columns(3).show(ui, |ui| {
                                for sl in Slicer::ALL {
                                    ui.label(sl.name());
                                    match s.slicer_paths[sl as usize].clone() {
                                        Some(p) => {
                                            ui.label(p.display().to_string());
                                            if ui.small_button(tr("Forget")).clicked() {
                                                s.slicer_paths[sl as usize] = None;
                                                changed = true;
                                            }
                                        }
                                        None => {
                                            ui.weak(tr("found where it usually installs"));
                                            ui.label("");
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                            ui.weak(tr("The printer, nozzle and print checks are on the 3D Print tab."));
                        }
                        Page::Performance => {
                            changed |= ui
                                .checkbox(&mut s.lod, tr("Draw large models simplified while the view moves"))
                                .changed();
                            changed |= ui.checkbox(&mut s.show_fps, tr("Show the frame rate")).changed();
                        }
                    }
                });
            });
            if let Some(m) = &w.message {
                ui.colored_label(crate::ui::theme::WARNING, m);
            }
            ui.separator();
            ui.horizontal(|ui| {
                let label = if w.confirm_reset { tr("Click again to reset everything") } else { tr("Reset to defaults") };
                if ui
                    .button(label)
                    .on_hover_text(tr("Every setting back to how 3BCAD comes; recent files are kept"))
                    .clicked()
                {
                    if w.confirm_reset {
                        app.files.settings.reset();
                        changed = true;
                        w.confirm_reset = false;
                    } else {
                        w.confirm_reset = true;
                    }
                }
                ui.weak(match crate::files::config_dir() {
                    Some(d) => trf("Saved in {}", &[&d.join("settings.txt").display()]),
                    None => tr("Not saved: no user folder").to_string(),
                });
            });
        });
    if changed {
        app.files.settings_changed = true;
        app.settings_applied = None;
    }
    if open {
        app.settings_window = Some(w);
    }
}

fn shortcuts(ui: &mut egui::Ui, w: &mut SettingsWindow, s: &mut crate::files::Settings, changed: &mut bool) {
    ui.weak(tr("Click a shortcut, then press the keys for it (Esc cancels)."));
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        egui::Grid::new("shortcuts").num_columns(3).striped(true).show(ui, |ui| {
            for c in Command::ALL {
                ui.label(tr(c.name()));
                let text = if w.capture == Some(c) {
                    tr("Press the keys…").to_string()
                } else {
                    commands::key_of(&s.keys, c).map_or_else(|| tr("none").to_string(), |k| commands::text(&k))
                };
                if ui.button(RichText::new(text).monospace()).clicked() {
                    w.capture = Some(c);
                    w.message = None;
                }
                ui.horizontal(|ui| {
                    if ui.small_button(tr("Clear")).clicked() {
                        s.keys.insert(c, None);
                        *changed = true;
                    }
                    if s.keys.contains_key(&c) && ui.small_button(tr("Default")).clicked() {
                        // The default may have been given to another command meanwhile.
                        match c.default_key().and_then(|k| commands::conflict(&s.keys, c, k)) {
                            Some(other) => w.message = Some(trf("{} has that shortcut now", &[&tr(other.name())])),
                            None => {
                                s.keys.remove(&c);
                                *changed = true;
                            }
                        }
                    }
                });
                ui.end_row();
            }
        });
    });
    if ui.button(tr("Reset all shortcuts")).clicked() {
        s.keys.clear();
        *changed = true;
    }
}
