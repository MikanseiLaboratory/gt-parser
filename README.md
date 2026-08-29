# gt-parser

vMix GT Title Designer の `.gtzip` / `.gtxml` を解析し、HTML に変換する。ロジックは Rust の `gt-core` に置き、第1段は CLI のみ。

## 必要環境

- Rust stable（検証: 1.98.0 / edition 2024）

## ビルド

```bash
cargo build --release
```

バイナリ: `target/release/gt-parser`

## 使い方

```bash
# HTML へ変換（outdir/index.html と warnings.json）
gt-parser convert path/to/title.gtzip -o outdir
gt-parser convert path/to/title.gtxml -o outdir

# IR の要約を JSON で表示
gt-parser inspect path/to/title.gtzip
```

`-o` を省略すると、入力ファイル名から `{stem}_html` ディレクトリを作る。

## 第1段でできること

- Composition / Layer / TextBlock / Rectangle / Ellipse / Triangle
- 単色 Fill / Stroke（`#AARRGGBB`）
- 絶対配置 HTML/CSS（SVG シェイプ）
- UTF-16 / UTF-8 の `document.xml`
- 未知タグ・属性・Storyboard などを捨てずに警告

## まだできないこと

画像、グラデーション、エフェクト、アニメーション、Ticker / Text3D / QR、Web UI、Wasm グラフィック要素。これらは GitHub Issue（第2段以降）で追跡する。

詳細は [technologystack.md](technologystack.md)、[directorystructure.md](directorystructure.md)、[docs/FORMAT.md](docs/FORMAT.md) を参照。
