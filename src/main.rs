#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod clock;
mod config;
mod pty;
mod shell_integration;
mod state;
mod term;
mod theme;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let config = config::Config::load();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([960.0, 600.0])
        .with_min_inner_size([320.0, 200.0])
        .with_title("tanaterm");

    // Dock/window icon when launched via `cargo run` (the .app bundle uses
    // Contents/Resources/tanaterm.icns instead; see scripts/bundle-macos.sh).
    match eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(err) => eprintln!("tanaterm: failed to load app icon: {err}"),
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "tanaterm",
        options,
        Box::new(move |cc| Ok(Box::new(app::TanaTermApp::new(cc, config)))),
    )
}
