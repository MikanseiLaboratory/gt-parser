# 技術スタック

このファイルは本リポジトリの正とする。版の変更は Issue で提案し、承認を得てから行う。

検証日: 2026-09-04（crates.io 上の最新安定版。既存クレートの版は据え置き）。

## 言語 / ビルド

| 項目 | 版 |
| --- | --- |
| Rust | 1.98.0（`rust-toolchain.toml` は `stable`） |
| Edition | 2024 |
| MSRV | 1.98（検証時の stable。`zip` 8.6 は 1.88 以上） |
| Cargo workspace resolver | 3 |
| CI | GitHub Actions（fmt / clippy / test / CLI smoke） |

## クレート

| クレート | 版 | 用途 |
| --- | --- | --- |
| quick-xml | 0.42.0 | `document.xml` のイベント駆動パーサ |
| zip | 8.6.0 | `.gtzip` の展開と書き込み（STORED / DEFLATE） |
| clap | 4.6.6 | CLI（derive） |
| thiserror | 2.0.20 | `gt-core` のエラー型 |
| anyhow | 1.0.104 | CLI のエラー報告 |
| html-escape | 0.2.15 | HTML テキスト / 属性エスケープ |
| serde | 1.0.229 | inspect / IR / warnings の JSON |
| serde_json | 1.0.151 | JSON 入出力 |
| pretty_assertions | 1.4.1 | テスト差分 |
| tokio | 1.53.1 | 非同期ランタイム。`fs` / `io-std` / `rt` / `rt-multi-thread` / `macros` / `io-util` / `sync`。`gt-core` では optional feature `fs` |
| uuid | 1.18.1 | GTZIP 書き出し時の v4 GUID（feature `write`） |
| wasm-bindgen | 0.2.104 | `gt-wasm` の JS 境界 |

I/O は `tokio::fs` と `tokio::task::spawn_blocking`（ZIP 展開）を標準とする。XML パース・HTML 生成・評価器・書き出しは同期関数。CLI / MCP は `#[tokio::main]`。Wasm は `tokio` なし（`gt-core` の default-features を切る）。

MCP は公式 `rmcp` ではなく、stdio 上の JSON-RPC 2.0 を `gt-mcp` が直接話す（Tokio 1.53.1 と整合。ロジックは `gt-core` のみ）。

第1段では画像デコーダを入れない。アセットはパッケージ内のバイト列として保持するだけとする。

## ブラウザ

| 技術 | 用途 |
| --- | --- |
| wasm-bindgen / wasm-pack | `gt-core` を `wasm32-unknown-unknown` に載せる |
| Custom Element `<gt-graphic>` | 表示核。編集シェルはその上の薄い UI |
| 評価器フレーム | タイムライン再生。CLI convert のみ CSS `@keyframes` |

編集シェルはシステムフォント + 既存 `.gt-stage`。独自テーマは作らない。タイムラインの色はアニメ種類の識別のみ。

## 非目標

- GPU タイトルとのピクセル完全一致
- 独自の Web デザインシステム
- Tauri / GPUI（#9）
- 既存クレートの版上げ
