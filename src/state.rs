//! UI 状態モデル。
//!
//! ハンドオフ (`tmp/design_handoff_tanaterm/README.md` の "State Management" 節)
//! および `tanaterm-data.jsx` の seed を Rust 側で表現する。
//!
//! Phase 0 ではこれらの構造体はまだ実 PTY と接続されておらず、
//! `seed()` が返す mock データを Phase 1 の UI 描画に流す前提。
//! PTY 配線は Phase 2 で `Session.pty_id` 経由で行う。

use serde::{Deserialize, Serialize};

/// Session の表示状態。status dot の色に対応。
///
/// - `Live`: prompt 待ち（sage）
/// - `Busy`: コマンド実行中（amber, パルス）
/// - `Err`: 直近 exit が非0（rust）
/// - `Idle`: バックグラウンド（fg-3）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Live,
    Busy,
    Err,
    Idle,
}

/// 1 つのコマンド実行＝1 ブロック。
///
/// Phase 0 では `output` は手書きの色スパン (`OutputSpan`) で持ち、
/// Phase 2 で `vt100::Parser` 由来の cell 群に差し替える。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    pub cmd: String,
    pub pwd: String,
    /// 表示用の時刻文字列 ("14:02" 等)。
    /// Phase 2 で `started_at: SystemTime` に差し替え。
    pub time: String,
    /// `None` = 実行中。`Some(0)` = 正常終了。
    pub exit_code: Option<i32>,
    pub output: Vec<OutputSpan>,
}

/// ターミナル出力の 1 色スパン。`tanaterm.css` の `.block .out .X` クラスに対応。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSpan {
    pub color: OutputColor,
    pub text: String,
}

/// 出力色クラス。CSS の `.g`/`.r`/`.a`/`.b`/`.m`/`.d` と、無印（fg-1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputColor {
    /// 既定: `--fg-1`。
    Default,
    /// `.g` sage（成功）。
    Sage,
    /// `.r` rust（エラー）。
    Rust,
    /// `.a` amber。
    Amber,
    /// `.b` azure（URL / 数値）。
    Azure,
    /// `.m` plum。
    Plum,
    /// `.d` fg-3（注釈）。
    Dim,
}

/// 1 つの開いている terminal セッション。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub pwd: String,
    pub status: SessionStatus,
    #[serde(default)]
    pub pinned: bool,
    /// ssh 等の remote セッションで "user@host" を入れる。ローカルなら None。
    #[serde(default)]
    pub remote: Option<String>,
    /// `node v22.16.0` のような runtime ラベル（任意）。
    #[serde(default)]
    pub node: Option<String>,
    /// このセッションで実行されたコマンドブロック群。新しいほど末尾。
    #[serde(default)]
    pub blocks: Vec<Block>,
    /// 入力行の現在のバッファ。
    #[serde(skip)]
    pub input_buffer: String,
    /// ↑↓ で辿るコマンド履歴（古いほど先頭、新しいほど末尾）。
    #[serde(skip)]
    pub history: Vec<String>,
    /// 履歴ブラウズ中の位置。`None` = 新規入力中、`Some(i)` = `history[i]` を編集中。
    #[serde(skip)]
    pub history_cursor: Option<usize>,
}

/// Shelf に登録された "お気に入りディレクトリ"。
///
/// クリックで新規セッションを spawn して `cd` する（Phase 2 で配線）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShelfItem {
    pub id: String,
    pub label: String,
    pub path: String,
    /// filter chip のタグ。"work" / "personal" / "remote" / 任意の文字列。
    pub tag: String,
    #[serde(default)]
    pub remote: Option<String>,
    #[serde(default)]
    pub uses: u32,
}

/// 右 rail の Commands エントリ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: String,
    pub cmd: String,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub uses: u32,
    /// "2m", "1h" のような表示用文字列。
    /// Phase 2 で `last_used_at: SystemTime` に差し替え予定。
    #[serde(default)]
    pub when: Option<String>,
}

/// inline rename の対象（セッション名 / shelf ラベル）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameTarget {
    Session(String),
    Shelf(String),
}

