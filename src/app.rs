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
        register_cjk_fallback(&cc.egui_ctx);
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
        // 順序: まずグローバルショートカット（⌘W 等）を consume し、残ったキーから
        // Ctrl 修飾の制御キーを PTY 転送用に consume する。逆順にすると将来
        // handle_shortcuts に追加した Ctrl 系ショートカットが PTY 側に流れてしまう。
        self.handle_shortcuts(ctx);
        self.forward_pty_control_keys(ctx);

        // 1) PTY 出力を取り込んで AppState に反映（描画前に最新化）。
        self.pump_pty(ctx);

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
    ///
    /// `EndBlock` を検知し、経過秒 ≥ 15 かつウィンドウ非フォーカスなら
    /// macOS 通知を発火する（22: 長時間コマンド完了通知）。
    fn pump_pty(&mut self, ctx: &egui::Context) {
        for id in self.pty.session_ids() {
            let (actions, exited) = self.pty.drain(&id);
            for action in &actions {
                // EndBlock 適用**前**に実行中ブロック情報を取得する（22）。
                if let crate::term::TermAction::EndBlock { exit } = action {
                    // セッションの running ブロックを探す。
                    if let Some(session) = self.state.sessions.iter().find(|s| s.id == id) {
                        if let Some(block) = session.blocks.iter().rev().find(|b| b.running) {
                            let elapsed =
                                crate::clock::now_unix().saturating_sub(block.started_at_unix);
                            let window_focused = ctx.input(|i| i.focused);
                            if should_notify(elapsed, window_focused) {
                                let body = notify_body(&block.cmd, *exit, elapsed);
                                send_notification(&body);
                            }
                        }
                    }
                }
            }
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
        for action in std::mem::take(&mut self.state.pending) {
            match action {
                PendingPty::Spawn { id, cwd } => {
                    // 起動シェルはセッション固有の shell を使う（13: セッション準拠）。
                    // セッションが見つからなければグローバル ui.shell にフォールバック。
                    let shell = self
                        .state
                        .sessions
                        .iter()
                        .find(|s| s.id == id)
                        .map(|s| s.shell)
                        .unwrap_or(self.state.ui.shell);
                    // rows/cols は spawn 直前の has_running 状態を反映するように
                    // ここで都度計算する。pending に Spawn + Send が並んだ時、Send 側で
                    // 直近ブロックが running 化される前後で行高が変わる可能性に備える。
                    let (rows, cols) = term_grid_size(ctx, main_rect, &self.state);
                    let ok = self.pty.spawn(
                        &id,
                        SpawnSpec {
                            shell,
                            login_shell: self.config.login_shell,
                            program: self.config.shell_program(shell),
                            cwd: pty::expand_path(&cwd),
                            rows,
                            cols,
                        },
                        ctx,
                    );
                    // spawn 失敗時はセッションを exited 状態にして toast を出す（08）。
                    if !ok {
                        self.state.mark_session_exited(&id);
                        let now = ctx.input(|i| i.time);
                        self.state
                            .show_toast("シェルの起動に失敗しました", None, now);
                    }
                }
                PendingPty::Send { id, bytes } => {
                    // bytes から末尾の \n を除いた UTF-8 文字列をコマンドとして渡す（06）。
                    let cmd = std::str::from_utf8(&bytes)
                        .unwrap_or("")
                        .trim_end_matches('\n')
                        .to_string();
                    self.pty.on_submit(&id, &cmd);
                    self.pty.send(&id, &bytes);
                }
                PendingPty::SendRaw { id, bytes } => {
                    self.pty.send(&id, &bytes);
                }
                PendingPty::Close { id } => self.pty.close(&id),
            }
        }
    }

    /// ターミナル領域のサイズから rows/cols を算出し、変化時のみ全セッションへ反映する。
    fn sync_pty_size(&mut self, ctx: &egui::Context, main_rect: egui::Rect) {
        let size = term_grid_size(ctx, main_rect, &self.state);
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
        // フォントズーム: ⌘+ / ⌘= → 拡大、⌘- → 縮小、⌘0 → リセット。
        let zoom_in_plus = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Plus);
        let zoom_in_eq = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Equals);
        let zoom_out = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Minus);
        let zoom_reset = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num0);

        let mut do_new = false;
        let mut do_close = false;
        let mut do_focus_search = false;
        let mut font_delta: f32 = 0.0;
        let mut do_font_reset = false;
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
            // フォントズーム。
            if i.consume_shortcut(&zoom_in_plus) || i.consume_shortcut(&zoom_in_eq) {
                font_delta += 1.0;
            }
            if i.consume_shortcut(&zoom_out) {
                font_delta -= 1.0;
            }
            if i.consume_shortcut(&zoom_reset) {
                do_font_reset = true;
            }
        });

        if do_new {
            self.state.new_session("session", "~/");
            let now = ctx.input(|i| i.time);
            self.state.show_toast("New session", None, now);
        }
        if do_close {
            if let Some(id) = self.state.ui.active_session_id.clone() {
                // close_session が false の時は最後の 1 セッションなので toast を出す（10）。
                if !self.state.close_session(&id) {
                    let now = ctx.input(|i| i.time);
                    self.state
                        .show_toast("最後のセッションは閉じられません", None, now);
                }
            }
        }
        if do_focus_search {
            ctx.memory_mut(|m| m.request_focus(ui::topbar::search_id()));
        }
        // フォントサイズ変更（⌘+/−/0）。
        if do_font_reset {
            self.state.set_font_size(self.config.font_size);
            let now = ctx.input(|i| i.time);
            let size = self.state.ui.font_size as u32;
            self.state
                .show_toast(format!("font size: {size}"), None, now);
        } else if font_delta != 0.0 {
            self.state.adjust_font_size(font_delta);
            let now = ctx.input(|i| i.time);
            let size = self.state.ui.font_size as u32;
            self.state
                .show_toast(format!("font size: {size}"), None, now);
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.state.ui.rename_target.is_some() {
                self.state.cancel_rename();
            }
            if self.state.ui.command_add_active {
                self.state.cancel_command_add();
            }
            // 検索欄にフォーカスがある時の Esc は query をクリアしてフォーカスを手放す。
            let search_focused = ctx.memory(|m| m.focused() == Some(ui::topbar::search_id()));
            if search_focused {
                self.state.ui.search_query.clear();
                ctx.memory_mut(|m| m.surrender_focus(ui::topbar::search_id()));
            }
        }
    }

    /// Ctrl 修飾キー（C/D/Z/\）を ASCII 制御文字としてアクティブセッションの PTY へ転送する。
    ///
    /// 入力行 `TextEdit` は Enter/Tab/↑↓ しか拾わないため、実行中コマンドへ SIGINT 等を
    /// 送る手段が他にない。`consume_key` で先取りすることで、Ctrl+C の "c" が TextEdit に
    /// 流れ込んで文字として挿入されるのも防ぐ。
    ///
    /// 転送条件（09: Ctrl キーガード）:
    /// 1. フォーカスガード: 入力行以外のウィジェット（検索欄・rename 等）にフォーカスが
    ///    ある場合は consume せず return する（それらの Ctrl 入力を奪わない）。
    /// 2. busy ガード: アクティブセッションの直近ブロックが running の時のみ転送する。
    ///    idle 中の Ctrl+C/D/Z/\ は consume も転送もしない（idle zsh への EOF 誤送防止）。
    fn forward_pty_control_keys(&mut self, ctx: &egui::Context) {
        // フォーカスガード（09）: 入力行以外のウィジェットにフォーカスがある場合は何もしない。
        let focused_elsewhere =
            ctx.memory(|m| m.focused().is_some_and(|id| id != ui::central::input_id()));
        // busy ガード（09）: アクティブセッションの直近ブロックが実行中のときのみ転送する。
        let busy = ui::central::has_running(&self.state);

        if !should_forward_ctrl(focused_elsewhere, busy) {
            return;
        }

        const BINDINGS: &[(egui::Key, u8)] = &[
            (egui::Key::C, 0x03),         // ETX  / SIGINT
            (egui::Key::D, 0x04),         // EOT  / EOF
            (egui::Key::Z, 0x1a),         // SUB  / SIGTSTP
            (egui::Key::Backslash, 0x1c), // FS   / SIGQUIT
        ];
        let mut to_send: Vec<u8> = Vec::new();
        ctx.input_mut(|i| {
            for &(key, byte) in BINDINGS {
                if i.consume_key(egui::Modifiers::CTRL, key) {
                    to_send.push(byte);
                }
            }
        });
        for byte in to_send {
            self.state.push_pty_send_raw(vec![byte]);
        }
    }
}

