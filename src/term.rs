//! ターミナル出力のブロック化レイヤ（Phase 2）。
//!
//! PTY の raw byte stream を、コマンド単位の「ブロック」に分解する。
//! `docs/block-boundary-spike.md` の Decision Log で確定した方式に従う:
//!
//! - **一次方式: OSC 133**（Final Term semantic prompts）。shell integration が
//!   `precmd`/`preexec` で `OSC 133;A/C/D` を発行する（`shell_integration.rs` が注入）。
//!   `C`(コマンド開始)〜`D`(コマンド終了; exit code 付き) の間だけを出力としてブロックに溜める。
//!   プロンプト本文・コマンドのエコー（`B`〜`C`）は描画しない（自前の prompt 行を出すため）。
//! - **保険: heuristic**。OSC 133 が一度も来ない場合のみ作動する縮退モード。
//!   OSC 7（cwd 通知）でディレクトリを追い、行頭 `$ `/`% `/`# ` をプロンプトとみなして
//!   ブロックを区切る。exit code は取れないため err バッジは出さない（仕様サブセット）。
//!
//! ANSI(SGR) 色は `OutputSpan`（既存 mock と同じ型）に変換し、`central.rs` の
//! 既存描画経路をそのまま使う。`\r` 上書きや全画面 TUI は忠実には扱わない（後続課題）。

use crate::state::{OutputColor, OutputSpan};

/// scanner が raw stream から取り出す論理イベント。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockEvent {
    /// `OSC 133;A` — プロンプト開始。
    PromptStart,
    /// `OSC 133;C` — コマンド実行開始。
    CommandStart,
    /// `OSC 133;D[;exit]` — コマンド終了。exit code が取れた場合のみ `Some`。
    CommandEnd { exit: Option<i32> },
    /// `OSC 7;file://host/path` — cwd 通知。`path` はデコード済みパス。
    Cwd(String),
    /// 上記以外の素のバイト列（vt100 出力 + SGR エスケープを含む）。
    Output(Vec<u8>),
}

/// OSC シーケンス（133 / 7）を剥がして `BlockEvent` 化する状態機械。
///
/// チャンク境界をまたいでも壊れないよう、ESC/OSC の途中状態を保持する。
/// OSC 以外のエスケープ（CSI の SGR など）は剥がさず `Output` にそのまま残し、
/// 色付けは `SgrConverter` 側に委ねる。
#[derive(Default)]
pub struct OscScanner {
    state: ScanState,
    /// OSC 本文（`133;D;0` のような `;` 区切り文字列）の蓄積。
    osc: Vec<u8>,
}

#[derive(Default, PartialEq, Eq)]
enum ScanState {
    #[default]
    Ground,
    /// ESC を 1 つ受けた。次が `]` なら OSC、それ以外は通常エスケープとして通す。
    Esc,
    /// OSC 本文を収集中（`ESC ]` の後）。
    Osc,
    /// OSC 収集中に ESC を受けた（ST = `ESC \` 候補）。
    OscEsc,
}

