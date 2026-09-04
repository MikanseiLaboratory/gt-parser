# LLM / 機械向けオーサリング

生の `document.xml` は書かない。エンコード、既定属性の省略、GUID 再生成で壊れやすい。推奨ループは **IR JSON → `preview` HTML → `pack` GTZIP** である。

## 最小オブジェクト

トップレベル Layer の直下だけが vMix データフィールドになる。

| 種類 | フィールド | 備考 |
| --- | --- | --- |
| TextBlock / Text3D | `Name.Text` | |
| Image / QrCode / ImageSequence | `Name.Source` | 論理パスは `\` 区切り |
| Ticker | `Name.Text` またはテンプレート子ごと | 本文は Template に書く |
| Rectangle / Ellipse | `Name.Fill.Color` / `Name.Fill.Bitmap` | DataFlags が無い形状はイベント源にしない |

`Hidden` / `NoEvents` はイベント源に出さない。

## IR の約束

- Layer 子は `"node": "object"` または `"node": "layer"`。
- `Location` は **左上**（パース後の IR）。ファイルへ書くときは Anchor 点に戻す。
- Brush は `solid` / `linear_gradient` / `radial_gradient` / `picture`。
- アニメーションの `kind` は開放列挙。未知 Type を落とさない。
- オブジェクトあたり最大 3 本（`None` は数えない）。
- Image Sequence は 1 resource・複数 source。1 枚に潰さない。
- Storyboard は `(Type, DataName)` で識別。Type 省略は TransitionIn。

## CLI

```bash
gt-parser schema
gt-parser fields title.gtzip
gt-parser preview title.gtzip -o out
gt-parser pack ir.json -o out.gtzip --asset folder\\pic.png=./pic.png
```

## MCP

`gt_schema` / `gt_inspect` / `gt_preview` / `gt_validate` / `gt_assign_asset` / `gt_write` / `gt_convert` / `gt_list_storyboards` / `gt_evaluate_frame` / `gt_add_storyboard` / `gt_add_animation` / `gt_set_animation` / `gt_delete_animation`。

タイムライン操作は IR を手で書き換えず、これらのツールを使う。
