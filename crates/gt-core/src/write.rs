//! Serialize a `GtDocument` back to GT Designer XML / `.gtzip`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write as IoWrite};
#[cfg(feature = "fs")]
use std::path::Path;

use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::error::{Error, Result};
use crate::fields::is_known_field;
use crate::model::{
    Animation, Bounding, Fill, FillKind, GtDocument, GtObject, Layer, LayerChild, Point2,
    Storyboard, UnknownNode, anchor_point_from_top_left, flatten_objects, trim_float,
};
use crate::package::{Package, sniff_image};
use crate::resolve::effective_storyboard_type;

#[derive(Debug, Clone, Default)]
pub struct WriteAssets {
    pub blobs: BTreeMap<String, Vec<u8>>,
    pub sequences: BTreeMap<String, Vec<String>>,
}

impl WriteAssets {
    pub fn from_package(package: &Package) -> Self {
        let mut blobs = BTreeMap::new();
        for (logical, guid) in &package.resources.path_to_entry {
            if let Some(bytes) = package.files.get(guid) {
                blobs.insert(logical.replace('/', "\\"), bytes.clone());
            }
        }
        if blobs.is_empty() {
            for (name, bytes) in &package.files {
                let lower = name.to_ascii_lowercase();
                if lower == "document.xml"
                    || lower == "resources.xml"
                    || lower == "[content_types].xml"
                    || lower.ends_with('/')
                {
                    continue;
                }
                blobs.insert(name.replace('/', "\\"), bytes.clone());
            }
        }
        let sequences = package
            .resources
            .path_to_sequence
            .iter()
            .filter(|(_, frames)| frames.len() > 1)
            .map(|(anchor, frames)| {
                let logicals: Vec<String> = frames
                    .iter()
                    .filter_map(|guid| {
                        package
                            .resources
                            .path_to_entry
                            .iter()
                            .find(|(_, entry)| *entry == guid)
                            .map(|(path, _)| path.replace('/', "\\"))
                            .or_else(|| Some(guid.clone()))
                    })
                    .collect();
                (anchor.replace('/', "\\"), logicals)
            })
            .collect();
        Self { blobs, sequences }
    }

    pub fn insert(&mut self, logical: impl Into<String>, bytes: Vec<u8>) {
        self.blobs.insert(logical.into().replace('/', "\\"), bytes);
    }
}

pub fn serialize_document_xml(document: &GtDocument) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str(&format!(
        "<Composition Width=\"{}\" Height=\"{}\">\n",
        trim_float(document.width),
        trim_float(document.height)
    ));
    for layer in &document.layers {
        out.push_str(&serialize_layer(layer, 1));
    }
    for storyboard in &document.storyboards {
        if storyboard.is_scoped()
            && !is_known_field(document, storyboard.data_name.as_deref().unwrap_or(""))
        {
            continue;
        }
        out.push_str(&serialize_storyboard(storyboard, 1));
    }
    out.push_str("</Composition>\n");
    out
}

pub fn write_gtzip_bytes(document: &GtDocument, assets: &WriteAssets) -> Result<Vec<u8>> {
    let guid_map = assign_guids(assets);
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(content_types_xml(&guid_map).as_bytes())?;

    zip.start_file("document.xml", options)?;
    zip.write_all(serialize_document_xml(document).as_bytes())?;

    zip.start_file("resources.xml", options)?;
    zip.write_all(resources_xml(assets, &guid_map).as_bytes())?;

    for (logical, bytes) in &assets.blobs {
        let guid = guid_map
            .get(logical)
            .ok_or_else(|| Error::Invalid(format!("missing guid for {logical}")))?;
        zip.start_file(guid, options)?;
        zip.write_all(bytes)?;
    }

    Ok(zip.finish()?.into_inner())
}

#[cfg(feature = "fs")]
pub async fn write_gtzip_path(
    path: impl AsRef<Path>,
    document: &GtDocument,
    assets: &WriteAssets,
) -> Result<()> {
    let bytes = write_gtzip_bytes(document, assets)?;
    let path = path.as_ref();
    let tmp = path.with_extension("gtzip_tmp");
    tokio::fs::write(&tmp, &bytes).await?;
    if let Err(error) = tokio::fs::rename(&tmp, path).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(error.into());
    }
    Ok(())
}

fn assign_guids(assets: &WriteAssets) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for key in assets.blobs.keys() {
        map.insert(key.clone(), uuid::Uuid::new_v4().to_string());
    }
    map
}

fn content_types_xml(guid_map: &BTreeMap<String, String>) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="text/xml" />
  <Default Extension="png" ContentType="image/png" />
