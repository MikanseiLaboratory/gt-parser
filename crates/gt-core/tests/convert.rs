use std::io::{Cursor, Write};
use std::path::PathBuf;

use gt_core::model::{FillKind, ObjectKind};
use gt_core::package::{Package, decode_xml_bytes};
use gt_core::{convert_package, convert_path};
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

#[test]
fn parses_basic_gtxml_objects() {
    let conversion = convert_path(fixture("basic.gtxml")).unwrap();
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
    assert!(
        conversion
            .warnings
            .iter()
            .any(|warning| warning.code == "unsupported.storyboard")
    );
    assert!(
        conversion
            .warnings
            .iter()
            .any(|warning| warning.code == "unsupported.bounding")
    );
    assert!(conversion.html.contains("data-gt-name=\"Text 1\""));
    assert!(conversion.html.contains("HERE WE ARE"));
    assert!(conversion.html.contains("data-gt-type=\"Rectangle\""));
    assert!(conversion.html.contains("data-gt-type=\"Ellipse\""));
    assert!(conversion.html.contains("data-gt-type=\"Triangle\""));
}

#[test]
fn preserves_unknown_nodes_and_escapes_text() {
    let conversion = convert_path(fixture("unknown.gtxml")).unwrap();
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
            .any(|warning| warning.code == "unsupported.object.image")
    );
    assert!(conversion.html.contains("Hello &amp; GT"));
    assert!(!conversion.html.contains("data-gt-type=\"Image\""));
}

#[test]
fn utf16_gtzip_stored_and_deflated() {
    let xml = std::fs::read_to_string(fixture("basic.gtxml")).unwrap();
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

#[test]
fn golden_html_matches_basic_fixture() {
    let conversion = convert_path(fixture("basic.gtxml")).unwrap();
    let expected = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden/basic.html"),
    )
    .unwrap()
    .replace("\r\n", "\n");
    assert_eq!(conversion.html.replace("\r\n", "\n"), expected);
}

#[test]
fn solid_fill_parsed_from_brush() {
    let conversion = convert_path(fixture("basic.gtxml")).unwrap();
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
