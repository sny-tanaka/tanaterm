//! Phase 1 mock UI のオーケストレータ。
//!
//! 各 panel は `ui::{topbar, left_rail, right_rail, central, statusbar, toast}` に分割し、
//! ここでは:
//! - egui Visuals 初期化（`theme::apply`）
//! - キーボードショートカット（⌘1/⌘2/⌘T/⌘W/⌘K）
//! - パネル描画順（CentralPanel を最後に show する egui 制約を遵守）
//! - Toast の TTL 失効ケア
//!
//! PTY は Phase 2 で central から呼ぶ。Phase 1 では `state.run_active_input` が mock 実行。

use eframe::egui;

use crate::config::Config;
use crate::state::AppState;
use crate::theme;
use crate::ui;

pub struct TanaTermApp {
    #[allow(dead_code)] // Phase 2 で PTY 起動時の shell 選択などに利用予定。
    config: Config,
    state: AppState,
}

impl TanaTermApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        theme::apply(&cc.egui_ctx);
        ui::widgets::register_icons(&cc.egui_ctx);
        Self {
            config,
            state: AppState::seed(),
        }
    }
}

impl eframe::App for TanaTermApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);
        self.state.tick_toast(now);
        self.handle_shortcuts(ctx);

        // egui の制約: CentralPanel は最後に show すること。残りの
        // TopBottomPanel / SidePanel は CentralPanel より前に show する。
        ui::topbar::show(ctx, &mut self.state);
        ui::statusbar::show(ctx, &self.state);
        if self.state.ui.rail_left_visible {
            ui::left_rail::show(ctx, &mut self.state);
        }
        if self.state.ui.rail_right_visible {
            ui::right_rail::show(ctx, &mut self.state);
        }
        ui::central::show(ctx, &mut self.state);

        // Toast は最後に central 領域上にオーバレイ描画。
        let main_rect = central_main_rect(ctx, &self.state);
        ui::toast::show(ctx, &self.state, main_rect);
    }
}

impl TanaTermApp {
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // rename 中は Esc キャンセル / Enter 確定が個別 widget 側で処理されるので
        // ここではグローバル系のみハンドル。
        let toggle_left = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num1);
        let toggle_right = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num2);
        let new_session = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::T);
        let close_session = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::W);
        let focus_search = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::K);

        let mut do_new = false;
        let mut do_close = false;
        let mut do_focus_search = false;
        ctx.input_mut(|i| {
            if i.consume_shortcut(&toggle_left) {
                self.state.ui.rail_left_visible = !self.state.ui.rail_left_visible;
            }
            if i.consume_shortcut(&toggle_right) {
                self.state.ui.rail_right_visible = !self.state.ui.rail_right_visible;
            }
            if i.consume_shortcut(&new_session) {
                do_new = true;
            }
            if i.consume_shortcut(&close_session) {
                do_close = true;
            }
            if i.consume_shortcut(&focus_search) {
                do_focus_search = true;
            }
        });

        if do_new {
            self.state.new_session("new session", "~/");
            let now = ctx.input(|i| i.time);
            self.state.show_toast("New session", None, now);
        }
        if do_close {
            if let Some(id) = self.state.ui.active_session_id.clone() {
                self.state.close_session(&id);
            }
        }
        if do_focus_search {
            ctx.memory_mut(|m| m.request_focus(ui::topbar::search_id()));
        }

        // rename 中に Esc が押されたら inline rename をキャンセルする
        // （session / shelf 共通のキャンセル経路をここに一本化している）。
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.state.ui.rename_target.is_some()
        {
            self.state.cancel_rename();
        }
    }
}

/// Toast 描画用に CentralPanel 領域を概算する。
/// 左右 rail と top/bottom bar を除いた残り。
fn central_main_rect(ctx: &egui::Context, state: &AppState) -> egui::Rect {
    let screen = ctx.screen_rect();
    let left = if state.ui.rail_left_visible {
        theme::dims::RAIL_L
    } else {
        0.0
    };
    let right = if state.ui.rail_right_visible {
        theme::dims::RAIL_R
    } else {
        0.0
    };
    egui::Rect::from_min_max(
        egui::pos2(screen.left() + left, screen.top() + theme::dims::TOPBAR),
        egui::pos2(
            screen.right() - right,
            screen.bottom() - theme::dims::STATUSBAR,
        ),
    )
}