/// UI 状態（永続化対象は Phase 3 で抜き出す）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiState {
    pub active_session_id: Option<String>,
    pub rail_left_visible: bool,
    pub rail_right_visible: bool,
    /// 入力行で切り替えるシェル（Phase 2 で実 PTY の起動シェルに反映）。
    #[serde(default)]
    pub shell: crate::config::Shell,
    /// TopBar グローバル検索の入力値。
    #[serde(skip)]
    pub search_query: String,
    /// inline rename 中の対象（`None` ＝ rename 中でない）。session 名と shelf ラベル兼用。
    #[serde(skip)]
    pub rename_target: Option<RenameTarget>,
    /// inline rename のバッファ。
    #[serde(skip)]
    pub rename_buffer: String,
    /// rename 開始直後の 1 フレームだけ TextEdit に focus を要求するためのフラグ。
    /// `true` の間は request_focus を呼び、呼んだら false に戻す。
    /// これがないと、ユーザが他ウィジェットを click した瞬間に毎フレーム focus を奪い返す。
    #[serde(skip)]
    pub rename_focus_pending: bool,
    /// 表示中の toast（`ctx.input(|i| i.time)` 基準で TTL 経過後に消える）。
    #[serde(skip)]
    pub toast: Option<Toast>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            active_session_id: None,
            rail_left_visible: true,
            rail_right_visible: true,
            shell: crate::config::Shell::default(),
            search_query: String::new(),
            rename_target: None,
            rename_buffer: String::new(),
            rename_focus_pending: false,
            toast: None,
        }
    }
}

/// Toast 通知。main 領域の右下に表示し、`ttl_secs` 経過で自動消滅する。
#[derive(Debug, Clone)]
pub struct Toast {
    pub label: String,
    pub detail: Option<String>,
    /// `ctx.input(|i| i.time)` 基準の表示開始時刻（秒）。
    pub spawned_at: f64,
    pub ttl_secs: f64,
}

impl Toast {
    pub fn new(label: impl Into<String>, detail: Option<String>, now: f64) -> Self {
        Self {
            label: label.into(),
            detail,
            spawned_at: now,
            // 仕様の 1.4-1.8s 範囲の中央値。
            ttl_secs: 1.6,
        }
    }

    pub fn expired(&self, now: f64) -> bool {
        now - self.spawned_at >= self.ttl_secs
    }
}

/// アプリ全体の状態。Phase 1 ではこれを mock で埋めて UI を駆動する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub sessions: Vec<Session>,
    pub shelf: Vec<ShelfItem>,
    pub commands: Vec<Command>,
    pub ui: UiState,
    /// 実行時に生成する Session/Block の ID 採番カウンタ。
    /// seed の "s1".."s5" / "b1".."b4" と衝突しないよう 100 から始める。
    #[serde(skip, default = "default_id_counter")]
    pub next_id: u64,
}

fn default_id_counter() -> u64 {
    100
}

impl AppState {
    /// `tanaterm-data.jsx` 相当の seed フィクスチャ。
    /// Phase 1 UI のサインオフはこのデータに対して行う。
    pub fn seed() -> Self {
        let sessions = seed::sessions();
        let active_session_id = sessions.first().map(|s| s.id.clone());

        Self {
            sessions,
            shelf: seed::shelf(),
            commands: seed::commands(),
            ui: UiState {
                active_session_id,
                ..UiState::default()
            },
            next_id: default_id_counter(),
        }
    }

    fn fresh_id(&mut self, prefix: &str) -> String {
        let id = self.next_id;
        self.next_id += 1;
        format!("{prefix}{id}")
    }

    /// アクティブセッションへの参照。
    pub fn active(&self) -> Option<&Session> {
        let id = self.ui.active_session_id.as_deref()?;
        self.sessions.iter().find(|s| s.id == id)
    }

