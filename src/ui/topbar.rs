//! TopBar (`grid-area: top`, 40px)。
//!
//! 仕様: ブランド / レール toggle / breadcrumb / グローバル検索 / 右 cluster。
//! - 信号機（traffic lights）は egui ではネイティブ chrome に任せるので描かない
//! - テーマ toggle (sun) は warm-dark 固定なので非表示
//! - AI / 設定 / More はスタブボタン（クリックで toast を出すのみ）

use eframe::egui;

use crate::state::AppState;
use crate::theme;
use crate::ui::widgets::{self, icon_button};

/// `ctx.memory_mut(|m| m.request_focus(SEARCH_ID))` で ⌘K から focus する。
pub fn search_id() -> egui::Id {
    egui::Id::new("tanaterm.topbar.search")
}

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    egui::TopBottomPanel::top("topbar")
        .exact_height(theme::dims::TOPBAR)
        .frame(
            egui::Frame::none()
                .fill(theme::BG_0)
                .stroke(egui::Stroke::new(1.0, theme::LINE))
                .inner_margin(egui::Margin::symmetric(12.0, 0.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                brand(ui);
                rail_toggles(ui, state);
                breadcrumb(ui, state);
                center_and_right(ui, state);
            });
        });
}

fn brand(ui: &mut egui::Ui) {
    // 18x18 amber rounded-square + 棚 in dark.
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, theme::AMBER);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "棚",
        egui::FontId::proportional(12.0),
        egui::Color32::from_rgb(0x1a, 0x12, 0x0a),
    );
    ui.label(
        egui::RichText::new("tanaterm")
            .strong()
            .color(theme::FG_0)
            .size(13.0),
    );
    ui.label(
        egui::RichText::new(env!("CARGO_PKG_VERSION"))
            .color(theme::FG_2)
            .size(11.0),
    );
}

fn rail_toggles(ui: &mut egui::Ui, state: &mut AppState) {
    if icon_button(ui, "⟦", state.ui.rail_left_visible).clicked() {
        state.ui.rail_left_visible = !state.ui.rail_left_visible;
    }
    if icon_button(ui, "⟧", state.ui.rail_right_visible).clicked() {
        state.ui.rail_right_visible = !state.ui.rail_right_visible;
    }
}

fn breadcrumb(ui: &mut egui::Ui, state: &AppState) {
    let pwd = state.active().map(|s| s.pwd.clone()).unwrap_or_default();

    egui::Frame::none()
        .fill(egui::Color32::from_rgba_premultiplied(0, 0, 0, 46))
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(10.0, 4.0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let parts: Vec<&str> = pwd.split('/').filter(|s| !s.is_empty()).collect();
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    ui.label(egui::RichText::new("/").size(12.0).color(theme::FG_3));
                }
                let is_leaf = i == parts.len() - 1;
                let color = if is_leaf { theme::FG_0 } else { theme::FG_1 };
                ui.label(egui::RichText::new(*part).size(12.0).color(color));
            }
        });
}

fn center_and_right(ui: &mut egui::Ui, state: &mut AppState) {
    // 右クラスタを先に置きたいので、まず仮想的に右寄せ → 残った中央領域を search に。
    let right_cluster_w = 28.0 * 3.0 + 4.0 * 2.0;
    let avail = ui.available_width();
    let target_search_w = avail.min(520.0 + right_cluster_w);
    let search_w = (target_search_w - right_cluster_w).max(120.0);
    let pad = ((avail - search_w - right_cluster_w) / 2.0).max(0.0);

    ui.add_space(pad);
    search_input(ui, state, search_w);
    ui.add_space(pad);

    // 右クラスタ。
    if icon_button(ui, "AI", false).clicked() {
        let now = ui.input(|i| i.time);
        state.show_toast("AI ask is a stub for now", None, now);
    }
    if icon_button(ui, "⚙", false).clicked() {
        let now = ui.input(|i| i.time);
        state.show_toast("Preferences not wired yet", None, now);
    }
    if icon_button(ui, "⋯", false).clicked() {
        let now = ui.input(|i| i.time);
        state.show_toast("More menu is a stub", None, now);
    }
}

fn search_input(ui: &mut egui::Ui, state: &mut AppState, width: f32) {
    egui::Frame::none()
        .fill(theme::BG_1)
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(10.0, 4.0))
        .show(ui, |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🔍").size(12.0).color(theme::FG_2));
                let edit = egui::TextEdit::singleline(&mut state.ui.search_query)
                    .id(search_id())
                    .desired_width(width - 60.0)
                    .frame(false)
                    .text_color(theme::FG_0)
                    .hint_text(
                        egui::RichText::new("Search sessions, shelf, commands, history…")
                            .color(theme::FG_3)
                            .size(12.0),
                    );
                ui.add(edit);
                widgets::kbd(ui, "⌘K");
            });
        });
}
