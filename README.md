# gt-parser

vMix GT Title Designer の `.gtzip` / `.gtxml` を解析し、HTML に変換する。ロジックは Rust の `gt-core` に置き、CLI から呼び出す。

## 必要環境

- Rust stable（検証: 1.98.0 / edition 2024）
- 非同期ランタイムは Tokio 1.53.1（CLI は `#[tokio::main]`）

## ビルド

```bash
cargo build --release
```

バイナリ: `target/release/gt-parser`

## 使い方

```bash
# HTML へ変換（outdir/index.html、warnings.json、必要なら outdir/assets/）
gt-parser convert path/to/title.gtzip -o outdir
gt-parser convert path/to/title.gtxml -o outdir

# 画像を data URI で埋め込み（自己完結 HTML）
gt-parser convert path/to/title.gtzip -o outdir --embed-assets

# 再生する Storyboard を指定（既定: TransitionIn。Type 省略も TransitionIn）
gt-parser convert path/to/title.gtzip -o outdir --storyboard TransitionOut

# IR の要約を JSON で表示
gt-parser inspect path/to/title.gtzip
```

`-o` を省略すると、入力ファイル名から `{stem}_html` ディレクトリを作る。

## できること（第1〜5段）

- Composition / Layer / TextBlock / Rectangle / Ellipse / Triangle
- Image（`Image.Bitmap` + `resources.xml` の GUID 対応、GTXML の相対パス）
- Picture Fill、Linear / Radial Gradient、Size Mode（未指定は縦横比維持の contain、明示の Normal / Stretch / Centered）
- Bounding + Padding の静的解決、Radius、Rotate
- Opacity、Shadow、Crop + Feather、Mask、Skew、Texture Flip、Reflection 近似
- Compositing: Blend は標準。Replace / Additive は `mix-blend-mode` で近似し警告
- Storyboard: TransitionIn / TransitionOut / Continuous。Reveal / Fade / Fly / ZoomFade / Move / Scale / Rotate
- Ticker（速度 px/frame、既定 30fps 換算）
- QR（埋め込み画像優先。生成ライブラリは未導入で警告）
- Text3D / Cube: 警告付きのベストエフォート近似
- Image Sequence（`resources.xml` の複数 `<source>` を CSS でループ）
- UTF-16 / UTF-8 の `document.xml`
- 未知タグ・属性は捨てずに警告

## まだできないこと

- 第6段: Wasm グラフィック要素と Web 運用（#7）
- 第7段: 運用形態（バッチ / 自己完結 HTML の配布形態 / API）（#8）
- AutoSize=Shrink の実行時追従、DataChangeIn / Out の実行時接続
- GPU タイトルとのピクセル完全一致

## CI

GitHub Actions（`.github/workflows/ci.yml`）で `cargo fmt`、`clippy`、`test`、CLI のスモーク実行を行います。

詳細は [technologystack.md](technologystack.md)、[directorystructure.md](directorystructure.md)、[docs/FORMAT.md](docs/FORMAT.md) を参照。