    /// アクティブセッションへの可変参照。
    pub fn active_mut(&mut self) -> Option<&mut Session> {
        let id = self.ui.active_session_id.clone()?;
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    /// 指定セッションをアクティブにする。rename 中なら確定してから切替。
    pub fn focus_session(&mut self, id: &str) {
        if self.ui.rename_target.is_some() {
            self.commit_rename();
        }
        if self.sessions.iter().any(|s| s.id == id) {
            self.ui.active_session_id = Some(id.into());
        }
    }

    /// 新規セッションを末尾に追加して active にする。`label` は表示名、
    /// `pwd` は初期 cwd（shelf クリック時はそのパス、`⌘T` 時は `~/`）。
    pub fn new_session(&mut self, label: impl Into<String>, pwd: impl Into<String>) -> String {
        let id = self.fresh_id("s");
        let session = Session {
            id: id.clone(),
            name: label.into(),
            pwd: pwd.into(),
            status: SessionStatus::Live,
            pinned: false,
            remote: None,
            node: None,
            blocks: Vec::new(),
            input_buffer: String::new(),
            history: Vec::new(),
            history_cursor: None,
        };
        self.sessions.push(session);
        self.ui.active_session_id = Some(id.clone());
        id
    }

    /// 指定セッションを閉じる。最後の 1 つは閉じない（仕様メモ "空セッションで起動" に揃える）。
    pub fn close_session(&mut self, id: &str) {
        if self.sessions.len() <= 1 {
            return;
        }
        let Some(pos) = self.sessions.iter().position(|s| s.id == id) else {
            return;
        };
        self.sessions.remove(pos);
        if self.ui.active_session_id.as_deref() == Some(id) {
            let next = self.sessions.get(pos).or_else(|| self.sessions.last());
            self.ui.active_session_id = next.map(|s| s.id.clone());
        }
        if self.is_renaming_session(id) {
            self.cancel_rename();
        }
    }

    /// アクティブセッションの input_buffer を 1 コマンドとして実行する（mock）。
    /// 何も追加せず空入力は無視。実行ブロックは exit 0 / 簡易出力で append する。
    pub fn run_active_input(&mut self, now_hhmm: String) {
        let block_id = self.fresh_id("b");
        let Some(session) = self.active_mut() else {
            return;
        };
        let cmd = session.input_buffer.trim().to_string();
        if cmd.is_empty() {
            return;
        }
        let pwd = session.pwd.clone();
        session.input_buffer.clear();
        session.history_cursor = None;
        if session.history.last().map(String::as_str) != Some(cmd.as_str()) {
            session.history.push(cmd.clone());
        }
        let block = Block {
            id: block_id,
            cmd: cmd.clone(),
            pwd,
            time: now_hhmm,
            // mock: 常に成功扱い。Phase 2 で実 PTY の exit code に差し替え。
            exit_code: Some(0),
            output: vec![OutputSpan {
                color: OutputColor::Dim,
                text: format!("(mock) ran `{cmd}`\n"),
            }],
        };
        session.blocks.push(block);
    }

    /// ↑↓ で履歴を辿る。`delta = -1` で 1 つ古い、`+1` で 1 つ新しい。
    /// 端を越えたら現在のバッファを保持しない（仕様: シェル準拠の単純動作）。
    pub fn step_history(&mut self, delta: i32) {
        let Some(session) = self.active_mut() else {
            return;
        };
        if session.history.is_empty() {
            return;
        }
        let n = session.history.len();
        let new_cursor: Option<usize> = match (session.history_cursor, delta) {
            (None, d) if d < 0 => Some(n - 1),
            (None, _) => None,
            (Some(i), d) if d < 0 => Some(i.saturating_sub(1)),
            (Some(i), _) => {
                let next = i + 1;
                if next >= n { None } else { Some(next) }
            }
        };
        session.history_cursor = new_cursor;
        session.input_buffer = match new_cursor {
            Some(i) => session.history[i].clone(),
            None => String::new(),
        };
    }

    /// 右 rail のコマンドをアクティブセッションの input_buffer に挿入する。
    /// 既存入力があっても上書きする（仕様: "insert" 挙動）。
    pub fn insert_command(&mut self, cmd_id: &str, now: f64) {
        let Some(text) = self.commands.iter().find(|c| c.id == cmd_id).map(|c| c.cmd.clone()) else {
            return;
        };
        if let Some(session) = self.active_mut() {
            session.input_buffer = text.clone();
            session.history_cursor = None;
        }
        self.show_toast(format!("Inserted {text}"), None, now);
    }

    /// コマンドの pin を反転する。
    pub fn toggle_command_pin(&mut self, cmd_id: &str) {
        if let Some(c) = self.commands.iter_mut().find(|c| c.id == cmd_id) {
            c.pinned = !c.pinned;
        }
    }

    /// session の inline rename を開始する。
    pub fn start_session_rename(&mut self, session_id: &str) {
        if let Some(s) = self.sessions.iter().find(|s| s.id == session_id) {
            self.ui.rename_target = Some(RenameTarget::Session(session_id.into()));
            self.ui.rename_buffer = s.name.clone();
            self.ui.rename_focus_pending = true;
        }
    }

    /// shelf ラベルの inline rename を開始する。
    pub fn start_shelf_rename(&mut self, shelf_id: &str) {
        if let Some(s) = self.shelf.iter().find(|s| s.id == shelf_id) {
            self.ui.rename_target = Some(RenameTarget::Shelf(shelf_id.into()));
            self.ui.rename_buffer = s.label.clone();
            self.ui.rename_focus_pending = true;
        }
    }

    /// このセッションが rename 中か。
    pub fn is_renaming_session(&self, id: &str) -> bool {
        matches!(&self.ui.rename_target, Some(RenameTarget::Session(s)) if s == id)
    }

    /// この shelf 項目が rename 中か。
    pub fn is_renaming_shelf(&self, id: &str) -> bool {
        matches!(&self.ui.rename_target, Some(RenameTarget::Shelf(s)) if s == id)
    }

    /// rename buffer を確定して対象（session 名 / shelf ラベル）に反映する。空文字は no-op。
    pub fn commit_rename(&mut self) {
        let Some(target) = self.ui.rename_target.take() else {
            return;
        };
        let new_name = std::mem::take(&mut self.ui.rename_buffer);
        self.ui.rename_focus_pending = false;
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return;
        }
        match target {
            RenameTarget::Session(id) => {
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
                    s.name = trimmed.to_string();
                }
            }
            RenameTarget::Shelf(id) => {
                if let Some(s) = self.shelf.iter_mut().find(|s| s.id == id) {
                    s.label = trimmed.to_string();
                }
            }
        }
    }

    /// rename を破棄する。
    pub fn cancel_rename(&mut self) {
        self.ui.rename_target = None;
        self.ui.rename_buffer.clear();
        self.ui.rename_focus_pending = false;
    }

    /// Toast を 1 つ表示する。すでに表示中の Toast は置き換える。
    pub fn show_toast(&mut self, label: impl Into<String>, detail: Option<String>, now: f64) {
        self.ui.toast = Some(Toast::new(label, detail, now));
    }

    /// TTL 過ぎた Toast を片付ける。update ループ毎に呼ぶ。
    pub fn tick_toast(&mut self, now: f64) {
        if let Some(t) = &self.ui.toast {
            if t.expired(now) {
                self.ui.toast = None;
            }
        }
    }

    /// 入力行のシェルを zsh↔bash でトグルする（mock）。
    pub fn toggle_shell(&mut self) {
        use crate::config::Shell;
        self.ui.shell = match self.ui.shell {
            Shell::Zsh => Shell::Bash,
            Shell::Bash => Shell::Zsh,
        };
    }

    /// アクティブセッションの現在 pwd を SHELF に追加する。
    pub fn add_active_to_shelf(&mut self, now: f64) {
        let Some(path) = self.active().map(|s| s.pwd.clone()) else {
            return;
        };
        // 末尾スラッシュを除いた最後のセグメントを label に。`~/` や `/` のような
        // ホーム/ルートは "home" にフォールバックする。
        let last = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        let label = if last.is_empty() || last == "~" {
            "home".to_string()
        } else {
            last.to_string()
        };
        let id = self.fresh_id("f");
        self.shelf.push(ShelfItem {
            id,
            label: label.clone(),
            path: path.clone(),
            tag: "work".into(),
            remote: None,
            uses: 0,
        });
        self.show_toast(format!("Added \"{label}\" to shelf"), Some(path), now);
    }
}

