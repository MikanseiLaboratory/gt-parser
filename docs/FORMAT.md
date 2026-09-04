# GT Title 形式（逆引き）

公式スキーマは公開されていない。正本は [GT-Plus GTZIP-Format.md](https://github.com/Coow/GT-Plus/blob/main/GTZIP-Format.md) と実 `.gtzip`、および本リポジトリの合成フィクスチャである。実ファイルで属性名が異なる場合は IR の unknown に保持し、警告を出す。

旧合成タグ（`<LinearGradientBrush>`、属性 `Rotate` の度数）は **読み取り互換**。書き出しは実 GT / GT-Plus 形にする。

## パッケージ

### `.gtzip`

ZIP コンテナ。読み取りは STORED / DEFLATE、UTF-16 / UTF-8（BOM あり・なし）を受け付ける。

書き出し（GT-Plus `GtZipWriter` 準拠）:

- `[Content_Types].xml`（OPC。GUID は `Override` + `application/octet-stream`）
- `document.xml`（**UTF-8・BOM なし**。vMix は BOM を拒否する）
- `resources.xml`（宣言なし UTF-8。シーケンスは 1 resource・複数 source。フレームを単独 resource に二重出力しない）
- GUID 名の生バイナリ（保存のたびに振り直す。バイト一致は目標にしない）

`resources.xml` の例:

```xml
<resources>
  <resource filename="guid-folder\layout.png">
    <source guid="271c65b5-b7e2-487e-9651-8f8b61143f12">guid-folder\layout.png</source>
  </resource>
</resources>
```

同一 `<resource>` に複数 `<source>` がある場合は Image Sequence のフレーム列である。順序が再生順。

### `.gtxml`

`document.xml` 相当のプレーン XML。画像は入力ファイル相対の外部パス参照。

## `document.xml` 骨格

```xml
<Composition Width="1920" Height="1080">
  <Layer Name="Layer 1" Dimensions="1920,1080,0">
    <Layer.Composition>
      <Composition Width="1920" Height="1080">
        <!-- objects -->
      </Composition>
    </Layer.Composition>
  </Layer>
  <Storyboard>
    <Storyboard.Animations>
      <Reveal Object="Layer 1" Interpolation="CubicEasingInOut"/>
    </Storyboard.Animations>
  </Storyboard>
  <Storyboard Type="TransitionOut">
    <Storyboard.Animations>
      <Fly Object="Layer 1" Interpolation="CubicEasingInOut" Direction="Bottom"/>
    </Storyboard.Animations>
  </Storyboard>
</Composition>
```

- 色: `#AARRGGBB`
- `Location`: ファイル上は **アンカー点**。IR は左上 `Location - (fx*W, fy*H)` を持つ
- `Dimensions`: `width,height,depth`
- `Anchor` 省略は TopLeft。既定属性は書き出しで省略する
- Storyboard の `Type` 省略は TransitionIn
- テキスト改行は属性内 CRLF。FontWeight は `Regular`（`Normal` ではない）
- 折り返し属性は `TextWordWrapping`（`WordWrapping` も読む）

## HTML 化するオブジェクト

- `TextBlock`（テキスト属性 + Fill）
- `Rectangle` / `Ellipse` / `Triangle`（単色 / グラデーション / Bitmap Fill、SVG）
- `Image`（`<Image.Bitmap><Bitmap Source="..." Position="..."/>`。Source は `\` 区切り）
- `Ticker`（`<Ticker.Template>` を保持。Speed 既定 1 / Direction=Left / Type=Replace）
- `Text3D` / `QrCode` / 未知タグはタグを保持して書き戻す

## Fill / Stroke

実 GT の Brush:

- `<Brush Color="#AARRGGBB">` 単色
- `<Brush Type="LinearGradient|RadialGradient" StartPoint="..." EndPoint="..." WrapX WrapY>` + `<Brush.Stops><GradientStop Position Color>`
- `<Brush Type="Bitmap">` + `<Brush.Bitmap><Bitmap Source>`

読み取り互換の旧タグ: `LinearGradientBrush` / `RadialGradientBrush`（`Angle` は Start/End に変換）、`Picture` / `PictureFill` / `ImageBrush`。

線幅は `StrokeThickness`（旧 `Thickness` も読む）。`StrokeStyle DashStyle`、Rectangle `Style=Square` を保持する。

## 画像

- Size Mode: Normal / Stretch / Centered / TopRight。**属性省略の既定は Centered**
- ZIP 内バイトは `resources.xml` の `guid` で引く

## Bounding / 配置

- `<Rectangle.Bounding><Bounding Object="Text 1" Padding="15,15,15,15"/></Rectangle.Bounding>`
- Padding は Left, Top, Right, Bottom（1値は全辺、2値は左右/上下）
- 解決は同一レイヤー内。ソースがさらに Bounding なら無効。1px 下限。非表示 / 透明オーナーはスキップ

## エフェクト

- `Opacity`（0–1。1 より大きい値は 0–100 と解釈）
- `Visible="False"` は `visibility:hidden`（マスク参照用に DOM は残す）
- `*.Transform` の `Rotate="rx,ry,rz"`（ラジアン）。旧属性 `Rotate`（度数）は読み取り互換
- `TextEffect`、Shadow / Skew / Crop / Mask / Flip / Reflection / CompositingMode

## DataFlags

オブジェクト属性。IR に残す。フィールド推論の入力。`Hidden` / `NoEvents` はイベント源にしない。

## Storyboard / アニメーション

種別: TransitionIn（省略時）、TransitionOut、Page1–10、DataChangeIn / Out、Continuous。

要素名は開放列挙。少なくとも `Fade` / `Fly` / `Reveal` / `ZoomFade` / `Move` / `Scale` / `Rotate` / `Bounce` / `Expand` / `Scroll` / `Hidden` / `RotateContinuous` / `ImageSequenceLoop` / `FillOffset` / `StrokeOffset` / `Blink` / `None` を保持する。

属性: `Object`, `Duration`, `Delay`, `Interpolation`, `Direction`, `Reverse`, `CenterAxis`, `Speed`。未知属性は `extra_attrs`。

補間名は GT どおり反転（`CubicEasingIn` = 減速）。オブジェクトあたり最大 3 本（`None` は数えない）。

評価器（`gt-core` `anim`）はドキュメントを書き換えない。`TransitionOut` と `DataChangeIn` は巻き戻し再生。連続系は Duration を無視し Speed で進める。`FillOffset` / `StrokeOffset` / `Blink` はノーオペでも落とさない。

CLI convert の静的 HTML は CSS `@keyframes`（1 ストーリーボード）。エディタとタイムラインは評価器のフレームを毎ティック載せる。

## HTML 規約（Wasm と共有）

- ルート: `.gt-stage`（Composition の幅・高さ、`position: relative`、透明背景）
- オブジェクト: `data-gt-name`, `data-gt-type`
- テキスト: `.gt-text`（`textContent` 差し替え可能）
- 画像: `.gt-image`、Ticker: `.gt-ticker`
- インラインスタイル中心。CLI 変換は共有 `@keyframes` + `animation`
