//! Central: term-head / ターミナル出力 / Input 行。
//!
//! Phase 1 では mock: Enter で `AppState::run_active_input` を呼んで block を append する。

use eframe::egui;

use crate::clock;
use crate::state::{AppState, Block, OutputColor, OutputSpan, Session};
use crate::theme;
use crate::ui::widgets::{self, status_dot};

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
            // alt_screen バナーは running_card より優先して表示する（07: alt-screen 検知）。
            // input_row より後に show するので、input_row のすぐ上に積まれる。
            let session_id = state.ui.active_session_id.clone();
            let is_alt = session_id
                .as_deref()
                .is_some_and(|id| state.is_alt_screen(id));
            if is_alt {
                egui::TopBottomPanel::bottom("alt_screen_banner")
                    .frame(egui::Frame::none().fill(theme::BG_1))
                    .show_separator_line(false)
                    .show_inside(ui, alt_screen_banner);
            } else if has_running(state) {
                // running_card は busy 時のみ存在。alt_screen 中は表示しない。
                egui::TopBottomPanel::bottom("running_card")
                    .frame(egui::Frame::none().fill(theme::BG_1))
                    .show_separator_line(false)
                    .show_inside(ui, |ui| running_card(ui, state));
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(theme::BG_1))
                .show_inside(ui, |ui| term_area(ui, state));
        });
}

/// alternate screen（vim / less / htop 等の全画面 TUI）中のバナー（07: alt-screen 検知）。
///
/// running_card と同じ位置・形式（bottom panel、amber 枠）で表示する。
/// 入力行は通常どおり機能する（busy 中なので stdin 送信になる）。
fn alt_screen_banner(ui: &mut egui::Ui) {
    egui::Frame::none()
        .fill(theme::BG_2)
        .stroke(egui::Stroke::new(1.0, theme::AMBER.gamma_multiply(0.45)))
        .rounding(8.0)
        .inner_margin(egui::Margin {
            left: 12.0,
            right: 12.0,
            top: 8.0,
            bottom: 8.0,
        })
        .outer_margin(egui::Margin {
            left: 12.0,
            right: 12.0,
            top: 0.0,
            bottom: 6.0,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let now = ui.input(|i| i.time);
                // 縦バーを pulse させて視線を引く（running_card と同じスタイル）。
                let (bar_rect, _) =
                    ui.allocate_exact_size(egui::vec2(3.0, 16.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    bar_rect,
                    1.5,
                    theme::AMBER.gamma_multiply(widgets::pulse_opacity(now)),
                );
                ui.add_space(8.0);

                ui.label(
                    egui::RichText::new("interactive app")
                        .color(theme::AMBER)
                        .size(13.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("— 画面描画は非対応です（q 等で終了してください）")
                        .color(theme::FG_2)
                        .size(12.0),
                );
            });
        });
    // フレーム更新を要求してバーの pulse を動かし続ける。
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(40));
}

/// アクティブセッションの直近ブロックが実行中なら true。
pub fn has_running(state: &AppState) -> bool {
    state
        .active()
        .and_then(|s| s.blocks.last())
        .is_some_and(|b| b.running)
}