/// `tanaterm-data.jsx` の INITIAL_* に対応する seed データ。
mod seed {
    use super::{Block, Command, OutputColor, OutputSpan, Session, SessionStatus, ShelfItem};

    pub(super) fn sessions() -> Vec<Session> {
        vec![
            Session {
                id: "s1".into(),
                name: "tanaterm·dev".into(),
                pwd: "~/work/tanaterm".into(),
                status: SessionStatus::Live,
                pinned: true,
                remote: None,
                node: Some("v22.16.0".into()),
                blocks: vec![
                    Block {
                        id: "b1".into(),
                        cmd: "ls -la".into(),
                        pwd: "~/work/tanaterm".into(),
                        time: "14:02".into(),
                        exit_code: Some(0),
                        output: vec![
                            span(OutputColor::Dim, "total 48\n"),
                            span(OutputColor::Azure, "drwxr-xr-x"),
                            span(OutputColor::Dim, "  12 tanaka  staff   384 May 21 14:01 "),
                            span(OutputColor::Default, ".\n"),
                            span(OutputColor::Azure, "drwxr-xr-x"),
                            span(OutputColor::Dim, "   4 tanaka  staff   128 May 18 09:12 "),
                            span(OutputColor::Default, "..\n"),
                            span(OutputColor::Dim, "-rw-r--r--   1 tanaka  staff  1234 May 21 13:58 "),
                            span(OutputColor::Default, "Cargo.toml\n"),
                            span(OutputColor::Azure, "drwxr-xr-x"),
                            span(OutputColor::Dim, "   8 tanaka  staff   256 May 21 14:00 "),
                            span(OutputColor::Default, "src\n"),
                        ],
                    },
                    Block {
                        id: "b2".into(),
                        cmd: "pnpm typecheck".into(),
                        pwd: "~/work/tanaterm".into(),
                        time: "14:03".into(),
                        exit_code: Some(0),
                        output: vec![
                            span(OutputColor::Dim, "> tanaterm@0.4.1 typecheck\n> tsc --noEmit\n\n"),
                            span(OutputColor::Sage, "✓ "),
                            span(OutputColor::Default, "0 errors  · 3.42s\n"),
                        ],
                    },
                    Block {
                        id: "b3".into(),
                        cmd: "pnpm dev".into(),
                        pwd: "~/work/tanaterm".into(),
                        time: "14:04".into(),
                        // 実行中。
                        exit_code: None,
                        output: vec![
                            span(OutputColor::Dim, "> tanaterm@0.4.1 dev\n> vite\n\n"),
                            span(OutputColor::Amber, "  VITE v5.2 "),
                            span(OutputColor::Dim, " ready in 318 ms\n\n"),
                            span(OutputColor::Sage, "  ➜  "),
                            span(OutputColor::Default, "Local:   "),
                            span(OutputColor::Azure, "http://localhost:5173/\n"),
                            span(OutputColor::Sage, "  ➜  "),
                            span(OutputColor::Default, "Network: use --host to expose\n"),
                        ],
                    },
                ],
                input_buffer: String::new(),
                history: vec!["ls -la".into(), "pnpm typecheck".into(), "pnpm dev".into()],
                history_cursor: None,
            },
            Session {
                id: "s2".into(),
                name: "api·logs".into(),
                pwd: "~/work/api".into(),
                status: SessionStatus::Busy,
                pinned: false,
                remote: None,
                node: Some("v20.11.1".into()),
                blocks: vec![Block {
                    id: "b4".into(),
                    cmd: "docker compose logs -f api".into(),
                    pwd: "~/work/api".into(),
                    time: "13:48".into(),
                    exit_code: None,
                    output: vec![span(OutputColor::Dim, "Streaming logs…")],
                }],
                input_buffer: String::new(),
                history: vec!["docker compose logs -f api".into()],
                history_cursor: None,
            },
            Session {
                id: "s3".into(),
                name: "scratch".into(),
                pwd: "~/Downloads".into(),
                status: SessionStatus::Live,
                pinned: false,
                remote: None,
                node: None,
                blocks: Vec::new(),
                input_buffer: String::new(),
                history: Vec::new(),
                history_cursor: None,
            },
            Session {
                id: "s4".into(),
                name: "ssh prod-01".into(),
                pwd: "/var/log".into(),
                status: SessionStatus::Err,
                pinned: false,
                remote: Some("tanaka@prod-01".into()),
                node: None,
                blocks: Vec::new(),
                input_buffer: String::new(),
                history: Vec::new(),
                history_cursor: None,
            },
            Session {
                id: "s5".into(),
                name: "notes".into(),
                pwd: "~/notes".into(),
                status: SessionStatus::Idle,
                pinned: false,
                remote: None,
                node: None,
                blocks: Vec::new(),
                input_buffer: String::new(),
                history: Vec::new(),
                history_cursor: None,
            },
        ]
    }

