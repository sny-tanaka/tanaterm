//! 共通 UI atoms。
//!
//! - `status_dot`: live/busy/err/idle の色付き丸。busy は CSS の pulse 1.2s に近い波形でアニメーション。
//! - `kbd`: ⌘ や ↵ を入れる小さなボックス。
//! - `section_header` / `section_header_plain`: rail セクションの大文字ヘッダ。
//! - `icon_button`: 28x28 の枠付きボタン。
//! - `sidebar_toggle_button`: TopBar のレール toggle 用、角丸枠＋縦線を painter で直接描く 28x28 ボタン。
//! - `paint_brand_mark`: 棚（shelf）ブランドマーク。amber 角丸＋濃い横線 2 本を painter で描く。
//! - `paint_return_arrow`: ↵ Enter アイコン。縦線＋横線＋矢じりを painter で描く。
//! - `kbd_return_button`: 入力欄右の送信ボタン。`kbd("↵")` の painter 版を click 化したもの。
//! - `section_divider`: セクション見出し直下の「棚板ライン」。1px LINE_2 ＋ 上 1px に amber 5% 反射。
//! - `row_hairline`: rail アイテム間の薄い区切り線（1px LINE、左右 6px インセット）。
//! - `icon_image`: keep.png / edit.png を tint して描く SVG 由来アイコン。
//! - `row`: rail 行の共通枠（hover / active で背景を切替・全面クリック）。
//!
//! いずれも warm-dark トークン (`theme::*`) を直接参照する。

use eframe::egui;

use crate::state::SessionStatus;
use crate::theme;

pub const BUSY_PULSE_PERIOD: f64 = 1.2;

/// CSS `.pulse 1.2s infinite` を近似した opacity 波形 (0.35..1.0)。
pub fn pulse_opacity(now: f64) -> f32 {
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
            // egui に gamma 補正込みで multiply してもらう。手動で `from_rgba_premultiplied`
            // を組むと RGB と alpha の比率がずれる（特に色値が 255 から離れている時）。
            (theme::AMBER.gamma_multiply(pulse_opacity(now)), None)
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

/// kbd 風の小バッジ。`⌘K` などに使う。
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

/// `↵` の代わりに return-arrow を painter で描く kbd 風の送信ボタン。
/// egui デフォルトフォントが U+21B5 を持たないため、テキストの "↵" だと豆腐になる。
///
/// 入力欄右に置く submit ボタンとして使う。見た目はキーキャップ風で、hover 時に
/// 矢印を amber へ切り替えて「押せること」を示す。クリック判定は Frame の外側
/// rect 全体に被せる。
pub fn kbd_return_button(ui: &mut egui::Ui) -> egui::Response {
    // 先に rect を予約し、その上から Frame と interact を被せる。
    // Frame::show().response の rect は Frame の外側に一致するので、その rect を
    // ui.interact に渡せば矢印（painter 描画）の上をクリックしても確実にヒットする。
    let frame_resp = egui::Frame::none()
        .fill(theme::BG_2)
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .rounding(4.0)
        .inner_margin(egui::Margin::symmetric(4.0, 1.0))
        .show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(11.0, 11.0), egui::Sense::hover());
            // hover 時は矢印を amber にする（押せることの視認性）。
            // 実際の hovered 判定は外側 rect で行うため、ここでは描画だけ済ませて
            // 1 フレーム遅延で色を切り替える（PointingHand と同タイミングになる）。
            let hovered = ui.rect_contains_pointer(rect);
            let color = if hovered { theme::AMBER } else { theme::FG_1 };
            paint_return_arrow(ui.painter(), rect, color);
        })
        .response;
    let resp = ui.interact(
        frame_resp.rect,
        ui.id().with("kbd_return_button"),
        egui::Sense::click(),
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// inline ラベル（"open ↵" 等）の代わりに使う小さな return-arrow。
/// 11x11 をその場に allocate して painter で描く。色は呼び出し側で指定。
pub fn return_arrow_inline(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(11.0, 11.0), egui::Sense::hover());
    paint_return_arrow(ui.painter(), rect, color);
}

