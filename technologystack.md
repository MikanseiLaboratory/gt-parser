# 技術スタック

このファイルは本リポジトリの正とする。版の変更は Issue で提案し、承認を得てから行う。

検証日: 2026-08-29（crates.io 上の最新安定版）。

## 言語 / ビルド

| 項目 | 版 |
| --- | --- |
| Rust | 1.98.0（`rust-toolchain.toml` は `stable`） |
| Edition | 2024 |
| MSRV | 1.98（検証時の stable。`zip` 8.6 は 1.88 以上） |
| Cargo workspace resolver | 3 |
| CI | GitHub Actions（fmt / clippy / test / CLI smoke） |

## クレート（第1段）

| クレート | 版 | 用途 |
| --- | --- | --- |
| quick-xml | 0.42.0 | `document.xml` のイベント駆動パーサ |
| zip | 8.6.0 | `.gtzip` の展開（STORED / DEFLATE） |
| clap | 4.6.6 | CLI（derive） |
| thiserror | 2.0.20 | `gt-core` のエラー型 |
| anyhow | 1.0.104 | CLI のエラー報告 |
| html-escape | 0.2.15 | HTML テキスト / 属性エスケープ |
| serde | 1.0.229 | inspect / warnings の JSON |
| serde_json | 1.0.151 | JSON 入出力 |
| pretty_assertions | 1.4.1 | テスト差分 |
| tokio | 1.53.1 | 非同期ランタイム。I/O と CLI の標準。`fs` / `rt` / `rt-multi-thread` / `macros` / `io-util` / `sync` |

I/O は `tokio::fs` と `tokio::task::spawn_blocking`（ZIP 展開）を標準とする。XML パースと HTML 生成は CPU 処理のため同期関数のまま呼び出す。CLI は `#[tokio::main]`（マルチスレッド runtime）を使う。

第1段では画像デコーダを入れない。アセットはパッケージ内のバイト列として保持するだけとする。

## 後段で使う予定の技術（Issue で実装）

| 技術 | 用途 |
| --- | --- |
| wasm-bindgen / wasm-pack | `gt-core` を `wasm32-unknown-unknown` に載せる |
| Custom Element `<gt-graphic>` | グラフィック要素として読み込み Wasm 運用 |
| CSS / Web Animations | Storyboard 再生 |
| 薄い HTTP API（将来） | `gt-core` を呼ぶだけ。ロジックは複製しない |

## 非目標

- GT ファイルへの再パック
- GPU タイトルとのピクセル完全一致
- 独自の Web デザインシステム
