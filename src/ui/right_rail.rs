//! 右 rail (commands)。
//!
//! - PINNED / RECENT を SESSIONS/SHELF と同様に縦並びで常時表示（タブ・検索欄なし）
//! - PINNED はリサイズ可能な上部パネル（高さは永続化）、RECENT が残りを埋める
//! - command rows: pin アイコン（keep.png を tint）/ name / desc + when / use-count pill /
//!   hover で "insert" ラベル
//! - pin アイコン: Recent では押すと pin（PINNED へ移動）、Pinned では押すと解除

use eframe::egui;

use crate::state::{AppState, Command};
use crate::theme;
use crate::ui::left_rail::rename_edit;
use crate::ui::widgets::{self, row, text_button, RowState};

/// 追加フォーム cmd 欄の固定 Id。
fn add_cmd_id() -> egui::Id {
    egui::Id::new("tanaterm.command_add.cmd")
}

/// 追加フォーム desc 欄の固定 Id。
fn add_desc_id() -> egui::Id {
    egui::Id::new("tanaterm.command_add.desc")
}

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    egui::SidePanel::right("rail_r")
        .exact_width(theme::dims::RAIL_R)
        .resizable(false)
        .frame(
            egui::Frame::none()
                .fill(theme::BG_0)
                .stroke(egui::Stroke::new(1.0, theme::LINE)),
        )
        .show(ctx, |ui| {
            // COMMANDS は PINNED / RECENT を束ねる親見出し（数値カウントは出さない）。
            ui.add_space(10.0);
            widgets::section_header_plain(ui, "COMMANDS");
            command_add(ui, state);

            // PINNED: リサイズ可能な上部パネル。
            egui::TopBottomPanel::top("rail_r_pinned")
                .resizable(true)
                .default_height(240.0)
                .frame(egui::Frame::none())
                .show_inside(ui, |ui| {
                    ui.add_space(6.0);
                    widgets::section_header_plain(ui, "PINNED");
                    commands_section(ui, state, Section::Pinned);
                });

            // RECENT: 残り高さを埋める。
            egui::CentralPanel::default()
                .frame(egui::Frame::none())
                .show_inside(ui, |ui| {
                    ui.add_space(6.0);
                    widgets::section_header_plain(ui, "RECENT");
                    commands_section(ui, state, Section::Recent);
                });
        });
}

/// commands の縦並びセクション種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Pinned,
    Recent,
}

impl Section {
    /// セクションに属するか（pin 状態ベース）。
    fn contains(self, c: &Command) -> bool {
        match self {
            Section::Pinned => c.pinned,
            Section::Recent => !c.pinned && c.when.is_some(),
        }
    }
}

/// COMMANDS 見出し下の inline 追加 UI。閉じている時は "+ add command" ボタン、
/// 開いている時は cmd / desc の 2 入力欄。cmd で Enter 確定 / Esc キャンセル（Esc は app 側）。
fn command_add(ui: &mut egui::Ui, state: &mut AppState) {
    if !state.ui.command_add_active {
        if text_button(ui, "+ add command", None) {
            state.start_command_add();
        }
        return;
    }

    // 確定は Enter のみ（`lost_focus && Enter`）。Tab 移動やフォーム外クリックでの blur 単独では
    // 確定しない（rename_edit が blur でも確定するのとは非対称。誤確定を避けるため）。
    let mut commit = false;
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        egui::Frame::none()
            .stroke(egui::Stroke::new(1.0, theme::LINE_2))
            .rounding(6.0)
            .inner_margin(egui::Margin::symmetric(8.0, 6.0))
            .show(ui, |ui| {
                ui.set_width((ui.available_width() - 10.0).max(60.0));
                ui.spacing_mut().item_spacing.y = 4.0;

                let cmd_edit = egui::TextEdit::singleline(&mut state.ui.command_add_cmd)
                    .id(add_cmd_id())
                    .frame(false)
                    .desired_width(f32::INFINITY)
                    .text_color(theme::FG_0)
                    .font(egui::FontId::monospace(12.5))
                    .hint_text(egui::RichText::new("command").color(theme::FG_3).size(12.0));
                let cmd_resp = ui.add(cmd_edit);
                if state.ui.command_add_focus_pending {
                    cmd_resp.request_focus();
                    state.ui.command_add_focus_pending = false;
                }
                // cmd 欄で Enter → 確定。
                if cmd_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    commit = true;
                }

                let desc_edit = egui::TextEdit::singleline(&mut state.ui.command_add_desc)
                    .id(add_desc_id())
                    .frame(false)
                    .desired_width(f32::INFINITY)
                    .text_color(theme::FG_2)
                    .font(egui::FontId::proportional(11.0))
                    .hint_text(
                        egui::RichText::new("description (optional)")
                            .color(theme::FG_3)
                            .size(11.0),
                    );
                let desc_resp = ui.add(desc_edit);
                if desc_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    commit = true;
                }

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("↵ add").size(10.0).color(theme::AMBER));
                    ui.label(egui::RichText::new("· esc cancel").size(10.0).color(theme::FG_3));
                });
            });
    });

    if commit {
        state.commit_command_add();
    }
}

