//! StatusBar (`grid-area: status`, 24px)。
//!
//! 左から: ● connected (sage swatch) / 📁 pwd / N sessions
//! 右クラスタ: shell + OS / encoding + line ending / clock

use eframe::egui;

use crate::clock;
use crate::state::AppState;
use crate::theme;

pub fn show(ctx: &egui::Context, state: &AppState) {
    egui::TopBottomPanel::bottom("statusbar")
        .exact_height(theme::dims::STATUSBAR)
        .frame(
            egui::Frame::none()
                .fill(theme::BG_0)
                .stroke(egui::Stroke::new(1.0, theme::LINE))
                .inner_margin(egui::Margin::symmetric(12.0, 0.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                left_cluster(ui, state);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    right_cluster(ui, state);
                });
            });
        });
    // 24px / 60fps なら毎フレームで充分。
    ctx.request_repaint_after(std::time::Duration::from_millis(500));
}

fn left_cluster(ui: &mut egui::Ui, state: &AppState) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, theme::SAGE);
    ui.label(
        egui::RichText::new("connected")
            .color(theme::FG_1)
            .size(11.0),
    );
    if let Some(s) = state.active() {
        ui.label(egui::RichText::new("·").color(theme::FG_3).size(11.0));
        ui.label(
            egui::RichText::new(format!("📁 {}", s.pwd))
                .color(theme::FG_2)
                .size(11.0),
        );
    }
    ui.label(egui::RichText::new("·").color(theme::FG_3).size(11.0));
    ui.label(
        egui::RichText::new(format!("{} sessions", state.sessions.len()))
            .color(theme::FG_2)
            .size(11.0),
    );
}

fn right_cluster(ui: &mut egui::Ui, _state: &AppState) {
    ui.label(
        egui::RichText::new(clock::now_hhmmss())
            .color(theme::FG_1)
            .size(11.0),
    );
    ui.label(egui::RichText::new("·").color(theme::FG_3).size(11.0));
    ui.label(
        egui::RichText::new("UTF-8 · LF")
            .color(theme::FG_2)
            .size(11.0),
    );
    ui.label(egui::RichText::new("·").color(theme::FG_3).size(11.0));
    ui.label(
        egui::RichText::new("zsh · macOS")
            .color(theme::FG_2)
            .size(11.0),
    );
}