/// `↵` Enter シンボルを `rect` 内に painter で描く。
///
/// 形状: 右上から下に短い縦線 → 中央水平に左へ伸びる横線 → 左端に矢じり。
/// stroke 太さは rect 高さに比例して決め、kbd サイズ〜empty placeholder サイズまでスケールさせる。
pub fn paint_return_arrow(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let stroke_w = (rect.height() * 0.14).clamp(1.0, 3.0);
    let stroke = egui::Stroke::new(stroke_w, color);
    let top = rect.top() + rect.height() * 0.18;
    let mid_y = rect.center().y + rect.height() * 0.08;
    let right_x = rect.right() - rect.width() * 0.10;
    let left_x = rect.left() + rect.width() * 0.18;

    painter.line_segment(
        [
            egui::pos2(right_x, top),
            egui::pos2(right_x, mid_y + stroke_w * 0.5),
        ],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(right_x, mid_y), egui::pos2(left_x, mid_y)],
        stroke,
    );
    let head = rect.width() * 0.18;
    painter.line_segment(
        [
            egui::pos2(left_x, mid_y),
            egui::pos2(left_x + head, mid_y - head),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(left_x, mid_y),
            egui::pos2(left_x + head, mid_y + head),
        ],
        stroke,
    );
}

/// 棚（shelf）ブランドマークを `rect` 内に painter で描く。
///
/// 形状: `fill` で塗った角丸正方形に `line` 色の水平線 2 本（棚の段を表現）。
/// 漢字「棚」が egui デフォルトフォントに無い対策で、12px の TopBar ブランドから
/// 56px の empty placeholder まで同じ形状でスケールする。
pub fn paint_brand_mark(
    painter: &egui::Painter,
    rect: egui::Rect,
    fill: egui::Color32,
    line: egui::Color32,
) {
    painter.rect_filled(rect, rect.width() * 0.22, fill);
    let inset = rect.width() * 0.22;
    let stroke_w = (rect.width() * 0.085).max(1.2);
    let stroke = egui::Stroke::new(stroke_w, line);
    let y_top = rect.top() + rect.height() * 0.38;
    let y_bot = rect.top() + rect.height() * 0.66;
    painter.line_segment(
        [
            egui::pos2(rect.left() + inset, y_top),
            egui::pos2(rect.right() - inset, y_top),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(rect.left() + inset, y_bot),
            egui::pos2(rect.right() - inset, y_bot),
        ],
        stroke,
    );
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
    section_divider(ui);
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
    section_divider(ui);
}

/// セクション見出し直下の「棚板ライン」。
///
/// 1px LINE_2 罫線 + その上 1px に amber 5% 反射を重ねる（"光が棚板の縁に
/// 当たる" 表現）。`docs/redesign.md` §2 の棚メタファー強化用。
pub fn section_divider(ui: &mut egui::Ui) {
    let avail = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(avail, 2.0), egui::Sense::hover());
    let painter = ui.painter();
    let top = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 1.0));
    let bottom = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.bottom() - 1.0),
        egui::vec2(rect.width(), 1.0),
    );
    painter.rect_filled(top, 0.0, theme::AMBER_GLOW);
    painter.rect_filled(bottom, 0.0, theme::LINE_2);
}

/// rail アイテム間の薄い区切り線（1px LINE、左右 6px インセット）。
///
/// 縦リスト中、各 row の後に呼び出すとアイテム間 hairline になる。最終行の
/// 後にも 1 本入るのは仕様（外枠の罫線と合流して悪目立ちはしない）。
pub fn row_hairline(ui: &mut egui::Ui) {
    let avail = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(avail, 1.0), egui::Sense::hover());
    let inset = 6.0;
    let line = egui::Rect::from_min_size(
        egui::pos2(rect.left() + inset, rect.top()),
        egui::vec2((rect.width() - inset * 2.0).max(0.0), 1.0),
    );
    ui.painter().rect_filled(line, 0.0, theme::LINE);
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

