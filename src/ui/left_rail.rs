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
use crate::ui::widgets::{self, row, status_dot, text_button, Icon, RowState};

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
                    ui.add_space(14.0);
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
                    ui.add_space(14.0);
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

fn sessions_list(ui: &mut egui::Ui, state: &mut AppState) {
    let session_ids: Vec<String> = state.sessions.iter().map(|s| s.id.clone()).collect();
    let active_id = state.ui.active_session_id.clone();
    let last_idx = session_ids.len().saturating_sub(1);

    egui::ScrollArea::vertical()
        .id_source("rail_l_sessions_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(theme::spacing::PAD_Y);
            let row_w = (ui.available_width() - 12.0).max(80.0);
            for (i, id) in session_ids.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    session_row(ui, state, id, active_id.as_deref() == Some(id), row_w);
                });
                ui.add_space(2.0);
                // アイテム間 hairline（最終行の後には引かない）。
                if i != last_idx {
                    widgets::row_hairline(ui);
                }
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
    let row_state = if active {
        RowState::Active
    } else {
        RowState::Default
    };

    let mut commit_now = false;
    let mut close_rect = egui::Rect::NOTHING;

    let resp = row(
        ui,
        row_state,
        ("session", id),
        row_w,
        40.0,
        |ui, hovered| {
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
                            // pinned 時は名前の右に amber 小ドットを描く。
                            // 漢字 ⌖ (U+2316) は egui デフォルトフォントに無く豆腐になるため。
                            // 幅基準は外側 allocate_ui_with_layout で確定済みの meta_w を使う
                            // （`ui.available_width()` だと内側 spacing 等で揺らぐ可能性がある）。
                            // pinned 時のみ name と marker の間に item_spacing が入るので
                            // 同条件で差し引く。
                            let (marker_w, name_max) = if pinned {
                                let m = 12.0;
                                let sp = ui.spacing().item_spacing.x;
                                (m, (meta_w - m - sp).max(20.0))
                            } else {
                                (0.0, meta_w.max(20.0))
                            };
                            ui.allocate_ui(egui::vec2(name_max, 16.0), |ui| {
                                widgets::truncating_text(ui, &name, name_color, 12.5);
                            });
                            if pinned {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(marker_w, 12.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().circle_filled(rect.center(), 3.0, theme::AMBER);
                            }
                        }
                    });
                    path_line(ui, &pwd, remote.as_deref());
                },
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
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
        },
    );

    if commit_now {
        state.commit_rename();
    } else if resp.double_clicked() {
        state.start_session_rename(id);
    } else if resp.clicked() && !renaming {
        let on_close = resp
            .interact_pointer_pos()
            .is_some_and(|p| close_rect.contains(p));
        if on_close {
            // close_session が false の時は最後の 1 セッションなので toast を出す（10）。
            if !state.close_session(id) {
                let now = ui.input(|i| i.time);
                state.show_toast("最後のセッションは閉じられません", None, now);
            }
        } else {
            state.focus_session(id);
        }
    }
}

fn shelf_list(ui: &mut egui::Ui, state: &mut AppState) {
    let ids: Vec<String> = state.shelf.iter().map(|s| s.id.clone()).collect();
    let last_idx = ids.len().saturating_sub(1);

    egui::ScrollArea::vertical()
        .id_source("rail_l_shelf_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(theme::spacing::PAD_Y);
            let row_w = (ui.available_width() - 12.0).max(80.0);
            for (i, id) in ids.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    shelf_row(ui, state, id, row_w);
                });
                ui.add_space(2.0);
                if i != last_idx {
                    widgets::row_hairline(ui);
                }
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
    let mut delete_rect = egui::Rect::NOTHING;

    let resp = row(
        ui,
        RowState::Default,
        ("shelf", id),
        row_w,
        36.0,
        |ui, hovered| {
            // 先頭の edit アイコン（クリックでラベル編集）。
            let edit_color = if hovered { theme::FG_1 } else { theme::FG_3 };
            edit_rect = widgets::icon_image(ui, Icon::Edit, edit_color, 13.0);
            ui.add_space(2.0);
            ui.label(egui::RichText::new("📁").size(13.0).color(theme::AMBER));
            ui.add_space(4.0);

            // 右端の delete × と "open ↵" 分の幅を確保し、meta 列をその残りに制限する。
            let action_w = 80.0;
            let meta_w = (ui.available_width() - action_w).max(40.0);
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

            // 右端: hover で削除 × （最右）と "open ↵"。× はクリックで shelf 項目を削除。
            // delete_rect は × が見えている（hover 中）時だけ有効にし、誤削除を防ぐ。
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if hovered && !renaming {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                    delete_rect = rect;
                    let over = ui.rect_contains_pointer(rect);
                    let color = if over { theme::RUST } else { theme::FG_2 };
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "×",
                        egui::FontId::proportional(14.0),
                        color,
                    );
                    ui.add_space(4.0);
                    // right_to_left レイアウト内: 最初に追加した方が右に来る。
                    // ↵ を先に置いて「open ↵」の並び（左→右）を再現する。
                    widgets::return_arrow_inline(ui, theme::AMBER);
                    ui.label(egui::RichText::new("open").size(10.0).color(theme::AMBER));
                }
            });
        },
    );

    if commit_now {
        state.commit_rename();
    } else if resp.clicked() && !renaming {
        let pos = resp.interact_pointer_pos();
        let on_edit = pos.is_some_and(|p| edit_rect.contains(p));
        let on_delete = pos.is_some_and(|p| delete_rect.contains(p));
        if on_delete {
            state.remove_shelf(id);
            let now = ui.input(|i| i.time);
            state.show_toast(format!("Removed \"{label}\""), None, now);
        } else if on_edit {
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
pub(crate) fn rename_edit(ui: &mut egui::Ui, state: &mut AppState) -> bool {
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
