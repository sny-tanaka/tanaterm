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
/// 出力は `term::SgrConverter` が ANSI(SGR) を解釈して `OutputSpan` 列に変換したもの。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    pub cmd: String,
    pub pwd: String,
    /// 表示用の時刻文字列 ("14:02" 等)。
    pub time: String,
    /// 出力取り込み中（amber border + Busy）。OSC 133 の C〜D 間、または heuristic で
    /// 次コマンド送信までが `true`。
    #[serde(default)]
    pub running: bool,
    /// 終了コード。`Some(0)` = 正常、`Some(非0)` = err バッジ。
    /// `None` は「実行中」または「heuristic で exit 不明のまま終了」。`running` と併せて判断する。
    pub exit_code: Option<i32>,
    pub output: Vec<OutputSpan>,
}

/// ターミナル出力の 1 色スパン。`tanaterm.css` の `.block .out .X` クラスに対応。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSpan {
    pub color: OutputColor,
    pub text: String,
}

/// 出力色クラス。CSS の `.g`/`.r`/`.a`/`.b`/`.m`/`.d` と、無印（fg-1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputColor {
    /// 既定: `--fg-1`。
    #[default]
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
    /// OSC 7 で追跡する実 cwd（Tab 補完・表示用）。未取得なら `None`。
    #[serde(skip)]
    pub cwd: Option<std::path::PathBuf>,
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

/// inline rename / inline 編集の対象（セッション名 / shelf ラベル / command 文字列）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameTarget {
    Session(String),
    Shelf(String),
    Command(String),
}

/// UI 側で発生した PTY 操作要求。`app.rs` が毎フレーム drain して [`crate::pty::PtyManager`] に適用する。
///
/// UI 層（`&mut AppState` しか持たない）を `PtyManager` から疎結合に保つためのキュー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingPty {
    /// 新規セッションのシェルを起動する。`cwd` は表示パス（`~/work/foo` 等）。
    Spawn { id: String, cwd: String },
    /// セッションへバイト列（コマンド / キー入力）を送る。
    Send { id: String, bytes: Vec<u8> },
    /// セッションのシェルを終了する。
    Close { id: String },
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
    /// COMMANDS の inline 追加フォームが開いているか。
    #[serde(skip)]
    pub command_add_active: bool,
    /// 追加フォームの cmd 入力バッファ。
    #[serde(skip)]
    pub command_add_cmd: String,
    /// 追加フォームの desc 入力バッファ。
    #[serde(skip)]
    pub command_add_desc: String,
    /// 追加フォームを開いた直後の 1 フレームだけ cmd 欄に focus を要求するフラグ。
    #[serde(skip)]
    pub command_add_focus_pending: bool,
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
            command_add_active: false,
            command_add_cmd: String::new(),
            command_add_desc: String::new(),
            command_add_focus_pending: false,
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

/// 起動間で永続化する状態のサブセット（Phase 3）。
///
/// `eframe::App::save` で app config dir（`app.ron`）に保存し、起動時に復元する。
/// blocks / 入力バッファ / cwd / toast 等の実行時データは持たない（再起動でリセット）。
/// theme / accent / density は固定なので対象外。SESSIONS/PINNED のリサイズ高さは
/// 別途 eframe の egui memory 永続化が担う（ここでは扱わない）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistentState {
    #[serde(default)]
    pub sessions: Vec<PersistentSession>,
    #[serde(default)]
    pub active_session_id: Option<String>,
    #[serde(default)]
    pub shelf: Vec<ShelfItem>,
    /// commands 一覧（追加/編集/pin を含めて丸ごと保存する）。空なら seed を使う。
    #[serde(default)]
    pub commands: Vec<Command>,
    #[serde(default = "default_true")]
    pub rail_left_visible: bool,
    #[serde(default = "default_true")]
    pub rail_right_visible: bool,
    #[serde(default)]
    pub shell: crate::config::Shell,
}

/// 永続化するセッションの最小情報（name / pwd / pinned）。blocks は復元しない。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentSession {
    pub id: String,
    pub name: String,
    pub pwd: String,
    #[serde(default)]
    pub pinned: bool,
}

fn default_true() -> bool {
    true
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
    /// UI 側で発生した PTY 操作要求のキュー。`app.rs` が毎フレーム drain する。
    #[serde(skip)]
    pub pending: Vec<PendingPty>,
}

