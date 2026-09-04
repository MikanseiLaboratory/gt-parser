# gt-parser

vMix GT Title Designer の `.gtzip` / `.gtxml` を解析し、HTML プレビューと GTZIP 書き出しを行う。ロジックは Rust の `gt-core` に置き、CLI / MCP / Wasm から呼び出す。

## 必要環境

- Rust stable（検証: 1.98.0 / edition 2024）
- 非同期ランタイムは Tokio 1.53.1（CLI は `#[tokio::main]`）

## ビルド

```bash
cargo build --release
```

バイナリ: `target/release/gt-parser`

ブラウザ編集用 Wasm:

```bash
wasm-pack build crates/gt-wasm --target web --out-dir ../../web/editor/pkg
```

`web/editor/` を静的サーバで開く。

## 使い方

```bash
# HTML へ変換（outdir/index.html、warnings.json、必要なら outdir/assets/）
gt-parser convert path/to/title.gtzip -o outdir
gt-parser convert path/to/title.gtxml -o outdir --embed-assets --storyboard TransitionOut

# 自己完結 HTML
gt-parser preview path/to/title.gtzip -o outdir

# IR の要約 / フィールド / スキーマ
gt-parser inspect path/to/title.gtzip
gt-parser fields path/to/title.gtzip
gt-parser schema

# IR JSON または既存タイトルから GTZIP を書く
gt-parser pack ir.json -o out.gtzip --asset folder\\pic.png=./pic.png
gt-parser pack title.gtzip -o out.gtzip

# MCP（stdio）
gt-parser mcp
```

Cursor の MCP 設定例:

```json
{
  "mcpServers": {
    "gt-parser": {
      "command": "gt-parser",
      "args": ["mcp"]
    }
  }
}
```

LLM は生 `document.xml` を書かず、IR JSON → preview → pack を使う。約束は [docs/AUTHORING.md](docs/AUTHORING.md)。

## できること

- Composition / Layer / TextBlock / Rectangle / Ellipse / Triangle / Image / Ticker
- 実 GT の Brush（Solid / LinearGradient / RadialGradient / Bitmap）、旧合成タグは読み取り互換
- Anchor 点、Transform の 3 軸回転、StrokeThickness、DataFlags、Ticker.Template
- Image Sequence（1 resource・複数 source）
- Bounding（同一レイヤー、循環ガード、1px 下限、非表示オーナー停止）
- Storyboard は開放列挙。評価器でスクラブ（ファイルは汚さない）
- GTZIP 書き出し（UTF-8 BOM なし、GUID 再生成）
- MCP とブラウザ編集（選択、フィールド、画像アサイン、タイムライン、GTZIP 書き出し）

## まだできないこと

- GPU タイトルとのピクセル完全一致
- ライブ Ticker の 60fps 物理シミュレーション
- Tauri / GPUI デスクトップ（#9）
- GT-Plus のガイド、動画書き出し、カーソルワープ式数値スクラブ

## CI

GitHub Actions（`.github/workflows/ci.yml`）で `cargo fmt`、`clippy`、`test`、CLI のスモーク実行を行います。

詳細は [technologystack.md](technologystack.md)、[directorystructure.md](directorystructure.md)、[docs/FORMAT.md](docs/FORMAT.md) を参照。
