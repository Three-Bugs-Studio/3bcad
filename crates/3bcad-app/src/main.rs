// Release builds are a GUI app on Windows: no console window behind it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("3BCAD {}", app::VERSION))
            .with_inner_size([1400.0, 900.0]),
        renderer: eframe::Renderer::Wgpu,
        depth_buffer: app::DEPTH_BITS,
        multisampling: app::MSAA as u16,
        ..Default::default()
    };
    eframe::run_native("3BCAD", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)?))))
}
