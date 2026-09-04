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

#[tokio::test]
async fn real_gt_brush_type_is_not_flattened_to_solid() {
    let xml = r##"<Composition Width="100" Height="100">
  <Layer Name="L" Dimensions="100,100,0" Location="0,0,0">
    <Layer.Composition>
      <Composition Width="100" Height="100">
        <Rectangle Name="Grad" Dimensions="80,40,0" Location="50,30,0" Anchor="MiddleCenter" StrokeThickness="2">
          <Rectangle.Fill>
            <Brush Type="LinearGradient" Color="#FFFFFFFF" StartPoint="0,0.5" EndPoint="1,0.5">
              <Brush.Stops>
                <GradientStop Color="#FFFF0000" />
                <GradientStop Position="1" Color="#FF0000FF" />
              </Brush.Stops>
            </Brush>
          </Rectangle.Fill>
        </Rectangle>
      </Composition>
    </Layer.Composition>
  </Layer>
</Composition>"##;
    let document = gt_core::parse::parse_document(xml).unwrap();
    let rect = match &document.layers[0].objects[0] {
        gt_core::model::LayerChild::Object(object) => object,
        _ => panic!("expected object"),
    };
    assert!((rect.location.x - 10.0).abs() < f64::EPSILON);
    assert_eq!(rect.anchor.as_deref(), Some("MiddleCenter"));
    assert_eq!(rect.stroke.thickness, Some(2.0));
    match &rect.fill.kind {
        FillKind::LinearGradient {
            start, end, stops, ..
        } => {
            assert!((start.x - 0.0).abs() < f64::EPSILON);
            assert!((end.x - 1.0).abs() < f64::EPSILON);
            assert_eq!(stops.len(), 2);
        }
        other => panic!("expected linear gradient, got {other:?}"),
    }
}

#[tokio::test]
async fn write_gtzip_round_trips_objects_and_sequences() {
    let png = tokio::fs::read(fixture("tiny.png")).await.unwrap();
    let conversion = convert_path(fixture("basic.gtxml")).await.unwrap();
    let mut assets = gt_core::WriteAssets::default();
    assets.insert("folder\\tiny.png", png.clone());
    assets.sequences.insert(
        "folder\\tiny.png".to_string(),
        vec!["folder\\tiny.png".to_string()],
    );
    let bytes = gt_core::write_gtzip_bytes(&conversion.document, &assets).unwrap();
    let package = Package::from_zip_bytes(PathBuf::from("roundtrip.gtzip"), &bytes).unwrap();
    assert!(package.document_xml.contains("HERE WE ARE"));
    assert!(package.document_xml.contains("utf-8"));
    assert!(!package.document_xml.as_bytes().starts_with(&[0xFF, 0xFE]));
    let again = convert_package(&package).unwrap();
    assert_eq!(again.document.width, 1920.0);
    assert_eq!(
        again.document.layers[0].objects.len(),
        conversion.document.layers[0].objects.len()
    );
}

#[tokio::test]
async fn write_gtzip_keeps_sequence_sources_and_brush_type() {
    let png = tokio::fs::read(fixture("tiny.png")).await.unwrap();
    let xml = r##"<Composition Width="20" Height="20">
  <Layer Name="L" Dimensions="20,20,0">
    <Layer.Composition>
      <Composition Width="20" Height="20">
        <Rectangle Name="Grad" Dimensions="10,10,0" Location="5,5,0" Anchor="MiddleCenter" DataFlags="ShowVisible">
          <Rectangle.Fill>
            <Brush Type="LinearGradient" StartPoint="0,0.5" EndPoint="1,0.5">
              <Brush.Stops>
                <GradientStop Color="#FFFF0000" />
                <GradientStop Position="1" Color="#FF0000FF" />
              </Brush.Stops>
            </Brush>
          </Rectangle.Fill>
        </Rectangle>
        <Image Name="Seq" Dimensions="10,10,0" Location="0,0,0">
          <Image.Bitmap><Bitmap Source="folder\frame1.png"/></Image.Bitmap>
        </Image>
      </Composition>
    </Layer.Composition>
  </Layer>
</Composition>"##;
    let document = gt_core::parse::parse_document(xml).unwrap();
    let mut assets = gt_core::WriteAssets::default();
    assets.insert("folder\\frame1.png", png.clone());
    assets.insert("folder\\frame2.png", png);
    assets.sequences.insert(
        "folder\\frame1.png".to_string(),
        vec![
            "folder\\frame1.png".to_string(),
            "folder\\frame2.png".to_string(),
        ],
    );
    let bytes = gt_core::write_gtzip_bytes(&document, &assets).unwrap();
    let package = Package::from_zip_bytes(PathBuf::from("seq.gtzip"), &bytes).unwrap();
    assert!(package.document_xml.contains("Type=\"LinearGradient\""));
    assert!(package.document_xml.contains("DataFlags=\"ShowVisible\""));
    assert!(package.document_xml.contains("Anchor=\"MiddleCenter\""));
    assert_eq!(
        package
            .resources
            .path_to_sequence
            .values()
            .next()
            .map(Vec::len)
            .unwrap_or(0),
        2
    );
}

#[test]
fn open_animation_types_are_kept() {
    let xml = r#"<Composition Width="10" Height="10">
  <Layer Name="L" Dimensions="10,10,0"><Layer.Composition><Composition Width="10" Height="10">
    <Rectangle Name="R" Dimensions="10,10,0" Location="0,0,0" DataFlags="ShowVisible"/>
  </Composition></Layer.Composition></Layer>
  <Storyboard Type="Continuous">
    <Storyboard.Animations>
      <RotateContinuous Object="R" Speed="2" Direction="Right"/>
      <Blink Object="R"/>
    </Storyboard.Animations>
  </Storyboard>
</Composition>"#;
    let document = gt_core::parse::parse_document(xml).unwrap();
    assert_eq!(document.storyboards[0].animations.len(), 2);
    assert_eq!(
        document.storyboards[0].animations[0].kind,
        "RotateContinuous"
    );
    assert_eq!(
        document.storyboards[0].animations[0].speed.as_deref(),
        Some("2")
    );
    let rect = match &document.layers[0].objects[0] {
        gt_core::model::LayerChild::Object(object) => object,
        _ => panic!("object"),
    };
    assert_eq!(rect.data_flags.as_deref(), Some("ShowVisible"));
}