impl OscScanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// バイト列を処理して順序を保ったイベント列を返す。
    pub fn process(&mut self, bytes: &[u8]) -> Vec<BlockEvent> {
        let mut events = Vec::new();
        let mut out: Vec<u8> = Vec::new();

        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            match self.state {
                ScanState::Ground => {
                    if b == 0x1b {
                        self.state = ScanState::Esc;
                    } else {
                        out.push(b);
                    }
                    i += 1;
                }
                ScanState::Esc => {
                    if b == b']' {
                        // OSC 開始。導入子 `ESC ]` は出力に残さない。
                        self.state = ScanState::Osc;
                        self.osc.clear();
                        i += 1;
                    } else {
                        // OSC 以外のエスケープ。ESC ごと出力に戻して通常処理へ。
                        out.push(0x1b);
                        self.state = ScanState::Ground;
                        // b は再処理せずここで通常出力に積む（CSI 等は SgrConverter が解釈）。
                        out.push(b);
                        i += 1;
                    }
                }
                ScanState::Osc => {
                    match b {
                        0x07 => {
                            // BEL = ST。OSC 確定。
                            flush_out(&mut out, &mut events);
                            if let Some(ev) = parse_osc(&self.osc) {
                                events.push(ev);
                            }
                            self.state = ScanState::Ground;
                            i += 1;
                        }
                        0x1b => {
                            self.state = ScanState::OscEsc;
                            i += 1;
                        }
                        _ => {
                            self.osc.push(b);
                            i += 1;
                        }
                    }
                }
                ScanState::OscEsc => {
                    if b == b'\\' {
                        // ESC \ = ST。OSC 確定。
                        flush_out(&mut out, &mut events);
                        if let Some(ev) = parse_osc(&self.osc) {
                            events.push(ev);
                        }
                        self.state = ScanState::Ground;
                        i += 1;
                    } else {
                        // 不正終端。今の OSC を確定し、ESC から再解釈する。
                        flush_out(&mut out, &mut events);
                        if let Some(ev) = parse_osc(&self.osc) {
                            events.push(ev);
                        }
                        self.state = ScanState::Esc;
                        // i は進めない（この byte を Esc 状態で読み直す）。
                    }
                }
            }
        }

        flush_out(&mut out, &mut events);
        events
    }
}

/// 蓄積中の素バイト列を `Output` イベントとして 1 つ吐き出す（空なら何もしない）。
fn flush_out(out: &mut Vec<u8>, events: &mut Vec<BlockEvent>) {
    if !out.is_empty() {
        events.push(BlockEvent::Output(std::mem::take(out)));
    }
}

/// OSC 本文（`133;A` / `133;D;0` / `7;file://host/path` 等）をイベント化する。
/// 未対応の OSC（タイトル設定 `0;`/`2;` 等）は `None`（＝破棄）。
fn parse_osc(body: &[u8]) -> Option<BlockEvent> {
    let s = std::str::from_utf8(body).ok()?;
    let mut parts = s.split(';');
    let code = parts.next()?;
    match code {
        "133" => match parts.next()? {
            "A" => Some(BlockEvent::PromptStart),
            "C" => Some(BlockEvent::CommandStart),
            "D" => {
                let exit = parts.next().and_then(|e| e.parse::<i32>().ok());
                Some(BlockEvent::CommandEnd { exit })
            }
            // B（プロンプト終了）は本実装では使わない。
            _ => None,
        },
        "7" => {
            // `7;file://host/path`。`7;` を剥がし（パスに `;` があっても保持）host 部を捨てる。
            let uri = s.strip_prefix("7;").unwrap_or("");
            let path = uri
                .strip_prefix("file://")
                .map(|rest| match rest.find('/') {
                    Some(idx) => rest[idx..].to_string(),
                    None => rest.to_string(),
                })
                .unwrap_or_else(|| uri.to_string());
            Some(BlockEvent::Cwd(percent_decode(&path)))
        }
        _ => None,
    }
}

/// OSC 7 のパスに含まれる最小限の %XX をデコードする（スペース等）。
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// ANSI(SGR) を `OutputColor` に解釈しつつ、素バイト列を `OutputSpan` 列へ変換する。
///
/// ブロックをまたいで現在色を保つので、ブロック単位で 1 つ持ち、`CommandStart` で
/// リセットする。`\r` は破棄（CRLF→LF 相当）し、色以外の CSI は読み飛ばす。
#[derive(Default)]
pub struct SgrConverter {
    color: OutputColor,
    /// CSI / エスケープの途中状態（チャンク跨ぎ対応）。
    esc: EscState,
    /// `ESC [` のパラメータ蓄積。
    params: Vec<u8>,
}

#[derive(Default, PartialEq, Eq)]
enum EscState {
    #[default]
    Text,
    /// ESC を受けた。
    Esc,
    /// `ESC [`（CSI）パラメータ収集中。
    Csi,
}

