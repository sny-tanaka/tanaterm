//! Central: term-head / ターミナル出力 / Input 行。
//!
//! Phase 1 では mock: Enter で `AppState::run_active_input` を呼んで block を append する。

use eframe::egui;

use crate::clock;
use crate::state::{AppState, Block, OutputColor, OutputSpan, Session};
use crate::theme;
use crate::ui::widgets::{self, kbd, status_dot};

pub fn input_id() -> egui::Id {
    egui::Id::new("tanaterm.central.input")
}

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(theme::BG_1))
        .show(ctx, |ui| {
            // term-head（上）/ input 行（下）を panel として固定し、ターミナル出力が
            // 残り高さを埋める。これで入力行が画面下に固定され、見切れない。
            // パネルの境界線は show_separator_line が描く（手動 hline 不要）。
            egui::TopBottomPanel::top("term_head")
                .frame(egui::Frame::none().fill(theme::BG_1))
                .show_inside(ui, |ui| term_head(ui, state));
            egui::TopBottomPanel::bottom("input_row")
                .frame(egui::Frame::none().fill(theme::BG_0))
                .show_inside(ui, |ui| input_row(ui, state));
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(theme::BG_1))
                .show_inside(ui, |ui| term_area(ui, state));
        });
}

fn term_head(ui: &mut egui::Ui, state: &AppState) {
    egui::Frame::none()
        .fill(theme::BG_1)
        .stroke(egui::Stroke::new(0.0, egui::Color32::TRANSPARENT))
        .inner_margin(egui::Margin {
            left: 16.0,
            right: 16.0,
            top: 11.0,
            bottom: 11.0,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let Some(session) = state.active() else {
                    ui.label(egui::RichText::new("(no session)").color(theme::FG_2));
                    return;
                };
                status_dot(ui, session.status, 8.0);
                ui.label(
                    egui::RichText::new(&session.name)
                        .strong()
                        .size(12.5)
                        .color(theme::FG_0),
                );
                ui.label(egui::RichText::new("·").color(theme::FG_3));
                ui.label(
                    egui::RichText::new(&session.pwd)
                        .color(theme::FG_1)
                        .size(11.5),
                );
                if let Some(n) = &session.node {
                    ui.label(egui::RichText::new("·").color(theme::FG_3));
                    ui.label(
                        egui::RichText::new(format!("node {n}"))
                            .color(theme::AZURE)
                            .size(11.5),
                    );
                }
                ui.label(egui::RichText::new("·").color(theme::FG_3));
                ui.label(
                    egui::RichText::new(format!("{} blocks", session.blocks.len()))
                        .color(theme::FG_2)
                        .size(11.5),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let _ = widgets::icon_button(ui, "⋯", false);
                    let _ = widgets::icon_button(ui, "▾", false);
                    let _ = widgets::icon_button(ui, "▸", false);
                });
            });
        });
}

fn term_area(ui: &mut egui::Ui, state: &AppState) {
    let Some(session) = state.active() else {
        return;
    };

    if session.blocks.is_empty() {
        empty_placeholder(ui, session);
        return;
    }

    egui::ScrollArea::vertical()
        .id_source("term_area")
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(14.0);
            for block in &session.blocks {
                draw_block(ui, block);
                ui.add_space(8.0);
            }
        });
}

fn empty_placeholder(ui: &mut egui::Ui, session: &Session) {
    ui.with_layout(
        egui::Layout::centered_and_justified(egui::Direction::TopDown),
        |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(egui::RichText::new("棚").size(72.0).color(theme::AMBER));
                ui.label(
                    egui::RichText::new(format!("empty session at {}", session.pwd))
                        .size(12.0)
                        .color(theme::FG_2),
                );
            });
        },
    );
}

