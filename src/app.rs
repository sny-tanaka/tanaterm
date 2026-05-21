//! アプリのオーケストレータ（Phase 2: 実 PTY 配線）。
//!
//! - egui Visuals 初期化（`theme::apply`）
//! - キーボードショートカット（⌘1/⌘2/⌘T/⌘W/⌘K）
//! - パネル描画順（CentralPanel を最後に show する egui 制約を遵守）
//! - PTY 入出力の駆動: 毎フレーム [`PtyManager::drain`] で出力を取り込み `AppState` に反映し、
//!   描画後に `AppState::pending`（spawn / send / close）を `PtyManager` へ適用する
//! - ターミナル領域サイズから PTY の rows/cols を算出し、変化時のみ resize（debounce）

use eframe::egui;

use crate::config::Config;
use crate::pty::{self, PtyManager, SpawnSpec};
use crate::state::{AppState, PendingPty, PersistentState};
use crate::theme;
use crate::ui;

/// `eframe` storage（`app.ron`）に AppState サブセットを保存する際のキー。
const STATE_KEY: &str = "tanaterm_state";

pub struct TanaTermApp {
    config: Config,
    state: AppState,
    pty: PtyManager,
    /// 直近に PTY へ通知した (rows, cols)。変化時のみ resize する。
    last_size: (u16, u16),
}

impl TanaTermApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        theme::apply(&cc.egui_ctx);
        ui::widgets::register_icons(&cc.egui_ctx);

        // 保存済み状態があれば復元、なければ空 1 セッションで起動（shell は設定ファイル既定）。
        let persisted: Option<PersistentState> =
            cc.storage.and_then(|s| eframe::get_value(s, STATE_KEY));
        let state = match persisted {
            // 復元時は保存済みの shell（ユーザが UI でトグルした最後の値）を優先する。
            Some(p) => AppState::from_persistent(p),
            // 初回起動のみ設定ファイルの shell を入力行トグルの既定値にする。
            None => {
                let mut s = AppState::boot();
                s.ui.shell = config.shell;
                s
            }
        };

        Self {
            config,
            state,
            pty: PtyManager::new(),
            last_size: (0, 0),
        }
    }
}

impl eframe::App for TanaTermApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);
        self.state.tick_toast(now);
        self.handle_shortcuts(ctx);

        // 1) PTY 出力を取り込んで AppState に反映（描画前に最新化）。
        self.pump_pty();

        // 2) パネル描画（CentralPanel は最後）。
        ui::topbar::show(ctx, &mut self.state);
        ui::statusbar::show(ctx, &self.state);
        if self.state.ui.rail_left_visible {
            ui::left_rail::show(ctx, &mut self.state);
        }
        if self.state.ui.rail_right_visible {
            ui::right_rail::show(ctx, &mut self.state);
        }
        ui::central::show(ctx, &mut self.state);

        let main_rect = central_main_rect(ctx, &self.state);
        ui::toast::show(ctx, &self.state, main_rect);

        // 3) 描画中に積まれた pending（spawn/send/close）を PTY へ適用。
        self.apply_pending(ctx, main_rect);

        // 4) ターミナルサイズが変わっていたら PTY を resize。
        self.sync_pty_size(ctx, main_rect);
    }

    /// eframe が定期的（既定 30 秒）＆終了時に呼ぶ。永続化サブセットを保存する。
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, STATE_KEY, &self.state.to_persistent());
    }
}

impl TanaTermApp {
    /// 各セッションの PTY 出力を引き取り、ブロックイベントとして AppState に適用する。
    fn pump_pty(&mut self) {
        for id in self.pty.session_ids() {
            let (actions, exited) = self.pty.drain(&id);
            for action in actions {
                self.state.apply_term_action(&id, action);
            }
            if exited {
                self.state.mark_session_exited(&id);
                self.pty.close(&id);
            }
        }
    }

    /// `AppState::pending` を PtyManager に適用する。
    fn apply_pending(&mut self, ctx: &egui::Context, main_rect: egui::Rect) {
        if self.state.pending.is_empty() {
            return;
        }
        let (rows, cols) = term_grid_size(ctx, main_rect);
        // 起動シェルは入力行のトグル（UiState.shell）を反映する（plan 2.1）。
        let shell = self.state.ui.shell;
        for action in std::mem::take(&mut self.state.pending) {
            match action {
                PendingPty::Spawn { id, cwd } => {
                    self.pty.spawn(
                        &id,
                        SpawnSpec {
                            shell,
                            login_shell: self.config.login_shell,
                            cwd: pty::expand_path(&cwd),
                            rows,
                            cols,
                        },
                        ctx,
                    );
                }
                PendingPty::Send { id, bytes } => {
                    self.pty.on_submit(&id);
                    self.pty.send(&id, &bytes);
                }
                PendingPty::Close { id } => self.pty.close(&id),
            }
        }
    }

    /// ターミナル領域のサイズから rows/cols を算出し、変化時のみ全セッションへ反映する。
    fn sync_pty_size(&mut self, ctx: &egui::Context, main_rect: egui::Rect) {
        let size = term_grid_size(ctx, main_rect);
        if size == self.last_size {
            return;
        }
        self.last_size = size;
        for id in self.pty.session_ids() {
            self.pty.resize(&id, size.0, size.1);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
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
            self.state.new_session("session", "~/");
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

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.state.ui.rename_target.is_some() {
                self.state.cancel_rename();
            }
            if self.state.ui.command_add_active {
                self.state.cancel_command_add();
            }
        }
    }
}

/// Toast 描画用に CentralPanel 領域を概算する。左右 rail と top/bottom bar を除いた残り。
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

/// ターミナル出力領域のピクセル寸法から PTY の (rows, cols) を見積もる。
///
/// term_head（上）と input 行（下）の高さ分を差し引いた概算。等幅フォントの 1 文字幅・
/// 行高を egui のフォントメトリクスから取り、最低サイズでクランプする。
fn term_grid_size(ctx: &egui::Context, main_rect: egui::Rect) -> (u16, u16) {
    const FONT: f32 = 12.5;
    // term_head ≈ 44px、input 行（meta + 入力枠 + margin）≈ 96px。
    const RESERVED_H: f32 = 140.0;
    const PAD_X: f32 = 32.0;

    let (char_w, line_h) = ctx.fonts(|f| {
        let font = egui::FontId::monospace(FONT);
        let w = f.glyph_width(&font, 'M').max(1.0);
        let h = f.row_height(&font).max(1.0);
        (w, h)
    });

    let cols = ((main_rect.width() - PAD_X) / char_w).floor().max(20.0) as u16;
    let rows = ((main_rect.height() - RESERVED_H) / line_h)
        .floor()
        .max(4.0) as u16;
    (rows, cols)
}
