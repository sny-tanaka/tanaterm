//! 共通 UI atoms。
//!
//! - `status_dot`: live/busy/err/idle の色付き丸。busy は CSS の pulse 1.2s に近い波形でアニメーション。
//! - `kbd`: ⌘ や ↵ を入れる小さなボックス。
//! - `section_header` / `section_header_plain`: rail セクションの大文字ヘッダ。
//! - `icon_button`: 28x28 の枠付きボタン。
//! - `icon_image`: keep.png / edit.png を tint して描く SVG 由来アイコン。
//! - `row`: rail 行の共通枠（hover / active で背景を切替・全面クリック）。
//!
//! いずれも warm-dark トークン (`theme::*`) を直接参照する。

use eframe::egui;

use crate::state::SessionStatus;
use crate::theme;

const BUSY_PULSE_PERIOD: f64 = 1.2;

/// CSS `.pulse 1.2s infinite` を近似した opacity 波形 (0.35..1.0)。
fn pulse_opacity(now: f64) -> f32 {
    let t = (now % BUSY_PULSE_PERIOD) / BUSY_PULSE_PERIOD;
    let cos = (t * std::f64::consts::TAU).cos();
    (0.35 + 0.325 * (1.0 + cos)) as f32
}

/// 1 つの status dot を描画する。
///
/// `size` は dot 本体の直径。glow ありの場合（live）は半径 +3px の二重円。
/// `busy` の場合は pulse でα値を変える＋次フレームへの repaint を仕込む。
pub fn status_dot(ui: &mut egui::Ui, status: SessionStatus, size: f32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(size + 6.0, size + 6.0), egui::Sense::hover());
    let center = rect.center();
    let painter = ui.painter();
    let now = ui.input(|i| i.time);

    let (fill, glow) = match status {
        SessionStatus::Live => (theme::SAGE, Some(theme::SAGE_SOFT)),
        SessionStatus::Busy => {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(40));
            let alpha = pulse_opacity(now);
            let mut c = theme::AMBER;
            c = egui::Color32::from_rgba_premultiplied(
                (c.r() as f32 * alpha) as u8,
                (c.g() as f32 * alpha) as u8,
                (c.b() as f32 * alpha) as u8,
                (255.0 * alpha) as u8,
            );
            (c, None)
        }
        SessionStatus::Err => (theme::RUST, None),
        SessionStatus::Idle => (theme::FG_3, None),
    };
    if let Some(soft) = glow {
        painter.circle_filled(center, size / 2.0 + 3.0, soft);
    }
    painter.circle_filled(center, size / 2.0, fill);
}

/// 複数色スパンを 1 行にまとめ、幅を超えたら "…" 切り詰めするラベル。
///
/// CSS の `white-space:nowrap;overflow:hidden;text-overflow:ellipsis` 相当。
/// rail 行の pwd/path/branch のように「panel 幅に収めたい行」に使う。これがないと
/// 折り返さない長いパスが行の desired-width を押し上げ、SidePanel が content 幅に
/// 合わせて広がってしまう（左右 rail とターミナルの間に余白が出る不具合になる）。
///
/// **前提**: `max_width` に `ui.available_width()` を使うため、必ず上位で幅が確定した
/// child Ui（= `row` の child_ui や `allocate_ui_with_layout` の中）から呼ぶこと。
/// `SidePanel` 直下や ScrollArea 内ループで直接呼ぶと、available_width の参照が
/// パネル幅成長のフィードバックに寄与しうる。
///
/// `spans` は `(テキスト, 色)` の並び。`size` は pt。
pub fn truncating_line(ui: &mut egui::Ui, spans: &[(&str, egui::Color32)], size: f32) {
    let mut job = egui::text::LayoutJob::default();
    for (text, color) in spans {
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(size),
                color: *color,
                ..Default::default()
            },
        );
    }
    job.wrap = egui::text::TextWrapping {
        // 極小ウィンドウ等で available_width が 0 以下になる場合に備えてガード。
        max_width: ui.available_width().max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.add(egui::Label::new(job));
}

/// 単色テキストを 1 行で "…" 切り詰めするラベル。
pub fn truncating_text(ui: &mut egui::Ui, text: &str, color: egui::Color32, size: f32) {
    truncating_line(ui, &[(text, color)], size);
}

/// kbd 風の小バッジ。`⌘K` / `↵` などに使う。
pub fn kbd(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let text = egui::RichText::new(label).size(10.0).color(theme::FG_1);
    egui::Frame::none()
        .fill(theme::BG_2)
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .rounding(4.0)
        .inner_margin(egui::Margin::symmetric(4.0, 1.0))
        .show(ui, |ui| ui.label(text))
        .response
}

/// rail のセクション見出し（"SESSIONS  5" 等）。
pub fn section_header(ui: &mut egui::Ui, title: &str, count: usize) {
    ui.horizontal(|ui| {
        ui.add_space(theme::spacing::PAD_X);
        ui.label(
            egui::RichText::new(title)
                .size(10.5)
                .strong()
                .color(theme::FG_2),
        );
        ui.label(
            egui::RichText::new(format!("{count}"))
                .size(10.5)
                .color(theme::FG_3),
        );
    });
    ui.add_space(2.0);
}

/// カウント数値なしのセクション見出し（"COMMANDS" のような親見出し用）。
pub fn section_header_plain(ui: &mut egui::Ui, title: &str) {
    ui.horizontal(|ui| {
        ui.add_space(theme::spacing::PAD_X);
        ui.label(
            egui::RichText::new(title)
                .size(10.5)
                .strong()
                .color(theme::FG_2),
        );
    });
    ui.add_space(2.0);
}

