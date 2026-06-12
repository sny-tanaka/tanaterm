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
                    } else if b == 0x1b {
                        // 連続 ESC（ESC ESC ...）の場合、先行 ESC を出力に積み、
                        // Esc 状態を維持して次バイトで OSC か否かを判定する。
                        out.push(0x1b);
                        i += 1;
                        // state は Esc のまま維持（`i` は進める）。
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

/// SGR 変換の論理イベント。テキスト span 列と行上書き（\r 単独）の区別。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SgrEvent {
    /// テキスト span 列。
    Spans(Vec<OutputSpan>),
    /// \r 単独（CRLF ではない）。「現在行の先頭へ戻る」= 直前行の上書き開始。
    CarriageReturn,
    /// alternate screen の切替。`true` = 入り（h）、`false` = 出る（l）。
    AltScreen(bool),
}

/// ANSI(SGR) を `OutputColor` に解釈しつつ、素バイト列を `SgrEvent` 列へ変換する。
///
/// ブロックをまたいで現在色を保つので、ブロック単位で 1 つ持ち、`CommandStart` で
/// リセットする。色以外の CSI は読み飛ばす（ただし alternate screen CSI は
/// `SgrEvent::AltScreen` として通知する）。
#[derive(Default)]
pub struct SgrConverter {
    color: OutputColor,
    /// CSI / エスケープの途中状態（チャンク跨ぎ対応）。
    esc: EscState,
    /// `ESC [` のパラメータ蓄積。
    params: Vec<u8>,
    /// CSI 中間バイト（0x20–0x2F）を受け取ったことを示すフラグ。
    /// 中間バイト付きシーケンスは終端が `m` でも SGR として解釈しない（ECMA-48 準拠）。
    has_intermediate: bool,
    /// UTF-8 マルチバイト文字がチャンク境界で分断された場合の未確定バイト列。
    /// 次回 `convert()` 呼び出しで続きのバイトと連結して解釈する。
    pending: Vec<u8>,
    /// チャンク末尾が `\r` で終わった場合の持ち越しフラグ。
    /// 次チャンク先頭が `\n` なら CRLF として改行のみ発行し、それ以外なら
    /// `CarriageReturn` を先に発行してからそのバイトを処理する。
    cr_pending: bool,
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
        // ブロック跨ぎの UTF-8 未確定バイトも破棄する。
        self.pending.clear();
        self.cr_pending = false;
    }

    /// 素バイト列を `SgrEvent` 列に変換する。同色は 1 span にまとめる。
    pub fn convert(&mut self, bytes: &[u8]) -> Vec<SgrEvent> {
        let mut events: Vec<SgrEvent> = Vec::new();
        let mut spans: Vec<OutputSpan> = Vec::new();
        let mut buf = String::new();

        // cr_pending 持ち越し処理: チャンク先頭バイトで CRLF か素の CR かを判定する。
        if self.cr_pending {
            self.cr_pending = false;
            if bytes.first() == Some(&b'\n') {
                // CRLF → \n として扱う（buf に改行を積んで残りを処理）。
                buf.push('\n');
                // 残りのバイトを処理するため bytes の先頭を 1 つ skip する。
                return self.convert_inner(&bytes[1..], events, spans, buf);
            } else {
                // 素の CR → CarriageReturn を先に発行。
                push_span(&mut spans, &mut buf, self.color);
                flush_spans(&mut events, &mut spans);
                events.push(SgrEvent::CarriageReturn);
                // 続けて bytes 全体を処理。
                return self.convert_inner(bytes, events, spans, buf);
            }
        }

        self.convert_inner(bytes, events, spans, buf)
    }

    fn convert_inner(
        &mut self,
        bytes: &[u8],
        mut events: Vec<SgrEvent>,
        mut spans: Vec<OutputSpan>,
        mut buf: String,
    ) -> Vec<SgrEvent> {
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            match self.esc {
                EscState::Text => match b {
                    0x1b => {
                        // ESC は ASCII であり、正規 UTF-8 マルチバイト途中には現れない。
                        // ここで pending に未確定バイトが残っていれば不正入力として lossy flush。
                        flush_pending_lossy(&mut self.pending, &mut buf);
                        self.esc = EscState::Esc;
                        i += 1;
                    }
                    b'\r' => {
                        // \r も ASCII なので pending を lossy flush する。
                        flush_pending_lossy(&mut self.pending, &mut buf);
                        // 次バイトを先読みして CRLF か素の CR かを判定する。
                        if bytes.get(i + 1) == Some(&b'\n') {
                            // CRLF → 改行として扱う（CarriageReturn は出さない）。
                            buf.push('\n');
                            i += 2; // \r と \n を両方消費する。
                        } else if i + 1 < bytes.len() {
                            // 素の CR（次バイトは \n でない）。
                            push_span(&mut spans, &mut buf, self.color);
                            flush_spans(&mut events, &mut spans);
                            events.push(SgrEvent::CarriageReturn);
                            i += 1;
                        } else {
                            // チャンク末尾が \r → 次チャンクで判定を繰り越す。
                            self.cr_pending = true;
                            i += 1;
                        }
                    }
                    0x07 => {
                        // BEL は無視。ASCII なので同様に pending を lossy flush。
                        flush_pending_lossy(&mut self.pending, &mut buf);
                        i += 1;
                    }
                    _ => {
                        self.pending.push(b);
                        i += 1;
                    }
                },
                EscState::Esc => {
                    if b == b'[' {
                        self.esc = EscState::Csi;
                        self.params.clear();
                        self.has_intermediate = false;
                    } else {
                        // CSI 以外のエスケープ（`ESC ( B` 等）は 1 バイトで打ち切る簡易処理。
                        self.esc = EscState::Text;
                    }
                    i += 1;
                }
                EscState::Csi => {
                    if (0x40..=0x7e).contains(&b) {
                        // 終端バイト（ECMA-48: 0x40–0x7E）。
                        if b == b'm' && !self.has_intermediate {
                            // pending を flush してから色を切り替える。
                            flush_pending_lossy(&mut self.pending, &mut buf);
                            push_span(&mut spans, &mut buf, self.color);
                            self.color = apply_sgr(self.color, &self.params);
                        } else if (b == b'h' || b == b'l') && !self.has_intermediate {
                            // alternate screen CSI: params が `?` で始まり、1049/1047/47 を含む。
                            if let Some(alt) = detect_alt_screen(&self.params, b) {
                                flush_pending_lossy(&mut self.pending, &mut buf);
                                push_span(&mut spans, &mut buf, self.color);
                                flush_spans(&mut events, &mut spans);
                                events.push(SgrEvent::AltScreen(alt));
                            }
                        }
                        self.esc = EscState::Text;
                        self.params.clear();
                        self.has_intermediate = false;
                    } else if (0x20..=0x2f).contains(&b) {
                        // 中間バイト（ECMA-48: 0x20–0x2F）。読み飛ばし、フラグを立てる。
                        self.has_intermediate = true;
                    } else if (0x30..=0x3f).contains(&b) {
                        // パラメータバイト（ECMA-48: 0x30–0x3F）。
                        self.params.push(b);
                    } else {
                        // 不正バイト（0x00–0x1F 等）は CSI を打ち切る。
                        self.esc = EscState::Text;
                        self.params.clear();
                        self.has_intermediate = false;
                    }
                    i += 1;
                }
            }
        }

        // チャンク末尾で完全な UTF-8 になっている部分だけを buf に取り込み、
        // 不完全な末尾バイト列は self.pending に残して次回 convert() に持ち越す。
        flush_pending_partial(&mut self.pending, &mut buf);
        push_span(&mut spans, &mut buf, self.color);
        flush_spans(&mut events, &mut spans);
        events
    }
}