/// Ctrl 制御キーを PTY へ転送してよいか判定する純関数（09: Ctrl キーガード）。
///
/// - `focused_elsewhere`: 入力行以外のウィジェットにフォーカスがある。
/// - `busy`: アクティブセッションの直近ブロックが実行中。
///
/// 転送してよい（true）のは「busy かつ入力行以外にフォーカスがない」場合のみ。
fn should_forward_ctrl(focused_elsewhere: bool, busy: bool) -> bool {
    busy && !focused_elsewhere
}

// ── 22: 長時間コマンド完了通知 ────────────────────────────────────────────

/// 通知を発火すべきか判定する純関数（22: 長時間コマンド完了通知）。
///
/// - `elapsed_secs`: コマンド開始から完了までの経過秒数。
/// - `window_focused`: ウィンドウがフォーカスを持っているか。
///
/// 経過秒 ≥ 15 かつウィンドウ非フォーカスの場合に `true` を返す。
fn should_notify(elapsed_secs: u64, window_focused: bool) -> bool {
    elapsed_secs >= 15 && !window_focused
}

/// AppleScript の文字列リテラル用に `\` と `"` をエスケープする（22: インジェクション対策）。
///
/// テストはすべての OS でコンパイル・実行する。macOS 以外では `send_notification` 内でのみ
/// 呼ばれるが cfg 条件が絞られていないためここでは `cfg_attr` で警告を抑える。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn applescript_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c => out.push(c),
        }
    }
    out
}

