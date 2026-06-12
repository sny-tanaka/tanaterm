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
    /// コマンド送信時の Unix epoch 秒。`running` 中の経過時間表示に使う。
    /// 既存データには無いフィールドなので、deserialize 時は 0 になる（UI 側で
    /// saturating_sub するため "0" でも経過秒数が壊れない）。
    #[serde(default)]
    pub started_at_unix: u64,
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
    /// alternate screen（vim / less 等）が表示中か（07: alt-screen 検知）。
    /// `true` の間は `Append` / `OverwriteLine` / `AppendDetached` を無視する。
    /// `EndBlock` 時に false に戻す（leave シーケンス取りこぼし保険）。
    #[serde(skip)]
    pub alt_screen: bool,
    /// シェルが終了（`exit` / Ctrl+D / spawn 失敗）したか（08: シェル終了検知）。
    /// `true` の間は入力を受け付けず、restart 導線を表示する。
    /// `restart_session` で false に戻す。
    #[serde(skip)]
    pub shell_exited: bool,
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
    /// 最終実行時刻（Unix epoch 秒）。実行されたコマンドだけが `Some` になり、
    /// RECENT セクションの対象＆並び順（新しい順）に使う。表示の "Xm ago" はここから算出。
    #[serde(default)]
    pub last_used: Option<u64>,
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
    /// 入力行から確定したユーザコマンドを送る。`on_submit()`（SGR リセット + 出力取り込み開始）を伴う。
    Send { id: String, bytes: Vec<u8> },
    /// 制御文字等の生バイトを実行中シェルへ転送する。`on_submit()` は呼ばない。
    SendRaw { id: String, bytes: Vec<u8> },
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
    /// ターミナル本文のフォントサイズ（pt）。⌘+/− で変更、⌘0 で config 既定にリセット。
    /// 8.0..=24.0 にクランプ。serde default は 13.0（起動時は Config.font_size で上書き）。
    #[serde(default = "default_font_size")]
    pub font_size: f32,
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
            font_size: default_font_size(),
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
    /// ⌘+/− で変更したフォントサイズ。0.0 は「未保存」として boot 時の Config 値を使う。
    #[serde(default)]
    pub font_size: f32,
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