/// 28x28 のアイコンボタン（TopBar / section header 末尾用）。
pub fn icon_button(ui: &mut egui::Ui, glyph: &str, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    let (bg, fg) = if on {
        (theme::AMBER_SOFT, theme::AMBER)
    } else if resp.hovered() {
        (theme::BG_2, theme::FG_0)
    } else {
        (egui::Color32::TRANSPARENT, theme::FG_1)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, bg);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(13.0),
        fg,
    );
    resp
}

/// rail / 検索行などで使う「枠付き行」。hover / active で背景色を変える。
///
/// `width` は行の固定幅。**`ui.available_width()` を使ってはいけない**:
/// egui の SidePanel は content の min_rect 幅を消費するため、行が
/// available_width を読むと「行が available を読む→パネルが content 幅に広がる→
/// 次フレームの available が増える」という正のフィードバックでパネルが
/// 無制限に広がってしまう（rail とターミナルの間に余白が出る不具合の真因）。
/// 呼び出し側が ScrollArea の外で 1 度だけ確定した安定幅を渡すこと。
///
/// `id_source` は行のクリック判定 Id 種。行内の重複しない値（session id 等）を渡す。
///
/// クリック判定は**子ウィジェット描画後に行全体を `ui.interact` で被せる**ことで、
/// テキストやピルの上をクリックしても確実に行クリックになるようにする
/// （子ラベルが上に乗ると行下の click sense に当たらない不具合の対策）。
/// hover 中は手のひらカーソルにする。行内に独自ボタン（close X / pin）がある場合は
/// 呼び出し側が `Response::interact_pointer_pos()` と各ボタンの rect で振り分けること。
///
/// `add_contents` には行内描画用の child Ui と `row_hovered` フラグが渡る。
pub fn row(
    ui: &mut egui::Ui,
    state: RowState,
    id_source: impl std::hash::Hash,
    width: f32,
    height: f32,
    add_contents: impl FnOnce(&mut egui::Ui, bool),
) -> egui::Response {
    // まず領域だけ確保（クリックは後段の interact で取る）。
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());

    // hover 判定は幾何的な `rect_contains_pointer`。子ウィジェットの occlusion で
    // 解除されないので、テキスト上にカーソルを移してもホバーが維持される。
    let row_hovered = ui.rect_contains_pointer(rect);
    let bg = match state {
        RowState::Active => theme::BG_3,
        RowState::Default if row_hovered => theme::BG_2,
        RowState::Default => egui::Color32::TRANSPARENT,
    };
    ui.painter().rect_filled(rect, 6.0, bg);

    // active セッションの左 amber アクセント (`.sess.active::before`)。
    if state == RowState::Active {
        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.left() - 6.0, rect.top() + 6.0),
            egui::pos2(rect.left() - 4.0, rect.bottom() - 6.0),
        );
        ui.painter().rect_filled(bar, 1.0, theme::AMBER);
    }

    let inner = rect.shrink2(egui::vec2(8.0, 0.0));
    let mut child = ui.child_ui(
        inner,
        egui::Layout::left_to_right(egui::Align::Center),
        None,
    );
    add_contents(&mut child, row_hovered);

    // 子描画後に行全体へクリック interact を被せる（最前面なので確実に当たる）。
    let resp = ui.interact(rect, ui.id().with(("row", id_source)), egui::Sense::click());
    if row_hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// アイコン種別。`assets/icon/*.png`（白塗り）を tint して使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// keep.png（ピン）。
    Pin,
    /// edit.png（鉛筆）。
    Edit,
}

impl Icon {
    fn name(self) -> &'static str {
        match self {
            Icon::Pin => "pin",
            Icon::Edit => "edit",
        }
    }

    /// テクスチャ未登録時のフォールバックグリフ。
    fn fallback_glyph(self) -> &'static str {
        match self {
            Icon::Pin => "⌖",
            Icon::Edit => "✎",
        }
    }
}

fn icon_texture_id(icon: Icon) -> egui::Id {
    egui::Id::new(("tanaterm.icon", icon.name()))
}

/// アイコン用 PNG（白塗り）を読み込んでテクスチャを `ctx.data` に登録する。
/// app 起動時に 1 度だけ呼ぶ。失敗時は何もしない（描画側がグリフでフォールバック）。
pub fn register_icons(ctx: &egui::Context) {
    register_one(ctx, Icon::Pin, include_bytes!("../../assets/icon/keep.png"));
    register_one(
        ctx,
        Icon::Edit,
        include_bytes!("../../assets/icon/edit.png"),
    );
}

fn register_one(ctx: &egui::Context, icon: Icon, png: &[u8]) {
    let Ok(data) = eframe::icon_data::from_png_bytes(png) else {
        return;
    };
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [data.width as usize, data.height as usize],
        &data.rgba,
    );
    let handle = ctx.load_texture(icon.name(), image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(icon_texture_id(icon), handle));
}

/// アイコンを `color` で tint して描画し、その rect を返す（クリック判定は呼び出し側）。
/// テクスチャ未登録時はグリフでフォールバックする。
pub fn icon_image(ui: &mut egui::Ui, icon: Icon, color: egui::Color32, size: f32) -> egui::Rect {
    let handle: Option<egui::TextureHandle> = ui.ctx().data(|d| d.get_temp(icon_texture_id(icon)));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    match handle {
        Some(h) => {
            egui::Image::new(&h).tint(color).paint_at(ui, rect);
        }
        None => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                icon.fallback_glyph(),
                egui::FontId::proportional(size),
                color,
            );
        }
    }
    rect
}

/// `row` のスタイル指定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    Default,
    Active,
}