/// input_row の上にスティッキー表示する「実行中コマンド」カード。
/// スクロールでブロックが画面外に流れても、現在 running なコマンドと経過時間 / 中断ヒント
/// がここに常駐するので「何が動いているか / どう止めるか」を見失わない。
///
/// 呼び出し側 (`show()`) が `has_running(state)` で gating する前提のため、ここでの
/// `running` 再判定は防衛的なフォールバック。`has_running` の判定条件を変える時は
/// 両者の整合を維持すること。
fn running_card(ui: &mut egui::Ui, state: &AppState) {
    let Some(session) = state.active() else {
        return;
    };
    let Some(block) = session.blocks.last().filter(|b| b.running) else {
        return;
    };

    let now = ui.input(|i| i.time);
    let elapsed = crate::clock::now_unix().saturating_sub(block.started_at_unix);
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(40));

    egui::Frame::none()
        .fill(theme::BG_2)
        .stroke(egui::Stroke::new(1.0, theme::AMBER.gamma_multiply(0.45)))
        .rounding(8.0)
        .inner_margin(egui::Margin {
            left: 12.0,
            right: 12.0,
            top: 8.0,
            bottom: 8.0,
        })
        .outer_margin(egui::Margin {
            left: 12.0,
            right: 12.0,
            top: 0.0,
            bottom: 6.0,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // 縦バーを pulse させて視線を引く。
                let (bar_rect, _) =
                    ui.allocate_exact_size(egui::vec2(3.0, 16.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    bar_rect,
                    1.5,
                    theme::AMBER.gamma_multiply(widgets::pulse_opacity(now)),
                );
                ui.add_space(8.0);

                ui.add(egui::Spinner::new().color(theme::AMBER).size(14.0));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(truncate_for_hint(&block.cmd, 60))
                        .color(theme::FG_0)
                        .size(13.0)
                        .strong(),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("^C で中断")
                            .color(theme::RUST)
                            .size(10.5),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(format_elapsed(elapsed))
                            .color(theme::AMBER)
                            .size(11.0),
                    );
                });
            });
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

    // 下揃え: 前フレームの content 高さを memory に記録し、viewport との差分を
    // ScrollArea 先頭の spacer として入れる。content が viewport より小さい時は
    // 最新ブロックが下端に張り付き、超えると spacer が 0 になって通常スクロール。
    // `Layout::bottom_up` を使わないのは、その中で `ui.horizontal()` 配下の multi-line
    // テキスト（ls 出力など）の wrap 計算が崩れて行が重なって描画されるため。
    let cache_id = egui::Id::new(("term_area_content_h", &session.id));
    let viewport_h = ui.available_height();
    let prev_content_h: f32 = ui.memory(|m| m.data.get_temp(cache_id).unwrap_or(0.0_f32));
    let filler = (viewport_h - prev_content_h).max(0.0);

    egui::ScrollArea::vertical()
        .id_source("term_area")
        // ブロック数が viewport を超えた時、コマンド実行中の出力追記で下端追従させる。
        // 下端にいない時はスティックしない（egui 既定挙動）ので、過去履歴閲覧中は干渉しない。
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if filler > 0.0 {
                ui.add_space(filler);
            }
            let content_top = ui.cursor().top();
            ui.add_space(14.0);
            // docs/redesign.md §4: ブロック間に余白 + 1px hairline を入れる。
            // 最終ブロック後は hairline 無しで余白のみ（外枠の divider と二重に
            // ならないようにする）。
            let last = session.blocks.len().saturating_sub(1);
            for (i, block) in session.blocks.iter().enumerate() {
                draw_block(ui, block);
                if i != last {
                    ui.add_space(4.0);
                    block_divider(ui);
                    ui.add_space(4.0);
                } else {
                    ui.add_space(8.0);
                }
            }
            let content_h = ui.cursor().top() - content_top;
            ui.memory_mut(|m| m.data.insert_temp(cache_id, content_h));
        });
}

/// ターミナルブロック間に引く 1px hairline（左右 12px インセット）。
///
/// docs/redesign.md §4 のブロック区切り。色は `LINE` で、棚板ラインより薄い。
fn block_divider(ui: &mut egui::Ui) {
    let avail = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(avail, 1.0), egui::Sense::hover());
    let inset = 12.0;
    let line = egui::Rect::from_min_size(
        egui::pos2(rect.left() + inset, rect.top()),
        egui::vec2((rect.width() - inset * 2.0).max(0.0), 1.0),
    );
    ui.painter().rect_filled(line, 0.0, theme::LINE);
}

fn empty_placeholder(ui: &mut egui::Ui, session: &Session) {
    ui.with_layout(
        egui::Layout::centered_and_justified(egui::Direction::TopDown),
        |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                // 漢字 "棚" の代わりに painter で shelf マークを描く（TopBar ブランドと同形）。
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::hover());
                widgets::paint_brand_mark(
                    ui.painter(),
                    rect,
                    theme::AMBER,
                    egui::Color32::from_rgb(0x1a, 0x12, 0x0a),
                );
                ui.add_space(8.0);
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
        // 実行中は status_dot と同じ pulse 波形で縦バーを点滅させ、視線を引く。
        // cmd_line 側の spinner も同じ ctx を共有するので、ここの 40ms repaint で
        // 一括して再描画される（spinner 側で追加の request_repaint_after は不要）。
        let color = if running {
            let now = ui.input(|i| i.time);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(40));
            border.gamma_multiply(widgets::pulse_opacity(now))
        } else {
            border
        };
        ui.painter().rect_filled(bar, 1.0, color);
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
        if block.running {
            // 40ms repaint は draw_block の縦バー pulse 側で発行済みなのでここでは追加しない。
            // spinner は egui の painter ベース（フォント非依存）でフォントに glyph が無い
            // 環境（macOS 標準 fontset で braille 等が豆腐になる）でも安定して描画される。
            let elapsed = crate::clock::now_unix().saturating_sub(block.started_at_unix);
            ui.add_space(8.0);
            ui.add(egui::Spinner::new().color(theme::AMBER).size(14.0));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format_elapsed(elapsed))
                    .color(theme::AMBER)
                    .size(11.0),
            );
        }
    });
}

