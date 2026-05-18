# tanaterm

Rust で書かれた macOS 向けのミニマルなターミナル GUI です。
[Warp](https://www.warp.dev/) や [iTerm2](https://iterm2.com/) の、ごく初期段階の小さな兄弟のようなものだと考えてください。

> The English README is available at [README.md](./README.md).

## ステータス

まだごく初期段階です。これはブートストラップ段階であり、tanaterm は
**1 つ**のウィンドウに**1 つ**のターミナルを開いてシェルを使えるようにするだけです。
意図的にそれだけにしています。タブ・分割・テーマ・コマンドパレットなどの
機能は今後追加していく予定です。

## 機能

- 実際の PTY に接続された、リサイズ可能な単一ターミナルウィンドウ
- 設定ファイルで `bash` / `zsh` を選択可能
- 256 色 ANSI レンダリング
- ログインシェル対応（`~/.zprofile` / `~/.bash_profile` などを読み込み）

## 必要環境

- macOS
- 比較的新しい安定版の [Rust ツールチェイン](https://rustup.rs/)（1.74 以降）

## ビルドと実行

```sh
git clone https://github.com/sny-tanaka/tanaterm.git
cd tanaterm
cargo run --release
```

## パッケージング（macOS .app）

`cargo run` は素のバイナリを起動するため、Finder では汎用実行ファイルの
アイコンで表示されます。Finder / Dock 用のアプリアイコンを付けるには
`.app` バンドルをビルドします。

```sh
./scripts/bundle-macos.sh
```

`target/release/tanaterm.app` が生成されます（アイコンは `assets/icon.png`
から macOS 標準の `sips` / `iconutil` で生成。追加ツール不要）。Finder で
ダブルクリックするか、`/Applications` にドラッグして使えます。

## 設定

初回起動時に、tanaterm はデフォルトの設定ファイルを書き出し、そのパスを
標準エラー出力に表示します。設定ファイルの場所は次のとおりです。

```
~/Library/Application Support/tanaterm/config.toml
```

例:

```toml
# 起動するシェル: "bash" または "zsh"
shell = "zsh"

# ターミナルのフォントサイズ（ポイント）
font_size = 14.0

# ログインシェルとして起動する（~/.zprofile, ~/.bash_profile などを読み込む）
login_shell = true
```

ファイルを編集し、tanaterm を再起動すると変更が反映されます。

| キー          | 型               | 既定値  | 説明                                            |
| ------------- | ---------------- | ------- | ----------------------------------------------- |
| `shell`       | `"bash"`/`"zsh"` | `"zsh"` | 起動するシェル（`/bin/bash` / `/bin/zsh`）      |
| `font_size`   | 浮動小数点       | `14.0`  | 等幅フォントのサイズ（ポイント）                |
| `login_shell` | 真偽値           | `true`  | シェルを `-l`（ログインシェル）で起動するか     |

## プロジェクト構成

| ファイル         | 役割                                             |
| ---------------- | ------------------------------------------------ |
| `src/main.rs`    | エントリポイント、ウィンドウ設定                 |
| `src/config.rs`  | `config.toml` の読み込み・書き出し               |
| `src/pty.rs`     | PTY 上でのシェル起動、読み取りスレッド            |
| `src/app.rs`     | ターミナルグリッドの egui 描画とキー入力処理      |
| `scripts/bundle-macos.sh` | リリースバイナリを `.app` バンドル化する |

## ライセンス

[MIT](./LICENSE)