/// 通知 body 文字列を組み立てる（22: 長時間コマンド完了通知）。
///
/// cmd は 40 文字程度で切り詰め（`truncate_for_hint` と同等ロジック）。
fn notify_body(cmd: &str, exit: Option<i32>, elapsed_secs: u64) -> String {
    // cmd を最大 40 char に切り詰める（マルチバイト文字を含む場合は chars 単位）。
    const MAX_CMD_CHARS: usize = 40;
    let total = cmd.chars().count();
    let display_cmd = if total <= MAX_CMD_CHARS {
        cmd.to_string()
    } else {
        let kept: String = cmd.chars().take(MAX_CMD_CHARS - 1).collect();
        format!("{kept}…")
    };

    // 経過時間フォーマット（"12s" / "1m23s" / "1h02m"）。
    let elapsed_str = if elapsed_secs < 60 {
        format!("{elapsed_secs}s")
    } else if elapsed_secs < 3600 {
        let m = elapsed_secs / 60;
        let s = elapsed_secs % 60;
        format!("{m}m{s:02}s")
    } else {
        let h = elapsed_secs / 3600;
        let m = (elapsed_secs % 3600) / 60;
        format!("{h}h{m:02}m")
    };

    match exit {
        Some(code) => {
            format!("「{display_cmd} が完了しました (exit {code} · {elapsed_str})」")
        }
        None => {
            format!("「{display_cmd} が完了しました ({elapsed_str})」")
        }
    }
}