impl SgrConverter {
    pub fn new() -> Self {
        Self::default()
    }

    /// 新しいブロックの先頭で色状態をリセットする。
    pub fn reset(&mut self) {
        self.color = OutputColor::Default;
        self.esc = EscState::Text;
        self.params.clear();
    }

    /// 素バイト列を `OutputSpan` 列に変換する。同色は 1 span にまとめる。
    pub fn convert(&mut self, bytes: &[u8]) -> Vec<OutputSpan> {
        let mut spans: Vec<OutputSpan> = Vec::new();
        let mut buf = String::new();
        let mut pending: Vec<u8> = Vec::new(); // UTF-8 のマルチバイト境界対策

        for &b in bytes {
            match self.esc {
                EscState::Text => match b {
                    0x1b => self.esc = EscState::Esc,
                    b'\r' => {} // CRLF→LF 相当。素の \r は破棄する。
                    0x07 => {}  // BEL は無視。
                    _ => pending.push(b),
                },
                EscState::Esc => {
                    if b == b'[' {
                        self.esc = EscState::Csi;
                        self.params.clear();
                    } else {
                        // CSI 以外のエスケープ（`ESC ( B` 等）は 1 バイトで打ち切る簡易処理。
                        self.esc = EscState::Text;
                    }
                }
                EscState::Csi => {
                    if (0x30..=0x3f).contains(&b) {
                        // パラメータ / 中間バイト。
                        self.params.push(b);
                    } else {
                        // 終端バイト（@..~）。`m` のみ SGR として解釈、他は読み飛ばす。
                        if b == b'm' {
                            // pending を flush してから色を切り替える。
                            flush_pending(&mut pending, &mut buf);
                            push_span(&mut spans, &mut buf, self.color);
                            self.color = apply_sgr(self.color, &self.params);
                        }
                        self.esc = EscState::Text;
                        self.params.clear();
                    }
                }
            }
        }

        flush_pending(&mut pending, &mut buf);
        push_span(&mut spans, &mut buf, self.color);
        spans
    }
}

/// pending（生バイト）を UTF-8 として buf に取り込む。不正バイトは置換文字になる。
fn flush_pending(pending: &mut Vec<u8>, buf: &mut String) {
    if pending.is_empty() {
        return;
    }
    buf.push_str(&String::from_utf8_lossy(pending));
    pending.clear();
}

/// buf に溜まったテキストを 1 span にして push し、buf を空にする。
fn push_span(spans: &mut Vec<OutputSpan>, buf: &mut String, color: OutputColor) {
    if buf.is_empty() {
        return;
    }
    // 直前 span が同色ならテキストを連結（span 数を抑える）。
    if let Some(last) = spans.last_mut() {
        if last.color == color {
            last.text.push_str(buf);
            buf.clear();
            return;
        }
    }
    spans.push(OutputSpan {
        color,
        text: std::mem::take(buf),
    });
}

/// SGR パラメータ列（`"31"`, `"1;32"`, `"38;5;1"` 等）から新しい前景色を決める。
/// 背景・装飾は無視。256/truecolor は基本 16 色だけ拾い、それ以外は Default。
fn apply_sgr(current: OutputColor, params: &[u8]) -> OutputColor {
    let text = std::str::from_utf8(params).unwrap_or("");
    // 空（`ESC[m`）は reset 扱い。
    if text.is_empty() {
        return OutputColor::Default;
    }
    let codes: Vec<i32> = text.split(';').filter_map(|p| p.parse().ok()).collect();
    let mut color = current;
    let mut i = 0;
    while i < codes.len() {
        match codes[i] {
            0 => color = OutputColor::Default,
            2 => color = OutputColor::Dim, // faint
            30 | 37 | 39 | 97 => color = OutputColor::Default,
            90 => color = OutputColor::Dim, // bright black = grey
            31 | 91 => color = OutputColor::Rust,
            32 | 92 => color = OutputColor::Sage,
            33 | 93 => color = OutputColor::Amber,
            34 | 94 | 36 | 96 => color = OutputColor::Azure,
            35 | 95 => color = OutputColor::Plum,
            38 => {
                // 38;5;n または 38;2;r;g;b。基本 16 色のみ拾って残りはスキップ。
                match codes.get(i + 1) {
                    Some(5) => {
                        color = ansi256_to_color(codes.get(i + 2).copied().unwrap_or(-1));
                        i += 2;
                    }
                    Some(2) => {
                        color = OutputColor::Default;
                        i += 4;
                    }
                    _ => {}
                }
            }
            48 => {
                // 背景色指定。パラメータ分だけ読み飛ばす。
                match codes.get(i + 1) {
                    Some(5) => i += 2,
                    Some(2) => i += 4,
                    _ => {}
                }
            }
            _ => {}
        }
        i += 1;
    }
    color
}