/// サイドバー toggle アイコンの向き。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSide {
    Left,
    Right,
}

/// 28x28 のサイドバー toggle ボタン。デザインの `Icon.sidebarL` / `sidebarR`
/// （角丸矩形＋片側寄りの縦線）を painter で直接描く。
/// Unicode グリフは egui デフォルトフォントに無いため四角化するのを避ける目的。
pub fn sidebar_toggle_button(ui: &mut egui::Ui, side: SidebarSide, on: bool) -> egui::Response {
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

    // デザイン viewBox は 16x16。中の枠は (1.5,2.5)-(14.5,13.5) = 13x11、
    // 縦線は sidebarL なら x=5.5、sidebarR なら x=10.5（viewBox 中心 8 からの ±2.5）。
    // 14px 表示にスケール（14/16 = 0.875）。
    let s = 14.0 / 16.0;
    let center = rect.center();
    let icon_rect = egui::Rect::from_center_size(center, egui::vec2(13.0 * s, 11.0 * s));
    let stroke = egui::Stroke::new(1.4, fg);
    painter.rect_stroke(icon_rect, 1.5 * s, stroke);

    let line_dx = match side {
        SidebarSide::Left => -2.5 * s,
        SidebarSide::Right => 2.5 * s,
    };
    let line_x = center.x + line_dx;
    painter.line_segment(
        [
            egui::pos2(line_x, icon_rect.top()),
            egui::pos2(line_x, icon_rect.bottom()),
        ],
        stroke,
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
    // docs/redesign.md §3: 幅 2→3px、右側だけ角丸、背後に amber 5% glow。
    if state == RowState::Active {
        let bar_left = rect.left() - 6.0;
        let bar_right = bar_left + 3.0;
        let top = rect.top() + 6.0;
        let bottom = rect.bottom() - 6.0;
        let bar =
            egui::Rect::from_min_max(egui::pos2(bar_left, top), egui::pos2(bar_right, bottom));
        // glow（背後に同心の amber 5% 矩形を 1 段だけ広げて敷く）。
        // CSS の box-shadow: 0 0 8px は egui で完全再現できないため、
        // 矩形 1 段の近似（縦 +4 / 横 +6）で「滲み」を表現する。
        let glow = bar.expand2(egui::vec2(6.0, 4.0));
        ui.painter().rect_filled(glow, 4.0, theme::AMBER_GLOW);
        // 右側だけ角丸（`border-radius: 0 2px 2px 0`）。
        let rounding = egui::Rounding {
            nw: 0.0,
            ne: 2.0,
            sw: 0.0,
            se: 2.0,
        };
        ui.painter().rect_filled(bar, rounding, theme::AMBER);
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

/// セクション見出し下のテキストボタン（"+ new session" / "★ add current folder" /
/// "+ add command" 共通）。クリックで true。
///
/// 左右に余白を取り、`kbd_hint` の有無に関わらず同じ幅（パネル幅 - 左右余白）に固定する。
const TEXT_BUTTON_PAD: f32 = 10.0;
const TEXT_BUTTON_INNER_X: f32 = 8.0; // Frame inner_margin の水平片側

pub fn text_button(ui: &mut egui::Ui, label: &str, kbd_hint: Option<&str>) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.add_space(TEXT_BUTTON_PAD);
        let btn_w = (ui.available_width() - TEXT_BUTTON_PAD).max(40.0);
        let resp = egui::Frame::none()
            .stroke(egui::Stroke::new(1.0, theme::LINE_2))
            .rounding(6.0)
            .inner_margin(egui::Margin::symmetric(TEXT_BUTTON_INNER_X, 6.0))
            .show(ui, |ui| {
                ui.set_width(btn_w - TEXT_BUTTON_INNER_X * 2.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(label).size(12.0).color(theme::FG_1));
                    if let Some(k) = kbd_hint {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            kbd(ui, k);
                        });
                    }
                });
            })
            .response
            .interact(egui::Sense::click());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        clicked = resp.clicked();
    });
    clicked
}
