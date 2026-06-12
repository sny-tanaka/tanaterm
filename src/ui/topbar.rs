//! TopBar (`grid-area: top`, 40px)。
//!
//! 仕様: ブランド / レール toggle / breadcrumb / グローバル検索。
//! - 信号機（traffic lights）は egui ではネイティブ chrome に任せるので描かない
//! - テーマ toggle (sun) は warm-dark 固定なので非表示
//! - AI / 設定 / More のスタブボタンは未配線のため削除済み（必要時に復活）

use eframe::egui;

use crate::state::AppState;
use crate::theme;
use crate::ui::widgets::{self, sidebar_toggle_button, SidebarSide};

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
    // 18x18 amber 角丸 + 棚（shelf）を表す横線 2 本。漢字 "棚" は egui デフォルト
    // フォントに無く豆腐になるため、painter で shelf 形を直接描く。
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    widgets::paint_brand_mark(
        ui.painter(),
        rect,
        theme::AMBER,
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
    if sidebar_toggle_button(ui, SidebarSide::Left, state.ui.rail_left_visible).clicked() {
        state.ui.rail_left_visible = !state.ui.rail_left_visible;
    }
    if sidebar_toggle_button(ui, SidebarSide::Right, state.ui.rail_right_visible).clicked() {
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
    // search を残り領域の中央に配置する。
    //
    // この時点の `avail` は brand / rail_toggles / breadcrumb 消費後の残余幅。
    // 配置順: add_space(pad) [sp] search [sp] add_space(pad)
    //   avail = 2*pad + search_w + 2*spacing
    //
    // 幅が狭く pad=0 に丸められる場合でも add_space 前後の item_spacing は残るので、
    // search は spacing 分の余白を両側に持って中央に収まる（左寄せにはならない）。
    let spacing = ui.spacing().item_spacing.x;
    let avail = ui.available_width();
    let search_w = (avail - 2.0 * spacing).clamp(120.0, 520.0);
    let pad = ((avail - search_w - 2.0 * spacing) / 2.0).max(0.0);

    ui.add_space(pad);
    search_input(ui, state, search_w);
    ui.add_space(pad);
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
                        egui::RichText::new("Search sessions, shelf, commands…")
                            .color(theme::FG_3)
                            .size(12.0),
                    );
                ui.add(edit);
                widgets::kbd(ui, "⌘K");
            });
        });
}
