//! マルチセッション PTY マネージャ（Phase 2）。
//!
//! セッションごとに `portable-pty` でシェルを起動し、reader thread が raw byte を
//! チャンネルへ流す。UI スレッドは毎フレーム [`PtyManager::drain`] でバイトを引き取り、
//! [`crate::term::SessionTerm`] でブロックイベントへ変換する（vt100 直結はしない）。
//!
//! 起動シェルには [`crate::shell_integration`] が OSC 133 / OSC 7 hook を注入する。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use eframe::egui;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};

use crate::config::Shell;
use crate::shell_integration::{self, Integration};
use crate::term::SessionTerm;

/// セッション起動パラメータ。
pub struct SpawnSpec {
    pub shell: Shell,
    pub login_shell: bool,
    /// 初期 cwd（実パス）。`None` なら `$HOME`。
    pub cwd: Option<PathBuf>,
    pub rows: u16,
    pub cols: u16,
}

/// 1 セッション分の PTY とブロック化レイヤ。
struct PtyHandle {
    pty: PtySession,
    term: SessionTerm,
}

/// セッション ID → PTY のマップ。`app.rs` が 1 つ所有する。
#[derive(Default)]
pub struct PtyManager {
    handles: HashMap<String, PtyHandle>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 指定セッションのシェルを起動する。既存 ID は no-op（既存なら true を返す）。
    ///
    /// 起動に成功した場合 `true`、失敗した場合 `false` を返す。
    /// 呼び出し元（`app.rs::apply_pending`）は `false` の場合に
    /// `state.mark_session_exited` ＋ toast を表示する（08: spawn 失敗検知）。
    pub fn spawn(&mut self, id: &str, spec: SpawnSpec, ctx: &egui::Context) -> bool {
        if self.handles.contains_key(id) {
            return true;
        }
        match PtySession::spawn(spec, ctx.clone()) {
            Ok(pty) => {
                self.handles.insert(
                    id.to_string(),
                    PtyHandle {
                        pty,
                        term: SessionTerm::new(),
                    },
                );
                true
            }
            Err(err) => {
                eprintln!("tanaterm: failed to spawn pty for {id}: {err}");
                false
            }
        }
    }

    pub fn close(&mut self, id: &str) {
        self.handles.remove(id);
    }

    /// 入力行からの送信前に呼ぶ（ブロック取り込み状態の初期化）。
    /// `cmd` は Auto モードのエコー除去用（06: heuristic echo strip）。
    pub fn on_submit(&mut self, id: &str, cmd: &str) {
        if let Some(h) = self.handles.get_mut(id) {
            h.term.on_submit(cmd);
        }
    }

    /// キー入力 / コマンドをシェルへ送る。
    pub fn send(&mut self, id: &str, bytes: &[u8]) {
        if let Some(h) = self.handles.get_mut(id) {
            h.pty.send(bytes);
        }
    }

    pub fn resize(&mut self, id: &str, rows: u16, cols: u16) {
        if let Some(h) = self.handles.get_mut(id) {
            h.pty.resize(rows, cols);
        }
    }

    /// 溜まった出力を引き取り、ブロックイベント（[`crate::term::TermAction`]）へ変換して返す。
    /// 第 2 戻り値はシェルが終了したか（チャンネル切断）。
    pub fn drain(&mut self, id: &str) -> (Vec<crate::term::TermAction>, bool) {
        let Some(h) = self.handles.get_mut(id) else {
            return (Vec::new(), false);
        };
        let (bytes, exited) = h.pty.drain();
        let actions = if bytes.is_empty() {
            Vec::new()
        } else {
            h.term.feed(&bytes)
        };
        (actions, exited)
    }

    /// PTY を持っているセッション ID 一覧。
    pub fn session_ids(&self) -> Vec<String> {
        self.handles.keys().cloned().collect()
    }
}

/// 1 本の PTY に紐づくシェルプロセスと出力チャンネル。
struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    rx: Receiver<Vec<u8>>,
    rows: u16,
    cols: u16,
    /// integration の一時 rc（Drop で消える）。注入なしなら `None`。
    _integration: Option<Integration>,
}