    pub(super) fn shelf() -> Vec<ShelfItem> {
        vec![
            shelf_item("f1", "tanaterm", "~/work/tanaterm", "work", None, 142),
            shelf_item("f2", "api", "~/work/api", "work", None, 88),
            shelf_item("f3", "frontend-lp", "~/work/mitsucari/frontend-lp", "work", None, 34),
            shelf_item("f4", "dotfiles", "~/.config", "personal", None, 60),
            shelf_item("f5", "notes", "~/notes", "personal", None, 21),
            shelf_item("f6", "downloads", "~/Downloads", "personal", None, 7),
            shelf_item("f7", "prod logs", "/var/log", "remote", Some("prod-01"), 12),
            shelf_item("f8", "sandbox", "~/tmp/sandbox", "work", None, 3),
        ]
    }

    pub(super) fn commands() -> Vec<Command> {
        vec![
            // pinned
            command("c1", "df -h", "disk free by mount", true, 412, None),
            command("c2", "cd -", "previous directory", true, 88, None),
            command("c3", "ps aux | grep $name", "find a process", true, 54, None),
            command("c4", "pnpm dev", "start dev server", true, 201, None),
            command(
                "c5",
                "docker compose logs -f $svc",
                "tail service logs",
                true,
                73,
                None,
            ),
            command(
                "c6",
                "kubectl get pods -A",
                "all pods, all namespaces",
                true,
                32,
                None,
            ),
            // recent
            command("c10", "pnpm typecheck", "tsc --noEmit", false, 14, Some("2m")),
            command("c11", "curl -I $url", "check response headers", false, 9, Some("6m")),
            command(
                "c12",
                "rg --hidden -g '!node_modules'",
                "ripgrep, skip node_modules",
                false,
                5,
                Some("11m"),
            ),
            command(
                "c13",
                "find . -name '*.tsx' | xargs wc -l",
                "line count, all tsx",
                false,
                3,
                Some("22m"),
            ),
            command("c14", "lsof -i :5173", "who's on the port", false, 2, Some("34m")),
            command("c15", "tar -czf bundle.tgz dist/", "create gzip archive", false, 2, Some("1h")),
            command("c16", "caffeinate -di", "keep mac awake", false, 1, Some("2h")),
            command("c17", "history | tail -50", "last 50 history entries", false, 1, Some("3h")),
        ]
    }

