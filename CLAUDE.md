# CLAUDE.md

このファイルは tanaterm リポジトリで作業する Claude Code 向けのガイダンスです。
**会話・コードコメント・ドキュメントは日本語で。**

## プロジェクト概要

tanaterm（棚 = shelf）は **Rust + egui** 製の macOS 向けターミナル GUI。
ハンドオフ（`tmp/design_handoff_tanaterm/`）の **Warp 風 3 ペイン UI** へ全面リデザイン中。

- **現状**: Phase 0（基盤）+ Phase 1（mock データで対話 UI 完成）まで実装済み。次は **Phase 2（実 PTY 配線）**。
- **計画と仕様の正は `docs/` にある**:
  - `docs/redesign-plan.md` — フェーズ別 WBS と **Decision Log（確定仕様）**。作業前に必読。
  - `docs/block-boundary-spike.md` — **Phase 2 着手ゲート**（ブロック境界方式 OSC 133 / ヒューリスティックの判断待ち）。
- ⚠️ **`docs/` と `tmp/` は `.gitignore` でローカル管理**（コミットされない）。クローンした人の手元には無いので、仕様の正は本 CLAUDE.md にも要点を転記してある。

## ビルド・テスト・Lint（CI と必ず一致させること）

`.github/workflows` の CI は以下を実行する。**push 前に必ずローカルで同じものを通す**:

```bash
cargo fmt --all -- --check                  # 手で整形しただけだと落ちる。先に `cargo fmt --all` を実行
cargo clippy --all-targets -- -D warnings   # warning も error 扱い。`--no-deps` だけでは不十分
cargo build --locked
```

- 過去に CI 失敗 #1 = フォーマット未整形だった。**コミット前に `cargo fmt --all` は必須**。
- テスト: `cargo test`（`state.rs` のロジックを `#[cfg(test)]` で担保。UI レイアウトは egui 依存でユニットテスト困難）。
- 実行: `cargo run`、またはリリースビルド `cargo build --release` → `./target/release/tanaterm`。

## Git / PR

- **既定ブランチは `develop`**（master/main ではない）。機能ブランチを切り、PR は `develop` 向け。develop へ直接 push しない。
- マージは **squash**（`... (#N)` のコミットになる）。
- PR は **`gh` で作成**。このリポジトリには PR テンプレも `.claude/` も無い（mitsucari の `/create-pr` スキルは使わない）。
- コミットメッセージ末尾に `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`。

## アーキテクチャ（`src/`）

- `main.rs` — エントリ。Config ロード、eframe 起動（`persistence` feature 有効）。
- `app.rs` — `TanaTermApp`（オーケストレータ）。`update()` でパネル描画、キーボードショートカット、theme 適用、アイコン登録。
- `theme.rs` — warm-dark デザイントークン（`Color32` 定数）+ egui `Visuals/Style`。**色は必ずここから参照**。
- `state.rs` — `AppState` / `Session` / `Block` / `ShelfItem` / `Command` / `UiState` + seed フィクスチャ + ロジックメソッド（テスト対象）。
- `clock.rs` — 時刻フォーマット（mock 用、JST）。
- `ui/` — パネル別: `topbar` / `left_rail`（Sessions+Shelf）/ `right_rail`（Commands）/ `central`（ターミナル+入力）/ `statusbar` / `toast` / `widgets`（共通 atoms）。
- `pty.rs` — portable-pty + vt100。**Phase 0/1 では未配線（`#[allow(dead_code)]`）**。Phase 2 で `app`/`central` から呼ぶ。
- `config.rs` — `config.toml`（shell / font_size / login_shell）の永続化。
- `assets/icon/` — `keep.svg`/`edit.svg` と、それを白塗り PNG 化した `keep.png`/`edit.png`（`include_bytes!` で埋め込み）。

## 確定した設計方針（Decision Log 要点）

- **テーマは warm-dark 単一固定**（paper / accent 切替 / テーマトグルなし）。
- **density は regular 単一固定**（行高26 / gap8 / pad12×8）。
- **フォントは egui デフォルト**（JetBrains Mono 同梱しない）。
- **git 非前提**: ブランチ/dirty 等の git 情報は表示・データモデルとも**持たない**。
- **Commands**: `COMMANDS` 親見出し下に Pinned / Recent を縦並び常時表示（タブ・検索欄なし）。pin で両者間を移動。使用回数・カウント数値は出さない。
- **Shelf**: フィルタ chip なし。ラベルは行頭 edit アイコンで inline 編集。「★ add current folder」で現在 pwd を登録。「open ↵」はテキストのみで行全体クリックで開く。
- **セクション高さ**（SESSIONS/PINNED）はドラッグでリサイズ可。高さは eframe persistence（`app.ron`）で永続化。
- **アイコン**: SVG を `rsvg-convert` で白塗り PNG 化 → texture を tint 描画（resvg 等の重い実行時依存は入れない）。

## egui 実装の落とし穴（ハマりどころ・必読）

- **SidePanel は content の min_rect 幅を消費する**（`exact_width` は描画/clip 幅を固定するだけ）。行が `ui.available_width()` を読むと「行が読む→パネルが content 幅に広がる→次フレームで available が増える」という正のフィードバックで rail が無制限に広がる（rail とターミナルの間に余白が出る不具合の真因）。
  → `widgets::row` は **ScrollArea の外で 1 度だけ確定した固定幅**を受け取る。長文は `widgets::truncating_line`（1行 ellipsis 切り詰め）で吸収する。
- **行クリック判定**: 子ラベル/ピルが上に乗ると、行下の click sense に当たらず「テキスト上でクリックが効かない」。
  → `row` は子描画**後**に行全体へ `ui.interact` を被せる。行内サブボタン（close X / pin / edit）は `Response::interact_pointer_pos()` と各 rect の `contains` で振り分ける。
- **hover 判定は `ui.rect_contains_pointer(rect)`**（`Response::hovered()` は子要素 occlusion でテキスト上だと外れる）。hover 中は `set_cursor_icon(PointingHand)`。
- **`CentralPanel` は最後に show する**（前に `TopBottomPanel`/`SidePanel`）。ネストパネルも `show_inside` で同様。
- **リストは `ScrollArea::vertical().auto_shrink([false, false])`** でパネルの残り高さを埋める（縦の余り防止）。
- **eframe persistence は clean 終了時のみ保存**。`app.ron` はウィンドウを閉じた（⌘Q）時に書かれる。強制 kill では保存されない。

## アプリ起動・スクリーンショット（UI 確認手順 / macOS）

UI 変更は Claude 自身でスクショ確認できる:

1. `./target/release/tanaterm 2>/dev/null &` で起動。
2. `osascript` (System Events) で最前面化＆ウィンドウ矩形取得（**アクセシビリティ権限が必要**。無いと -1728 エラー）。
3. `screencapture -R<x,y,w,h> /tmp/shot.png` でウィンドウ単体を撮影 → Read で確認。
   - ウィンドウは別ディスプレイ/Space に出ることがありフルスクリーン撮影に写らないため、矩形指定の `-R` を使う。
4. 停止は **`for pid in $(pgrep -x tanaterm); do kill -9 $pid; done`**（この環境では `pkill -x` が効かない）。
