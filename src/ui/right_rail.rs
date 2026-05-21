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
use crate::ui::widgets::{self, row, RowState};

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

fn commands_section(ui: &mut egui::Ui, state: &mut AppState, section: Section) {
    let visible: Vec<Command> = state
        .commands
        .iter()
        .filter(|c| section.contains(c))
        .cloned()
        .collect();

    if visible.is_empty() {
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

    let mut to_toggle_pin: Option<String> = None;
    let mut to_insert: Option<String> = None;

    egui::ScrollArea::vertical()
        .id_source(match section {
            Section::Pinned => "rail_r_pinned_scroll",
            Section::Recent => "rail_r_recent_scroll",
        })
        .auto_shrink([false, false]) // パネルの残り高さを埋める
        .show(ui, |ui| {
            ui.add_space(theme::spacing::PAD_Y);
            let row_w = (ui.available_width() - 12.0).max(80.0);
            for cmd in visible {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    let resp = command_row(ui, &cmd, row_w);
                    if resp.clicked_pin {
                        to_toggle_pin = Some(cmd.id.clone());
                    } else if resp.clicked_insert {
                        to_insert = Some(cmd.id.clone());
                    }
                });
                ui.add_space(2.0);
            }
        });

    if let Some(id) = to_toggle_pin {
        state.toggle_command_pin(&id);
    }
    if let Some(id) = to_insert {
        let now = ui.input(|i| i.time);
        state.insert_command(&id, now);
    }
}

struct CommandRowResponse {
    clicked_insert: bool,
    clicked_pin: bool,
}

fn command_row(ui: &mut egui::Ui, cmd: &Command, row_w: f32) -> CommandRowResponse {
    let mut pin_rect = egui::Rect::NOTHING;
    let resp = row(ui, RowState::Default, ("cmd", &cmd.id), row_w, 38.0, |ui, hovered| {
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
                widgets::truncating_text(ui, &cmd.cmd, theme::FG_0, 12.5);
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
            if hovered {
                ui.label(egui::RichText::new("insert").size(10.0).color(theme::AMBER));
            }
        });
    });

    // クリック位置が pin アイコン上なら pin トグル、それ以外は挿入。
    let on_pin = resp
        .interact_pointer_pos()
        .is_some_and(|p| pin_rect.contains(p));
    CommandRowResponse {
        clicked_pin: resp.clicked() && on_pin,
        clicked_insert: resp.clicked() && !on_pin,
    }
}
