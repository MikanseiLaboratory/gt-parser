use std::path::PathBuf;

use gt_core::anim::{TimelineSegment, evaluate_segments, evaluate_storyboard};
use gt_core::edit::{
    AnimationPatch, add_animation, add_storyboard, delete_animation, set_animation,
};
use gt_core::fields::{list_fields, set_field};
use gt_core::schema::{AUTHORING_SCHEMA_JSON, FORMAT_SUMMARY};
use gt_core::write::{WriteAssets, write_gtzip_bytes};
use gt_core::{ConvertOptions, Package, convert_package_with, convert_path_with};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub async fn run_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(&line)?;
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(json!({}));
        let response = match method {
            "initialize" => ok(
                id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {},
                        "resources": {},
                        "prompts": {}
                    },
                    "serverInfo": { "name": "gt-mcp", "version": env!("CARGO_PKG_VERSION") }
                }),
            ),
            "notifications/initialized" => continue,
            "ping" => ok(id, json!({})),
            "tools/list" => ok(id, json!({ "tools": tools() })),
            "tools/call" => match call_tool(&params).await {
                Ok(result) => ok(id, result),
                Err(error) => fail(id, error),
            },
            "resources/list" => ok(
                id,
                json!({
                    "resources": [
                        { "uri": "gt://schema", "name": "Authoring schema", "mimeType": "application/json" },
                        { "uri": "gt://docs/format", "name": "GT format summary", "mimeType": "text/plain" }
                    ]
                }),
            ),
            "resources/read" => {
                let uri = params.get("uri").and_then(Value::as_str).unwrap_or("");
                let text = match uri {
                    "gt://schema" => AUTHORING_SCHEMA_JSON,
                    "gt://docs/format" => FORMAT_SUMMARY,
                    _ => "",
                };
                ok(
                    id,
                    json!({ "contents": [{ "uri": uri, "mimeType": "text/plain", "text": text }] }),
                )
            }
            "prompts/list" => ok(
                id,
                json!({
                    "prompts": [
                        { "name": "create_lower_third", "description": "Author a lower-third IR then pack to GTZIP" }
                    ]
                }),
            ),
            "prompts/get" => ok(
                id,
                json!({
                    "description": "Create a lower third",
                    "messages": [{
                        "role": "user",
                        "content": { "type": "text", "text": "Create a 1920x1080 lower third IR with a TextBlock named Title and a Rectangle bound to it. Preview HTML, then pack a GTZIP." }
                    }]
                }),
            ),
            _ => fail(id, format!("unknown method {method}")),
        };
        stdout
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        stdout.flush().await?;
    }
    Ok(())
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn fail(id: Value, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": message } })
}

fn tools() -> Vec<Value> {
    [
        "gt_schema",
        "gt_inspect",
        "gt_preview",
        "gt_validate",
        "gt_assign_asset",
        "gt_write",
        "gt_convert",
        "gt_list_storyboards",
        "gt_evaluate_frame",
        "gt_add_storyboard",
        "gt_add_animation",
        "gt_set_animation",
        "gt_delete_animation",
    ]
    .into_iter()
    .map(|name| {
        json!({
            "name": name,
            "description": name,
            "inputSchema": { "type": "object", "additionalProperties": true }
        })
    })
    .collect()
}

