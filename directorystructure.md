# ディレクトリ構成

```
gt-parser/
  Cargo.toml                 # workspace（gt-core, gt-cli）
  rust-toolchain.toml        # channel = stable, rustfmt/clippy
  .github/workflows/ci.yml   # fmt / clippy / test / CLI smoke
  crates/gt-core/            # パッケージ読込・IR・パーサ・HTML レンダラ
  crates/gt-cli/             # バイナリ `gt-parser`
  crates/gt-wasm/            # 未作成。第6段 Issue で追加
  fixtures/synthetic/        # 合成 .gtxml
  fixtures/golden/           # ゴールデン HTML（実 GT はリポジトリに置かない）
  docs/FORMAT.md             # 逆引き GT スキーマ
  technologystack.md
  directorystructure.md
  README.md
```

## gt-core モジュール

| モジュール | 責務 |
| --- | --- |
| `package` | `.gtzip` / `.gtxml` の ingest、UTF-16/UTF-8 デコード、`resources.xml`、アセット列挙 |
| `model` | `GtDocument` IR（Layer / Object / Fill / Storyboard / unknown 保持） |
| `parse` | XML → IR |
| `resolve` | Bounding の静的解決と未対応機能の警告収集 |
| `render::html` | IR → HTML/CSS（画像・グラデーション・エフェクト・Storyboard・Ticker） |
| `warn` | 警告コード |

CLI・将来の Wasm / Web はすべて `gt-core` のみを呼び、変換ロジックを複製しない。ファイル I/O の標準は Tokio（`Package::open` / `convert_path` / CLI は async）。`convert_package` と XML パースは同期のままとする。
