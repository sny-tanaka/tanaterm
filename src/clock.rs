//! 時刻フォーマットヘルパ。
//!
//! Phase 1 では mock コマンドの time タグ表示と StatusBar の時計に使う。
//! Phase 2 で実 PTY と繋ぐ際には、本当の wallclock を別系統で渡す想定。

use std::time::{SystemTime, UNIX_EPOCH};

/// 端末ターゲットが日本国内向けなので JST 固定（chrono を入れない）。
const JST_OFFSET_SECS: u64 = 9 * 3600;

/// 現在時刻を "HH:MM" で返す（JST）。
pub fn now_hhmm() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        + JST_OFFSET_SECS;
    let hh = (secs / 3600) % 24;
    let mm = (secs / 60) % 60;
    format!("{hh:02}:{mm:02}")
}

/// 現在時刻を "HH:MM:SS" で返す（JST）。StatusBar 用。
pub fn now_hhmmss() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        + JST_OFFSET_SECS;
    let hh = (secs / 3600) % 24;
    let mm = (secs / 60) % 60;
    let ss = secs % 60;
    format!("{hh:02}:{mm:02}:{ss:02}")
}
