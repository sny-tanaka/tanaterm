//! warm-dark テーマトークン。
//!
//! ハンドオフ (`tmp/design_handoff_tanaterm/tanaterm.css`) の CSS 変数を
//! Rust 側でそのまま使える `Color32` 定数として持つ。
//!
//! 計画 (`docs/redesign-plan.md`) の Decision Log に従い、
//! - テーマは warm-dark 単一固定（paper テーマや accent 切替は持たない）
//! - density は regular 単一固定
//! - フォントは egui デフォルトのまま（フォント定義はここでは持たない）

use eframe::egui::{self, Color32};

/// 背景: rails / topbar / statusbar など chrome 部 (`--bg-0`).
pub const BG_0: Color32 = Color32::from_rgb(0x14, 0x11, 0x0d);
/// 背景: ターミナルキャンバス / input bg (`--bg-1`).
pub const BG_1: Color32 = Color32::from_rgb(0x1a, 0x16, 0x12);
/// 背景: hover (`--bg-2`).
pub const BG_2: Color32 = Color32::from_rgb(0x22, 0x1d, 0x17);
/// 背景: active カード / tab active (`--bg-3`).
pub const BG_3: Color32 = Color32::from_rgb(0x2a, 0x23, 0x1b);

/// デフォルト境界線 (`--line`).
pub const LINE: Color32 = Color32::from_rgb(0x2e, 0x26, 0x20);
/// 強めの境界線 / input border (`--line-2`).
pub const LINE_2: Color32 = Color32::from_rgb(0x3a, 0x30, 0x25);

/// 主要テキスト (`--fg-0`).
pub const FG_0: Color32 = Color32::from_rgb(0xf5, 0xed, 0xe0);
/// 副次テキスト (`--fg-1`).
pub const FG_1: Color32 = Color32::from_rgb(0xb9, 0xa9, 0x8c);
/// muted (`--fg-2`).
pub const FG_2: Color32 = Color32::from_rgb(0x7a, 0x6a, 0x51);
/// 最暗 (`--fg-3`).
pub const FG_3: Color32 = Color32::from_rgb(0x57, 0x4a, 0x37);

/// 主アクセント (`--amber`).
pub const AMBER: Color32 = Color32::from_rgb(0xe8, 0xa6, 0x52);
/// amber 16% (`--amber-soft`). 半透明なので背景に重ねて使う。
///
/// CSS: `rgba(232, 166, 82, .16)`. premultiplied で
/// (232*.16, 166*.16, 82*.16, .16*255) = (37, 27, 13, 41).
pub const AMBER_SOFT: Color32 = Color32::from_rgba_premultiplied(37, 27, 13, 41);
/// git branch / success (`--sage`).
pub const SAGE: Color32 = Color32::from_rgb(0x87, 0xa8, 0x78);
/// sage 14% (`--sage-soft`). live dot のグロー用。
///
/// CSS: `rgba(135, 168, 120, .14)`. premultiplied で
/// (135*.14, 168*.14, 120*.14, .14*255) = (19, 24, 17, 36).
#[allow(dead_code)] // Phase 1: session live dot のグローで使う。
pub const SAGE_SOFT: Color32 = Color32::from_rgba_premultiplied(19, 24, 17, 36);
/// エラー (`--rust`).
#[allow(dead_code)] // Phase 1: err status dot / exit 1 バッジで使う。
pub const RUST: Color32 = Color32::from_rgb(0xd9, 0x7b, 0x6c);
/// 装飾 (`--plum`).
#[allow(dead_code)] // Phase 1: output color spans (.m) で使う。
pub const PLUM: Color32 = Color32::from_rgb(0xb4, 0x8e, 0xad);
/// URL / runtime version (`--azure`).
pub const AZURE: Color32 = Color32::from_rgb(0x7a, 0xb0, 0xc8);

/// 固定レイアウト寸法。CSS の `--rail-l` 等と一対一対応。
pub mod dims {
    /// 左 rail の幅。
    pub const RAIL_L: f32 = 248.0;
    /// 右 rail の幅。
    pub const RAIL_R: f32 = 280.0;
    /// TopBar の高さ。
    pub const TOPBAR: f32 = 40.0;
    /// StatusBar の高さ。
    pub const STATUSBAR: f32 = 24.0;
}

/// density = regular の spacing (`[data-density="regular"]`).
pub mod spacing {
    /// 行高。
    pub const ROW_H: f32 = 26.0;
    /// 要素間 gap。
    pub const GAP: f32 = 8.0;
    /// 水平方向 padding。
    pub const PAD_X: f32 = 12.0;
    /// 垂直方向 padding。
    pub const PAD_Y: f32 = 8.0;
}

/// egui の `Visuals` / `Style` を warm-dark に揃える。
///
/// `CreationContext::new()` 時点で 1 度だけ呼べばよい。
/// パネル個別の `Frame::fill` などはこの設定に依らないので、
/// 配色は各 UI 側で `theme::*` 定数を直接参照する前提。
pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    // ウィンドウ全体の地色。CentralPanel の既定 fill にも反映される。
    visuals.panel_fill = BG_1;
    visuals.window_fill = BG_0;
    visuals.extreme_bg_color = BG_1;
    visuals.faint_bg_color = BG_2;
    visuals.code_bg_color = BG_2;

    // テキスト系。
    visuals.override_text_color = Some(FG_0);
    visuals.hyperlink_color = AZURE;
    visuals.selection.bg_fill = AMBER_SOFT;
    visuals.selection.stroke = egui::Stroke::new(1.0, AMBER);

    // ボーダー。
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, LINE);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, LINE);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, LINE_2);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, AMBER);

    // hover / active の塗り。
    visuals.widgets.hovered.weak_bg_fill = BG_2;
    visuals.widgets.active.weak_bg_fill = BG_3;

    ctx.set_visuals(visuals);

    // 角丸・spacing は density=regular に固定。
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(spacing::GAP, spacing::GAP);
    style.spacing.button_padding = egui::vec2(spacing::PAD_X, spacing::PAD_Y);
    style.spacing.interact_size.y = spacing::ROW_H;
    ctx.set_style(style);
}
