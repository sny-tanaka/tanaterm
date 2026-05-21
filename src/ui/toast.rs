//! Toast (`grid-area: main` の右下、auto-dismiss)。
//!
//! `AppState::show_toast` で生成、`tick_toast` で TTL 経過分を消す。
//! ここでは「描画 + repaint 予約」だけを担当。

use eframe::egui;

use crate::state::AppState;
use crate::theme;

/// 中央パネル領域の右下にオーバーレイ描画する。
pub fn show(ctx: &egui::Context, state: &AppState, main_rect: egui::Rect) {
    let Some(toast) = &state.ui.toast else {
        return;
    };
    let now = ctx.input(|i| i.time);
    if toast.expired(now) {
        return;
    }
    // 寿命残りの間、再描画して dismiss を進める。下限 1 フレーム相当 (16ms) で
    // クランプし、残りが極小でも 0ms タイマを毎フレーム発行しないようにする。
    let remaining = (toast.ttl_secs - (now - toast.spawned_at)).max(0.0);
    ctx.request_repaint_after(std::time::Duration::from_millis(
        (remaining * 1000.0).clamp(16.0, 100.0) as u64,
    ));

    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("tanaterm.toast"));
    let painter = ctx.layer_painter(layer);

    // 仕様: bottom-right of main, 24 right padding, 86 bottom padding（input row の上）。
    let pad_right = 24.0;
    let pad_bottom = 86.0;
    let size = egui::vec2(280.0, 44.0);
    let top_left = egui::pos2(
        main_rect.right() - pad_right - size.x,
        main_rect.bottom() - pad_bottom - size.y,
    );
    let rect = egui::Rect::from_min_size(top_left, size);

    // 影（box-shadow 0 8 24 / 40% black の近似）。egui には blur がないので、
    // 6px ずらして少し拡張した黒矩形で近似する。
    let shadow = rect.translate(egui::vec2(0.0, 6.0)).expand(2.0);
    painter.rect_filled(
        shadow,
        8.0,
        egui::Color32::from_rgba_premultiplied(0, 0, 0, 80),
    );
    painter.rect_filled(rect, 8.0, theme::BG_3);
    painter.rect_stroke(rect, 8.0, egui::Stroke::new(1.0, theme::AMBER));

    // 左 amber pill アクセント。
    let pill = egui::Rect::from_min_size(
        rect.left_top() + egui::vec2(10.0, 10.0),
        egui::vec2(4.0, rect.height() - 20.0),
    );
    painter.rect_filled(pill, 2.0, theme::AMBER);

    // テキスト。
    painter.text(
        rect.left_top() + egui::vec2(22.0, 14.0),
        egui::Align2::LEFT_TOP,
        &toast.label,
        egui::FontId::proportional(12.0),
        theme::FG_0,
    );
    if let Some(detail) = &toast.detail {
        painter.text(
            rect.left_top() + egui::vec2(22.0, 28.0),
            egui::Align2::LEFT_TOP,
            detail,
            egui::FontId::proportional(10.5),
            theme::FG_2,
        );
    }
}