    fn span(color: OutputColor, text: &str) -> OutputSpan {
        OutputSpan {
            color,
            text: text.to_string(),
        }
    }

    fn shelf_item(
        id: &str,
        label: &str,
        path: &str,
        tag: &str,
        remote: Option<&str>,
        uses: u32,
    ) -> ShelfItem {
        ShelfItem {
            id: id.into(),
            label: label.into(),
            path: path.into(),
            tag: tag.into(),
            remote: remote.map(str::to_string),
            uses,
        }
    }

    fn command(
        id: &str,
        cmd: &str,
        desc: &str,
        pinned: bool,
        uses: u32,
        when: Option<&str>,
    ) -> Command {
        Command {
            id: id.into(),
            cmd: cmd.into(),
            desc: Some(desc.into()),
            pinned,
            uses,
            when: when.map(str::to_string),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> AppState {
        AppState::seed()
    }

    #[test]
    fn close_session_refuses_to_remove_last_one() {
        let mut s = fresh();
        let ids: Vec<String> = s.sessions.iter().map(|x| x.id.clone()).collect();
        // 最後の 1 つになるまで閉じる
        for id in &ids[..ids.len() - 1] {
            s.close_session(id);
        }
        assert_eq!(s.sessions.len(), 1, "下準備として 1 セッションになるはず");
        let last_id = s.sessions[0].id.clone();
        s.close_session(&last_id);
        assert_eq!(s.sessions.len(), 1, "最後の 1 つは閉じられない");
        assert_eq!(s.ui.active_session_id.as_deref(), Some(last_id.as_str()));
    }

    #[test]
    fn close_session_picks_neighbor_as_active() {
        let mut s = fresh();
        // active を s2 にして s2 を閉じると、削除位置にある旧 s3 がアクティブになる。
        s.focus_session("s2");
        s.close_session("s2");
        assert_eq!(s.ui.active_session_id.as_deref(), Some("s3"));
    }

    #[test]
    fn close_session_cancels_rename_for_removed_session() {
        let mut s = fresh();
        s.start_session_rename("s2");
        assert!(s.ui.rename_target.is_some());
        s.close_session("s2");
        assert!(s.ui.rename_target.is_none(), "削除対象の rename は破棄される");
    }

    #[test]
    fn step_history_walks_back_and_forward_then_clears() {
        let mut s = fresh();
        // seed の s1 は ["ls -la", "pnpm typecheck", "pnpm dev"]
        s.focus_session("s1");

        // ↑ 1 回 → 末尾 "pnpm dev"
        s.step_history(-1);
        assert_eq!(s.active().unwrap().input_buffer, "pnpm dev");
        // ↑ 1 回 → "pnpm typecheck"
        s.step_history(-1);
        assert_eq!(s.active().unwrap().input_buffer, "pnpm typecheck");
        // ↓ 1 回 → "pnpm dev"
        s.step_history(1);
        assert_eq!(s.active().unwrap().input_buffer, "pnpm dev");
        // ↓ もう 1 回 → 履歴を抜けて空
        s.step_history(1);
        assert_eq!(s.active().unwrap().input_buffer, "");
        assert!(s.active().unwrap().history_cursor.is_none());
    }

    #[test]
    fn step_history_is_noop_on_empty_history() {
        let mut s = fresh();
        s.focus_session("s3"); // history が空のセッション
        s.step_history(-1);
        assert_eq!(s.active().unwrap().input_buffer, "");
        assert!(s.active().unwrap().history_cursor.is_none());
    }

    #[test]
    fn commit_rename_applies_trimmed_buffer() {
        let mut s = fresh();
        s.start_session_rename("s5");
        s.ui.rename_buffer = "  renamed  ".into();
        s.commit_rename();
        let s5 = s.sessions.iter().find(|x| x.id == "s5").unwrap();
        assert_eq!(s5.name, "renamed", "trim される");
        assert!(s.ui.rename_target.is_none());
        assert_eq!(s.ui.rename_buffer, "");
    }

    #[test]
    fn commit_rename_with_empty_buffer_is_noop_on_name() {
        let mut s = fresh();
        let original = s.sessions.iter().find(|x| x.id == "s5").unwrap().name.clone();
        s.start_session_rename("s5");
        s.ui.rename_buffer = "   ".into();
        s.commit_rename();
        let s5 = s.sessions.iter().find(|x| x.id == "s5").unwrap();
        assert_eq!(s5.name, original, "空文字 rename は名前変更しない");
        assert!(s.ui.rename_target.is_none(), "rename 状態は閉じる");
    }

    #[test]
    fn run_active_input_does_not_duplicate_consecutive_history() {
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "ls".into();
        }
        s.run_active_input("12:00".into());
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "ls".into();
        }
        s.run_active_input("12:01".into());
        // 履歴は "ls" 1 件のみ。
        assert_eq!(s.active().unwrap().history, vec!["ls".to_string()]);
        // ブロックは 2 つ追加される。
        assert_eq!(s.active().unwrap().blocks.len(), 2);
    }

