//! Phase 1 mock UI のパネル別実装。
//!
//! `app.rs` の `update()` から panel 単位で呼び出される構成。各 panel は
//! `&mut AppState` を受け取り自前で UI 描画＋状態更新を行う。

pub mod central;
pub mod left_rail;
pub mod right_rail;
pub mod statusbar;
pub mod toast;
pub mod topbar;
pub mod widgets;