impl PtySession {
    fn spawn(spec: SpawnSpec, ctx: egui::Context) -> anyhow::Result<Self> {
        let SpawnSpec {
            shell,
            login_shell,
            cwd,
            rows,
            cols,
        } = spec;
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // shell integration（OSC 133 / OSC 7）。失敗しても integration なしで続行する。
        let integration = match shell_integration::prepare(shell) {
            Ok(i) => Some(i),
            Err(err) => {
                eprintln!("tanaterm: shell integration disabled ({err}); using heuristic mode");
                None
            }
        };

        let mut cmd = CommandBuilder::new(shell.program());
        let suppress_login = integration.as_ref().is_some_and(|i| i.suppress_login);
        if login_shell && !suppress_login {
            cmd.arg("-l");
        }
        if let Some(i) = &integration {
            for a in &i.args {
                cmd.arg(a);
            }
            for (k, v) in &i.env {
                cmd.env(k, v);
            }
        }
        cmd.env("TERM", "xterm-256color");

        let start_dir = cwd
            .filter(|p| p.is_dir())
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from));
        if let Some(dir) = start_dir {
            cmd.cwd(dir);
        }

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break; // 受信側（PtySession）が drop された。
                        }
                        ctx.request_repaint();
                    }
                }
            }
            // tx drop でチャンネル切断 → drain 側が exited を検知する。
        });

        Ok(Self {
            master: pair.master,
            writer,
            child,
            rx,
            rows,
            cols,
            _integration: integration,
        })
    }

    /// 溜まっている出力を 1 つの `Vec` にまとめて返す。第 2 戻り値はシェル終了フラグ。
    fn drain(&mut self) -> (Vec<u8>, bool) {
        let mut out = Vec::new();
        let mut exited = false;
        loop {
            match self.rx.try_recv() {
                Ok(chunk) => out.extend_from_slice(&chunk),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    exited = true;
                    break;
                }
            }
        }
        (out, exited)
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        if rows == self.rows && cols == self.cols {
            return;
        }
        self.rows = rows;
        self.cols = cols;
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    fn send(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// 表示用パス（`~/work/foo` など）を実ファイルパスへ展開する。
/// 先頭 `~` は `$HOME` に置換。それ以外はそのまま `PathBuf` 化する。
pub fn expand_path(display: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if display == "~" || display == "~/" {
        return home;
    }
    if let Some(rest) = display.strip_prefix("~/") {
        return home.map(|h| h.join(rest));
    }
    if display.starts_with('/') {
        return Some(PathBuf::from(display));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::TermAction;

    /// 実シェルを起動し、`echo` の出力がブロックに流れるまでをエンドツーエンドで確認する。
    /// 実機（macOS + zsh/bash）でのみ意味があるため `#[ignore]`。`cargo test -- --ignored` で実行。
    #[test]
    #[ignore]
    fn real_shell_echo_reaches_block_output() {
        let ctx = egui::Context::default();
        let mut mgr = PtyManager::new();
        mgr.spawn(
            "t1",
            SpawnSpec {
                shell: Shell::Zsh,
                login_shell: true,
                cwd: std::env::var_os("HOME").map(PathBuf::from),
                rows: 24,
                cols: 80,
            },
            &ctx,
        );

        // シェル初期化（最初のプロンプト）を待ってからコマンドを送る。
        std::thread::sleep(std::time::Duration::from_millis(800));
        let _ = mgr.drain("t1");
        mgr.on_submit("t1", "echo tanaterm_marker");
        mgr.send("t1", b"echo tanaterm_marker\n");

        let mut text = String::new();
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let (actions, _) = mgr.drain("t1");
            for a in actions {
                if let TermAction::Append(spans) = a {
                    for s in spans {
                        text.push_str(&s.text);
                    }
                }
            }
            if text.contains("tanaterm_marker") {
                break;
            }
        }
        assert!(
            text.contains("tanaterm_marker"),
            "echo の出力がブロックに届くはず。実際の取り込み: {text:?}"
        );
    }
}