"#,
    );
    for guid in guid_map.values() {
        out.push_str(&format!(
            "  <Override PartName=\"/{guid}\" ContentType=\"application/octet-stream\" />\n"
        ));
    }
    out.push_str("</Types>\n");
    out
}

fn resources_xml(assets: &WriteAssets, guid_map: &BTreeMap<String, String>) -> String {
    let mut out = String::from("<resources>");
    let mut claimed = BTreeSet::new();
    for (anchor, frames) in &assets.sequences {
        out.push_str(&format!(
            "<resource filename=\"{}\">",
            esc_attr(&anchor.replace('/', "\\"))
        ));
        for frame in frames {
            let key = frame.replace('/', "\\");
            if let Some(guid) = guid_map.get(&key).or_else(|| guid_map.get(frame)) {
                claimed.insert(key.clone());
                out.push_str(&format!(
                    "<source guid=\"{guid}\">{}</source>",
                    esc_attr(&key)
                ));
            }
        }
        out.push_str("</resource>");
    }
    for (logical, guid) in guid_map {
        if claimed.contains(logical) {
            continue;
        }
        let backslash = logical.replace('/', "\\");
        out.push_str(&format!(
            "<resource filename=\"{name}\"><source guid=\"{guid}\">{name}</source></resource>",
            name = esc_attr(&backslash)
        ));
    }
    out.push_str("</resources>");
    out
}

fn serialize_layer(layer: &Layer, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let inner_w = layer.inner_width.unwrap_or(layer.dimensions.x);
    let inner_h = layer.inner_height.unwrap_or(layer.dimensions.y);
    let mut out = format!(
        "{pad}<Layer Name=\"{name}\" Dimensions=\"{dim}\" Location=\"{loc}\"",
        name = esc_attr(&layer.name),
        dim = layer.dimensions.format_xyz(),
        loc = layer.location.format_xyz(),
    );
    if layer.locked {
        out.push_str(" Locked=\"True\"");
    }
    if !layer.visible {
        out.push_str(" Visible=\"False\"");
    }
    out.push_str(">\n");
    out.push_str(&format!(
        "{pad}  <Layer.Composition>\n{pad}    <Composition Width=\"{}\" Height=\"{}\">\n",
        trim_float(inner_w),
        trim_float(inner_h)
    ));
    for child in &layer.objects {
        match child {
            LayerChild::Object(object) => out.push_str(&serialize_object(object, indent + 3)),
            LayerChild::Layer(nested) => out.push_str(&serialize_layer(nested, indent + 3)),
        }
    }
    out.push_str(&format!(
        "{pad}    </Composition>\n{pad}  </Layer.Composition>\n{pad}</Layer>\n"
    ));
    out
}

