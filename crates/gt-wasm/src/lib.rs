use gt_core::anim::{TimelineSegment, evaluate_segments, evaluate_storyboard};
use gt_core::edit::{
    AnimationPatch, add_animation, add_storyboard, delete_animation, set_animation,
};
use gt_core::fields::{list_fields, set_field};
use gt_core::write::{WriteAssets, write_gtzip_bytes};
use gt_core::{ConvertOptions, Package, convert_package_with};
use wasm_bindgen::prelude::*;

fn parse_document_json(json: &str) -> Result<gt_core::GtDocument, JsError> {
    serde_json::from_str(json).map_err(|error| JsError::new(&error.to_string()))
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String, JsError> {
    serde_json::to_string(value).map_err(|error| JsError::new(&error.to_string()))
}

#[wasm_bindgen]
pub fn parse_gtzip(bytes: &[u8]) -> Result<String, JsError> {
    let package = Package::from_zip_bytes(std::path::PathBuf::from("memory.gtzip"), bytes)
        .map_err(|error| JsError::new(&error.to_string()))?;
    let document = gt_core::parse::parse_document(&package.document_xml)
        .map_err(|error| JsError::new(&error.to_string()))?;
    let conversion = convert_package_with(&package, ConvertOptions::default(), Some(document))
        .map_err(|error| JsError::new(&error.to_string()))?;
    to_json(&conversion.document)
}

#[wasm_bindgen]
pub fn parse_gtxml(xml: &str) -> Result<String, JsError> {
    let document =
        gt_core::parse::parse_document(xml).map_err(|error| JsError::new(&error.to_string()))?;
    to_json(&document)
}

#[wasm_bindgen]
pub fn to_html(document_json: &str, storyboard: &str) -> Result<String, JsError> {
    let document = parse_document_json(document_json)?;
    let package = Package {
        kind: gt_core::package::PackageKind::Gtxml,
        path: std::path::PathBuf::from("memory.gtxml"),
        document_xml: String::new(),
        files: Default::default(),
        resources: Default::default(),
    };
    let conversion = convert_package_with(
        &package,
        ConvertOptions {
            embed_assets: true,
            storyboard: storyboard.to_string(),
        },
        Some(document),
    )
    .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(conversion.html)
}

#[wasm_bindgen]
pub fn write_gtzip(document_json: &str) -> Result<Vec<u8>, JsError> {
    write_gtzip_assets(document_json, "{}")
}

#[wasm_bindgen]
pub fn write_gtzip_assets(document_json: &str, assets_json: &str) -> Result<Vec<u8>, JsError> {
    let document = parse_document_json(document_json)?;
    let mut assets = WriteAssets::default();
    if let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(assets_json)
    {
        for (name, value) in map {
            if let Some(b64) = value.as_str() {
                assets.insert(name, decode_b64(b64).map_err(|error| JsError::new(&error))?);
            }
        }
    }
    write_gtzip_bytes(&document, &assets).map_err(|error| JsError::new(&error.to_string()))
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

#[wasm_bindgen]
pub fn list_data_fields(document_json: &str) -> Result<String, JsError> {
    let document = parse_document_json(document_json)?;
    to_json(&list_fields(&document))
}

#[wasm_bindgen]
pub fn apply_field(document_json: &str, field: &str, value: &str) -> Result<String, JsError> {
    let mut document = parse_document_json(document_json)?;
    set_field(&mut document, field, value);
    to_json(&document)
}

#[wasm_bindgen]
pub fn evaluate_frame(
    document_json: &str,
    storyboard_index: usize,
    time: f64,
) -> Result<String, JsError> {
    let document = parse_document_json(document_json)?;
    to_json(&evaluate_storyboard(&document, storyboard_index, time))
}

#[wasm_bindgen]
pub fn evaluate_view(
    document_json: &str,
    segments_json: &str,
    time: f64,
) -> Result<String, JsError> {
    let document = parse_document_json(document_json)?;
    let segments: Vec<TimelineSegment> =
        serde_json::from_str(segments_json).map_err(|error| JsError::new(&error.to_string()))?;
    to_json(&evaluate_segments(&document, &segments, time))
}

#[wasm_bindgen]
pub fn edit_add_storyboard(
    document_json: &str,
    ty: &str,
    data_name: &str,
) -> Result<String, JsError> {
    let mut document = parse_document_json(document_json)?;
    add_storyboard(
        &mut document,
        if ty.is_empty() {
            None
        } else {
            Some(ty.to_string())
        },
        if data_name.is_empty() {
            None
        } else {
            Some(data_name.to_string())
        },
    )
    .map_err(|error| JsError::new(&error.to_string()))?;
    to_json(&document)
}

#[wasm_bindgen]
pub fn edit_add_animation(
    document_json: &str,
    storyboard_index: usize,
    object: &str,
    kind: &str,
) -> Result<String, JsError> {
    let mut document = parse_document_json(document_json)?;
    add_animation(
        &mut document,
        storyboard_index,
        object,
        if kind.is_empty() { None } else { Some(kind) },
    )
    .map_err(|error| JsError::new(&error.to_string()))?;
    to_json(&document)
}

#[wasm_bindgen]
pub fn edit_set_animation(
    document_json: &str,
    storyboard_index: usize,
    animation_index: usize,
    patch_json: &str,
) -> Result<String, JsError> {
    let mut document = parse_document_json(document_json)?;
    let patch: AnimationPatch = serde_json::from_str(patch_json).unwrap_or_default();
    set_animation(&mut document, storyboard_index, animation_index, patch)
        .map_err(|error| JsError::new(&error.to_string()))?;
    to_json(&document)
}

#[wasm_bindgen]
pub fn edit_delete_animation(
    document_json: &str,
    storyboard_index: usize,
    animation_index: usize,
) -> Result<String, JsError> {
    let mut document = parse_document_json(document_json)?;
    delete_animation(&mut document, storyboard_index, animation_index)
        .map_err(|error| JsError::new(&error.to_string()))?;
    to_json(&document)
}
