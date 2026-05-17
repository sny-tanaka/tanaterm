#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod pty;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let config = config::Config::load();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 600.0])
            .with_min_inner_size([320.0, 200.0])
            .with_title("tanaterm"),
        ..Default::default()
    };

    eframe::run_native(
        "tanaterm",
        options,
        Box::new(move |cc| Ok(Box::new(app::TanaTermApp::new(cc, config)))),
    )
}