fn serialize_object(object: &GtObject, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let tag = object.tag.as_str();
    let stored = anchor_point_from_top_left(
        &object.location,
        &object.dimensions,
        object.anchor.as_deref(),
    );
    let mut attrs = vec![
        ("Name", object.name.clone()),
        ("Dimensions", object.dimensions.format_xyz()),
        ("Location", stored.format_xyz()),
    ];
    if let Some(anchor) = &object.anchor
        && !anchor.eq_ignore_ascii_case("TopLeft")
    {
        attrs.push(("Anchor", anchor.clone()));
    }
    if !object.visible {
        attrs.push(("Visible", "False".to_string()));
    }
    if let Some(opacity) = object.opacity {
        attrs.push(("Opacity", trim_float(opacity)));
    }
    if object.locked {
        attrs.push(("Locked", "True".to_string()));
    }
    if let Some(flags) = data_flags_for_write(object) {
        attrs.push(("DataFlags", flags));
    }
    if object.kind == crate::model::ObjectKind::Image
        && let Some(mode) = &object.size_mode
        && !mode.eq_ignore_ascii_case("Centered")
    {
        attrs.push(("SizeMode", mode.clone()));
    }
    if let Some(text) = &object.text
        && object.kind != crate::model::ObjectKind::Ticker
    {
        attrs.push(("Text", encode_text_attr(text)));
    }
    push_text_attrs(&mut attrs, object);
    if let Some(auto) = &object.style.auto_size
        && !auto.eq_ignore_ascii_case("Fixed")
    {
        attrs.push(("AutoSize", auto.clone()));
    }
    if object.kind == crate::model::ObjectKind::Ticker
        && let Some(speed) = object.ticker_speed
        && (speed - 1.0).abs() > f64::EPSILON
    {
        attrs.push(("Speed", trim_float(speed)));
    }
    if object.kind == crate::model::ObjectKind::Ticker {
        if let Some(direction) = &object.ticker_direction
            && !direction.eq_ignore_ascii_case("Left")
        {
            attrs.push(("Direction", direction.clone()));
        }
        if let Some(kind) = &object.ticker_kind
            && !kind.eq_ignore_ascii_case("Replace")
        {
            attrs.push(("Type", kind.clone()));
        }
    }
    if let Some(thickness) = object.stroke.thickness {
        attrs.push(("StrokeThickness", trim_float(thickness)));
    }
    if let Some(style) = &object.rect_style
        && style.eq_ignore_ascii_case("Square")
    {
        attrs.push(("Style", style.clone()));
    }
    if let Some(radius) = object.radius {
        attrs.push(("Radius", trim_float(radius)));
    }
    let attr_xml = attrs
        .into_iter()
        .map(|(key, value)| format!("{key}=\"{}\"", esc_attr(&value)))
        .collect::<Vec<_>>()
        .join(" ");
    let mut children = String::new();
    if let Some(xyz) = &object.rotate_xyz
        && (xyz.x != 0.0 || xyz.y != 0.0 || xyz.z != 0.0)
    {
        children.push_str(&format!(
            "{pad}  <{tag}.Transform><Transform Rotate=\"{}\"/></{tag}.Transform>\n",
            xyz.format_xyz()
        ));
    }
    if !matches!(object.fill.kind, FillKind::Transparent) {
        children.push_str(&format!(
            "{pad}  <{tag}.Fill>{}</{tag}.Fill>\n",
            serialize_brush(&object.fill)
        ));
    }
    if !matches!(object.stroke.fill.kind, FillKind::Transparent)
        || object.stroke.thickness.is_some_and(|value| value > 0.0)
    {
        children.push_str(&format!(
            "{pad}  <{tag}.Stroke>{}</{tag}.Stroke>\n",
            serialize_brush(&object.stroke.fill)
        ));
    }
    if let Some(dash) = &object.stroke.dash_style
        && !dash.eq_ignore_ascii_case("Solid")
    {
        children.push_str(&format!(
            "{pad}  <{tag}.StrokeStyle><StrokeStyle DashStyle=\"{}\"/></{tag}.StrokeStyle>\n",
            esc_attr(dash)
        ));
    }
    if object.kind == crate::model::ObjectKind::Image
        && let Some(source) = &object.image_source
    {
        let mut bitmap = format!(
            "<Bitmap Source=\"{}\"",
            esc_attr(&source.replace('/', "\\"))
        );
        if let Some(position) = object.bitmap_position {
            bitmap.push_str(&format!(" Position=\"{}\"", trim_float(position)));
        }
        bitmap.push_str(" />");
        children.push_str(&format!("{pad}  <{tag}.Bitmap>{bitmap}</{tag}.Bitmap>\n"));
    }
    if object.kind == crate::model::ObjectKind::Ticker {
        let template = object
            .ticker_template
            .as_ref()
            .map(serialize_unknown)
            .unwrap_or_else(|| {
                format!(
                    "<TextBlock Text=\"{}\" />",
                    esc_attr(&encode_text_attr(object.text.as_deref().unwrap_or("")))
                )
            });
        children.push_str(&format!(
            "{pad}  <{tag}.Template>{template}</{tag}.Template>\n"
        ));
    }
    if let Some(crop) = &object.effects.crop {
        let mut crop_el = String::from("<Crop");
        if let Some(range) = &crop.range
            && range != "0,0,1,1"
        {
            crop_el.push_str(&format!(" Range=\"{}\"", esc_attr(range)));
        }
        if let Some(feather) = &crop.feather {
            crop_el.push_str(&format!(" Feather=\"{}\"", esc_attr(feather)));
        }
        crop_el.push_str(" />");
        children.push_str(&format!("{pad}  <{tag}.Crop>{crop_el}</{tag}.Crop>\n"));
    }
    if let Some(bounding) = &object.bounding
        && !bounding_is_default(bounding)
    {
        let mut node = String::from("<Bounding");
        if !bounding.object.is_empty() {
            node.push_str(&format!(" Object=\"{}\"", esc_attr(&bounding.object)));
        }
        if bounding.padding != "0,0,0,0" && !bounding.padding.is_empty() {
            node.push_str(&format!(" Padding=\"{}\"", esc_attr(&bounding.padding)));
        }
        node.push_str(" />");
        children.push_str(&format!("{pad}  <{tag}.Bounding>{node}</{tag}.Bounding>\n"));
    }
    if let Some(mask) = &object.effects.mask {
        children.push_str(&format!(
            "{pad}  <{tag}.Mask><Mask Object=\"{}\"/></{tag}.Mask>\n",
            esc_attr(mask)
        ));
    }
    if children.is_empty() {
        format!("{pad}<{tag} {attr_xml} />\n")
    } else {
        format!("{pad}<{tag} {attr_xml}>\n{children}{pad}</{tag}>\n")
    }
}