fn default_id_counter() -> u64 {
    100
}

/// `"s105"` / `"f3"` のような「英字プレフィックス + 数字」id 群から数値接尾辞の最大値を返す
/// （採番カウンタ復元用）。この形式以外（手編集による不正 id 等）は parse 失敗で無視する。
fn max_id_suffix<'a>(ids: impl Iterator<Item = &'a str>) -> Option<u64> {
    ids.filter_map(|id| {
        id.trim_start_matches(|c: char| !c.is_ascii_digit())
            .parse::<u64>()
            .ok()
    })
    .max()
}

/// 実パスの先頭が `$HOME` なら `~` に畳んだ表示文字列を返す。
fn collapse_home(real: &str) -> String {
    if let Some(home) = std::env::var_os("HOME").and_then(|h| h.into_string().ok()) {
        if real == home {
            return "~".to_string();
        }
        if let Some(rest) = real.strip_prefix(&format!("{home}/")) {
            return format!("~/{rest}");
        }
    }
    real.to_string()
}

/// Tab 補完で探索するディレクトリを決める。`dir_part` が絶対 / `~` ならそれを優先、
/// 相対なら `base`（セッション cwd）からの相対で解決する。
fn resolve_dir(base: &std::path::Path, dir_part: &str) -> std::path::PathBuf {
    if dir_part.is_empty() {
        return base.to_path_buf();
    }
    if let Some(expanded) = crate::pty::expand_path(dir_part) {
        return expanded;
    }
    base.join(dir_part)
}

/// 文字列群の共通接頭辞を返す（Tab 補完の複数候補時）。
fn common_prefix<'a>(mut iter: impl Iterator<Item = &'a str>) -> String {
    let Some(first) = iter.next() else {
        return String::new();
    };
    let mut prefix = first.to_string();
    for s in iter {
        while !s.starts_with(&prefix) {
            prefix.pop();
            if prefix.is_empty() {
                return prefix;
            }
        }
    }
    prefix
}

impl AppState {
    /// 実 PTY 起動用の初期状態（Phase 2 本番）。
    ///
    /// セッションは空の 1 本だけ（仕様メモ "空セッションで起動" に揃える）で、その PTY 起動を
    /// `pending` に積む。shelf / commands は seed カタログをそのまま使う。
    pub fn boot() -> Self {
        let id = "s1".to_string();
        let pwd = "~/".to_string();
        let session = Session {
            id: id.clone(),
            name: "session".into(),
            pwd: pwd.clone(),
            status: SessionStatus::Live,
            pinned: false,
            remote: None,
            node: None,
            blocks: Vec::new(),
            cwd: None,
            input_buffer: String::new(),
            history: Vec::new(),
            history_cursor: None,
        };
        Self {
            sessions: vec![session],
            shelf: seed::shelf(),
            commands: seed::commands(),
            ui: UiState {
                active_session_id: Some(id.clone()),
                ..UiState::default()
            },
            next_id: default_id_counter(),
            pending: vec![PendingPty::Spawn { id, cwd: pwd }],
        }
    }

    /// 現在状態から永続化サブセットを抽出する（`eframe::App::save` 用）。
    pub fn to_persistent(&self) -> PersistentState {
        PersistentState {
            sessions: self
                .sessions
                .iter()
                .map(|s| PersistentSession {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    pwd: s.pwd.clone(),
                    pinned: s.pinned,
                })
                .collect(),
            active_session_id: self.ui.active_session_id.clone(),
            shelf: self.shelf.clone(),
            commands: self.commands.clone(),
            rail_left_visible: self.ui.rail_left_visible,
            rail_right_visible: self.ui.rail_right_visible,
            shell: self.ui.shell,
        }
    }

