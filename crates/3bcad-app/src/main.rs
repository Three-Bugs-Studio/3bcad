// Release builds are a GUI app on Windows: no console window behind it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assembly;
mod commands;
mod drawing;
mod files;
mod i18n;
mod logfile;
mod print;
mod ui;
mod update;

use eframe::egui;

fn main() -> eframe::Result {
    let crashed = logfile::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("3BCAD {}", app::VERSION))
            .with_inner_size([1400.0, 900.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon-256.png"))
                    .expect("the bundled icon is a PNG"),
            ),
        renderer: eframe::Renderer::Wgpu,
        depth_buffer: app::DEPTH_BITS,
        multisampling: app::MSAA as u16,
        ..Default::default()
    };
    eframe::run_native(
        "3BCAD",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, crashed)?))),
    )
}