fn push_text_attrs(attrs: &mut Vec<(&str, String)>, object: &GtObject) {
    if !matches!(
        object.kind,
        crate::model::ObjectKind::TextBlock
            | crate::model::ObjectKind::Ticker
            | crate::model::ObjectKind::Text3D
    ) {
        return;
    }
    let style = &object.style;
    if let Some(family) = &style.font_family {
        attrs.push(("FontFamily", family.clone()));
    }
    if let Some(size) = style.font_size {
        attrs.push(("FontSize", trim_float(size)));
    }
    if let Some(weight) = &style.font_weight {
        let written = if weight.eq_ignore_ascii_case("Normal") {
            "Regular"
        } else {
            weight
        };
        attrs.push(("FontWeight", written.to_string()));
    }
    if let Some(align) = &style.text_align {
        attrs.push(("TextAlign", align.clone()));
    }
    if let Some(align) = &style.vertical_align {
        attrs.push(("VerticalAlign", align.clone()));
    }
    if let Some(spacing) = style.line_spacing {
        attrs.push(("LineSpacing", trim_float(spacing)));
    }
    if style.auto_upper_case || style.text_effect.as_deref() == Some("Uppercase") {
        attrs.push(("TextEffect", "Uppercase".to_string()));
    }
    if style.ignore_overhang.as_deref().is_some_and(truthy) {
        attrs.push(("IgnoreOverhang", "True".to_string()));
    }
    if style
        .word_wrapping
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("NoWrap"))
    {
        attrs.push(("TextWordWrapping", "NoWrap".to_string()));
    }
}

fn serialize_brush(fill: &Fill) -> String {
    match &fill.kind {
        FillKind::Solid { color } => format!("<Brush Color=\"{}\" />", color.to_argb()),
        FillKind::Transparent => "<Brush Color=\"#00000000\" />".to_string(),
        FillKind::LinearGradient {
            start,
            end,
            wrap,
            stops,
        } => brush_gradient("LinearGradient", *start, *end, wrap.as_deref(), stops),
        FillKind::RadialGradient { wrap, stops } => brush_gradient(
            "RadialGradient",
            Point2::new(0.5, 0.0),
            Point2::new(0.5, 1.0),
            wrap.as_deref(),
            stops,
        ),
        FillKind::Picture { source, .. } => format!(
            "<Brush Type=\"Bitmap\" Color=\"#FFFF0000\" StartPoint=\"0.5,1\" EndPoint=\"0.5,0\"><Brush.Bitmap><Bitmap Source=\"{}\" /></Brush.Bitmap></Brush>",
            esc_attr(&source.replace('/', "\\"))
        ),
        FillKind::Unsupported { node, .. } => serialize_unknown(node),
    }
}

fn brush_gradient(
    kind: &str,
    start: Point2,
    end: Point2,
    wrap: Option<&str>,
    stops: &[crate::model::GradientStop],
) -> String {
    let mut out = format!(
        "<Brush Type=\"{kind}\" Color=\"#FFFFFFFF\" StartPoint=\"{},{}\" EndPoint=\"{},{}\"",
        trim_float(start.x),
        trim_float(start.y),
        trim_float(end.x),
        trim_float(end.y)
    );
    if let Some(wrap) = wrap
        && !wrap.eq_ignore_ascii_case("Mirror")
    {
        out.push_str(&format!(
            " WrapX=\"{wrap}\" WrapY=\"{wrap}\"",
            wrap = esc_attr(wrap)
        ));
    }
    out.push('>');
    if !stops.is_empty() {
        out.push_str("<Brush.Stops>");
        for stop in stops {
            out.push_str("<GradientStop");
            if stop.offset != 0.0 {
                out.push_str(&format!(" Position=\"{}\"", trim_float(stop.offset)));
            }
            out.push_str(&format!(" Color=\"{}\" />", stop.color.to_argb()));
        }
        out.push_str("</Brush.Stops>");
    }
    out.push_str("</Brush>");
    out
}