/// 256 色番号のうち基本 8/16 色だけを palette にマップする。
fn ansi256_to_color(n: i32) -> OutputColor {
    match n {
        1 | 9 => OutputColor::Rust,
        2 | 10 => OutputColor::Sage,
        3 | 11 => OutputColor::Amber,
        4 | 12 | 6 | 14 => OutputColor::Azure,
        5 | 13 => OutputColor::Plum,
        8 => OutputColor::Dim,
        _ => OutputColor::Default,
    }
}

/// heuristic 用: 出力バイト列の中で「改行直後の行頭がプロンプトに見える」最初の位置
/// （その行の開始バイト offset）を返す。先頭行（offset 0）は出力本体とみなして区切らない。
/// プロンプト判定は ASCII のみを見るのでバイト境界で安全に slice できる。
fn prompt_split(raw: &[u8]) -> Option<usize> {
    for (i, &b) in raw.iter().enumerate() {
        if b == b'\n' {
            let start = i + 1;
            if byte_line_is_prompt(&raw[start..]) {
                return Some(start);
            }
        }
    }
    None
}

/// 行頭（先頭の空白を除く）が `$ ` / `% ` / `# ` で始まるか（ASCII バイト判定）。
fn byte_line_is_prompt(line: &[u8]) -> bool {
    let trimmed = line
        .iter()
        .position(|&b| b != b' ' && b != b'\t')
        .map(|p| &line[p..])
        .unwrap_or(&[]);
    matches!(trimmed.first(), Some(b'$') | Some(b'%') | Some(b'#'))
        && trimmed.get(1) == Some(&b' ')
}

/// scanner + converter + ブロックライフサイクルを 1 セッション分まとめた高レベル層。
///
/// `feed()` が返す [`TermAction`] を `AppState` 側が「末尾の実行中ブロック」に適用する。
/// OSC 133 を一度でも観測したら [`Mode::Osc133`] に確定し、以降 heuristic は使わない。
pub struct SessionTerm {
    scanner: OscScanner,
    sgr: SgrConverter,
    mode: Mode,
    /// 出力を実行中ブロックへ取り込んでいる最中か。
    in_command: bool,
}

#[derive(PartialEq, Eq)]
enum Mode {
    /// OSC 133 未観測。submit〜次 submit で区切る縮退モード。
    Auto,
    /// OSC 133 観測済み。C〜D を正として扱う。
    Osc133,
}

/// `AppState` が末尾の実行中ブロックに適用する操作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermAction {
    /// 実行中ブロックの出力に span 列を追記する。
    Append(Vec<OutputSpan>),
    /// Auto 取り込み中のエコーを捨てるため、実行中ブロックの出力をクリアする。
    ClearOutput,
    /// 実行中ブロックを終了する。`exit` は OSC 133 で取れた時のみ `Some`。
    EndBlock { exit: Option<i32> },
    /// cwd 更新（OSC 7）。
    SetCwd(String),
}

