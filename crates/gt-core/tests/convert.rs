use std::io::{Cursor, Write};
use std::path::PathBuf;

use gt_core::model::{FillKind, ObjectKind};
use gt_core::package::{Package, decode_xml_bytes};
use gt_core::{ConvertOptions, convert_package, convert_path, convert_path_with};
use pretty_assertions::assert_eq;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic")
        .join(name)
}

fn make_gtzip(xml: &str, stored: bool) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(if stored {
        CompressionMethod::Stored
    } else {
        CompressionMethod::Deflated
    });
    zip.start_file("document.xml", options).unwrap();
    let mut utf16 = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    zip.write_all(&utf16).unwrap();
    zip.finish().unwrap().into_inner()
}

#[tokio::test]
async fn parses_basic_gtxml_objects() {
    let conversion = convert_path(fixture("basic.gtxml")).await.unwrap();
    let report = conversion.document.inspect_report();
    assert_eq!(report.width, 1920.0);
    assert_eq!(report.height, 1080.0);
    assert_eq!(report.layers.len(), 1);
    let names: Vec<_> = report.layers[0]
        .objects
        .iter()
        .map(|object| (object.kind.as_str(), object.name.as_str()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Rectangle", "Rect 1"),
            ("Ellipse", "Circle 1"),
            ("Triangle", "Tri 1"),
            ("TextBlock", "Text 1"),
        ]
    );
    assert!(conversion.html.contains("data-gt-name=\"Text 1\""));
    assert!(conversion.html.contains("HERE WE ARE"));
    assert!(conversion.html.contains("data-gt-type=\"Rectangle\""));
    assert!(conversion.html.contains("data-gt-type=\"Ellipse\""));
    assert!(conversion.html.contains("data-gt-type=\"Triangle\""));
    assert!(conversion.html.contains("gt-reveal-left"));
    let rect = conversion.document.layers[0]
        .objects
        .iter()
        .find_map(|child| match child {
            gt_core::model::LayerChild::Object(object) if object.name == "Rect 1" => Some(object),
            _ => None,
        })
        .unwrap();
    assert!((rect.location.x - 45.0).abs() < f64::EPSILON);
    assert!((rect.dimensions.x - 1890.0).abs() < f64::EPSILON);
    assert!(
        !conversion
            .warnings
            .iter()
            .any(|warning| warning.code == "unsupported.bounding")
    );
}

#[tokio::test]
async fn preserves_unknown_nodes_and_escapes_text() {
    let conversion = convert_path(fixture("unknown.gtxml")).await.unwrap();
    let layer = &conversion.document.layers[0];
    let objects: Vec<_> = layer
        .objects
        .iter()
        .map(|child| match child {
            gt_core::model::LayerChild::Object(object) => object,
            gt_core::model::LayerChild::Layer(_) => panic!("unexpected nested layer"),
        })
        .collect();
    assert_eq!(objects[0].kind, ObjectKind::Image);
    assert_eq!(objects[1].text.as_deref(), Some("Hello & GT"));
    assert_eq!(objects[2].kind, ObjectKind::Unknown);
    assert_eq!(objects[2].tag, "FutureWidget");
    assert!(
        conversion
            .warnings
            .iter()
            .any(|warning| warning.code == "unsupported.image.source")
    );
    assert!(conversion.html.contains("Hello &amp; GT"));
    assert!(conversion.html.contains("data-gt-type=\"Image\""));
}

#[tokio::test]
async fn utf16_gtzip_stored_and_deflated() {
    let xml = tokio::fs::read_to_string(fixture("basic.gtxml"))
        .await
        .unwrap();
    for stored in [true, false] {
        let bytes = make_gtzip(&xml, stored);
        let package = Package::from_zip_bytes(PathBuf::from("memory.gtzip"), &bytes).unwrap();
        assert!(package.document_xml.contains("HERE WE ARE"));
        let conversion = convert_package(&package).unwrap();
        assert_eq!(conversion.document.width, 1920.0);
        let text = conversion.document.layers[0]
            .objects
            .iter()
            .find_map(|child| match child {
                gt_core::model::LayerChild::Object(object)
                    if object.kind == ObjectKind::TextBlock =>
                {
                    object.text.as_deref()
                }
                _ => None,
            });
        assert_eq!(text, Some("HERE WE ARE"));
    }
}

#[test]
fn decode_utf16_without_bom_when_xml_marker_present() {
    let xml = "<Composition Width=\"2\" Height=\"3\"/>";
    let mut bytes = Vec::new();
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    assert_eq!(decode_xml_bytes(&bytes).unwrap(), xml);
}

#[tokio::test]
async fn golden_html_matches_basic_fixture() {
    let conversion = convert_path(fixture("basic.gtxml")).await.unwrap();
    let expected = tokio::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden/basic.html"),
    )
    .await
    .unwrap()
    .replace("\r\n", "\n");
    assert_eq!(conversion.html.replace("\r\n", "\n"), expected);
}