    /// 永続化サブセットから状態を復元する。各セッションの PTY 起動を `pending` に積む。
    /// セッションが 0 件なら [`Self::boot`] と同じく空 1 セッションで起動する。
    pub fn from_persistent(p: PersistentState) -> Self {
        if p.sessions.is_empty() {
            let mut state = Self::boot();
            state.ui.shell = p.shell;
            return state;
        }

        let sessions: Vec<Session> = p
            .sessions
            .iter()
            .map(|ps| Session {
                id: ps.id.clone(),
                name: ps.name.clone(),
                pwd: ps.pwd.clone(),
                status: SessionStatus::Live,
                pinned: ps.pinned,
                remote: None,
                node: None,
                blocks: Vec::new(),
                cwd: None,
                input_buffer: String::new(),
                history: Vec::new(),
                history_cursor: None,
            })
            .collect();

        // commands は保存済みがあればそれを、無ければ seed を使う（追加/編集/pin 込みで保存）。
        let commands = if p.commands.is_empty() {
            seed::commands()
        } else {
            p.commands
        };

        // active が消えていたら先頭にフォールバック。
        let active_session_id = p
            .active_session_id
            .filter(|id| sessions.iter().any(|s| &s.id == id))
            .or_else(|| sessions.first().map(|s| s.id.clone()));

        // 復元 id（s/f/c の数値接尾辞）と衝突しないよう採番カウンタを進める。
        let next_id = max_id_suffix(
            sessions
                .iter()
                .map(|s| s.id.as_str())
                .chain(p.shelf.iter().map(|s| s.id.as_str()))
                .chain(commands.iter().map(|c| c.id.as_str())),
        )
        .map_or_else(default_id_counter, |m| (m + 1).max(default_id_counter()));

        let pending = sessions
            .iter()
            .map(|s| PendingPty::Spawn {
                id: s.id.clone(),
                cwd: s.pwd.clone(),
            })
            .collect();

        Self {
            sessions,
            shelf: p.shelf,
            commands,
            ui: UiState {
                active_session_id,
                rail_left_visible: p.rail_left_visible,
                rail_right_visible: p.rail_right_visible,
                shell: p.shell,
                ..UiState::default()
            },
            next_id,
            pending,
        }
    }

    /// `tanaterm-data.jsx` 相当の seed フィクスチャ（mock）。
    /// ユニットテスト用。実 PTY は起動しない（本番は [`Self::boot`]）。
    #[cfg(test)]
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
            pending: Vec::new(),
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

    /// 新規セッションを末尾に追加して active にし、PTY 起動を `pending` に積む。
    /// `label` は表示名、`pwd` は初期 cwd（shelf クリック時はそのパス、`⌘T` 時は `~/`）。
    pub fn new_session(&mut self, label: impl Into<String>, pwd: impl Into<String>) -> String {
        let id = self.fresh_id("s");
        let pwd = pwd.into();
        let session = Session {
            id: id.clone(),
            name: label.into(),
            pwd: pwd.clone(),
            status: SessionStatus::Live,
            pinned: false,
            remote: None,
            node: None,
            blocks: Vec::new(),
            cwd: None,
            input_buffer: String::new(),
            history: Vec::new(),
            history_cursor: None,
        };
        self.sessions.push(session);
        self.ui.active_session_id = Some(id.clone());
        self.pending.push(PendingPty::Spawn {
            id: id.clone(),
            cwd: pwd,
        });
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
        self.pending.push(PendingPty::Close { id: id.to_string() });
    }

    /// アクティブセッションの input_buffer を 1 コマンドとして実 PTY に送る。
    ///
    /// 空入力は無視。実行中ブロック（`running` = true, exit 未確定）を作って末尾に積み、
    /// セッションを Busy にし、`"<cmd>\n"` の送信を `pending` に積む。実出力・exit code は
    /// PTY 応答を [`Self::apply_term_action`] が後から流し込む。
    pub fn submit_input(&mut self, now_hhmm: String) {
        let block_id = self.fresh_id("b");
        let Some(session) = self.active_mut() else {
            return;
        };
        let cmd = session.input_buffer.trim().to_string();
        if cmd.is_empty() {
            return;
        }
        let id = session.id.clone();
        let pwd = session.pwd.clone();
        session.input_buffer.clear();
        session.history_cursor = None;
        if session.history.last().map(String::as_str) != Some(cmd.as_str()) {
            session.history.push(cmd.clone());
        }
        // heuristic モードで前ブロックが running のまま残っていたら exit 不明で閉じる。
        for b in session.blocks.iter_mut() {
            if b.running {
                b.running = false;
            }
        }
        session.blocks.push(Block {
            id: block_id,
            cmd: cmd.clone(),
            pwd,
            time: now_hhmm,
            running: true,
            exit_code: None,
            output: Vec::new(),
        });
        session.status = SessionStatus::Busy;
        self.pending.push(PendingPty::Send {
            id,
            bytes: format!("{cmd}\n").into_bytes(),
        });
    }