fn draw_block(ui: &mut egui::Ui, block: &Block) {
    let running = block.running;
    let err = matches!(block.exit_code, Some(c) if c != 0);
    let border = if running {
        theme::AMBER
    } else if err {
        theme::RUST
    } else {
        egui::Color32::TRANSPARENT
    };

    // 左 2px border + 12px padding (`.block` のレイアウト)。
    // 先に中身を描いて実際の高さを得てから border を描くことで、
    // border の長さがブロック内容の高さにぴったり揃う（固定高だと余る/足りない）。
    let block_left_pad = 12.0;
    let resp = ui.horizontal(|ui| {
        ui.add_space(block_left_pad);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            prompt_line(ui, block);
            cmd_line(ui, block, err);
            output(ui, block);
        });
    });

    if border != egui::Color32::TRANSPARENT {
        let rect = resp.response.rect;
        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.top()),
            egui::pos2(rect.left() + 2.0, rect.bottom()),
        );
        ui.painter().rect_filled(bar, 1.0, border);
    }
}

fn prompt_line(ui: &mut egui::Ui, block: &Block) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(
            egui::RichText::new("▶")
                .strong()
                .color(theme::AMBER)
                .size(11.5),
        );
        ui.label(
            egui::RichText::new(&block.pwd)
                .color(theme::FG_2)
                .size(11.5),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(&block.time)
                    .color(theme::FG_3)
                    .size(10.5),
            );
        });
    });
}

fn cmd_line(ui: &mut egui::Ui, block: &Block, err: bool) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("$")
                .strong()
                .color(theme::AMBER)
                .size(13.0),
        );
        ui.label(
            egui::RichText::new(&block.cmd)
                .color(theme::FG_0)
                .size(13.0),
        );
        if err {
            if let Some(code) = block.exit_code {
                egui::Frame::none()
                    .stroke(egui::Stroke::new(1.0, theme::RUST))
                    .rounding(999.0)
                    .inner_margin(egui::Margin::symmetric(6.0, 1.0))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(format!("exit {code}"))
                                .color(theme::RUST)
                                .size(10.5),
                        );
                    });
            }
        }
    });
}

fn output(ui: &mut egui::Ui, block: &Block) {
    let mut layout = egui::text::LayoutJob::default();
    for span in &block.output {
        push_span(&mut layout, span);
    }
    ui.label(layout);
}

fn push_span(job: &mut egui::text::LayoutJob, span: &OutputSpan) {
    let color = match span.color {
        OutputColor::Default => theme::FG_1,
        OutputColor::Sage => theme::SAGE,
        OutputColor::Rust => theme::RUST,
        OutputColor::Amber => theme::AMBER,
        OutputColor::Azure => theme::AZURE,
        OutputColor::Plum => theme::PLUM,
        OutputColor::Dim => theme::FG_3,
    };
    job.append(
        &span.text,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::monospace(12.5),
            color,
            ..Default::default()
        },
    );
}

fn input_row(ui: &mut egui::Ui, state: &mut AppState) {
    // 上罫線は bottom panel の separator が描く。
    egui::Frame::none()
        .fill(theme::BG_0)
        .inner_margin(egui::Margin {
            left: 14.0,
            right: 14.0,
            top: 10.0,
            bottom: 10.0,
        })
        .show(ui, |ui| {
            input_meta(ui, state);
            ui.add_space(6.0);
            input_line(ui, state);
        });
}

fn input_meta(ui: &mut egui::Ui, state: &mut AppState) {
    let Some((pwd, hist_len)) = state.active().map(|s| (s.pwd.clone(), s.history.len())) else {
        return;
    };
    let shell_label = match state.ui.shell {
        crate::config::Shell::Zsh => "zsh",
        crate::config::Shell::Bash => "bash",
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.label(egui::RichText::new("▶").color(theme::AMBER).size(11.0));
        ui.label(egui::RichText::new(pwd).color(theme::FG_1).size(11.0));
        // shell pill はクリックで zsh↔bash トグル（切替導線）。
        let shell_resp =
            pill(ui, shell_label, false, true).on_hover_text("click to switch shell (zsh / bash)");
        if shell_resp.clicked() {
            state.toggle_shell();
        }
        pill(ui, "tana", true, false);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("history: {hist_len} · tab to autocomplete"))
                    .color(theme::FG_2)
                    .size(11.0),
            );
        });
    });
}