fn commands_section(ui: &mut egui::Ui, state: &mut AppState, section: Section) {
    let ids: Vec<String> = state
        .commands
        .iter()
        .filter(|c| section.contains(c))
        .map(|c| c.id.clone())
        .collect();

    if ids.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(theme::spacing::PAD_X);
            let msg = match section {
                Section::Pinned => "no pinned commands",
                Section::Recent => "no recent commands",
            };
            ui.label(egui::RichText::new(msg).size(11.0).color(theme::FG_3));
        });
        return;
    }

    egui::ScrollArea::vertical()
        .id_source(match section {
            Section::Pinned => "rail_r_pinned_scroll",
            Section::Recent => "rail_r_recent_scroll",
        })
        .auto_shrink([false, false]) // パネルの残り高さを埋める
        .show(ui, |ui| {
            ui.add_space(theme::spacing::PAD_Y);
            let row_w = (ui.available_width() - 12.0).max(80.0);
            for id in ids {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    command_row(ui, state, &id, row_w);
                });
                ui.add_space(2.0);
            }
        });
}

fn command_row(ui: &mut egui::Ui, state: &mut AppState, id: &str, row_w: f32) {
    let cmd: Command = {
        let Some(c) = state.commands.iter().find(|c| c.id == id) else {
            return;
        };
        c.clone()
    };
    let renaming = state.is_renaming_command(id);

    let mut commit_now = false;
    let mut pin_rect = egui::Rect::NOTHING;

    let resp = row(
        ui,
        RowState::Default,
        ("cmd", id),
        row_w,
        38.0,
        |ui, hovered| {
            // 左: pin アイコン（pinned=amber / 非 pinned は hover で明るく）。
            let pin_color = if cmd.pinned {
                theme::AMBER
            } else if hovered {
                theme::FG_1
            } else {
                theme::FG_3
            };
            pin_rect = widgets::icon_image(ui, widgets::Icon::Pin, pin_color, 14.0);
            ui.add_space(4.0);

            // hover 時の "insert" ラベル分を右に確保し、meta 列をその残り幅に制限する。
            let action_w = 48.0;
            let meta_w = (ui.available_width() - action_w).max(40.0);
            ui.allocate_ui_with_layout(
                egui::vec2(meta_w, 30.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    if renaming {
                        commit_now = rename_edit(ui, state);
                    } else {
                        widgets::truncating_text(ui, &cmd.cmd, theme::FG_0, 12.5);
                    }
                    let mut spans: Vec<(&str, egui::Color32)> = Vec::new();
                    if let Some(d) = &cmd.desc {
                        spans.push((d.as_str(), theme::FG_2));
                    }
                    let when_label = cmd.when.as_ref().map(|w| format!("  ·  {w} ago"));
                    if let Some(wl) = &when_label {
                        spans.push((wl.as_str(), theme::FG_2));
                    }
                    if !spans.is_empty() {
                        widgets::truncating_line(ui, &spans, 10.5);
                    }
                },
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if hovered && !renaming {
                    ui.label(egui::RichText::new("insert").size(10.0).color(theme::AMBER));
                }
            });
        },
    );

    if commit_now {
        state.commit_rename();
    } else if resp.double_clicked() && !renaming {
        // ダブルクリックで cmd 文字列を inline 編集（編集中の再開始でバッファを潰さない）。
        state.start_command_rename(id);
    } else if resp.clicked() && !renaming {
        // クリック位置が pin アイコン上なら pin トグル、それ以外は挿入。
        let on_pin = resp
            .interact_pointer_pos()
            .is_some_and(|p| pin_rect.contains(p));
        if on_pin {
            state.toggle_command_pin(id);
        } else {
            let now = ui.input(|i| i.time);
            state.insert_command(id, now);
        }
    }
}