impl Default for SessionTerm {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionTerm {
    pub fn new() -> Self {
        Self {
            scanner: OscScanner::new(),
            sgr: SgrConverter::new(),
            mode: Mode::Auto,
            in_command: false,
        }
    }

    /// 入力行からコマンドが送信された時に呼ぶ。色状態をリセットし、
    /// Auto モードでは即座に出力取り込みを開始する（OSC 133 確定後は C を待つ）。
    pub fn on_submit(&mut self) {
        self.sgr.reset();
        self.in_command = matches!(self.mode, Mode::Auto);
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<TermAction> {
        let mut actions = Vec::new();
        for ev in self.scanner.process(bytes) {
            match ev {
                BlockEvent::CommandStart => {
                    let was_auto_capturing = self.in_command;
                    self.mode = Mode::Osc133;
                    self.in_command = true;
                    self.sgr.reset();
                    // Auto で先取りしていたエコーを捨てる（osc133 確定の初回コマンド対策）。
                    if was_auto_capturing {
                        actions.push(TermAction::ClearOutput);
                    }
                }
                BlockEvent::CommandEnd { exit } => {
                    if self.in_command {
                        actions.push(TermAction::EndBlock { exit });
                        self.in_command = false;
                    }
                }
                BlockEvent::PromptStart => {
                    // Auto モードでプロンプトが来たら直前コマンドの終わりとみなす
                    // （実際には OSC 133 整合シェルでしか A は来ないため通常は Osc133 側）。
                    if matches!(self.mode, Mode::Auto) && self.in_command {
                        actions.push(TermAction::EndBlock { exit: None });
                        self.in_command = false;
                    }
                }
                BlockEvent::Cwd(path) => actions.push(TermAction::SetCwd(path)),
                BlockEvent::Output(raw) => {
                    if !self.in_command {
                        // echo / プロンプト本文なので捨てる。
                        continue;
                    }
                    // Auto（OSC 133 未観測）モードでは行頭 `$`/`%`/`#` をプロンプトとみなし、
                    // そこでブロックを区切る（exit 不明）。OSC 133 確定後は使わない。
                    if matches!(self.mode, Mode::Auto) {
                        if let Some(cut) = prompt_split(&raw) {
                            let spans = self.sgr.convert(&raw[..cut]);
                            if !spans.is_empty() {
                                actions.push(TermAction::Append(spans));
                            }
                            actions.push(TermAction::EndBlock { exit: None });
                            self.in_command = false;
                            continue;
                        }
                    }
                    let spans = self.sgr.convert(&raw);
                    if !spans.is_empty() {
                        actions.push(TermAction::Append(spans));
                    }
                }
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outputs(events: &[BlockEvent]) -> Vec<u8> {
        events
            .iter()
            .filter_map(|e| match e {
                BlockEvent::Output(b) => Some(b.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn scanner_extracts_osc133_and_keeps_output() {
        let mut s = OscScanner::new();
        // C ; "hi" ; D;0
        let input = b"\x1b]133;C\x07hi\x1b]133;D;0\x07";
        let ev = s.process(input);
        assert_eq!(ev[0], BlockEvent::CommandStart);
        assert_eq!(ev[1], BlockEvent::Output(b"hi".to_vec()));
        assert_eq!(ev[2], BlockEvent::CommandEnd { exit: Some(0) });
    }

    #[test]
    fn scanner_handles_st_terminator() {
        let mut s = OscScanner::new();
        let input = b"\x1b]133;A\x1b\\done";
        let ev = s.process(input);
        assert_eq!(ev[0], BlockEvent::PromptStart);
        assert_eq!(outputs(&ev), b"done");
    }

    #[test]
    fn scanner_survives_split_across_chunks() {
        let mut s = OscScanner::new();
        let mut ev = s.process(b"\x1b]133");
        ev.extend(s.process(b";D;1\x07tail"));
        assert!(ev.contains(&BlockEvent::CommandEnd { exit: Some(1) }));
        assert_eq!(outputs(&ev), b"tail");
    }

    #[test]
    fn scanner_keeps_sgr_in_output() {
        let mut s = OscScanner::new();
        // SGR (CSI) は剥がさず Output に残す。
        let ev = s.process(b"\x1b[31mred\x1b[0m");
        assert_eq!(outputs(&ev), b"\x1b[31mred\x1b[0m");
    }

    #[test]
    fn scanner_parses_osc7_cwd() {
        let mut s = OscScanner::new();
        let ev = s.process(b"\x1b]7;file://host/Users/me/work\x07");
        assert_eq!(ev[0], BlockEvent::Cwd("/Users/me/work".into()));
    }

    #[test]
    fn osc7_percent_decode() {
        let mut s = OscScanner::new();
        let ev = s.process(b"\x1b]7;file://host/a%20b\x07");
        assert_eq!(ev[0], BlockEvent::Cwd("/a b".into()));
    }

    #[test]
    fn sgr_converts_colors() {
        let mut c = SgrConverter::new();
        let spans = c.convert(b"\x1b[32mok\x1b[0m done");
        assert_eq!(spans[0].color, OutputColor::Sage);
        assert_eq!(spans[0].text, "ok");
        assert_eq!(spans[1].color, OutputColor::Default);
        assert_eq!(spans[1].text, " done");
    }

    #[test]
    fn sgr_drops_carriage_return() {
        let mut c = SgrConverter::new();
        let spans = c.convert(b"line\r\nnext");
        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, "line\nnext");
    }

    #[test]
    fn sgr_color_persists_across_calls() {
        let mut c = SgrConverter::new();
        let _ = c.convert(b"\x1b[31m");
        let spans = c.convert(b"still red");
        assert_eq!(spans[0].color, OutputColor::Rust);
    }

    #[test]
    fn sgr_skips_non_sgr_csi() {
        let mut c = SgrConverter::new();
        // カーソル移動 CSI(H) は読み飛ばし、テキストだけ残る。
        let spans = c.convert(b"a\x1b[2Jb");
        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, "ab");
    }

    #[test]
    fn prompt_split_finds_second_line_prompt() {
        // 1 行目は出力本体、2 行目がプロンプト → 2 行目開始位置で区切る。
        let raw = b"hello\n$ ";
        assert_eq!(prompt_split(raw), Some(6));
        // プロンプトがなければ None。
        assert_eq!(prompt_split(b"line1\nline2"), None);
        // 先頭行がプロンプト風でも区切らない（出力本体扱い）。
        assert_eq!(prompt_split(b"$ still output"), None);
        // `echo $VAR` のような行頭でない $ は誤検出しない。
        assert_eq!(prompt_split(b"out\necho $VAR"), None);
    }

    #[test]
    fn session_term_osc133_flow_drops_echo_and_captures_output() {
        let mut t = SessionTerm::new();
        t.on_submit();
        // echo "ls" → C → "file\n" → D;0
        let actions = t.feed(b"ls\r\n\x1b]133;C\x07file\n\x1b]133;D;0\x07");
        // C の手前で Auto 取り込みしたエコーは ClearOutput で捨てられる。
        assert!(actions.contains(&TermAction::ClearOutput));
        assert!(actions
            .iter()
            .any(|a| matches!(a, TermAction::EndBlock { exit: Some(0) })));
        // 出力に "file" が含まれる。
        let appended: String = actions
            .iter()
            .filter_map(|a| match a {
                TermAction::Append(spans) => Some(spans.iter().map(|s| s.text.clone()).collect()),
                _ => None,
            })
            .collect::<Vec<String>>()
            .join("");
        assert!(appended.contains("file"));
    }

    #[test]
    fn session_term_heuristic_ends_on_prompt_line() {
        let mut t = SessionTerm::new();
        t.on_submit(); // Auto モード（OSC 133 未観測）
        let actions = t.feed(b"output line\n$ ");
        assert!(actions
            .iter()
            .any(|a| matches!(a, TermAction::EndBlock { exit: None })));
    }
}