/// input meta の pill。`clickable` の場合は hover で手のカーソルにする。
fn pill(ui: &mut egui::Ui, text: &str, accent: bool, clickable: bool) -> egui::Response {
    let (fg, stroke, fill) = if accent {
        (theme::AMBER, theme::AMBER, theme::AMBER_SOFT)
    } else {
        (theme::FG_1, theme::LINE, theme::BG_1)
    };
    let resp = egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .rounding(999.0)
        .inner_margin(egui::Margin::symmetric(7.0, 1.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(10.5).color(fg));
        })
        .response;
    if !clickable {
        return resp;
    }
    let resp = resp.interact(egui::Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn input_line(ui: &mut egui::Ui, state: &mut AppState) {
    let mut submit = false;
    let mut history_up = false;
    let mut history_down = false;
    let mut complete = false;

    // input_buffer を一旦取り出し、TextEdit に渡している間は state を非借用にする。
    // describe→state.* を呼び出せる構造を保つための定石パターン。
    let has_active = state.active().is_some();
    let mut buf = state
        .active_mut()
        .map(|s| std::mem::take(&mut s.input_buffer))
        .unwrap_or_default();

    let frame_resp = egui::Frame::none()
        .fill(theme::BG_1)
        .stroke(egui::Stroke::new(1.0, theme::LINE_2))
        .rounding(8.0)
        .inner_margin(egui::Margin {
            left: 12.0,
            right: 12.0,
            top: 9.0,
            bottom: 9.0,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("$")
                        .strong()
                        .color(theme::AMBER)
                        .size(13.5),
                );
                if has_active {
                    let edit = egui::TextEdit::singleline(&mut buf)
                        .id(input_id())
                        .frame(false)
                        .text_color(theme::FG_0)
                        .font(egui::FontId::monospace(13.5))
                        .desired_width(ui.available_width() - 100.0)
                        .hint_text(
                            egui::RichText::new("type a command…")
                                .color(theme::FG_3)
                                .size(13.0),
                        );
                    let r = ui.add(edit);
                    // singleline は Enter でフォーカスを失うため、その瞬間は has_focus() が
                    // false になる。Enter は lost_focus() + キー押下で検出する（egui の定石）。
                    if r.lost_focus() && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                    if r.has_focus() {
                        ui.ctx().input_mut(|i| {
                            if i.key_pressed(egui::Key::ArrowUp) {
                                history_up = true;
                            }
                            if i.key_pressed(egui::Key::ArrowDown) {
                                history_down = true;
                            }
                            // Tab はフォーカス移動に使われるので consume して補完に回す。
                            if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                                complete = true;
                            }
                        });
                    }
                } else {
                    ui.label(
                        egui::RichText::new("(no session)")
                            .color(theme::FG_3)
                            .size(13.0),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    kbd(ui, "↵");
                    ui.label(egui::RichText::new("press").color(theme::FG_3).size(11.0));
                });
            });
        })
        .response;

    // バッファを戻す（履歴 step 系は別途 buf 内容を上書きするので、その後に
    // 再取得した buffer も反映される）。
    if let Some(s) = state.active_mut() {
        s.input_buffer = buf;
    }

    // focus ring (1px amber + 3px amber-soft 風)。
    if ui.ctx().memory(|m| m.has_focus(input_id())) {
        ui.painter().rect_stroke(
            frame_resp.rect.expand(2.0),
            8.0,
            egui::Stroke::new(2.0, theme::AMBER_SOFT),
        );
        ui.painter()
            .rect_stroke(frame_resp.rect, 8.0, egui::Stroke::new(1.0, theme::AMBER));
    }

    if submit {
        state.submit_input(clock::now_hhmm());
        // 連続入力できるよう入力欄に再フォーカスする（Enter で外れた分を取り戻す）。
        ui.ctx().memory_mut(|m| m.request_focus(input_id()));
    }
    if history_up {
        state.step_history(-1);
    }
    if history_down {
        state.step_history(1);
    }
    if complete {
        state.tab_complete();
    }
}