    #[test]
    fn run_active_input_ignores_blank_input() {
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "   ".into();
        }
        s.run_active_input("12:00".into());
        assert!(s.active().unwrap().blocks.is_empty());
        assert!(s.active().unwrap().history.is_empty());
    }

    #[test]
    fn rename_focus_pending_is_set_once_per_start() {
        let mut s = fresh();
        s.start_session_rename("s5");
        assert!(s.ui.rename_focus_pending, "開始時に focus_pending が立つ");
        // 描画ループが消化済みのつもりで手動でクリアし、
        // commit/cancel 後にも立たないことを確認する。
        s.ui.rename_focus_pending = false;
        s.commit_rename();
        assert!(!s.ui.rename_focus_pending);

        s.start_session_rename("s5");
        assert!(s.ui.rename_focus_pending);
        s.cancel_rename();
        assert!(!s.ui.rename_focus_pending);
    }

    #[test]
    fn shelf_rename_applies_to_label() {
        let mut s = fresh();
        s.start_shelf_rename("f1");
        assert!(s.is_renaming_shelf("f1"));
        assert!(!s.is_renaming_session("f1"));
        s.ui.rename_buffer = "  my-proj  ".into();
        s.commit_rename();
        let f1 = s.shelf.iter().find(|x| x.id == "f1").unwrap();
        assert_eq!(f1.label, "my-proj");
        assert!(s.ui.rename_target.is_none());
    }

    #[test]
    fn add_active_to_shelf_appends_with_label() {
        let mut s = fresh();
        s.focus_session("s1"); // pwd = ~/work/tanaterm
        let before = s.shelf.len();
        s.add_active_to_shelf(0.0);
        assert_eq!(s.shelf.len(), before + 1);
        assert_eq!(s.shelf.last().unwrap().label, "tanaterm");
        assert_eq!(s.shelf.last().unwrap().path, "~/work/tanaterm");
    }
}