#[tokio::test]
async fn solid_fill_parsed_from_brush() {
    let conversion = convert_path(fixture("basic.gtxml")).await.unwrap();
    let rect = conversion.document.layers[0]
        .objects
        .iter()
        .find_map(|child| match child {
            gt_core::model::LayerChild::Object(object) if object.name == "Rect 1" => Some(object),
            _ => None,
        })
        .unwrap();
    match &rect.fill.kind {
        FillKind::Solid { color } => {
            assert_eq!((color.a, color.r, color.g, color.b), (255, 255, 0, 0));
        }
        other => panic!("expected solid fill, got {other:?}"),
    }
}

#[tokio::test]
async fn images_and_picture_fill_emit_assets() {
    let conversion = convert_path(fixture("image.gtxml")).await.unwrap();
    assert!(conversion.html.contains("data-gt-type=\"Image\""));
    assert!(conversion.html.contains("object-fit:fill"));
    assert!(conversion.html.contains("object-fit:contain"));
    assert!(conversion.html.contains("assets/tiny.png") || conversion.html.contains("tiny.png"));
    assert!(conversion.html.contains("url(#gt-pat-"));
    assert!(!conversion.assets.is_empty());
}

#[tokio::test]
async fn embed_assets_uses_data_uri() {
    let conversion = convert_path_with(
        fixture("image.gtxml"),
        ConvertOptions {
            embed_assets: true,
            storyboard: "TransitionIn".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(conversion.html.contains("data:image/png;base64,"));
    assert!(conversion.assets.is_empty());
}

#[tokio::test]
async fn gradients_rotate_and_radius_render() {
    let conversion = convert_path(fixture("gradient.gtxml")).await.unwrap();
    assert!(conversion.html.contains("linearGradient"));
    assert!(conversion.html.contains("radialGradient"));
    assert!(conversion.html.contains("rotate(15.000deg)"));
    assert!(conversion.html.contains("rx=\"8.000\""));
}

#[tokio::test]
async fn effects_opacity_shadow_crop_and_blend() {
    let conversion = convert_path(fixture("effects.gtxml")).await.unwrap();
    assert!(conversion.html.contains("opacity:0.500"));
    assert!(conversion.html.contains("drop-shadow"));
    assert!(conversion.html.contains("clip-path:inset"));
    assert!(conversion.html.contains("mix-blend-mode:plus-lighter"));
    assert!(conversion.html.contains("mask-image:url"));
    assert!(
        conversion
            .warnings
            .iter()
            .any(|warning| warning.code == "approximate.compositing")
    );
}

#[tokio::test]
async fn storyboard_switch_changes_animation() {
    let inn = convert_path(fixture("storyboard.gtxml")).await.unwrap();
    assert!(inn.html.contains("gt-reveal-left"));
    assert!(inn.html.contains("gt-fade-in"));
    let out = convert_path_with(
        fixture("storyboard.gtxml"),
        ConvertOptions {
            embed_assets: false,
            storyboard: "TransitionOut".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(out.html.contains("gt-fly-bottom"));
    assert!(!out.html.contains("animation:gt-reveal-left"));
}

#[tokio::test]
async fn ticker_scrolls_from_template() {
    let conversion = convert_path(fixture("ticker.gtxml")).await.unwrap();
    assert!(conversion.html.contains("data-gt-type=\"Ticker\""));
    assert!(conversion.html.contains("BREAKING NEWS"));
    assert!(conversion.html.contains("gt-ticker-left"));
}

#[tokio::test]
async fn specials_qr_text3d_and_empty_qr_warning() {
    let conversion = convert_path(fixture("specials.gtxml")).await.unwrap();
    assert!(conversion.html.contains("data-gt-type=\"Text3D\""));
    assert!(conversion.html.contains("data-gt-type=\"QrCode\""));
    assert!(conversion.html.contains("data-gt-type=\"ImageSequence\""));
    assert!(
        conversion
            .warnings
            .iter()
            .any(|warning| warning.code == "approximate.text3d")
    );
    assert!(
        conversion
            .warnings
            .iter()
            .any(|warning| warning.code == "approximate.cube")
    );
    assert!(
        conversion
            .warnings
            .iter()
            .any(|warning| warning.code == "unsupported.qr.generation")
    );
}

#[tokio::test]
async fn gtzip_with_resources_maps_guid_images() {
    let png = tokio::fs::read(fixture("tiny.png")).await.unwrap();
    let xml = r#"<Composition Width="10" Height="10"><Layer Name="L" Dimensions="10,10,0"><Layer.Composition><Composition Width="10" Height="10"><Image Name="Pic" Dimensions="10,10,0" Location="0,0,0"><Image.Bitmap><Bitmap Source="folder\tiny.png"/></Image.Bitmap></Image></Composition></Layer.Composition></Layer></Composition>"#;
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("document.xml", options).unwrap();
    zip.write_all(xml.as_bytes()).unwrap();
    zip.start_file("resources.xml", options).unwrap();
    zip.write_all(
        br#"<resources><resource filename="folder\tiny.png"><source guid="abc-guid">folder\tiny.png</source></resource></resources>"#,
    )
    .unwrap();
    zip.start_file("abc-guid", options).unwrap();
    zip.write_all(&png).unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let package = Package::from_zip_bytes(PathBuf::from("mapped.gtzip"), &bytes).unwrap();
    let conversion = convert_package(&package).unwrap();
    assert!(!conversion.assets.is_empty());
    assert!(conversion.html.contains("data-gt-type=\"Image\""));
    assert!(conversion.html.contains("assets/"));
}