/// alternate screen CSI を検知する。
///
/// params が `?` で始まり、`;` 区切りのパラメータに `1049`/`1047`/`47` のいずれかを含み、
/// 終端バイトが `h`（true）または `l`（false）の場合に `Some(bool)` を返す。
fn detect_alt_screen(params: &[u8], terminator: u8) -> Option<bool> {
    let s = std::str::from_utf8(params).ok()?;
    let rest = s.strip_prefix('?')?;
    // `;` 区切りの各パラメータに 1049/1047/47 が含まれるか確認する。
    let has_alt = rest.split(';').any(|p| matches!(p, "1049" | "1047" | "47"));
    if has_alt {
        Some(terminator == b'h')
    } else {
        None
    }
}

/// spans が空でなければ `SgrEvent::Spans` に変換して events に push する。
fn flush_spans(events: &mut Vec<SgrEvent>, spans: &mut Vec<OutputSpan>) {
    if !spans.is_empty() {
        events.push(SgrEvent::Spans(std::mem::take(spans)));
    }
}

/// pending（生バイト）を UTF-8 として buf に lossy で取り込む。不正バイトは置換文字になる。
///
/// ESC / \r / BEL 等の ASCII が届いた時点で呼ぶ（正規マルチバイト途中には現れないため、
/// その時点で pending に残っているバイト列は不正入力とみなしてよい）。
fn flush_pending_lossy(pending: &mut Vec<u8>, buf: &mut String) {
    if pending.is_empty() {
        return;
    }
    buf.push_str(&String::from_utf8_lossy(pending));
    pending.clear();
}

