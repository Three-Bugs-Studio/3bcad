//! The update window: what a newer release brings, installing it, and restarting into it.

use crate::i18n::{tr, trf};
use std::time::Duration;

use eframe::egui::{self, Color32};

use crate::app::{App, VERSION};
use crate::update::{can_install, State};

const ERROR: Color32 = Color32::from_rgb(190, 50, 30);

enum Act {
    Install,
    Skip(String),
    Restart,
    Close,
}

/// Takes in background update results and shows the window when there is something to say.
pub fn window(app: &mut App, ctx: &egui::Context) {
    if let Some(msg) = app.update.poll(&app.files.settings.skip_update) {
        app.status = msg;
    }
    if app.update.busy() {
        ctx.request_repaint_after(Duration::from_millis(200));
    }
    if !app.update.open {
        return;
    }
    let mut open = true;
    let mut act = None;
    egui::Window::new(tr("Update"))
        .open(&mut open)
        .collapsible(false)
        .default_width(460.0)
        .show(ctx, |ui| match &app.update.state {
            State::Available(r) | State::Failed(r, _) => {
                ui.heading(trf("3BCAD {} is available", &[&r.version]));
                ui.label(trf("You have {}.", &[&VERSION]));
                if let State::Failed(_, why) = &app.update.state {
                    ui.colored_label(ERROR, trf("The update did not install: {}", &[&why]));
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .show(ui, |ui| ui.label(&r.notes));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_install(r), egui::Button::new(tr("Update now")))
                        .on_hover_text(tr(
                            "Download it, check it against the release's checksums and install it",
                        ))
                        .on_disabled_hover_text(tr("Download it from the release page (not possible from this build)"))
                        .clicked()
                    {
                        act = Some(Act::Install);
                    }
                    ui.hyperlink_to(tr("Release page"), &r.page);
                    if ui.button(tr("Skip this version")).clicked() {
                        act = Some(Act::Skip(r.version.clone()));
                    }
                    if ui.button(tr("Later")).clicked() {
                        act = Some(Act::Close);
                    }
                });
            }
            State::Installing(r) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(trf("Downloading and checking 3BCAD {}…", &[&r.version]));
                });
            }
            State::Installed(v) => {
                ui.heading(trf("3BCAD {} is installed", &[&v]));
                ui.label(tr("It starts the next time 3BCAD opens."));
                ui.horizontal(|ui| {
                    if ui.button(tr("Restart now")).clicked() {
                        act = Some(Act::Restart);
                    }
                    if ui.button(tr("Later")).clicked() {
                        act = Some(Act::Close);
                    }
                });
            }
            State::Idle | State::Checking => act = Some(Act::Close),
        });
    if !open {
        app.update.open = false;
    }
    match act {
        Some(Act::Install) => app.update.install(),
        Some(Act::Skip(v)) => {
            app.files.settings.skip_update = v;
            app.files.settings_changed = true;
            app.update.state = State::Idle;
            app.update.open = false;
        }
        Some(Act::Restart) => {
            app.update.open = false;
            app.restart(ctx);
        }
        Some(Act::Close) => app.update.open = false,
        None => {}
    }
}