fn serialize_storyboard(storyboard: &Storyboard, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut out = format!("{pad}<Storyboard");
    let ty = effective_storyboard_type(storyboard.storyboard_type.as_deref());
    if !ty.eq_ignore_ascii_case("TransitionIn") {
        out.push_str(&format!(" Type=\"{}\"", esc_attr(ty)));
    }
    if storyboard.is_scoped() {
        out.push_str(&format!(
            " DataName=\"{}\"",
            esc_attr(storyboard.data_name.as_deref().unwrap_or(""))
        ));
    }
    out.push_str(">\n");
    out.push_str(&format!("{pad}  <Storyboard.Animations>\n"));
    for animation in &storyboard.animations {
        out.push_str(&serialize_animation(animation, indent + 2));
    }
    out.push_str(&format!(
        "{pad}  </Storyboard.Animations>\n{pad}</Storyboard>\n"
    ));
    out
}

fn serialize_animation(animation: &Animation, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let mut attrs = vec![format!(
        "Object=\"{}\"",
        esc_attr(animation.object.as_deref().unwrap_or(""))
    )];
    if animation.delay_secs() != 0.0 {
        attrs.push(format!("Delay=\"{}\"", trim_float(animation.delay_secs())));
    }
    if !animation.is_continuous_type()
        && animation
            .duration
            .as_deref()
            .is_some_and(|value| value != "1")
    {
        attrs.push(format!(
            "Duration=\"{}\"",
            trim_float(animation.duration_secs())
        ));
    } else if animation.duration.is_some() && !animation.is_continuous_type() {
        // keep explicit non-default durations only; default 1 is omitted
    }
    if animation.reversed {
        attrs.push("Reverse=\"True\"".to_string());
    }
    if let Some(interpolation) = &animation.interpolation
        && !interpolation.eq_ignore_ascii_case("Linear")
    {
        attrs.push(format!("Interpolation=\"{}\"", esc_attr(interpolation)));
    }
    let default_dir = if animation.kind == "Scroll" {
        "Bottom"
    } else {
        "Left"
    };
    if let Some(direction) = &animation.direction
        && !direction.eq_ignore_ascii_case(default_dir)
    {
        attrs.push(format!("Direction=\"{}\"", esc_attr(direction)));
    }
    if let Some(axis) = &animation.center_axis
        && !axis.eq_ignore_ascii_case("Both")
    {
        attrs.push(format!("CenterAxis=\"{}\"", esc_attr(axis)));
    }
    if animation.is_continuous_type()
        && animation.speed.as_deref().is_some_and(|value| value != "1")
    {
        attrs.push(format!("Speed=\"{}\"", trim_float(animation.speed_value())));
    }
    for (key, value) in &animation.extra_attrs {
        attrs.push(format!("{key}=\"{}\"", esc_attr(value)));
    }
    format!(
        "{pad}<{kind} {} />\n",
        attrs.join(" "),
        kind = animation.kind
    )
}

fn serialize_unknown(node: &UnknownNode) -> String {
    let attrs = node
        .attributes
        .iter()
        .map(|(key, value)| format!("{key}=\"{}\"", esc_attr(value)))
        .collect::<Vec<_>>()
        .join(" ");
    let space = if attrs.is_empty() {
        String::new()
    } else {
        format!(" {attrs}")
    };
    if node.children.is_empty() && node.text.is_none() {
        format!("<{}{space} />", node.tag)
    } else {
        let mut inner = node
            .children
            .iter()
            .map(serialize_unknown)
            .collect::<String>();
        if let Some(text) = &node.text {
            inner.push_str(&esc_text(text));
        }
        format!("<{tag}{space}>{inner}</{tag}>", tag = node.tag)
    }
}

fn data_flags_for_write(object: &GtObject) -> Option<String> {
    let raw = object.data_flags.as_deref()?.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("None") {
        return None;
    }
    if object.kind.is_shape() && raw.eq_ignore_ascii_case("Hidden") {
        return None;
    }
    Some(raw.to_string())
}

fn bounding_is_default(bounding: &Bounding) -> bool {
    bounding.object.is_empty() && (bounding.padding.is_empty() || bounding.padding == "0,0,0,0")
}

fn encode_text_attr(text: &str) -> String {
    text.replace('\r', "").replace('\n', "\r\n")
}

fn truthy(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

fn esc_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn esc_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn collect_image_logical_paths(document: &GtDocument) -> Vec<String> {
    let mut paths = Vec::new();
    for layer in &document.layers {
        for object in flatten_objects(layer) {
            if let Some(source) = &object.image_source {
                paths.push(source.clone());
            }
            if let FillKind::Picture { source, .. } = &object.fill.kind {
                paths.push(source.clone());
            }
            if let FillKind::Picture { source, .. } = &object.stroke.fill.kind {
                paths.push(source.clone());
            }
        }
    }
    paths
}

#[allow(dead_code)]
pub fn sniff_or_bin(bytes: &[u8]) -> (&'static str, &'static str) {
    sniff_image(bytes)
}