/// macOS 通知を送る。失敗してもアプリに影響させない（22: 長時間コマンド完了通知）。
///
/// macOS 以外は no-op。
fn send_notification(body: &str) {
    #[cfg(target_os = "macos")]
    {
        let escaped_body = applescript_escape(body);
        let script = format!("display notification \"{escaped_body}\" with title \"tanaterm\"");
        // シェル経由にせず、osascript を直接 spawn する（インジェクション対策）。
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .spawn();
    }
    // macOS 以外は no-op（コンパイル対象外）。
    #[cfg(not(target_os = "macos"))]
    let _ = body;
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
/// term_head（上）と input 行（下）の高さ分を差し引いた概算。busy 時は running_card が
/// input 行の上に挟まる（≈ 40px）ぶんも差し引く。等幅フォントの 1 文字幅・行高を egui の
/// フォントメトリクスから取り、最低サイズでクランプする。
fn term_grid_size(ctx: &egui::Context, main_rect: egui::Rect, state: &AppState) -> (u16, u16) {
    // term_head ≈ 44px、input 行（meta + 入力枠 + margin）≈ 96px。
    const RESERVED_BASE_H: f32 = 140.0;
    // running_card（frame + outer margin）≈ 40px。
    const RUNNING_CARD_H: f32 = 40.0;
    const PAD_X: f32 = 32.0;

    let reserved_h = if ui::central::has_running(state) {
        RESERVED_BASE_H + RUNNING_CARD_H
    } else {
        RESERVED_BASE_H
    };

    let (char_w, line_h) = ctx.fonts(|f| {
        let font = egui::FontId::monospace(state.ui.font_size);
        let w = f.glyph_width(&font, 'M').max(1.0);
        let h = f.row_height(&font).max(1.0);
        (w, h)
    });

    let cols = ((main_rect.width() - PAD_X) / char_w).floor().max(20.0) as u16;
    let rows = ((main_rect.height() - reserved_h) / line_h)
        .floor()
        .max(4.0) as u16;
    (rows, cols)
}

/// macOS の CJK フォントを fallback として fontset に追加する。
///
/// egui のデフォルトは Latin のみなので、何もしないと日本語/中国語/韓国語が
/// 全部豆腐になる。`/System/Library/Fonts/` から候補を順に探し、最初に読めた
/// .ttc/.ttf を Proportional / Monospace 両 family の末尾に積んでフォールバック
/// 順を作る。見つからない場合は何もせず、CJK が出ない既存挙動のままにする。
fn register_cjk_fallback(ctx: &egui::Context) {
    // 日本語の仮名と CJK 統合漢字をカバーしているフォントを優先順位順に試す。
    // Hiragino Sans GB は中国語向けだが、ヒラギノ系の仮名グリフを同梱しているため
    // 日本語入力でもひらがな/カタカナ/漢字すべて描画できる。
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/Supplemental/AppleGothic.ttf",
    ];

    for path in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        const NAME: &str = "system-cjk";
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert(NAME.into(), egui::FontData::from_owned(bytes));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(NAME.into());
        }
        ctx.set_fonts(fonts);
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::{applescript_escape, notify_body, should_forward_ctrl, should_notify};

    /// Ctrl 転送判定の真理値表（09: Ctrl キーガード）。
    #[test]
    fn should_forward_ctrl_truth_table() {
        // busy かつ非 focused_elsewhere の時のみ true。
        assert!(
            should_forward_ctrl(false, true),
            "busy かつフォーカス通常 → 転送する"
        );
        // idle 中は転送しない（idle zsh への EOF 誤送を防ぐ）。
        assert!(
            !should_forward_ctrl(false, false),
            "idle かつフォーカス通常 → 転送しない"
        );
        // 他ウィジェットにフォーカスがある時は転送しない。
        assert!(
            !should_forward_ctrl(true, true),
            "busy だが他ウィジェットにフォーカス → 転送しない"
        );
        assert!(
            !should_forward_ctrl(true, false),
            "idle かつ他ウィジェットにフォーカス → 転送しない"
        );
    }

    // ── 22: should_notify / applescript_escape テスト ─────────────────────

    /// 通知発火条件の真理値表（22: 長時間コマンド完了通知）。
    #[test]
    fn should_notify_truth_table() {
        // 経過 15 秒以上かつ非フォーカス → 通知する。
        assert!(
            should_notify(15, false),
            "経過 15s・非フォーカス → 通知する"
        );
        assert!(
            should_notify(60, false),
            "経過 60s・非フォーカス → 通知する"
        );
        // 経過 14 秒以下 → 通知しない。
        assert!(!should_notify(14, false), "経過 14s → 通知しない");
        assert!(!should_notify(0, false), "経過 0s → 通知しない");
        // ウィンドウフォーカス中 → 通知しない（経過秒に関わらず）。
        assert!(
            !should_notify(15, true),
            "経過 15s でもフォーカス中 → 通知しない"
        );
        assert!(
            !should_notify(100, true),
            "経過 100s でもフォーカス中 → 通知しない"
        );
    }

    /// AppleScript エスケープ: `"` と `\` が正しくエスケープされる（22: インジェクション対策）。
    #[test]
    fn applescript_escape_handles_special_chars() {
        // 通常の文字列は変換なし。
        assert_eq!(applescript_escape("hello"), "hello");
        // ダブルクォートのエスケープ。
        assert_eq!(applescript_escape(r#"say "hi""#), r#"say \"hi\""#);
        // バックスラッシュのエスケープ。
        assert_eq!(applescript_escape(r"C:\tmp"), r"C:\\tmp");
        // 両方含む。
        assert_eq!(applescript_escape(r#"echo \"foo\""#), r#"echo \\\"foo\\\""#);
        // 日本語を含む文字列は ASCII 以外に変換なし。
        assert_eq!(
            applescript_escape("コマンド完了"),
            "コマンド完了",
            "日本語文字は変換しない"
        );
    }

    /// notify_body: コマンド名が 40 文字超の場合に切り詰める。
    #[test]
    fn notify_body_truncates_long_cmd() {
        let long_cmd = "a".repeat(50);
        let body = notify_body(&long_cmd, Some(0), 20);
        // 40 char 以下に切り詰められていること（"…" 分含む）。
        let cmd_part: String = long_cmd.chars().take(39).collect();
        assert!(
            body.contains(&format!("{cmd_part}…")),
            "40 文字超は切り詰められる: {body}"
        );
    }

    /// notify_body: exit code が含まれる形式を確認。
    #[test]
    fn notify_body_includes_exit_code_and_elapsed() {
        let body = notify_body("cargo build", Some(0), 30);
        assert!(body.contains("cargo build"), "cmd が含まれる: {body}");
        assert!(body.contains("exit 0"), "exit code が含まれる: {body}");
        assert!(body.contains("30s"), "経過時間が含まれる: {body}");
    }

    /// notify_body: exit code が None の場合。
    #[test]
    fn notify_body_without_exit_code() {
        let body = notify_body("long task", None, 120);
        assert!(body.contains("long task"), "cmd が含まれる: {body}");
        assert!(!body.contains("exit"), "exit code なし: {body}");
        assert!(body.contains("2m"), "経過時間が含まれる: {body}");
    }
}
