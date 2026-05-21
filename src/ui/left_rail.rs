//! 左 rail (sessions + shelf)。
//!
//! - SESSIONS: 新規ボタン（先頭・左右 padding 付き）/ status dot / pin / 2-line meta /
//!   hover で close × / dbl-click で inline rename。リサイズ可能な上部パネル
//! - SHELF: 先頭の edit アイコンでラベル編集 / hover で "open ↵" テキスト /
//!   行クリックで新規セッション+toast。見出し下の "+ add current folder" で現在 pwd を登録。
//!   残り高さを埋める
//!
//! SESSIONS の高さはユーザがドラッグで変更でき、eframe persistence で永続化される。

use eframe::egui;

use crate::state::AppState;
use crate::theme;
use crate::ui::widgets::{self, kbd, row, status_dot, Icon, RowState};

/// inline rename の `TextEdit` 用固定 Id。`Esc` 検出と確定のため。
pub fn rename_id() -> egui::Id {
    egui::Id::new("tanaterm.rename")
}

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    egui::SidePanel::left("rail_l")
        .exact_width(theme::dims::RAIL_L)
        .resizable(false)
        .frame(
            egui::Frame::none()
                .fill(theme::BG_0)
                .stroke(egui::Stroke::new(1.0, theme::LINE)),
        )
        .show(ctx, |ui| {
            // SESSIONS: リサイズ可能な上部パネル。高さは egui が永続化する。
            egui::TopBottomPanel::top("rail_l_sessions")
                .resizable(true)
                .default_height(300.0)
                .frame(egui::Frame::none())
                .show_inside(ui, |ui| {
                    ui.add_space(10.0);
                    widgets::section_header(ui, "SESSIONS", state.sessions.len());
                    if text_button(ui, "+ new session", Some("⌘T")) {
                        state.new_session("new session", "~/");
                        let now = ui.input(|i| i.time);
                        state.show_toast("New session", None, now);
                    }
                    ui.add_space(4.0);
                    sessions_list(ui, state);
                });

            // SHELF: 残り高さを埋める。
            egui::CentralPanel::default()
                .frame(egui::Frame::none())
                .show_inside(ui, |ui| {
                    ui.add_space(10.0);
                    widgets::section_header(ui, "SHELF", state.shelf.len());
                    if text_button(ui, "★ add current folder", None) {
                        let now = ui.input(|i| i.time);
                        state.add_active_to_shelf(now);
                    }
                    ui.add_space(4.0);
                    shelf_list(ui, state);
                });
        });
}

/// SESSIONS / SHELF 見出し下のテキストボタン。クリックで true。
/// 左右に 10px の余白を取り、kbd ヒストの有無に関わらず同じ幅（パネル幅 - 左右余白）に固定する。
const TEXT_BUTTON_PAD: f32 = 10.0;
const TEXT_BUTTON_INNER_X: f32 = 8.0; // Frame inner_margin の水平片側

fn text_button(ui: &mut egui::Ui, label: &str, kbd_hint: Option<&str>) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.add_space(TEXT_BUTTON_PAD);
        // ボタン外形の幅。右にも同じ余白を残す。
        let btn_w = (ui.available_width() - TEXT_BUTTON_PAD).max(40.0);
        let resp = egui::Frame::none()
            .stroke(egui::Stroke::new(1.0, theme::LINE_2))
            .rounding(6.0)
            .inner_margin(egui::Margin::symmetric(TEXT_BUTTON_INNER_X, 6.0))
            .show(ui, |ui| {
                // Frame 内容幅を明示固定して、kbd ヒントの有無で幅が変わらないようにする。
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

fn sessions_list(ui: &mut egui::Ui, state: &mut AppState) {
    let session_ids: Vec<String> = state.sessions.iter().map(|s| s.id.clone()).collect();
    let active_id = state.ui.active_session_id.clone();

    egui::ScrollArea::vertical()
        .id_source("rail_l_sessions_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(theme::spacing::PAD_Y);
            let row_w = (ui.available_width() - 12.0).max(80.0);
            for id in session_ids {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    session_row(ui, state, &id, active_id.as_deref() == Some(&id), row_w);
                });
                ui.add_space(2.0);
            }
        });
}

fn session_row(ui: &mut egui::Ui, state: &mut AppState, id: &str, active: bool, row_w: f32) {
    let (name, pwd, status, pinned, remote) = {
        let Some(s) = state.sessions.iter().find(|s| s.id == id) else {
            return;
        };
        (
            s.name.clone(),
            s.pwd.clone(),
            s.status,
            s.pinned,
            s.remote.clone(),
        )
    };
    let renaming = state.is_renaming_session(id);
    let row_state = if active { RowState::Active } else { RowState::Default };

    let mut commit_now = false;
    let mut close_rect = egui::Rect::NOTHING;

    let resp = row(ui, row_state, ("session", id), row_w, 40.0, |ui, hovered| {
        status_dot(ui, status, 8.0);
        ui.add_space(4.0);

        let close_w = 22.0;
        let meta_w = (ui.available_width() - close_w).max(40.0);
        ui.allocate_ui_with_layout(
            egui::vec2(meta_w, 32.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.horizontal(|ui| {
                    if renaming {
                        commit_now = rename_edit(ui, state);
                    } else {
                        let name_color = if active { theme::FG_0 } else { theme::FG_1 };
                        if pinned {
                            widgets::truncating_line(
                                ui,
                                &[(&name, name_color), ("  ⌖", theme::AMBER)],
                                12.5,
                            );
                        } else {
                            widgets::truncating_text(ui, &name, name_color, 12.5);
                        }
                    }
                });
                path_line(ui, &pwd, remote.as_deref());
            },
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
            close_rect = rect;
            if hovered {
                let over = ui.rect_contains_pointer(rect);
                let color = if over { theme::RUST } else { theme::FG_2 };
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "×",
                    egui::FontId::proportional(14.0),
                    color,
                );
            }
        });
    });

    if commit_now {
        state.commit_rename();
    } else if resp.double_clicked() {
        state.start_session_rename(id);
    } else if resp.clicked() && !renaming {
        let on_close = resp
            .interact_pointer_pos()
            .is_some_and(|p| close_rect.contains(p));
        if on_close {
            state.close_session(id);
        } else {
            state.focus_session(id);
        }
    }
}