/// 経過秒数を Warp 風に簡潔フォーマット ("12s" / "1m23s" / "1h02m")。
fn format_elapsed(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}m{s:02}s")
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        format!("{h}h{m:02}m")
    }
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
    let Some((pwd, hist_len, busy_cmd, shell_exited)) = state.active().map(|s| {
        let busy = s.blocks.last().filter(|b| b.running).map(|b| b.cmd.clone());
        (s.pwd.clone(), s.history.len(), busy, s.shell_exited)
    }) else {
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
            // shell_exited 時は「shell exited」を表示（08: シェル終了検知）。
            // 実行中: 入力が「新コマンド」ではなく「running プロセスの stdin」に流れる
            // ことを明示し、止め方（^C）も同時に提示する。
            // 通常: history 件数と tab 補完のヒント。
            if shell_exited {
                ui.label(
                    egui::RichText::new("shell exited")
                        .color(theme::FG_2)
                        .size(11.0),
                );
            } else {
                match busy_cmd {
                    Some(cmd) => {
                        ui.label(
                            egui::RichText::new("^C で中断")
                                .color(theme::RUST)
                                .size(11.0),
                        );
                        ui.label(egui::RichText::new("·").color(theme::FG_3).size(11.0));
                        ui.label(
                            egui::RichText::new(format!("stdin → {}", truncate_for_hint(&cmd, 32)))
                                .color(theme::AMBER)
                                .size(11.0),
                        );
                    }
                    None => {
                        ui.label(
                            egui::RichText::new(format!(
                                "history: {hist_len} · tab to autocomplete"
                            ))
                            .color(theme::FG_2)
                            .size(11.0),
                        );
                    }
                }
            }
        });
    });
}

