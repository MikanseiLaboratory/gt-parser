# ディレクトリ構成

```
gt-parser/
  Cargo.toml                 # workspace（gt-core, gt-cli, gt-mcp, gt-wasm）
  rust-toolchain.toml        # channel = stable, rustfmt/clippy
  .github/workflows/ci.yml   # fmt / clippy / test / CLI smoke
  crates/gt-core/            # パッケージ読込・IR・パーサ・書き出し・評価器
  crates/gt-cli/             # バイナリ `gt-parser`
  crates/gt-mcp/             # stdio MCP（gt-core を呼ぶだけ）
  crates/gt-wasm/            # Wasm API（gt-core を呼ぶだけ）
  web/editor/                # <gt-graphic> とタイムライン編集シェル
  fixtures/synthetic/        # 合成 .gtxml
  fixtures/golden/           # ゴールデン HTML（実 GT はリポジトリに置かない）
  docs/FORMAT.md             # 逆引き GT スキーマ
  docs/AUTHORING.md          # LLM / 機械向け IR 約束
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
| `write` | IR → `document.xml` / `.gtzip`（feature `write`） |
| `fields` | DataFlags 準拠の vMix フィールド一覧と代入 |
| `schema` | オーサリング JSON Schema と FORMAT 要約 |
| `anim` | 非破壊のフレーム評価（GT-Plus evaluator 相当） |
| `edit` | ストーリーボード / アニメーションの追加・変更・削除（3 本制限） |
| `render::html` | IR → HTML/CSS |
| `warn` | 警告コード |

CLI・MCP・Wasm / Web はすべて `gt-core` のみを呼び、変換ロジックを複製しない。ファイル I/O の標準は Tokio（`Package::open` / `convert_path` / CLI / MCP は async）。`convert_package`・XML パース・評価器・書き出しバイト列は同期のままとする。