fn default_font_size() -> f32 {
    13.0
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
            alt_screen: false,
            shell_exited: false,
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
            font_size: self.ui.font_size,
        }
    }

    /// 永続化サブセットから状態を復元する。各セッションの PTY 起動を `pending` に積む。
    /// セッションが 0 件なら [`Self::boot`] と同じく空 1 セッションで起動する。
    pub fn from_persistent(p: PersistentState) -> Self {
        if p.sessions.is_empty() {
            let mut state = Self::boot();
            state.ui.shell = p.shell;
            // font_size が 0.0 なら未保存（旧バージョンからの移行）なので default を使う。
            if p.font_size > 0.0 {
                state.ui.font_size = p.font_size;
            }
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
                alt_screen: false,
                shell_exited: false,
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

        // font_size が 0.0 なら未保存（旧バージョンからの移行）なので default を使う。
        let font_size = if p.font_size > 0.0 {
            p.font_size
        } else {
            default_font_size()
        };

        Self {
            sessions,
            shelf: p.shelf,
            commands,
            ui: UiState {
                active_session_id,
                rail_left_visible: p.rail_left_visible,
                rail_right_visible: p.rail_right_visible,
                shell: p.shell,
                font_size,
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
            alt_screen: false,
            shell_exited: false,
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
    ///
    /// 実際に閉じた場合は `true`、最後の 1 セッションのため閉じなかった場合は `false` を返す。
    /// 呼び出し元は `false` の時に「最後のセッションは閉じられません」toast を表示する（10）。
    pub fn close_session(&mut self, id: &str) -> bool {
        if self.sessions.len() <= 1 {
            return false;
        }
        let Some(pos) = self.sessions.iter().position(|s| s.id == id) else {
            return false;
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
        true
    }

    /// アクティブセッションに生バイト列を `PendingPty::SendRaw` として積む。
    ///
    /// Ctrl+C(0x03) などの制御文字を実行中シェルへ転送する用途。`Send` と違い
    /// `on_submit()`（SGR リセット・出力取り込み開始）は発火しない。
    /// アクティブセッションがない / `bytes` が空 / `shell_exited` な場合は何もしない。
    pub fn push_pty_send_raw(&mut self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        let Some(id) = self.ui.active_session_id.clone() else {
            return;
        };
        // シェル終了済みセッションへの送信は無視する（08: 幽霊 running ブロック防止）。
        if self.sessions.iter().any(|s| s.id == id && s.shell_exited) {
            return;
        }
        self.pending.push(PendingPty::SendRaw { id, bytes });
    }

    /// アクティブセッションの input_buffer を、実行中プロセスの stdin として
    /// 改行付きでそのまま転送する。`submit_input` と違い:
    /// - 新規ブロックを積まない / `running` 状態を変えない
    /// - `on_submit()`（SGR リセット）を呼ばない
    /// - 空 input でも `\n` だけは送る（シェルでの空 Enter と同等）
    ///
    /// 呼び出し元は busy（直近ブロックが `running`）を確認した上で呼ぶ前提。
    /// idle 中に呼ぶと裸の `\n` が PTY に流れて UI と PTY 状態がずれるため、
    /// 公開範囲を crate 内に絞っている。
    /// シェル終了済みセッションには no-op（08: 幽霊 running ブロック防止）。
    pub(crate) fn submit_input_as_stdin(&mut self) {
        // shell_exited なら何もしない（08）。
        if self.active().is_some_and(|s| s.shell_exited) {
            return;
        }
        let Some(session) = self.active_mut() else {
            return;
        };
        let buf = std::mem::take(&mut session.input_buffer);
        let mut bytes = buf.into_bytes();
        bytes.push(b'\n');
        self.push_pty_send_raw(bytes);
    }

    /// アクティブセッションの input_buffer を 1 コマンドとして実 PTY に送る。
    ///
    /// 空入力は無視。実行中ブロック（`running` = true, exit 未確定）を作って末尾に積み、
    /// セッションを Busy にし、`"<cmd>\n"` の送信を `pending` に積む。実出力・exit code は
    /// PTY 応答を [`Self::apply_term_action`] が後から流し込む。
    /// シェル終了済みセッションには no-op（08: 幽霊 running ブロック防止）。
    pub fn submit_input(&mut self, now_hhmm: String) {
        // shell_exited なら何もしない（08）。
        if self.active().is_some_and(|s| s.shell_exited) {
            return;
        }
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
            started_at_unix: crate::clock::now_unix(),
            running: true,
            exit_code: None,
            output: Vec::new(),
        });
        session.status = SessionStatus::Busy;
        self.pending.push(PendingPty::Send {
            id,
            bytes: format!("{cmd}\n").into_bytes(),
        });
        self.record_recent_command(&cmd, crate::clock::now_unix());
    }

    /// 実行されたコマンドを RECENT に記録する。同じ cmd が既にあれば（pinned 含め）
    /// `last_used` を更新し、無ければ未 pin の recent エントリを足す。RECENT は上限 `MAX_RECENT`
    /// 件に保ち、超えたら最も古い未 pin エントリを捨てる。
    fn record_recent_command(&mut self, cmd: &str, now_secs: u64) {
        const MAX_RECENT: usize = 30;
        if let Some(c) = self.commands.iter_mut().find(|c| c.cmd == cmd) {
            c.last_used = Some(now_secs);
            c.uses = c.uses.saturating_add(1);
            return;
        }
        let id = self.fresh_id("c");
        self.commands.push(Command {
            id,
            cmd: cmd.to_string(),
            desc: None,
            pinned: false,
            uses: 1,
            last_used: Some(now_secs),
        });
        // RECENT（未 pin かつ last_used あり）が上限を超えたら最古を 1 件捨てる。
        let recent = self
            .commands
            .iter()
            .filter(|c| !c.pinned && c.last_used.is_some())
            .count();
        if recent > MAX_RECENT {
            if let Some((idx, _)) = self
                .commands
                .iter()
                .enumerate()
                .filter(|(_, c)| !c.pinned && c.last_used.is_some())
                .min_by_key(|(_, c)| c.last_used)
            {
                self.commands.remove(idx);
            }
        }
    }

    /// shelf 項目を削除する。rename 中ならキャンセルしてから削除する。
    pub fn remove_shelf(&mut self, id: &str) {
        if self.is_renaming_shelf(id) {
            self.cancel_rename();
        }
        self.shelf.retain(|s| s.id != id);
    }

    /// 指定セッションの末尾の実行中ブロックへの可変参照。
    fn running_block_mut(&mut self, id: &str) -> Option<&mut Block> {
        let session = self.sessions.iter_mut().find(|s| s.id == id)?;
        session.blocks.iter_mut().rev().find(|b| b.running)
    }

    /// PTY 由来の [`crate::term::TermAction`] を該当セッションに適用する。
    pub fn apply_term_action(&mut self, id: &str, action: crate::term::TermAction) {
        use crate::term::TermAction;
        // alt_screen 中は Append / OverwriteLine / AppendDetached を無視する（07）。
        let is_alt_screen = self
            .sessions
            .iter()
            .find(|s| s.id == id)
            .is_some_and(|s| s.alt_screen);
        match action {
            TermAction::Append(spans) => {
                if is_alt_screen {
                    return;
                }
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
                // EndBlock 時は alt_screen を false に戻す（leave 取りこぼし保険）。
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
                    s.alt_screen = false;
                }
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
            TermAction::OverwriteLine => {
                if is_alt_screen {
                    return;
                }
                if let Some(b) = self.running_block_mut(id) {
                    truncate_after_last_newline(&mut b.output);
                }
            }
            TermAction::AppendDetached(spans) => {
                if is_alt_screen {
                    return;
                }
                let session = self.sessions.iter_mut().find(|s| s.id == id);
                let Some(session) = session else { return };
                let Some(last_block) = session.blocks.last_mut() else {
                    return; // ブロックが 0 件なら捨てる。
                };
                // 末尾ブロックの出力が改行で終わっていなければ先頭に `\n` を補う。
                let needs_newline = last_block
                    .output
                    .last()
                    .is_some_and(|s| !s.text.ends_with('\n'));
                if needs_newline {
                    last_block.output.push(OutputSpan {
                        color: OutputColor::Dim,
                        text: "\n".to_string(),
                    });
                }
                last_block.output.extend(spans);
            }
            TermAction::AltScreen(b) => {
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
                    s.alt_screen = b;
                }
            }
        }
    }

    /// alternate screen が表示中かどうかを返す。`central.rs` のバナー表示に使う。
    pub fn is_alt_screen(&self, id: &str) -> bool {
        self.sessions
            .iter()
            .find(|s| s.id == id)
            .is_some_and(|s| s.alt_screen)
    }

    /// シェルが終了（チャンネル切断 / spawn 失敗）したセッションを Idle にし、実行中ブロックを閉じる。
    ///
    /// `shell_exited = true` にすることで、入力行の TextEdit を無効化して
    /// restart 導線に切り替える（08: シェル終了検知）。
    pub fn mark_session_exited(&mut self, id: &str) {
        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
            s.status = SessionStatus::Idle;
            s.shell_exited = true;
            for b in s.blocks.iter_mut() {
                b.running = false;
            }
        }
    }

    /// 終了したシェルを再起動する（08: restart 導線）。
    ///
    /// `shell_exited` を false に戻し、status を Live にして
    /// `PendingPty::Spawn` を積む。PTY ハンドルは exited 時に close 済みなので
    /// `PtyManager::spawn` が新規起動する。
    pub fn restart_session(&mut self, id: &str) {
        let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) else {
            return;
        };
        s.shell_exited = false;
        s.status = SessionStatus::Live;
        let cwd = s
            .cwd
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| s.pwd.clone());
        self.pending.push(PendingPty::Spawn {
            id: id.to_string(),
            cwd,
        });
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
            last_used: None,
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

    /// ターミナルフォントサイズを 8.0..=24.0 にクランプして設定する。
    pub fn set_font_size(&mut self, size: f32) {
        self.ui.font_size = size.clamp(8.0, 24.0);
    }

    /// 現在のフォントサイズに `delta` を加算して [`Self::set_font_size`] へ渡す。
    pub fn adjust_font_size(&mut self, delta: f32) {
        let new = self.ui.font_size + delta;
        self.set_font_size(new);
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

/// `\r` 上書き（プログレスバー）用: ブロック出力の最終行（最後の `'\n'` より後ろ）を削る。
///
/// - span を末尾から走査し、`'\n'` を含む span を見つけたらその span 内の
///   最後の `'\n'` 直後まで残して切り詰める。
/// - `'\n'` が見つからないまま先頭に達したら output 全体をクリアする。
/// - 空になった span は除去する。
pub(crate) fn truncate_after_last_newline(output: &mut Vec<OutputSpan>) {
    // 末尾から \n を探す。
    let len = output.len();
    for i in (0..len).rev() {
        if let Some(pos) = output[i].text.rfind('\n') {
            // この span の最後の \n 直後まで残す。
            output[i].text.truncate(pos + 1);
            // それより後ろの span を全て削除する。
            output.truncate(i + 1);
            // text が空になった span（\n だけなら空にならないが念のため）を除去する。
            // ただし \n を含む場合はその span は残す（text = "\n" は空でない）。
            output.retain(|s| !s.text.is_empty());
            return;
        }
    }
    // \n が見つからない → output 全体をクリア（最初の行だけで上書きが始まるケース）。
    output.clear();
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
                        started_at_unix: 0,
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
                        started_at_unix: 0,
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
                        started_at_unix: crate::clock::now_unix(),
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
                alt_screen: false,
                shell_exited: false,
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
                    started_at_unix: crate::clock::now_unix(),
                    running: true,
                    exit_code: None,
                    output: vec![span(OutputColor::Dim, "Streaming logs…")],
                }],
                cwd: None,
                input_buffer: String::new(),
                history: vec!["docker compose logs -f api".into()],
                history_cursor: None,
                alt_screen: false,
                shell_exited: false,
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
                alt_screen: false,
                shell_exited: false,
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
                alt_screen: false,
                shell_exited: false,
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
                alt_screen: false,
                shell_exited: false,
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

    /// 既定の PINNED コマンド。RECENT は実行履歴から動的に埋まるので seed では空。
    pub(super) fn commands() -> Vec<Command> {
        vec![
            command("c1", "df -h", "disk free by mount", 412),
            command("c2", "cd -", "previous directory", 88),
            command("c3", "ps aux | grep $name", "find a process", 54),
            command("c4", "pnpm dev", "start dev server", 201),
            command("c5", "docker compose logs -f $svc", "tail service logs", 73),
            command("c6", "kubectl get pods -A", "all pods, all namespaces", 32),
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

    /// 既定の PINNED コマンド 1 件（pinned=true, last_used=None）。
    fn command(id: &str, cmd: &str, desc: &str, uses: u32) -> Command {
        Command {
            id: id.into(),
            cmd: cmd.into(),
            desc: Some(desc.into()),
            pinned: true,
            uses,
            last_used: None,
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
        // 最後の 1 つになるまで閉じる（それ以外は true を返す）。
        for id in &ids[..ids.len() - 1] {
            assert!(
                s.close_session(id),
                "最後以外の close_session は true を返す"
            );
        }
        assert_eq!(s.sessions.len(), 1, "下準備として 1 セッションになるはず");
        let last_id = s.sessions[0].id.clone();
        // 最後の 1 セッションは閉じない → false を返す（10: 戻り値の assert）。
        let result = s.close_session(&last_id);
        assert!(!result, "最後の 1 つを閉じようとすると false が返る");
        assert_eq!(s.sessions.len(), 1, "最後の 1 つは閉じられない");
        assert_eq!(s.ui.active_session_id.as_deref(), Some(last_id.as_str()));
    }

    #[test]
    fn close_session_picks_neighbor_as_active() {
        let mut s = fresh();
        // active を s2 にして s2 を閉じると、削除位置にある旧 s3 がアクティブになる。
        s.focus_session("s2");
        let result = s.close_session("s2");
        assert!(result, "s2 を閉じると true を返す");
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
    fn push_pty_send_raw_queues_bytes_to_active_session() {
        let mut s = fresh();
        s.focus_session("s2");
        s.pending.clear();
        s.push_pty_send_raw(vec![0x03]);
        assert_eq!(
            s.pending.last(),
            Some(&PendingPty::SendRaw {
                id: "s2".into(),
                bytes: vec![0x03],
            })
        );
    }

    #[test]
    fn push_pty_send_raw_is_noop_without_active_session() {
        let mut s = fresh();
        s.ui.active_session_id = None;
        s.pending.clear();
        s.push_pty_send_raw(vec![0x03]);
        assert!(s.pending.is_empty(), "アクティブ無しなら積まない");
    }

    #[test]
    fn push_pty_send_raw_is_noop_for_empty_bytes() {
        let mut s = fresh();
        s.focus_session("s2");
        s.pending.clear();
        s.push_pty_send_raw(Vec::new());
        assert!(s.pending.is_empty(), "空バイト列は積まない");
    }

    #[test]
    fn submit_input_as_stdin_sends_buffer_with_newline_via_send_raw() {
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "y".into();
        }
        s.pending.clear();
        let before_block_count = s.active().unwrap().blocks.len();
        s.submit_input_as_stdin();
        // SendRaw で "y\n" が積まれる（Send ではない = on_submit 副作用なし）
        assert_eq!(
            s.pending.last(),
            Some(&PendingPty::SendRaw {
                id: "s3".into(),
                bytes: b"y\n".to_vec(),
            })
        );
        // 新規ブロックは積まれない
        assert_eq!(s.active().unwrap().blocks.len(), before_block_count);
        // input_buffer はクリアされる
        assert!(s.active().unwrap().input_buffer.is_empty());
    }

    #[test]
    fn submit_input_as_stdin_sends_lone_newline_for_empty_buffer() {
        let mut s = fresh();
        s.focus_session("s3");
        s.pending.clear();
        s.submit_input_as_stdin();
        assert_eq!(
            s.pending.last(),
            Some(&PendingPty::SendRaw {
                id: "s3".into(),
                bytes: b"\n".to_vec(),
            })
        );
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
    fn record_recent_command_adds_and_dedups() {
        let mut s = fresh();
        let pinned_before = s.commands.iter().filter(|c| c.pinned).count();
        s.record_recent_command("npm test", 1000);
        let recent: Vec<&Command> = s
            .commands
            .iter()
            .filter(|c| !c.pinned && c.last_used.is_some())
            .collect();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].cmd, "npm test");
        assert_eq!(recent[0].last_used, Some(1000));

        // 同じ cmd を再実行すると新規追加せず last_used を更新する。
        s.record_recent_command("npm test", 2000);
        let recent_after = s
            .commands
            .iter()
            .filter(|c| !c.pinned && c.last_used.is_some())
            .count();
        assert_eq!(recent_after, 1, "重複は作らない");
        let c = s.commands.iter().find(|c| c.cmd == "npm test").unwrap();
        assert_eq!(c.last_used, Some(2000));
        assert_eq!(c.uses, 2);
        // pinned 件数は変わらない。
        assert_eq!(
            s.commands.iter().filter(|c| c.pinned).count(),
            pinned_before
        );
    }

    #[test]
    fn record_recent_command_bumps_existing_pinned() {
        let mut s = fresh();
        // seed の pinned コマンド "df -h" を実行 → 新規 recent は作らず last_used 更新。
        let recent_before = s.commands.iter().filter(|c| !c.pinned).count();
        s.record_recent_command("df -h", 5000);
        assert_eq!(
            s.commands.iter().filter(|c| !c.pinned).count(),
            recent_before
        );
        let c = s.commands.iter().find(|c| c.cmd == "df -h").unwrap();
        assert!(c.pinned, "pinned のまま");
        assert_eq!(c.last_used, Some(5000));
    }

    #[test]
    fn submit_input_records_recent_command() {
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "echo hi".into();
        }
        s.submit_input("12:00".into());
        assert!(
            s.commands.iter().any(|c| c.cmd == "echo hi" && !c.pinned),
            "実行コマンドが recent に記録される"
        );
    }

    #[test]
    fn remove_shelf_deletes_item() {
        let mut s = fresh();
        let before = s.shelf.len();
        assert!(s.shelf.iter().any(|x| x.id == "f1"));
        s.remove_shelf("f1");
        assert_eq!(s.shelf.len(), before - 1);
        assert!(!s.shelf.iter().any(|x| x.id == "f1"));
    }

    #[test]
    fn remove_shelf_cancels_rename_for_removed_item() {
        let mut s = fresh();
        s.start_shelf_rename("f1");
        assert!(s.ui.rename_target.is_some());
        s.remove_shelf("f1");
        assert!(
            s.ui.rename_target.is_none(),
            "削除対象の rename は破棄される"
        );
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
        // seed は c1..c6 が pin 済み。c1 を未 pin に倒して往復を確認（c2 は pin 維持）。
        s.commands.iter_mut().find(|c| c.id == "c1").unwrap().pinned = false;

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
                .find(|c| c.id == "c2")
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

    // ── 04: truncate_after_last_newline テスト ─────────────────────────────────

    /// 複数 span に \n が含まれる場合、最後の \n 直後まで残してそれ以降を削る。
    #[test]
    fn truncate_after_last_newline_multi_span() {
        use crate::state::truncate_after_last_newline;
        let mut output = vec![
            OutputSpan {
                color: OutputColor::Default,
                text: "x\n".to_string(),
            },
            OutputSpan {
                color: OutputColor::Default,
                text: "y".to_string(),
            },
        ];
        truncate_after_last_newline(&mut output);
        // "x\n" までが残り "y" は削除される。
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].text, "x\n");
    }

    /// 単一 span で \n なし → output 全体クリア。
    #[test]
    fn truncate_after_last_newline_no_newline_clears() {
        use crate::state::truncate_after_last_newline;
        let mut output = vec![OutputSpan {
            color: OutputColor::Default,
            text: "abc".to_string(),
        }];
        truncate_after_last_newline(&mut output);
        assert!(output.is_empty(), "\\n なし → output クリア");
    }

    /// span 内の最後の \n 直後で切り詰める（span 途中の \n）。
    #[test]
    fn truncate_after_last_newline_cuts_within_span() {
        use crate::state::truncate_after_last_newline;
        let mut output = vec![OutputSpan {
            color: OutputColor::Default,
            text: "line1\nline2".to_string(),
        }];
        truncate_after_last_newline(&mut output);
        // "line1\n" まで残る。
        assert_eq!(output[0].text, "line1\n");
    }

    // ── 04: OverwriteLine state テスト ────────────────────────────────────────

    /// 実行中ブロック出力が "x\ny" で OverwriteLine → "x\n" になる。
    #[test]
    fn apply_term_action_overwrite_line() {
        use crate::term::TermAction;
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "echo".into();
        }
        s.submit_input("12:00".into());
        s.apply_term_action(
            "s3",
            TermAction::Append(vec![
                OutputSpan {
                    color: OutputColor::Default,
                    text: "x\n".into(),
                },
                OutputSpan {
                    color: OutputColor::Default,
                    text: "y".into(),
                },
            ]),
        );
        s.apply_term_action("s3", TermAction::OverwriteLine);
        let block = s.active().unwrap().blocks.last().unwrap();
        let text: String = block.output.iter().map(|sp| sp.text.as_str()).collect();
        assert_eq!(text, "x\n", "OverwriteLine 後は 'x\\n' のみ残る");
    }

    // ── 05: AppendDetached state テスト ───────────────────────────────────────

    /// ブロックが存在する時に AppendDetached が最終ブロックへ Dim で追記される。
    #[test]
    fn apply_term_action_append_detached_to_last_block() {
        use crate::term::TermAction;
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "sleep 5 &".into();
        }
        s.submit_input("12:00".into());
        // ブロックを完了状態にする。
        s.apply_term_action("s3", TermAction::EndBlock { exit: Some(0) });
        // AppendDetached を適用する。
        s.apply_term_action(
            "s3",
            TermAction::AppendDetached(vec![OutputSpan {
                color: OutputColor::Dim,
                text: "[1]  + done sleep 5\n".into(),
            }]),
        );
        let block = s.active().unwrap().blocks.last().unwrap();
        let text: String = block.output.iter().map(|sp| sp.text.as_str()).collect();
        assert!(
            text.contains("[1]  + done sleep 5"),
            "AppendDetached が最終ブロックへ追記される: {text:?}",
        );
        // Dim 色であること。
        assert!(
            block
                .output
                .last()
                .is_some_and(|sp| sp.color == OutputColor::Dim),
            "AppendDetached は Dim 色: {:?}",
            block.output.last(),
        );
    }

    /// ブロック 0 件では AppendDetached が何もしない。
    #[test]
    fn apply_term_action_append_detached_no_blocks_noop() {
        use crate::term::TermAction;
        let mut s = fresh();
        s.focus_session("s3"); // s3 は blocks: Vec::new()
        let block_count = s.active().unwrap().blocks.len();
        s.apply_term_action(
            "s3",
            TermAction::AppendDetached(vec![OutputSpan {
                color: OutputColor::Dim,
                text: "orphan\n".into(),
            }]),
        );
        assert_eq!(
            s.active().unwrap().blocks.len(),
            block_count,
            "ブロック 0 件では何も起きない",
        );
    }

    // ── 08: shell_exited / restart_session テスト ─────────────────────────────

    /// `mark_session_exited` 後の `submit_input` は no-op（幽霊 running ブロック防止）。
    #[test]
    fn submit_input_is_noop_after_shell_exited() {
        let mut s = fresh();
        s.focus_session("s3");
        s.mark_session_exited("s3");
        let blocks_before = s.active().unwrap().blocks.len();
        s.pending.clear();
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "ls".into();
        }
        s.submit_input("12:00".into());
        assert_eq!(
            s.active().unwrap().blocks.len(),
            blocks_before,
            "shell_exited 中は新規ブロックを積まない"
        );
        assert!(s.pending.is_empty(), "shell_exited 中は pending も積まない");
    }

    /// `mark_session_exited` 後の `submit_input_as_stdin` は no-op。
    #[test]
    fn submit_input_as_stdin_is_noop_after_shell_exited() {
        let mut s = fresh();
        s.focus_session("s3");
        s.mark_session_exited("s3");
        s.pending.clear();
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "y".into();
        }
        s.submit_input_as_stdin();
        assert!(s.pending.is_empty(), "shell_exited 中は SendRaw も積まない");
    }

    /// `mark_session_exited` 後の `push_pty_send_raw` は no-op。
    #[test]
    fn push_pty_send_raw_is_noop_after_shell_exited() {
        let mut s = fresh();
        s.focus_session("s3");
        s.mark_session_exited("s3");
        s.pending.clear();
        s.push_pty_send_raw(vec![0x03]);
        assert!(s.pending.is_empty(), "shell_exited 中は SendRaw も積まない");
    }

    /// `restart_session` で `shell_exited` が false に戻り、`PendingPty::Spawn` が積まれ、
    /// status が Live になること。
    #[test]
    fn restart_session_resets_state_and_queues_spawn() {
        let mut s = fresh();
        s.focus_session("s3");
        s.mark_session_exited("s3");
        assert!(s.active().unwrap().shell_exited, "終了済み状態になっている");
        assert_eq!(s.active().unwrap().status, SessionStatus::Idle);
        s.pending.clear();
        s.restart_session("s3");
        assert!(
            !s.active().unwrap().shell_exited,
            "restart で shell_exited が false に戻る"
        );
        assert_eq!(
            s.active().unwrap().status,
            SessionStatus::Live,
            "restart で status が Live になる"
        );
        assert_eq!(s.pending.len(), 1, "Spawn が 1 件積まれる");
        assert!(
            matches!(s.pending[0], PendingPty::Spawn { ref id, .. } if id == "s3"),
            "積まれる PendingPty は Spawn"
        );
    }

    // ── 07: AltScreen state テスト ────────────────────────────────────────────

    /// AltScreen(true) 中の Append が無視される。
    #[test]
    fn apply_term_action_alt_screen_suppresses_append() {
        use crate::term::TermAction;
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "vim".into();
        }
        s.submit_input("12:00".into());
        // AltScreen に入る。
        s.apply_term_action("s3", TermAction::AltScreen(true));
        // この Append は無視されるはず。
        s.apply_term_action(
            "s3",
            TermAction::Append(vec![OutputSpan {
                color: OutputColor::Default,
                text: "vim output".into(),
            }]),
        );
        let block = s.active().unwrap().blocks.last().unwrap();
        assert!(
            block.output.is_empty(),
            "AltScreen 中は Append が無視される"
        );

        // AltScreen(false) で解除後は Append が届く。
        s.apply_term_action("s3", TermAction::AltScreen(false));
        s.apply_term_action(
            "s3",
            TermAction::Append(vec![OutputSpan {
                color: OutputColor::Default,
                text: "after vim".into(),
            }]),
        );
        let block = s.active().unwrap().blocks.last().unwrap();
        let text: String = block.output.iter().map(|sp| sp.text.as_str()).collect();
        assert_eq!(text, "after vim", "AltScreen 解除後は Append が届く");
    }

    /// EndBlock で alt_screen が false に戻る。
    #[test]
    fn apply_term_action_end_block_resets_alt_screen() {
        use crate::term::TermAction;
        let mut s = fresh();
        s.focus_session("s3");
        if let Some(sess) = s.active_mut() {
            sess.input_buffer = "vim".into();
        }
        s.submit_input("12:00".into());
        s.apply_term_action("s3", TermAction::AltScreen(true));
        assert!(
            s.active().unwrap().alt_screen,
            "AltScreen true にセットされる"
        );
        s.apply_term_action("s3", TermAction::EndBlock { exit: Some(0) });
        assert!(
            !s.active().unwrap().alt_screen,
            "EndBlock で alt_screen が false に戻る"
        );
    }

    // ── 11: font_size クランプ / adjust テスト ───────────────────────────────

    /// 下限（7.0 → 8.0）と上限（30.0 → 24.0）のクランプ。
    #[test]
    fn set_font_size_clamps_to_range() {
        let mut s = fresh();
        s.set_font_size(7.0);
        assert_eq!(s.ui.font_size, 8.0, "下限 7.0 は 8.0 にクランプされる");
        s.set_font_size(30.0);
        assert_eq!(s.ui.font_size, 24.0, "上限 30.0 は 24.0 にクランプされる");
    }

    /// adjust_font_size: 加算・減算がクランプ込みで動く。
    #[test]
    fn adjust_font_size_adds_and_clamps() {
        let mut s = fresh();
        s.set_font_size(13.0);
        s.adjust_font_size(1.0);
        assert_eq!(s.ui.font_size, 14.0, "+1.0 → 14.0");
        s.adjust_font_size(-3.0);
        assert_eq!(s.ui.font_size, 11.0, "-3.0 → 11.0");
        // 上限超え
        s.set_font_size(23.0);
        s.adjust_font_size(5.0);
        assert_eq!(s.ui.font_size, 24.0, "上限超えはクランプ");
    }

    /// PersistentState 往復で font_size が保存・復元される。
    #[test]
    fn persistent_roundtrip_preserves_font_size() {
        let mut s = fresh();
        s.set_font_size(16.0);
        let p = s.to_persistent();
        assert_eq!(p.font_size, 16.0, "to_persistent に font_size が含まれる");
        let restored = AppState::from_persistent(p);
        assert_eq!(
            restored.ui.font_size, 16.0,
            "from_persistent で font_size が復元される"
        );
    }
}