/// hint バナー用に文字列を最大 `max_chars` 文字に切り詰め、超過分は `…` に置換する。
/// マルチバイト文字を含む可能性があるため `chars()` 単位で扱う。
fn truncate_for_hint(s: &str, max_chars: usize) -> String {
    debug_assert!(max_chars >= 2, "末尾 `…` を入れるため最低 2 文字必要");
    let total = s.chars().count();
    if total <= max_chars {
        return s.to_string();
    }
    let kept: String = s.chars().take(max_chars - 1).collect();
    format!("{kept}…")
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
    let mut do_restart = false;

    // input_buffer を一旦取り出し、TextEdit に渡している間は state を非借用にする。
    // describe→state.* を呼び出せる構造を保つための定石パターン。
    let has_active = state.active().is_some();
    // shell_exited 時は TextEdit の代わりに restart 導線を表示する（08: シェル終了検知）。
    let shell_exited = state.active().is_some_and(|s| s.shell_exited);
    // Busy 時は「入力が新コマンドでなく running プロセスの stdin へ届く」ことを hint で示す。
    let busy_cmd: Option<String> = state
        .active()
        .and_then(|s| s.blocks.last().filter(|b| b.running).map(|b| b.cmd.clone()));
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
                if shell_exited {
                    // シェル終了済み: TextEdit を表示せず restart 導線を表示する（08）。
                    ui.label(
                        egui::RichText::new("shell exited")
                            .color(theme::FG_2)
                            .size(13.0),
                    );
                    ui.add_space(8.0);
                    // 「restart shell ↵」はクリック可能テキスト（amber）。
                    let restart_resp = ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new("restart shell ↵")
                                    .color(theme::AMBER)
                                    .size(13.0),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if restart_resp.clicked() {
                        do_restart = true;
                    }
                    // Enter キーでも restart を呼べる（フォーカスは無いがグローバルに拾う）。
                    if ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)) {
                        do_restart = true;
                    }
                } else if has_active {
                    let hint = match &busy_cmd {
                        Some(cmd) => {
                            format!("type to send stdin to {}…", truncate_for_hint(cmd, 24))
                        }
                        None => "type a command…".to_string(),
                    };
                    let edit = egui::TextEdit::singleline(&mut buf)
                        .id(input_id())
                        .frame(false)
                        .text_color(theme::FG_0)
                        .font(egui::FontId::monospace(13.5))
                        .desired_width(ui.available_width() - 100.0)
                        .hint_text(egui::RichText::new(hint).color(theme::FG_3).size(13.0));
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
                    if !shell_exited {
                        // ↵ ボタンは見た目「リターンキー」だが実体は送信ボタン。クリックでも
                        // Enter 押下と同じ submit が走るようにする（lost_focus + Enter のキー
                        // ハンドリングと分岐は同じ）。busy 時は「stdin 送信」を意味する。
                        if widgets::kbd_return_button(ui).clicked() {
                            submit = true;
                        }
                        // busy 時は "send stdin"、idle 時は "press" にして
                        // input_meta の「stdin → {cmd}」とラベルの意図を揃える。
                        let leading = if busy_cmd.is_some() {
                            "send stdin"
                        } else {
                            "press"
                        };
                        ui.label(egui::RichText::new(leading).color(theme::FG_3).size(11.0));
                    }
                });
            });
        })
        .response;

    // バッファを戻す（履歴 step 系は別途 buf 内容を上書きするので、その後に
    // 再取得した buffer も反映される）。
    if let Some(s) = state.active_mut() {
        s.input_buffer = buf;
    }

    // focus ring。docs/redesign.md §5: 枠線の色変化に加え 3px の半透明 amber リング
    // (rgba(232,166,82,.14)) を外側に重ねる。1px amber の輪郭は据え置きで、輪郭の
    // すぐ外側に薄い amber ハロを敷くことで「フォーカスが当たっている」ことを強調する。
    if ui.ctx().memory(|m| m.has_focus(input_id())) {
        // 外側 3px の amber 14% リング（CSS の box-shadow: 0 0 0 3px に相当）。
        ui.painter().rect_stroke(
            frame_resp.rect.expand(1.5),
            8.0,
            egui::Stroke::new(3.0, theme::AMBER_RING),
        );
        // 内側 1px の amber 輪郭。
        ui.painter()
            .rect_stroke(frame_resp.rect, 8.0, egui::Stroke::new(1.0, theme::AMBER));
    }

    // shell_exited 時の restart 処理（08: restart 導線）。
    if do_restart {
        if let Some(id) = state.ui.active_session_id.clone() {
            state.restart_session(&id);
        }
    }

    if submit {
        // busy 時: 入力は新コマンドではなく、実行中プロセスの stdin として転送する。
        // hint「stdin → {cmd}」と一致させる。`busy_cmd` は関数頭で取得済み（同フレーム）。
        if busy_cmd.is_some() {
            state.submit_input_as_stdin();
        } else {
            state.submit_input(clock::now_hhmm());
        }
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

#[cfg(test)]
mod tests {
    use super::{format_elapsed, truncate_for_hint};

    #[test]
    fn truncate_for_hint_keeps_short_strings_as_is() {
        assert_eq!(truncate_for_hint("ls", 10), "ls");
        assert_eq!(truncate_for_hint("0123456789", 10), "0123456789");
    }

    #[test]
    fn truncate_for_hint_replaces_overflow_with_ellipsis() {
        assert_eq!(truncate_for_hint("01234567890", 10), "012345678…");
    }

    #[test]
    fn truncate_for_hint_counts_multibyte_chars_correctly() {
        // 6 文字 (3 漢字 + 3 ascii) は max=6 で無加工
        assert_eq!(truncate_for_hint("日本語abc", 6), "日本語abc");
        // 8 文字を max=5 で切り詰め
        assert_eq!(truncate_for_hint("日本語テスト用", 5), "日本語テ…");
    }

    #[test]
    fn format_elapsed_seconds_only() {
        assert_eq!(format_elapsed(0), "0s");
        assert_eq!(format_elapsed(59), "59s");
    }

    #[test]
    fn format_elapsed_minutes_with_zero_padded_seconds() {
        assert_eq!(format_elapsed(60), "1m00s");
        assert_eq!(format_elapsed(3599), "59m59s");
    }

    #[test]
    fn format_elapsed_hours_with_zero_padded_minutes() {
        assert_eq!(format_elapsed(3600), "1h00m");
        assert_eq!(format_elapsed(3661), "1h01m");
    }
}
