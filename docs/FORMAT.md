# GT Title 形式（逆引き）

公式スキーマは公開されていない。以下は [vMix GT Designer マニュアル](https://help.vmix.com/graphics/2/Introduction.html) と [pyGTGraphics](https://github.com/cyrillsemenov/pyGTGraphics) の生成結果、およびフォーラム上の展開報告から再構成した第1段時点の理解である。実ファイルで属性名が異なる場合は IR の unknown に保持し、警告を出す。

## パッケージ

### `.gtzip`

ZIP コンテナ。確認されているエントリ:

- `document.xml`（UTF-16 が多い。UTF-8 / BOM も受け付ける）
- `resources.xml`
- `[Content_Types].xml`（OPC 風）
- `thumbnail.png`（任意）
- 埋め込み画像（PNG / JPEG / BMP / GIF）

pyGTGraphics は `ZIP_STORED` で書くが、DEFLATE も読む。

### `.gtxml`

`document.xml` 相当のプレーン XML。画像は外部パス参照。

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
  <Storyboard Type="TransitionIn">
    <Storyboard.Animations>
      <Reveal Object="Rect 1" Duration="2" Delay="0" Interpolation="Linear" Direction="Left"/>
    </Storyboard.Animations>
  </Storyboard>
</Composition>
```

- 色: `#AARRGGBB`
- `Location`: `x,y,z`
- `Dimensions`: `width,height,depth`

## 第1段で HTML 化するオブジェクト

- `TextBlock`（テキスト属性 + 単色 Fill）
- `Rectangle` / `Ellipse` / `Triangle`（単色 Fill / Stroke、SVG）

## パースするが HTML 化しない（警告）

Image, Ticker, Text3D, QR, Gradient / Picture Fill, Bounding 解決, Rotate, Opacity, Storyboard / Animation, 未知タグ・未知属性。

## HTML 規約（後段の Wasm と共有）

- ルート: `.gt-stage`（Composition の幅・高さ、`position: relative`、透明背景）
- オブジェクト: `data-gt-name`, `data-gt-type`
- テキスト: `.gt-text`（`textContent` 差し替え可能）
- インラインスタイル中心