/// pending（生バイト）を UTF-8 として buf に取り込む。
///
/// 先頭から完全な UTF-8 として解釈できる部分だけを buf に書き込み、
/// チャンク末尾で途切れた不完全バイト列は pending に残す。
/// 途中に明確な不正バイトがある場合は置換文字（U+FFFD）を 1 つ push し、
/// そのバイトを捨ててループ継続する。
fn flush_pending_partial(pending: &mut Vec<u8>, buf: &mut String) {
    if pending.is_empty() {
        return;
    }
    let mut start = 0;
    loop {
        match std::str::from_utf8(&pending[start..]) {
            Ok(s) => {
                // 全バイト有効。
                buf.push_str(s);
                pending.clear();
                return;
            }
            Err(e) => {
                // 有効部分を先に flush。
                let valid_end = start + e.valid_up_to();
                if valid_end > start {
                    // Safety: valid_up_to() は UTF-8 境界を保証する。
                    buf.push_str(unsafe {
                        std::str::from_utf8_unchecked(&pending[start..valid_end])
                    });
                }
                match e.error_len() {
                    None => {
                        // 末尾で切れている（不完全マルチバイト）。次回に持ち越す。
                        pending.drain(..valid_end);
                        return;
                    }
                    Some(n) => {
                        // 明確な不正バイト列。置換文字を 1 つ push して n バイト捨てる。
                        buf.push('\u{FFFD}');
                        start = valid_end + n;
                        if start >= pending.len() {
                            pending.clear();
                            return;
                        }
                        // ループ継続（残りバイトを処理）。
                    }
                }
            }
        }
    }
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
    matches!(trimmed.first(), Some(b'$') | Some(b'%') | Some(b'#')) && trimmed.get(1) == Some(&b' ')
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
    /// OSC 133 モードで PromptStart（A）を受信してプロンプト表示中か（05: detached output）。
    /// `true` の間（A〜C）は出力を捨てる（プロンプト本文）。
    /// `CommandStart`（C）で false、`PromptStart`（A）で true にセットする。
    at_prompt: bool,
    /// Auto モードでのエコー除去用: `on_submit` に渡されたコマンド文字列（06）。
    /// `None` = 判定済み or Osc133 モード。
    pending_echo: Option<String>,
    /// エコー除去判定中に `\n` が来るまで溜めるバイトバッファ（06）。
    echo_buf: Vec<u8>,
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
    /// \r 単独（プログレスバー上書き）。実行中ブロックの出力の最終行を削除して上書きを開始する。
    OverwriteLine,
    /// C〜D 区間外（バックグラウンドジョブ等）の出力。最終ブロックへ Dim で追記する。
    AppendDetached(Vec<OutputSpan>),
    /// alternate screen の切替。`true` = 入り（vim 等）、`false` = 出る。
    AltScreen(bool),
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
            at_prompt: false,
            pending_echo: None,
            echo_buf: Vec::new(),
        }
    }

    /// 入力行からコマンドが送信された時に呼ぶ。色状態をリセットし、
    /// Auto モードでは即座に出力取り込みを開始する（OSC 133 確定後は C を待つ）。
    /// `cmd` は Auto モードのエコー除去用に保持する（06: heuristic echo strip）。
    pub fn on_submit(&mut self, cmd: &str) {
        self.sgr.reset();
        self.in_command = matches!(self.mode, Mode::Auto);
        // Auto モードのみ: エコー行を比較するためコマンドを保持する。
        if matches!(self.mode, Mode::Auto) {
            self.pending_echo = Some(cmd.trim().to_string());
            self.echo_buf.clear();
        }
    }

    /// `SgrEvent` 列を `TermAction` 列に変換して actions へ追記する内部ヘルパ。
    /// `in_command` 中のみ実際に発行する（プロンプト本文は捨てる）。
    fn push_sgr_events(&self, events: Vec<SgrEvent>, actions: &mut Vec<TermAction>) {
        for ev in events {
            match ev {
                SgrEvent::Spans(spans) => {
                    if !spans.is_empty() {
                        actions.push(TermAction::Append(spans));
                    }
                }
                SgrEvent::CarriageReturn => {
                    actions.push(TermAction::OverwriteLine);
                }
                SgrEvent::AltScreen(b) => {
                    actions.push(TermAction::AltScreen(b));
                }
            }
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<TermAction> {
        let mut actions = Vec::new();
        for ev in self.scanner.process(bytes) {
            match ev {
                BlockEvent::CommandStart => {
                    let was_auto_capturing = self.in_command;
                    self.mode = Mode::Osc133;
                    self.in_command = true;
                    self.at_prompt = false;
                    // Auto モード用のエコー除去バッファをクリアする（Osc133 では使わない）。
                    self.pending_echo = None;
                    self.echo_buf.clear();
                    self.sgr.reset();
                    // Auto で先取りしていたエコーを捨てる（osc133 確定の初回コマンド対策）。
                    if was_auto_capturing {
                        actions.push(TermAction::ClearOutput);
                    }
                }
                BlockEvent::CommandEnd { exit } => {
                    // Osc133 モード（= C を観測済み）でのみ EndBlock を発火する。
                    // Auto モードで D だけ先に来るのは「シェル起動直後の初回 precmd」のケース
                    // （hook 登録済みで PROMPT 表示前に走る precmd が D;0, A, OSC 7 を吐く）。
                    // ここで EndBlock すると、まだユーザーコマンド未実行の running ブロックを
                    // 閉じてしまい、続く OSC 133;C の Append も「running ブロックなし」で
                    // 捨てられ、初回コマンドが永久に実行されないように見えるバグになる。
                    // よって Auto モードでは EndBlock せず、Osc133 確定の合図として扱い、
                    // Auto で先取りしていた echo は ClearOutput で捨てて C を待つ。
                    if matches!(self.mode, Mode::Osc133) {
                        if self.in_command {
                            actions.push(TermAction::EndBlock { exit });
                            self.in_command = false;
                            self.at_prompt = false;
                        }
                    } else {
                        self.mode = Mode::Osc133;
                        if self.in_command {
                            actions.push(TermAction::ClearOutput);
                            self.in_command = false;
                        }
                    }
                }
                BlockEvent::PromptStart => {
                    // A も「シェルが OSC 133 を出す合図」。Auto モードで来たら
                    // Osc133 確定の合図とし、in_command なら ClearOutput で先取りを捨てる
                    // （ユーザーコマンド未実行のままブロックを閉じない）。
                    // Osc133 モードでの A 単独受信は at_prompt = true にして D 後の窓を閉じる。
                    if matches!(self.mode, Mode::Auto) {
                        self.mode = Mode::Osc133;
                        if self.in_command {
                            actions.push(TermAction::ClearOutput);
                            self.in_command = false;
                        }
                    }
                    // OSC 133 モードでは A でプロンプト表示中フラグを立てる（05: detached output）。
                    self.at_prompt = true;
                }
                BlockEvent::Cwd(path) => actions.push(TermAction::SetCwd(path)),
                BlockEvent::Output(raw) => {
                    if self.in_command {
                        // コマンド実行中の出力を取り込む。
                        // Auto（OSC 133 未観測）モードでは行頭 `$`/`%`/`#` をプロンプトとみなし、
                        // そこでブロックを区切る（exit 不明）。OSC 133 確定後は使わない。
                        if matches!(self.mode, Mode::Auto) {
                            if let Some(cut) = prompt_split(&raw) {
                                // プロンプト行より前の出力を処理する（echo 除去込み）。
                                let before = &raw[..cut];
                                let sgr_evs = self.process_with_echo_strip(before);
                                self.push_sgr_events(sgr_evs, &mut actions);
                                actions.push(TermAction::EndBlock { exit: None });
                                self.in_command = false;
                                continue;
                            }
                        }
                        // Auto モードのエコー除去処理（06）。
                        if matches!(self.mode, Mode::Auto) && self.pending_echo.is_some() {
                            let sgr_evs = self.process_with_echo_strip(&raw);
                            self.push_sgr_events(sgr_evs, &mut actions);
                        } else {
                            let sgr_evs = self.sgr.convert(&raw);
                            self.push_sgr_events(sgr_evs, &mut actions);
                        }
                    } else {
                        // コマンド外（プロンプト表示中 or D 後の区間）。
                        // Osc133 モードで at_prompt == false（D 後〜次 A の前）は
                        // バックグラウンドジョブ出力として取り込む（05: detached output）。
                        if matches!(self.mode, Mode::Osc133) && !self.at_prompt {
                            let sgr_evs = self.sgr.convert(&raw);
                            let spans: Vec<_> = sgr_evs
                                .into_iter()
                                .filter_map(|e| match e {
                                    SgrEvent::Spans(s) => Some(s),
                                    _ => None,
                                })
                                .flatten()
                                .collect();
                            if !spans.is_empty() {
                                // 空白のみの出力はプロンプト前後のノイズとして捨てる。
                                let all_text: String =
                                    spans.iter().map(|s| s.text.as_str()).collect();
                                if !all_text.trim().is_empty() {
                                    // Dim 色に上書きして副次出力と区別する。
                                    let dim_spans: Vec<_> = spans
                                        .into_iter()
                                        .map(|mut s| {
                                            s.color = crate::state::OutputColor::Dim;
                                            s
                                        })
                                        .collect();
                                    actions.push(TermAction::AppendDetached(dim_spans));
                                }
                            }
                        }
                        // at_prompt == true（A〜C 間 = プロンプト本文）は従来どおり捨てる。
                        // Auto モードの非 in_command も捨てる（プロンプトノイズ）。
                    }
                }
            }
        }
        actions
    }

    /// Auto モードのエコー除去処理（06: heuristic echo strip）。
    ///
    /// `pending_echo` に保持したコマンドと最初の出力行を比較し、一致すればその行を捨てる。
    /// `\n` を見るまではバイト列を `echo_buf` に溜めて判定を保留する。
    /// コマンド長 + 16 バイトを超えても `\n` が来なければ諦めてバッファごと流す。
    fn process_with_echo_strip(&mut self, raw: &[u8]) -> Vec<SgrEvent> {
        // pending_echo が無い（判定済み or Osc133 モード）場合はそのまま変換。
        if self.pending_echo.is_none() {
            return self.sgr.convert(raw);
        }

        let cmd_len = self.pending_echo.as_ref().map_or(0, |c| c.len());
        let buf_limit = cmd_len + 16;

        // echo_buf に未処理バイトが残っている状態で新しいバイトを結合して処理する。
        let combined: Vec<u8> = if self.echo_buf.is_empty() {
            raw.to_vec()
        } else {
            let mut c = std::mem::take(&mut self.echo_buf);
            c.extend_from_slice(raw);
            c
        };

        // `\n` の位置を探す。
        if let Some(nl_pos) = combined.iter().position(|&b| b == b'\n') {
            // 最初の行が確定した。echo と比較する。
            let first_line_raw = &combined[..nl_pos];
            // `\r` を除去して比較する（PTY は \r\n で終端することが多い）。
            let first_line = String::from_utf8_lossy(first_line_raw);
            let first_line_trimmed = first_line.trim_matches('\r').trim();

            let discard = self.pending_echo.as_deref() == Some(first_line_trimmed);

            self.pending_echo = None;
            self.echo_buf.clear();

            if discard {
                // エコー行を捨てて残りを変換する。
                let rest = &combined[nl_pos + 1..];
                if rest.is_empty() {
                    Vec::new()
                } else {
                    self.sgr.convert(rest)
                }
            } else {
                // エコーと不一致 → そのまま変換する。
                self.sgr.convert(&combined)
            }
        } else if combined.len() > buf_limit {
            // \n が来ないまま上限超過 → 諦めてバッファごと流す。
            self.pending_echo = None;
            self.echo_buf.clear();
            self.sgr.convert(&combined)
        } else {
            // まだ \n が来ていない → 持ち越す。
            self.echo_buf = combined;
            Vec::new()
        }
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

    // ── 03: ESC ESC ] で OSC を取りこぼさないことを確認するテスト ────────────

    /// ESC ESC ] 133;C BEL の並びで CommandStart が得られ、Output には ESC 1 個だけが残る。
    #[test]
    fn scanner_esc_esc_osc_is_not_dropped() {
        let mut s = OscScanner::new();
        let ev = s.process(b"\x1b\x1b]133;C\x07");
        // CommandStart が含まれる。
        assert!(
            ev.contains(&BlockEvent::CommandStart),
            "CommandStart が得られること: {ev:?}",
        );
        // Output には先行 ESC 1 個だけが残る（2 個は残らない）。
        let out = outputs(&ev);
        assert_eq!(out, b"\x1b", "Output には ESC 1 個: {out:?}");
    }

    #[test]
    fn sgr_converts_colors() {
        let mut c = SgrConverter::new();
        let events = c.convert(b"\x1b[32mok\x1b[0m done");
        let spans: Vec<_> = events
            .into_iter()
            .filter_map(|e| match e {
                SgrEvent::Spans(s) => Some(s),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(spans[0].color, OutputColor::Sage);
        assert_eq!(spans[0].text, "ok");
        assert_eq!(spans[1].color, OutputColor::Default);
        assert_eq!(spans[1].text, " done");
    }

    /// CRLF は \n だけに畳まれ、CarriageReturn イベントは発行されない。
    #[test]
    fn sgr_drops_carriage_return() {
        let mut c = SgrConverter::new();
        let events = c.convert(b"line\r\nnext");
        // CarriageReturn が出ないこと。
        assert!(
            !events.iter().any(|e| matches!(e, SgrEvent::CarriageReturn)),
            "CRLF は CarriageReturn を出さない: {events:?}",
        );
        let text: String = events
            .iter()
            .filter_map(|e| match e {
                SgrEvent::Spans(spans) => {
                    Some(spans.iter().map(|s| s.text.as_str()).collect::<String>())
                }
                _ => None,
            })
            .collect();
        assert_eq!(text, "line\nnext");
    }

    #[test]
    fn sgr_color_persists_across_calls() {
        let mut c = SgrConverter::new();
        let _ = c.convert(b"\x1b[31m");
        let events = c.convert(b"still red");
        let spans: Vec<_> = events
            .into_iter()
            .filter_map(|e| match e {
                SgrEvent::Spans(s) => Some(s),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(spans[0].color, OutputColor::Rust);
    }

    #[test]
    fn sgr_skips_non_sgr_csi() {
        let mut c = SgrConverter::new();
        // カーソル移動 CSI(H) は読み飛ばし、テキストだけ残る。
        let events = c.convert(b"a\x1b[2Jb");
        let text: String = events
            .iter()
            .filter_map(|e| match e {
                SgrEvent::Spans(spans) => {
                    Some(spans.iter().map(|s| s.text.as_str()).collect::<String>())
                }
                _ => None,
            })
            .collect();
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
        t.on_submit("");
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
        t.on_submit(""); // Auto モード（OSC 133 未観測）
        let actions = t.feed(b"output line\n$ ");
        assert!(actions
            .iter()
            .any(|a| matches!(a, TermAction::EndBlock { exit: None })));
    }

    /// 「初回コマンドが実行されない」回帰防止。
    ///
    /// シェル起動直後の初回 `precmd` で OSC 133;D;0, A, OSC 7 が出る。これは
    /// 「次のプロンプトの前奏」であり、ユーザーコマンドの終端ではない。
    /// `on_submit` で `in_command=true` (Auto) になった直後にこの D を受け取っても、
    /// EndBlock を発火させず、ClearOutput で先取りを捨てて C を待つこと。
    ///
    /// 元の実装では Auto モードでも D で EndBlock していたため、初回コマンドの
    /// 実 PTY 結果が「running ブロックなし」状態で全部捨てられていた。
    #[test]
    fn session_term_initial_precmd_d_does_not_close_user_block() {
        let mut t = SessionTerm::new();
        t.on_submit(""); // Auto モード, in_command=true

        // シェル初回 precmd: D;0 → A → OSC 7、続いて echo "ls", C, "out\n", D;0
        let raw = b"\x1b]133;D;0\x07\x1b]133;A\x07\x1b]7;file://h/tmp\x07ls\r\n\x1b]133;C\x07out\n\x1b]133;D;0\x07";
        let actions = t.feed(raw);

        // 初回 D で EndBlock(exit=0) してはいけない（その後 EndBlock 1 回は OK）。
        let end_blocks: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, TermAction::EndBlock { .. }))
            .collect();
        assert_eq!(
            end_blocks.len(),
            1,
            "EndBlock は最後の D だけで 1 回のみ発火する: {actions:?}",
        );
        // 初回 D の代わりに ClearOutput で先取りを捨てている。
        assert!(actions.contains(&TermAction::ClearOutput));
        // 実 ls 出力 "out" は Append されている。
        let appended: String = actions
            .iter()
            .filter_map(|a| match a {
                TermAction::Append(spans) => Some(spans.iter().map(|s| s.text.clone()).collect()),
                _ => None,
            })
            .collect::<Vec<String>>()
            .join("");
        assert!(
            appended.contains("out"),
            "ls 出力が block に届く: appended={appended:?}",
        );
    }

    /// Auto モードで A（PromptStart）だけ先に来た場合も同様に EndBlock せず、
    /// ClearOutput + Osc133 確定の合図として扱うこと。
    #[test]
    fn session_term_initial_prompt_a_does_not_close_user_block() {
        let mut t = SessionTerm::new();
        t.on_submit("");
        let actions = t.feed(b"\x1b]133;A\x07");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, TermAction::EndBlock { .. })),
            "EndBlock してはいけない: {actions:?}",
        );
        assert!(actions.contains(&TermAction::ClearOutput));
    }

    // ── 01: UTF-8 チャンク境界文字化け修正テスト ──────────────────────────────

    /// `SgrEvent` 列からテキストを結合するヘルパ（テスト用）。
    fn events_text(events: &[SgrEvent]) -> String {
        events
            .iter()
            .filter_map(|e| match e {
                SgrEvent::Spans(spans) => {
                    Some(spans.iter().map(|s| s.text.as_str()).collect::<String>())
                }
                _ => None,
            })
            .collect()
    }

    /// `SgrEvent` 列から spans を平坦化するヘルパ（テスト用）。
    fn events_spans(events: Vec<SgrEvent>) -> Vec<OutputSpan> {
        events
            .into_iter()
            .filter_map(|e| match e {
                SgrEvent::Spans(s) => Some(s),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// 「こんにちは」を 1 バイト目直後で 2 チャンクに分断しても文字化けしない。
    #[test]
    fn sgr_utf8_split_after_first_byte() {
        let mut c = SgrConverter::new();
        // "こ" = 0xe3 0x81 0x93。1 バイト目（0xe3）だけ先に届く。
        let konnnichiwa = "こんにちは".as_bytes();
        let (first, rest) = konnnichiwa.split_at(1);
        let s1 = c.convert(first);
        let s2 = c.convert(rest);
        let text: String = s1
            .iter()
            .chain(s2.iter())
            .filter_map(|e| match e {
                SgrEvent::Spans(spans) => {
                    Some(spans.iter().map(|s| s.text.as_str()).collect::<String>())
                }
                _ => None,
            })
            .collect();
        assert!(
            !text.contains('\u{FFFD}'),
            "置換文字が含まれてはいけない: {text:?}",
        );
        assert_eq!(text, "こんにちは");
    }

    /// 「こんにちは」を 2 バイト目直後で 2 チャンクに分断しても文字化けしない。
    #[test]
    fn sgr_utf8_split_after_second_byte() {
        let mut c = SgrConverter::new();
        // "こ" = 0xe3 0x81 0x93。2 バイト目（0x81）まで先に届く。
        let konnnichiwa = "こんにちは".as_bytes();
        let (first, rest) = konnnichiwa.split_at(2);
        let s1 = c.convert(first);
        let s2 = c.convert(rest);
        let text: String = s1
            .iter()
            .chain(s2.iter())
            .filter_map(|e| match e {
                SgrEvent::Spans(spans) => {
                    Some(spans.iter().map(|s| s.text.as_str()).collect::<String>())
                }
                _ => None,
            })
            .collect();
        assert!(
            !text.contains('\u{FFFD}'),
            "置換文字が含まれてはいけない: {text:?}",
        );
        assert_eq!(text, "こんにちは");
    }

    /// 単独の不正バイト（0xff）は置換文字になる。
    #[test]
    fn sgr_invalid_byte_becomes_replacement_char() {
        let mut c = SgrConverter::new();
        let events = c.convert(b"\xff");
        // convert() の末尾では partial flush するが、0xff は error_len=Some(1) なので
        // 即座に FFFD に変換される。
        let text = events_text(&events);
        assert_eq!(text, "\u{FFFD}");
    }

    /// reset() 後に保持バイトが持ち越されない。
    #[test]
    fn sgr_reset_clears_pending_bytes() {
        let mut c = SgrConverter::new();
        // "こ" の 1 バイト目だけ渡す（pending に残る）。
        let _ = c.convert(&"こ".as_bytes()[..1]);
        // reset() で pending を破棄する。
        c.reset();
        // reset 後に残りバイトを渡しても前の未確定バイトとは連結されず独立して処理される。
        // "こ" の 2〜3 バイト目（0x81 0x93）は 1 バイト目なしでは不正 → FFFD になる。
        // 各バイトが個別に不正となるため FFFD が 1 個以上出る。
        let events = c.convert(&"こ".as_bytes()[1..]);
        let text = events_text(&events);
        assert!(
            text.chars().all(|ch| ch == '\u{FFFD}') && !text.is_empty(),
            "reset 後は前の pending が持ち越されず不正バイト扱いになる: {text:?}",
        );
    }

    // ── 02: CSI 中間バイト誤終端修正テスト ────────────────────────────────────

    /// DECSCUSR (ESC [ 0 SP q) の SP（中間バイト）で打ち切られ、`q` が混入しないこと。
    #[test]
    fn sgr_csi_intermediate_byte_not_output() {
        let mut c = SgrConverter::new();
        let events = c.convert(b"a\x1b[0 qb");
        let text = events_text(&events);
        assert_eq!(text, "ab", "q が出力に混入してはいけない: {text:?}");
    }

    /// 中間バイト付きの `ESC [ 1 SP m` は SGR として解釈されず、色が変わらないこと。
    #[test]
    fn sgr_csi_intermediate_m_not_treated_as_sgr() {
        let mut c = SgrConverter::new();
        // 最初に赤にする。
        let _ = c.convert(b"\x1b[31m");
        // 中間バイト付き `ESC [ 1 SP m` を送る（SGR 扱いされれば bold 色になるが、無視されるはず）。
        let events = c.convert(b"\x1b[1 m x");
        let spans = events_spans(events);
        // 色はリセットされず Rust のまま。
        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert!(text.contains('x'), "テキスト 'x' が含まれる: {text:?}");
        // 最後の span の色が Rust であること（中間バイト付き m で色変更されていない）。
        let last_color = spans.last().map(|s| s.color);
        assert_eq!(
            last_color,
            Some(OutputColor::Rust),
            "中間バイト付き m は SGR 扱いしない: {spans:?}",
        );
    }

    // ── 04: \r 上書き（CarriageReturn）テスト ─────────────────────────────────

    /// `convert(b"ab\rcd")` → Spans("ab"), CarriageReturn, Spans("cd") の順。
    #[test]
    fn sgr_cr_emits_carriage_return_event() {
        let mut c = SgrConverter::new();
        let events = c.convert(b"ab\rcd");
        // 先頭が Spans("ab")、次が CarriageReturn、最後が Spans("cd")。
        assert!(
            matches!(&events[0], SgrEvent::Spans(s) if s[0].text == "ab"),
            "先頭 Spans(ab): {events:?}",
        );
        assert!(
            matches!(&events[1], SgrEvent::CarriageReturn),
            "CarriageReturn: {events:?}",
        );
        assert!(
            matches!(&events[2], SgrEvent::Spans(s) if s[0].text == "cd"),
            "末尾 Spans(cd): {events:?}",
        );
    }

    /// `convert(b"ab\r\ncd")` → CRLF は改行に畳まれ CarriageReturn が出ないこと。
    #[test]
    fn sgr_crlf_no_carriage_return() {
        let mut c = SgrConverter::new();
        let events = c.convert(b"ab\r\ncd");
        assert!(
            !events.iter().any(|e| matches!(e, SgrEvent::CarriageReturn)),
            "CRLF では CarriageReturn を出さない: {events:?}",
        );
        let text = events_text(&events);
        assert_eq!(text, "ab\ncd");
    }

    /// チャンク末尾 `\r` → 次が `\n`（CRLF）の場合、CarriageReturn が出ない。
    #[test]
    fn sgr_cr_pending_then_lf_is_crlf() {
        let mut c = SgrConverter::new();
        let e1 = c.convert(b"ab\r");
        let e2 = c.convert(b"\ncd");
        // e1 は Spans("ab") だけ（\r は次チャンクへ持ち越し）。
        assert!(
            !e1.iter().any(|e| matches!(e, SgrEvent::CarriageReturn)),
            "持ち越し CR で CarriageReturn を出さない: {e1:?}",
        );
        // e2 では CRLF → LF として取り込み、CarriageReturn なし。
        assert!(
            !e2.iter().any(|e| matches!(e, SgrEvent::CarriageReturn)),
            "次チャンク先頭が LF → CRLF 扱い: {e2:?}",
        );
        let text: String = e1
            .iter()
            .chain(e2.iter())
            .filter_map(|e| match e {
                SgrEvent::Spans(s) => Some(s.iter().map(|sp| sp.text.as_str()).collect::<String>()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "ab\ncd");
    }

    /// チャンク末尾 `\r` → 次が `\n` でない場合、CarriageReturn が先に発行される。
    #[test]
    fn sgr_cr_pending_then_non_lf_emits_cr() {
        let mut c = SgrConverter::new();
        let e1 = c.convert(b"ab\r");
        let e2 = c.convert(b"cd");
        // e1 は Spans("ab") のみ。
        assert_eq!(events_text(&e1), "ab");
        // e2 では CarriageReturn が先に出てから Spans("cd")。
        assert!(
            e2.iter().any(|e| matches!(e, SgrEvent::CarriageReturn)),
            "次チャンクが非 LF → CarriageReturn: {e2:?}",
        );
        assert_eq!(events_text(&e2), "cd");
    }

    /// `SessionTerm` に `"50%\r100%"` を流すと最終 Append テキストが `"100%"` になる e2e テスト。
    #[test]
    fn session_term_progress_bar_overwrite() {
        use crate::state::{AppState, OutputSpan};
        let mut t = SessionTerm::new();
        t.on_submit("test");
        // C → "50%\r100%" → D;0
        let actions = t.feed(b"\x1b]133;C\x07 50%\r100%\x1b]133;D;0\x07");
        // OverwriteLine が発行されている。
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, TermAction::OverwriteLine)),
            "OverwriteLine が発行される: {actions:?}",
        );
        // Append("100%") が含まれている。
        let appended: String = actions
            .iter()
            .filter_map(|a| match a {
                TermAction::Append(spans) => {
                    Some(spans.iter().map(|s| s.text.as_str()).collect::<String>())
                }
                _ => None,
            })
            .collect();
        assert!(
            appended.contains("100%"),
            "100% が Append される: {appended:?}"
        );

        // AppState に適用して最終テキストを確認する。
        let mut state = AppState::seed();
        state.focus_session("s3");
        if let Some(sess) = state.active_mut() {
            sess.input_buffer = "test".into();
        }
        state.submit_input("12:00".into());
        for action in actions {
            state.apply_term_action("s3", action);
        }
        let block = state.active().unwrap().blocks.last().unwrap();
        let text: String = block.output.iter().map(|s| s.text.as_str()).collect();
        // OverwriteLine で先頭行が削れ、100% が残る。
        assert!(
            text.contains("100%"),
            "最終出力に 100% が含まれる: {text:?}",
        );
        assert!(!text.contains("50%"), "50% は上書きされて消える: {text:?}",);
        // OutputSpan 型の使用を明示するため型注釈（未使用変数抑制）。
        let _: &[OutputSpan] = &block.output;
    }

    // ── 05: DetachedOutput テスト ──────────────────────────────────────────────

    /// OSC 133 モードで D → ジョブ通知 → A の順で AppendDetached が発行される。
    #[test]
    fn session_term_detached_output_emitted_after_end() {
        let mut t = SessionTerm::new();
        // まず Osc133 モードに切り替える（D → A）。
        t.on_submit("");
        let _ = t.feed(b"\x1b]133;C\x07out\n\x1b]133;D;0\x07"); // D で Osc133 確定 + in_command=false
                                                                // D の後（at_prompt=false）にジョブ通知。
        let actions = t.feed(b"[1]  + done sleep 5\n");
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, TermAction::AppendDetached(_))),
            "D 後の出力は AppendDetached: {actions:?}",
        );
    }

    /// OSC 133 モードで A → プロンプト文字列 → C の間は AppendDetached が出ない。
    #[test]
    fn session_term_detached_output_not_emitted_in_prompt() {
        let mut t = SessionTerm::new();
        // Osc133 確定。
        t.on_submit("");
        let _ = t.feed(b"\x1b]133;C\x07out\n\x1b]133;D;0\x07");
        // A（at_prompt=true）→ プロンプト文字列 → C。
        let actions = t.feed(b"\x1b]133;A\x07user@host:~$ \x1b]133;C\x07");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, TermAction::AppendDetached(_))),
            "A〜C 間は AppendDetached を出さない: {actions:?}",
        );
    }

    /// 空白のみの区間外出力は AppendDetached が発行されない。
    #[test]
    fn session_term_detached_whitespace_only_not_emitted() {
        let mut t = SessionTerm::new();
        t.on_submit("");
        let _ = t.feed(b"\x1b]133;C\x07out\n\x1b]133;D;0\x07");
        let actions = t.feed(b"   \n  \n");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, TermAction::AppendDetached(_))),
            "空白のみは AppendDetached を出さない: {actions:?}",
        );
    }

    // ── 06: Auto モードエコー除去テスト ───────────────────────────────────────

    /// Auto モードで `on_submit("ls")` → `"ls\r\nfile1\n"` → エコー行だけ捨てられる。
    #[test]
    fn session_term_auto_echo_stripped() {
        let mut t = SessionTerm::new();
        t.on_submit("ls");
        let actions = t.feed(b"ls\r\nfile1\n");
        let text: String = actions
            .iter()
            .filter_map(|a| match a {
                TermAction::Append(spans) => {
                    Some(spans.iter().map(|s| s.text.as_str()).collect::<String>())
                }
                _ => None,
            })
            .collect();
        assert_eq!(text, "file1\n", "エコー行 'ls' が捨てられる: text={text:?}");
    }

    /// Auto モードで先頭行が echo と不一致の場合は何も捨てられない。
    #[test]
    fn session_term_auto_echo_mismatch_kept() {
        let mut t = SessionTerm::new();
        t.on_submit("ls");
        let actions = t.feed(b"file1\nfile2\n");
        let text: String = actions
            .iter()
            .filter_map(|a| match a {
                TermAction::Append(spans) => {
                    Some(spans.iter().map(|s| s.text.as_str()).collect::<String>())
                }
                _ => None,
            })
            .collect();
        assert!(
            text.contains("file1") && text.contains("file2"),
            "不一致時は何も捨てない: {text:?}",
        );
    }

    /// チャンク分断でも正しくエコー行だけ捨てられる。
    #[test]
    fn session_term_auto_echo_chunked() {
        let mut t = SessionTerm::new();
        t.on_submit("ls");
        let _ = t.feed(b"ls");
        let actions = t.feed(b"\r\nfile1\n");
        let text: String = actions
            .iter()
            .filter_map(|a| match a {
                TermAction::Append(spans) => {
                    Some(spans.iter().map(|s| s.text.as_str()).collect::<String>())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            text, "file1\n",
            "チャンク分断でもエコー行が捨てられる: {text:?}"
        );
    }

    // ── 07: alternate screen テスト ────────────────────────────────────────────

    /// `\x1b[?1049h` → AltScreen(true)。
    #[test]
    fn sgr_alt_screen_enter_1049() {
        let mut c = SgrConverter::new();
        let events = c.convert(b"\x1b[?1049h");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SgrEvent::AltScreen(true))),
            "AltScreen(true) が発行される: {events:?}",
        );
    }

    /// `\x1b[?1049l` → AltScreen(false)。
    #[test]
    fn sgr_alt_screen_leave_1049() {
        let mut c = SgrConverter::new();
        let events = c.convert(b"\x1b[?1049l");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SgrEvent::AltScreen(false))),
            "AltScreen(false) が発行される: {events:?}",
        );
    }

    /// `\x1b[?47h` と `\x1b[?1047h` も AltScreen(true)。
    #[test]
    fn sgr_alt_screen_47_and_1047() {
        let mut c = SgrConverter::new();
        let e1 = c.convert(b"\x1b[?47h");
        assert!(
            e1.iter().any(|e| matches!(e, SgrEvent::AltScreen(true))),
            "?47h は AltScreen(true): {e1:?}",
        );
        let e2 = c.convert(b"\x1b[?1047h");
        assert!(
            e2.iter().any(|e| matches!(e, SgrEvent::AltScreen(true))),
            "?1047h は AltScreen(true): {e2:?}",
        );
    }

    /// `\x1b[?25h`（カーソル表示）では AltScreen が発行されない。
    #[test]
    fn sgr_alt_screen_cursor_show_not_triggered() {
        let mut c = SgrConverter::new();
        let events = c.convert(b"\x1b[?25h");
        assert!(
            !events.iter().any(|e| matches!(e, SgrEvent::AltScreen(_))),
            "?25h では AltScreen を出さない: {events:?}",
        );
    }

    /// SessionTerm: `\x1b[?1049h` → TermAction::AltScreen(true)。
    #[test]
    fn session_term_alt_screen_action() {
        let mut t = SessionTerm::new();
        t.on_submit("vim");
        let actions = t.feed(b"\x1b]133;C\x07\x1b[?1049h");
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, TermAction::AltScreen(true))),
            "AltScreen(true) action: {actions:?}",
        );
    }

    /// SessionTerm: `\x1b[?1049l` → TermAction::AltScreen(false)。
    #[test]
    fn session_term_alt_screen_leave_action() {
        let mut t = SessionTerm::new();
        t.on_submit("vim");
        let actions = t.feed(b"\x1b]133;C\x07\x1b[?1049h\x1b[?1049l");
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, TermAction::AltScreen(false))),
            "AltScreen(false) action: {actions:?}",
        );
    }
}
