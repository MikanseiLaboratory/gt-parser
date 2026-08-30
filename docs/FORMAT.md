# GT Title 形式（逆引き）

公式スキーマは公開されていない。以下は [vMix GT Designer マニュアル](https://help.vmix.com/graphics/2/Introduction.html)、[pyGTGraphics](https://github.com/cyrillsemenov/pyGTGraphics)、および実 `.gtzip` の `document.xml` / `resources.xml` から再構成した理解である（第5段時点）。実ファイルで属性名が異なる場合は IR の unknown に保持し、警告を出す。

## パッケージ

### `.gtzip`

ZIP コンテナ。確認されているエントリ:

- `document.xml`（UTF-16 が多い。UTF-8 / BOM も受け付ける）
- `resources.xml`（論理パス ↔ ZIP 内 GUID エントリ）
- `[Content_Types].xml`（OPC 風）
- `thumbnail.png`（任意）
- 埋め込み画像。実ファイルでは拡張子なし GUID 名が多く、中身は PNG / JPEG / BMP / GIF

pyGTGraphics は `ZIP_STORED` で書くが、DEFLATE も読む。

`resources.xml` の例:

```xml
<resources>
  <resource filename="guid-folder\layout.png">
    <source guid="271c65b5-b7e2-487e-9651-8f8b61143f12">guid-folder\layout.png</source>
  </resource>
</resources>
```

同一 `<resource>` に複数 `<source>` がある場合は Image Sequence のフレーム列である。

### `.gtxml`

`document.xml` 相当のプレーン XML。画像は入力ファイル相対の外部パス参照。

## `document.xml` 骨格

```xml
<Composition Width="1920" Height="1080">
  <Layer Name="Layer 1" Dimensions="1920,1080,0" Locked="False">
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
- `Location`: `x,y,z`
- `Dimensions`: `width,height,depth`
- Storyboard の `Type` 省略は TransitionIn として扱う
- 実ファイルの折り返し属性は `TextWordWrapping` が多い（`WordWrapping` も受け付ける）

## HTML 化するオブジェクト

- `TextBlock`（テキスト属性 + 単色 Fill）
- `Rectangle` / `Ellipse` / `Triangle`（単色 / グラデーション / Picture Fill、SVG）
- `Image`（`<Image.Bitmap><Bitmap Source="..."/></Image.Bitmap>`）
- `Ticker`（`<Ticker.Template>` 内 TextBlock の文言、`Speed` は px/frame、既定 30fps）
- `Text3D` / `QrCode` / `ImageSequence`（ベストエフォート。QR は埋め込み画像優先）

## Fill / Stroke

- `Brush Color="#AARRGGBB"` 単色
- `LinearGradientBrush` / `RadialGradientBrush` + `GradientStop`（`Angle`、`Wrap`: Mirror / Clamp / Wrap）
- `Picture` / `PictureFill` / `ImageBrush`（`Source`、`SizeMode`）

## 画像

- Size Mode: Normal / Stretch / Centered（既定 Stretch）
- ZIP 内バイトは `resources.xml` の `guid` で引く。見つからなければパス直指定も試す

## Bounding / 配置

- `<Rectangle.Bounding><Bounding Object="Text 1" Padding="15,15,15,15"/></Rectangle.Bounding>`
- Padding は Left, Top, Right, Bottom（1値は全辺、2値は左右/上下）
- 静的スナップショット。実行時追従は第6段

## エフェクト

- `Opacity`（0–1。1 より大きい値は 0–100 と解釈）
- `Visible="False"` は `visibility:hidden`（マスク参照用に DOM は残す）
- `<*.Effects><Effect Type="Shadow" BlurAmount="9" Mode="Shadow"/>`
- `<Effect Type="Skew" Angle="x,y"/>`
- `<*.Crop><Crop Range="L,T,R,B" Feather="..."/></*.Crop>`（Range は 0–1）
- `<*.Mask><Mask Object="fons"/></*.Mask>`
- CompositingMode: Blend / Replace / Additive（後者二つは近似 + 警告）

## Storyboard / アニメーション

種別: TransitionIn（省略時）、TransitionOut、Continuous、DataChangeIn / Out（IR 維持、実行時は第6段）。

実ファイルで確認したタグ: `Reveal`, `Fade`, `Fly`, `ZoomFade`, `ImageSequence`, `None`。加えて `Move` / `Scale` / `Rotate` / `Wipe` / `Spin` も CSS 化する。

属性: `Object`, `Duration`, `Delay`, `Interpolation`, `Direction`, `Reverse`, `CenterAxis`。

補間: Linear, CubicEasingIn / Out / InOut, BounceIn / Out。オブジェクトあたり最大 3 本。

CLI `--storyboard` で対象を選ぶ。

## HTML 規約（後段の Wasm と共有）

- ルート: `.gt-stage`（Composition の幅・高さ、`position: relative`、透明背景）
- オブジェクト: `data-gt-name`, `data-gt-type`
- テキスト: `.gt-text`（`textContent` 差し替え可能）
- 画像: `.gt-image`、Ticker: `.gt-ticker`
- インラインスタイル中心。アニメーションは共有 `@keyframes` + `animation`