    /// 指定セッションの末尾の実行中ブロックへの可変参照。
    fn running_block_mut(&mut self, id: &str) -> Option<&mut Block> {
        let session = self.sessions.iter_mut().find(|s| s.id == id)?;
        session.blocks.iter_mut().rev().find(|b| b.running)
    }

    /// PTY 由来の [`crate::term::TermAction`] を該当セッションに適用する。
    pub fn apply_term_action(&mut self, id: &str, action: crate::term::TermAction) {
        use crate::term::TermAction;
        match action {
            TermAction::Append(spans) => {
                if let Some(b) = self.running_block_mut(id) {
                    b.output.extend(spans);
                }
            }
            TermAction::ClearOutput => {
                if let Some(b) = self.running_block_mut(id) {
                    b.output.clear();
                }
            }
            TermAction::EndBlock { exit } => {
                if let Some(b) = self.running_block_mut(id) {
                    b.running = false;
                    b.exit_code = exit;
                }
                let status = match exit {
                    Some(c) if c != 0 => SessionStatus::Err,
                    _ => SessionStatus::Live,
                };
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
                    s.status = status;
                }
            }
            TermAction::SetCwd(path) => self.set_session_cwd(id, &path),
        }
    }

    /// シェルが終了（チャンネル切断）したセッションを Idle にし、実行中ブロックを閉じる。
    pub fn mark_session_exited(&mut self, id: &str) {
        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
            s.status = SessionStatus::Idle;
            for b in s.blocks.iter_mut() {
                b.running = false;
            }
        }
    }

    /// OSC 7 で得た実 cwd をセッションに反映する。表示 pwd は `$HOME` を `~` に畳む。
    fn set_session_cwd(&mut self, id: &str, real: &str) {
        let display = collapse_home(real);
        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
            s.cwd = Some(std::path::PathBuf::from(real));
            s.pwd = display;
        }
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
                if next >= n {
                    None
                } else {
                    Some(next)
                }
            }
        };
        session.history_cursor = new_cursor;
        session.input_buffer = match new_cursor {
            Some(i) => session.history[i].clone(),
            None => String::new(),
        };
    }

    /// 入力行末尾のトークンをパスとして最小補完する（2.6 の "Tab 補完 最小"）。
    ///
    /// 実シェルの補完は使わず、セッションの実 cwd を基準にファイル/ディレクトリ名を補う。
    /// 候補が 1 つならフルに、複数なら共通接頭辞まで補完。ディレクトリには `/` を付ける。
    pub fn tab_complete(&mut self) {
        let Some(session) = self.active() else {
            return;
        };
        let base = session
            .cwd
            .clone()
            .or_else(|| crate::pty::expand_path(&session.pwd));
        let Some(base) = base else { return };
        let buffer = session.input_buffer.clone();

        // 末尾トークン（空白区切り）の開始位置を求める。
        let token_start = buffer.rfind(char::is_whitespace).map_or(0, |i| i + 1);
        let token = &buffer[token_start..];

        // token を「ディレクトリ部」と「補完接頭辞」に分ける。
        let (dir_part, prefix) = match token.rfind('/') {
            Some(i) => (&token[..=i], &token[i + 1..]),
            None => ("", token),
        };
        let search_dir = resolve_dir(&base, dir_part);
        let Ok(entries) = std::fs::read_dir(&search_dir) else {
            return;
        };

        let mut matches: Vec<(String, bool)> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                if name.starts_with(prefix) {
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    Some((name, is_dir))
                } else {
                    None
                }
            })
            .collect();
        matches.sort();
        if matches.is_empty() {
            return;
        }

        let completion = if matches.len() == 1 {
            let (name, is_dir) = &matches[0];
            if *is_dir {
                format!("{name}/")
            } else {
                name.clone()
            }
        } else {
            common_prefix(matches.iter().map(|(n, _)| n.as_str()))
        };
        if completion.len() <= prefix.len() {
            return; // これ以上補完できない。
        }

        let new_token = format!("{dir_part}{completion}");
        if let Some(s) = self.active_mut() {
            s.input_buffer = format!("{}{}", &buffer[..token_start], new_token);
            s.history_cursor = None;
        }
    }

    /// 右 rail のコマンドをアクティブセッションの input_buffer に挿入する。
    /// 既存入力があっても上書きする（仕様: "insert" 挙動）。
    pub fn insert_command(&mut self, cmd_id: &str, now: f64) {
        let Some(text) = self
            .commands
            .iter()
            .find(|c| c.id == cmd_id)
            .map(|c| c.cmd.clone())
        else {
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

    /// command の inline 編集（cmd 文字列）を開始する。
    pub fn start_command_rename(&mut self, cmd_id: &str) {
        if let Some(c) = self.commands.iter().find(|c| c.id == cmd_id) {
            self.ui.rename_target = Some(RenameTarget::Command(cmd_id.into()));
            self.ui.rename_buffer = c.cmd.clone();
            self.ui.rename_focus_pending = true;
        }
    }

    /// この command が inline 編集中か。
    pub fn is_renaming_command(&self, id: &str) -> bool {
        matches!(&self.ui.rename_target, Some(RenameTarget::Command(s)) if s == id)
    }

    /// COMMANDS の inline 追加フォームを開く。
    pub fn start_command_add(&mut self) {
        if self.ui.rename_target.is_some() {
            self.commit_rename();
        }
        self.ui.command_add_active = true;
        self.ui.command_add_cmd.clear();
        self.ui.command_add_desc.clear();
        self.ui.command_add_focus_pending = true;
    }

    /// 追加フォームの内容を新規 command として確定する。cmd が空なら追加せず閉じる。
    /// 手動追加した command は「手元に残したい」ものとみなして pinned=true で PINNED に置く。
    pub fn commit_command_add(&mut self) {
        let cmd = std::mem::take(&mut self.ui.command_add_cmd)
            .trim()
            .to_string();
        let desc = std::mem::take(&mut self.ui.command_add_desc)
            .trim()
            .to_string();
        self.ui.command_add_active = false;
        self.ui.command_add_focus_pending = false;
        if cmd.is_empty() {
            return;
        }
        let id = self.fresh_id("c");
        self.commands.push(Command {
            id,
            cmd,
            desc: (!desc.is_empty()).then_some(desc),
            pinned: true,
            uses: 0,
            when: None,
        });
    }

    /// 追加フォームを破棄して閉じる。
    pub fn cancel_command_add(&mut self) {
        self.ui.command_add_active = false;
        self.ui.command_add_cmd.clear();
        self.ui.command_add_desc.clear();
        self.ui.command_add_focus_pending = false;
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
            RenameTarget::Command(id) => {
                if let Some(c) = self.commands.iter_mut().find(|c| c.id == id) {
                    c.cmd = trimmed.to_string();
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
    #[cfg(test)]
    use super::{Block, OutputColor, OutputSpan, Session, SessionStatus};
    use super::{Command, ShelfItem};

    #[cfg(test)]
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
                        running: false,
                        exit_code: Some(0),
                        output: vec![
                            span(OutputColor::Dim, "total 48\n"),
                            span(OutputColor::Azure, "drwxr-xr-x"),
                            span(OutputColor::Dim, "  12 tanaka  staff   384 May 21 14:01 "),
                            span(OutputColor::Default, ".\n"),
                            span(OutputColor::Azure, "drwxr-xr-x"),
                            span(OutputColor::Dim, "   4 tanaka  staff   128 May 18 09:12 "),
                            span(OutputColor::Default, "..\n"),
                            span(
                                OutputColor::Dim,
                                "-rw-r--r--   1 tanaka  staff  1234 May 21 13:58 ",
                            ),
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
                        running: false,
                        exit_code: Some(0),
                        output: vec![
                            span(
                                OutputColor::Dim,
                                "> tanaterm@0.4.1 typecheck\n> tsc --noEmit\n\n",
                            ),
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
                        running: true,
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
                cwd: None,
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
                    running: true,
                    exit_code: None,
                    output: vec![span(OutputColor::Dim, "Streaming logs…")],
                }],
                cwd: None,
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
                cwd: None,
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
                cwd: None,
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
                cwd: None,
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
            shelf_item(
                "f3",
                "frontend-lp",
                "~/work/mitsucari/frontend-lp",
                "work",
                None,
                34,
            ),
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
            command(
                "c3",
                "ps aux | grep $name",
                "find a process",
                true,
                54,
                None,
            ),
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
            command(
                "c10",
                "pnpm typecheck",
                "tsc --noEmit",
                false,
                14,
                Some("2m"),
            ),
            command(
                "c11",
                "curl -I $url",
                "check response headers",
                false,
                9,
                Some("6m"),
            ),
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
            command(
                "c14",
                "lsof -i :5173",
                "who's on the port",
                false,
                2,
                Some("34m"),
            ),
            command(
                "c15",
                "tar -czf bundle.tgz dist/",
                "create gzip archive",
                false,
                2,
                Some("1h"),
            ),
            command(
                "c16",
                "caffeinate -di",
                "keep mac awake",
                false,
                1,
                Some("2h"),
            ),
            command(
                "c17",
                "history | tail -50",
                "last 50 history entries",
                false,
                1,
                Some("3h"),
            ),
        ]
    }

    #[cfg(test)]
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
        assert!(
            s.ui.rename_target.is_none(),
            "削除対象の rename は破棄される"
        );
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
        let original = s
            .sessions
            .iter()
            .find(|x| x.id == "s5")
            .unwrap()
            .name
            .clone();
        s.start_session_rename("s5");
        s.ui.rename_buffer = "   ".into();
        s.commit_rename();
        let s5 = s.sessions.iter().find(|x| x.id == "s5").unwrap();
        assert_eq!(s5.name, original, "空文字 rename は名前変更しない");
        assert!(s.ui.rename_target.is_none(), "rename 状態は閉じる");
    }

    #[test]
    fn submit_input_does_not_duplicate_consecutive_history() {
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "ls".into();
        }
        s.submit_input("12:00".into());
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "ls".into();
        }
        s.submit_input("12:01".into());
        // 履歴は "ls" 1 件のみ。
        assert_eq!(s.active().unwrap().history, vec!["ls".to_string()]);
        // ブロックは 2 つ追加される。
        assert_eq!(s.active().unwrap().blocks.len(), 2);
    }

    #[test]
    fn submit_input_creates_running_block_and_queues_send() {
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "ls -la".into();
        }
        s.pending.clear();
        s.submit_input("12:00".into());
        let block = s.active().unwrap().blocks.last().unwrap();
        assert!(block.running, "送信直後は実行中");
        assert_eq!(block.exit_code, None);
        assert_eq!(s.active().unwrap().status, SessionStatus::Busy);
        assert_eq!(
            s.pending.last(),
            Some(&PendingPty::Send {
                id: "s3".into(),
                bytes: b"ls -la\n".to_vec()
            })
        );
    }

    #[test]
    fn submit_input_ignores_blank_input() {
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "   ".into();
        }
        s.submit_input("12:00".into());
        assert!(s.active().unwrap().blocks.is_empty());
        assert!(s.active().unwrap().history.is_empty());
    }

    #[test]
    fn apply_term_action_appends_and_ends_block() {
        use crate::term::TermAction;
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "echo hi".into();
        }
        s.submit_input("12:00".into());
        s.apply_term_action(
            "s3",
            TermAction::Append(vec![OutputSpan {
                color: OutputColor::Default,
                text: "hi\n".into(),
            }]),
        );
        s.apply_term_action("s3", TermAction::EndBlock { exit: Some(0) });
        let block = s.active().unwrap().blocks.last().unwrap();
        assert!(!block.running);
        assert_eq!(block.exit_code, Some(0));
        assert_eq!(block.output.len(), 1);
        assert_eq!(s.active().unwrap().status, SessionStatus::Live);
    }

    #[test]
    fn apply_term_action_nonzero_exit_sets_err() {
        use crate::term::TermAction;
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "false".into();
        }
        s.submit_input("12:00".into());
        s.apply_term_action("s3", TermAction::EndBlock { exit: Some(1) });
        assert_eq!(s.active().unwrap().status, SessionStatus::Err);
        assert_eq!(
            s.active().unwrap().blocks.last().unwrap().exit_code,
            Some(1)
        );
    }

    #[test]
    fn set_cwd_updates_display_and_real() {
        use crate::term::TermAction;
        let mut s = fresh();
        s.focus_session("s3");
        // HOME 配下は ~ に畳まれる。
        if let Some(home) = std::env::var_os("HOME").and_then(|h| h.into_string().ok()) {
            s.apply_term_action("s3", TermAction::SetCwd(format!("{home}/work/x")));
            assert_eq!(s.active().unwrap().pwd, "~/work/x");
            assert_eq!(
                s.active().unwrap().cwd,
                Some(std::path::PathBuf::from(format!("{home}/work/x")))
            );
        }
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
    fn commit_command_add_appends_pinned_command() {
        let mut s = fresh();
        s.start_command_add();
        s.ui.command_add_cmd = "  git status  ".into();
        s.ui.command_add_desc = "  working tree  ".into();
        s.commit_command_add();
        let added = s.commands.last().unwrap();
        assert_eq!(added.cmd, "git status", "trim される");
        assert_eq!(added.desc.as_deref(), Some("working tree"));
        assert!(added.pinned, "手動追加は pinned");
        assert!(!s.ui.command_add_active);
    }

    #[test]
    fn commit_command_add_ignores_empty_cmd() {
        let mut s = fresh();
        let before = s.commands.len();
        s.start_command_add();
        s.ui.command_add_cmd = "   ".into();
        s.commit_command_add();
        assert_eq!(s.commands.len(), before, "空 cmd は追加されない");
        assert!(!s.ui.command_add_active);
    }

    #[test]
    fn command_rename_updates_cmd_string() {
        let mut s = fresh();
        s.start_command_rename("c1");
        assert!(s.is_renaming_command("c1"));
        s.ui.rename_buffer = "  df -h --total  ".into();
        s.commit_rename();
        let c1 = s.commands.iter().find(|c| c.id == "c1").unwrap();
        assert_eq!(c1.cmd, "df -h --total");
    }

    #[test]
    fn persistent_roundtrip_preserves_added_command() {
        let mut s = fresh();
        s.start_command_add();
        s.ui.command_add_cmd = "my custom cmd".into();
        s.commit_command_add();
        let added_id = s.commands.last().unwrap().id.clone();

        let restored = AppState::from_persistent(s.to_persistent());
        let found = restored.commands.iter().find(|c| c.id == added_id);
        assert!(found.is_some(), "追加 command が再起動後も残る");
        assert_eq!(found.unwrap().cmd, "my custom cmd");
    }

    #[test]
    fn persistent_roundtrip_preserves_sessions_and_ui() {
        let mut s = fresh();
        s.focus_session("s2");
        s.ui.rail_left_visible = false;
        s.ui.shell = crate::config::Shell::Bash;
        // seed では c1 が pin 済み・c10 が未 pin。状態を反転させて往復を確認。
        s.commands.iter_mut().find(|c| c.id == "c1").unwrap().pinned = false;
        s.commands
            .iter_mut()
            .find(|c| c.id == "c10")
            .unwrap()
            .pinned = true;

        let restored = AppState::from_persistent(s.to_persistent());

        let ids: Vec<&str> = restored.sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["s1", "s2", "s3", "s4", "s5"]);
        assert_eq!(restored.ui.active_session_id.as_deref(), Some("s2"));
        assert!(!restored.ui.rail_left_visible);
        assert_eq!(restored.ui.shell, crate::config::Shell::Bash);
        // pin 状態が往復する。
        assert!(
            !restored
                .commands
                .iter()
                .find(|c| c.id == "c1")
                .unwrap()
                .pinned
        );
        assert!(
            restored
                .commands
                .iter()
                .find(|c| c.id == "c10")
                .unwrap()
                .pinned
        );
        // blocks は復元しない（実行時データ）。
        assert!(restored.sessions.iter().all(|s| s.blocks.is_empty()));
        // 各セッションの PTY 起動が積まれる。
        assert_eq!(restored.pending.len(), 5);
        assert!(restored
            .pending
            .iter()
            .all(|p| matches!(p, PendingPty::Spawn { .. })));
    }

    #[test]
    fn from_persistent_empty_falls_back_to_single_session() {
        let p = PersistentState {
            shell: crate::config::Shell::Bash,
            ..Default::default()
        };
        let restored = AppState::from_persistent(p);
        assert_eq!(restored.sessions.len(), 1);
        assert_eq!(restored.ui.shell, crate::config::Shell::Bash);
    }

    #[test]
    fn from_persistent_advances_next_id_past_restored_ids() {
        let p = PersistentState {
            sessions: vec![PersistentSession {
                id: "s150".into(),
                name: "old".into(),
                pwd: "~/".into(),
                pinned: false,
            }],
            ..Default::default()
        };
        let mut restored = AppState::from_persistent(p);
        // 復元 id (150) と衝突しない採番になる。
        let new_id = restored.new_session("x", "~/");
        let suffix: u64 = new_id.trim_start_matches('s').parse().unwrap();
        assert!(suffix > 150, "新規 id {new_id} は復元 id を超える");
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