async fn call_tool(params: &Value) -> Result<Value, String> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let text = match name {
        "gt_schema" => format!("{FORMAT_SUMMARY}\n{AUTHORING_SCHEMA_JSON}"),
        "gt_inspect" => {
            let path = arg_path(&args)?;
            let report = gt_core::inspect_path(path).await.map_err(err)?;
            serde_json::to_string_pretty(&report).map_err(err)?
        }
        "gt_preview" | "gt_convert" => {
            let path = arg_path(&args)?;
            let conversion = convert_path_with(
                path,
                ConvertOptions {
                    embed_assets: true,
                    storyboard: args
                        .get("storyboard")
                        .and_then(Value::as_str)
                        .unwrap_or("TransitionIn")
                        .to_string(),
                },
            )
            .await
            .map_err(err)?;
            conversion.html
        }
        "gt_validate" => {
            let document = load_document(&args).await?;
            let fields = list_fields(&document);
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "fields": fields,
                "storyboards": document.storyboards.len()
            }))
            .map_err(err)?
        }
        "gt_list_storyboards" => {
            let document = load_document(&args).await?;
            let list: Vec<Value> = document
                .storyboards
                .iter()
                .map(|storyboard| {
                    json!({
                        "type": storyboard.effective_type(),
                        "data_name": storyboard.data_name,
                        "duration": storyboard.duration(),
                        "animations": storyboard.animations.len()
                    })
                })
                .collect();
            serde_json::to_string_pretty(&list).map_err(err)?
        }
        "gt_evaluate_frame" => {
            let document = load_document(&args).await?;
            let time = args.get("time").and_then(Value::as_f64).unwrap_or(0.0);
            let frame = if let Some(index) = args.get("storyboard_index").and_then(Value::as_u64) {
                evaluate_storyboard(&document, index as usize, time)
            } else if let Some(segments) = args.get("segments").and_then(Value::as_array) {
                let parsed: Vec<TimelineSegment> = segments
                    .iter()
                    .filter_map(|value| serde_json::from_value(value.clone()).ok())
                    .collect();
                evaluate_segments(&document, &parsed, time)
            } else {
                evaluate_storyboard(&document, 0, time)
            };
            serde_json::to_string_pretty(&frame).map_err(err)?
        }
        "gt_add_storyboard" => {
            let mut document = load_document(&args).await?;
            let index = add_storyboard(
                &mut document,
                args.get("type").and_then(Value::as_str).map(str::to_string),
                args.get("data_name")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            )
            .map_err(err)?;
            serde_json::to_string_pretty(&json!({ "index": index, "document": document }))
                .map_err(err)?
        }
        "gt_add_animation" => {
            let mut document = load_document(&args).await?;
            let index = add_animation(
                &mut document,
                args.get("storyboard_index")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize,
                args.get("object").and_then(Value::as_str).unwrap_or(""),
                args.get("kind").and_then(Value::as_str),
            )
            .map_err(err)?;
            serde_json::to_string_pretty(&json!({ "index": index, "document": document }))
                .map_err(err)?
        }
        "gt_set_animation" => {
            let mut document = load_document(&args).await?;
            set_animation(
                &mut document,
                args.get("storyboard_index")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize,
                args.get("animation_index")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize,
                AnimationPatch {
                    kind: args.get("kind").and_then(Value::as_str).map(str::to_string),
                    object: args
                        .get("object")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    delay: args
                        .get("delay")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    duration: args
                        .get("duration")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    interpolation: args
                        .get("interpolation")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    direction: args
                        .get("direction")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    reversed: args.get("reversed").and_then(Value::as_bool),
                    center_axis: args
                        .get("center_axis")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    speed: args
                        .get("speed")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    muted: args.get("muted").and_then(Value::as_bool),
                },
            )
            .map_err(err)?;
            serde_json::to_string_pretty(&document).map_err(err)?
        }
        "gt_delete_animation" => {
            let mut document = load_document(&args).await?;
            delete_animation(
                &mut document,
                args.get("storyboard_index")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize,
                args.get("animation_index")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize,
            )
            .map_err(err)?;
            serde_json::to_string_pretty(&document).map_err(err)?
        }
        "gt_assign_asset" => {
            let mut document = load_document(&args).await?;
            let object = args
                .get("object")
                .or_else(|| args.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let logical = args
                .get("logical")
                .or_else(|| args.get("source"))
                .and_then(Value::as_str)
                .unwrap_or(object);
            if object.is_empty() {
                return Err("object or name is required".into());
            }
            let field = if object.contains('.') {
                object.to_string()
            } else if list_fields(&document).iter().any(|field| {
                field.object.eq_ignore_ascii_case(object) && field.kind == "Fill.Bitmap"
            }) {
                format!("{object}.Fill.Bitmap")
            } else {
                format!("{object}.Source")
            };
            if !set_field(&mut document, &field, logical) {
                set_field(&mut document, &format!("{object}.Fill.Bitmap"), logical);
            }
            let mut payload =
                json!({ "document": document, "logical": logical.replace('/', "\\") });
            if let Some(b64) = args
                .get("bytes")
                .or_else(|| args.get("data"))
                .and_then(Value::as_str)
            {
                payload["asset_base64"] = json!(b64);
            }
            payload.to_string()
        }
        "gt_write" => {
            let document = load_document(&args).await?;
            let mut assets = WriteAssets::default();
            if let Some(map) = args.get("assets").and_then(Value::as_object) {
                for (name, value) in map {
                    if let Some(b64) = value.as_str() {
                        let bytes = decode_b64(b64)?;
                        assets.insert(name, bytes);
                    }
                }
            }
            let bytes = write_gtzip_bytes(&document, &assets).map_err(err)?;
            json!({ "gtzip_base64": encode_b64(&bytes) }).to_string()
        }
        _ => return Err(format!("unknown tool {name}")),
    };
    Ok(json!({
        "content": [{ "type": "text", "text": text }]
    }))
}

async fn load_document(args: &Value) -> Result<gt_core::GtDocument, String> {
    if let Some(document) = args.get("document") {
        return serde_json::from_value(document.clone()).map_err(err);
    }
    if let Some(path) = args.get("path").and_then(Value::as_str) {
        let mut package = Package::open(path).await.map_err(err)?;
        let document = gt_core::parse::parse_document(&package.document_xml).map_err(err)?;
        package.load_external_images(&document).await.map_err(err)?;
        let conversion = convert_package_with(&package, ConvertOptions::default(), Some(document))
            .map_err(err)?;
        return Ok(conversion.document);
    }
    Err("provide path or document".into())
}

fn arg_path(args: &Value) -> Result<PathBuf, String> {
    args.get("path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "path is required".into())
}

fn err<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}

fn encode_b64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (a << 16) | (b << 8) | c;
        out.push(TABLE[((triple >> 18) & 63) as usize] as char);
        out.push(TABLE[((triple >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn decode_b64(input: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("invalid base64".into()),
        }
    }
    let cleaned: Vec<u8> = input
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
        .collect();
    let mut out = Vec::new();
    for chunk in cleaned.chunks(4) {
        let a = val(chunk[0])? as u32;
        let b = chunk.get(1).copied().map(val).transpose()?.unwrap_or(0) as u32;
        let c = chunk.get(2).copied().map(val).transpose()?.unwrap_or(0) as u32;
        let d = chunk.get(3).copied().map(val).transpose()?.unwrap_or(0) as u32;
        let triple = (a << 18) | (b << 12) | (c << 6) | d;
        out.push(((triple >> 16) & 255) as u8);
        if chunk.len() > 2 {
            out.push(((triple >> 8) & 255) as u8);
        }
        if chunk.len() > 3 {
            out.push((triple & 255) as u8);
        }
    }
    Ok(out)
}