fn shelf_list(ui: &mut egui::Ui, state: &mut AppState) {
    let ids: Vec<String> = state.shelf.iter().map(|s| s.id.clone()).collect();

    egui::ScrollArea::vertical()
        .id_source("rail_l_shelf_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(theme::spacing::PAD_Y);
            let row_w = (ui.available_width() - 12.0).max(80.0);
            for id in ids {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    shelf_row(ui, state, &id, row_w);
                });
                ui.add_space(2.0);
            }
        });
}

fn shelf_row(ui: &mut egui::Ui, state: &mut AppState, id: &str, row_w: f32) {
    let (label, path, remote) = {
        let Some(s) = state.shelf.iter().find(|s| s.id == id) else {
            return;
        };
        (s.label.clone(), s.path.clone(), s.remote.clone())
    };
    let renaming = state.is_renaming_shelf(id);

    let mut commit_now = false;
    let mut edit_rect = egui::Rect::NOTHING;

    let resp = row(ui, RowState::Default, ("shelf", id), row_w, 36.0, |ui, hovered| {
        // 先頭の edit アイコン（クリックでラベル編集）。
        let edit_color = if hovered { theme::FG_1 } else { theme::FG_3 };
        edit_rect = widgets::icon_image(ui, Icon::Edit, edit_color, 13.0);
        ui.add_space(2.0);
        ui.label(egui::RichText::new("📁").size(13.0).color(theme::AMBER));
        ui.add_space(4.0);

        let pill_w = 56.0;
        let meta_w = (ui.available_width() - pill_w).max(40.0);
        ui.allocate_ui_with_layout(
            egui::vec2(meta_w, 30.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                if renaming {
                    commit_now = rename_edit(ui, state);
                } else {
                    widgets::truncating_text(ui, &label, theme::FG_0, 12.0);
                }
                path_line(ui, &path, remote.as_deref());
            },
        );

        // 右端: hover で "open ↵" を**テキストのみ**で表示（ボタン見た目は廃止）。
        // クリックはデフォルトで open 挙動（行全体）なので装飾のみ。
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if hovered && !renaming {
                ui.label(egui::RichText::new("open ↵").size(10.0).color(theme::AMBER));
            }
        });
    });

    if commit_now {
        state.commit_rename();
    } else if resp.clicked() && !renaming {
        let on_edit = resp
            .interact_pointer_pos()
            .is_some_and(|p| edit_rect.contains(p));
        if on_edit {
            state.start_shelf_rename(id);
        } else {
            state.new_session(label.clone(), path.clone());
            let now = ui.input(|i| i.time);
            state.show_toast(format!("Opened \"{label}\""), Some(path), now);
        }
    }
}

/// inline rename 用 TextEdit を描画し、Enter / blur で確定すべきとき true を返す
/// （session / shelf 共通）。
///
/// Esc によるキャンセルは `app.rs` の handle_shortcuts（update 冒頭で実行）が
/// `rename_target` をクリアして一本化している。ここで Esc を見ないのは、
/// TextEdit が Esc を consume して `lost_focus` 後の判定が commit に倒れるのを避けるため。
fn rename_edit(ui: &mut egui::Ui, state: &mut AppState) -> bool {
    let edit = egui::TextEdit::singleline(&mut state.ui.rename_buffer)
        .id(rename_id())
        .frame(false)
        .text_color(theme::FG_0)
        .desired_width(ui.available_width() - 8.0);
    let r = ui.add(edit);
    // rename 開始直後の 1 フレームだけ focus を要求する。
    if state.ui.rename_focus_pending {
        r.request_focus();
        state.ui.rename_focus_pending = false;
    }
    // Enter / blur どちらも確定（仕様: blur/Enter commits）。
    r.lost_focus()
}

/// rail 行の 2 行目「[remote:] pwd」を 1 行で ellipsis 切り詰め描画する。
fn path_line(ui: &mut egui::Ui, pwd: &str, remote: Option<&str>) {
    let mut spans: Vec<(&str, egui::Color32)> = Vec::new();
    let remote_prefix = remote.map(|r| format!("{r}:"));
    if let Some(rp) = &remote_prefix {
        spans.push((rp.as_str(), theme::FG_2));
    }
    spans.push((pwd, theme::FG_2));
    widgets::truncating_line(ui, &spans, 10.5);
}
