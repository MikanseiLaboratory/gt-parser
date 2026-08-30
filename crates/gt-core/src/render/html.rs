use std::collections::BTreeMap;
use std::path::Path;

use crate::model::{
    Animation, Fill, FillKind, GtDocument, GtObject, Layer, LayerChild, ObjectKind, Storyboard,
    Stroke, TextStyle, flatten_objects,
};
use crate::package::{Package, sniff_image};
use crate::resolve::effective_storyboard_type;
use crate::warn::Warning;
use crate::{ConvertOptions, OutputAsset};

const STATIC_CSS: &str = r#"html, body {
  margin: 0;
  background: transparent;
}
.gt-stage {
  position: relative;
  overflow: hidden;
  background: transparent;
}
.gt-layer, .gt-object {
  position: absolute;
  box-sizing: border-box;
}
.gt-text {
  display: flex;
  overflow: hidden;
}
.gt-shape {
  display: block;
  width: 100%;
  height: 100%;
  overflow: visible;
}
.gt-image {
  overflow: hidden;
}
.gt-image img, .gt-image-frame {
  display: block;
  width: 100%;
  height: 100%;
}
.gt-ticker {
  overflow: hidden;
}
.gt-ticker-track {
  display: inline-flex;
  white-space: nowrap;
  will-change: transform;
}
@keyframes gt-fade-in { from { opacity: 0; } to { opacity: 1; } }
@keyframes gt-fade-out { from { opacity: 1; } to { opacity: 0; } }
@keyframes gt-reveal-left { from { clip-path: inset(0 100% 0 0); } to { clip-path: inset(0); } }
@keyframes gt-reveal-right { from { clip-path: inset(0 0 0 100%); } to { clip-path: inset(0); } }
@keyframes gt-reveal-top { from { clip-path: inset(100% 0 0 0); } to { clip-path: inset(0); } }
@keyframes gt-reveal-bottom { from { clip-path: inset(0 0 100% 0); } to { clip-path: inset(0); } }
@keyframes gt-reveal-center-x { from { clip-path: inset(0 50% 0 50%); } to { clip-path: inset(0); } }
@keyframes gt-reveal-center-y { from { clip-path: inset(50% 0 50% 0); } to { clip-path: inset(0); } }
@keyframes gt-fly-left { from { transform: translateX(-110%); } to { transform: none; } }
@keyframes gt-fly-right { from { transform: translateX(110%); } to { transform: none; } }
@keyframes gt-fly-top { from { transform: translateY(-110%); } to { transform: none; } }
@keyframes gt-fly-bottom { from { transform: translateY(110%); } to { transform: none; } }
@keyframes gt-zoomfade-in { from { opacity: 0; transform: scale(0.85); } to { opacity: 1; transform: none; } }
@keyframes gt-scale-in { from { transform: scale(0); } to { transform: none; } }
@keyframes gt-rotate-in { from { transform: rotate(-90deg); } to { transform: none; } }
@keyframes gt-spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
@keyframes gt-ticker-left { from { transform: translateX(100%); } to { transform: translateX(-100%); } }
@keyframes gt-ticker-right { from { transform: translateX(-100%); } to { transform: translateX(100%); } }
@keyframes gt-ticker-top { from { transform: translateY(100%); } to { transform: translateY(-100%); } }
@keyframes gt-ticker-bottom { from { transform: translateY(-100%); } to { transform: translateY(100%); } }
@keyframes gt-bounce-in {
  0% { transform: scale(0.3); }
  20% { transform: scale(1.1); }
  40% { transform: scale(0.9); }
  60% { transform: scale(1.03); }
  80% { transform: scale(0.97); }
  100% { transform: none; }
}
@keyframes gt-bounce-out {
  0% { transform: none; }
  20% { transform: scale(0.9); }
  50% { transform: scale(1.1); }
  100% { transform: scale(0.3); opacity: 0; }
}"#;

pub struct Rendered {
    pub html: String,
    pub assets: Vec<OutputAsset>,
    pub warnings: Vec<Warning>,
}

pub fn render(document: &GtDocument, package: &Package, options: &ConvertOptions) -> Rendered {
    let mut renderer = Renderer::new(document, package, options);
    let body = renderer.render_body(document);
    let css = renderer.stylesheet();
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>GT Title</title>
<style>
{css}
</style>
</head>
<body>
{body}</body>
</html>
"#
    );
    Rendered {
        html,
        assets: renderer.assets,
        warnings: renderer.warnings,
    }
}

struct Renderer<'a> {
    package: &'a Package,
    options: &'a ConvertOptions,
    assets: Vec<OutputAsset>,
    asset_urls: BTreeMap<String, String>,
    warnings: Vec<Warning>,
    gradient_id: usize,
    animations_by_name: BTreeMap<String, Vec<&'a Animation>>,
    objects_by_name: BTreeMap<String, &'a GtObject>,
    continuous: bool,
    seq_css: String,
}

impl<'a> Renderer<'a> {
    fn new(document: &'a GtDocument, package: &'a Package, options: &'a ConvertOptions) -> Self {
        let mut objects_by_name = BTreeMap::new();
        for layer in &document.layers {
            for object in flatten_objects(layer) {
                objects_by_name.insert(object.name.clone(), object);
            }
        }
        let mut animations_by_name: BTreeMap<String, Vec<&Animation>> = BTreeMap::new();
        let mut continuous = false;
        if let Some(storyboard) = select_storyboard(document, options) {
            continuous = effective_storyboard_type(storyboard.storyboard_type.as_deref())
                .eq_ignore_ascii_case("Continuous");
            for animation in &storyboard.animations {
                if let Some(name) = &animation.object {
                    let list = animations_by_name.entry(name.clone()).or_default();
                    if list.len() < 3 {
                        list.push(animation);
                    }
                }
            }
        }
        Self {
            package,
            options,
            assets: Vec::new(),
            asset_urls: BTreeMap::new(),
            warnings: Vec::new(),
            gradient_id: 0,
            animations_by_name,
            objects_by_name,
            continuous,
            seq_css: String::new(),
        }
    }

    fn stylesheet(&self) -> String {
        format!(
            "{static_css}\n{seq}",
            static_css = STATIC_CSS,
            seq = self.seq_css
        )
    }

    fn render_body(&mut self, document: &GtDocument) -> String {
        let mut body = String::new();
        body.push_str(&format!(
            r#"<div class="gt-stage" style="width:{:.3}px;height:{:.3}px;">"#,
            document.width, document.height
        ));
        body.push('\n');
        for layer in &document.layers {
            self.render_layer(&mut body, layer, 1);
        }
        body.push_str("</div>\n");
        body
    }

    fn render_layer(&mut self, out: &mut String, layer: &Layer, indent: usize) {
        let pad = "  ".repeat(indent);
        let mut style = format!(
            "left:{:.3}px;top:{:.3}px;width:{:.3}px;height:{:.3}px;",
            layer.location.x, layer.location.y, layer.dimensions.x, layer.dimensions.y
        );
        style.push_str(&self.animation_css(&layer.name));
        out.push_str(&format!(
            r#"{pad}<div class="gt-layer" data-gt-name="{name}" data-gt-type="Layer" style="{style}">"#,
            name = esc_attr(&layer.name),
            style = esc_attr(&style),
        ));
        out.push('\n');
        for child in &layer.objects {
            match child {
                LayerChild::Layer(nested) => self.render_layer(out, nested, indent + 1),
                LayerChild::Object(object) => self.render_object(out, object, indent + 1),
            }
        }
        out.push_str(&format!("{pad}</div>\n"));
    }

    fn render_object(&mut self, out: &mut String, object: &GtObject, indent: usize) {
        if !object.kind.renders_html() {
            return;
        }
        match object.kind {
            ObjectKind::TextBlock | ObjectKind::Text3D => self.render_text(out, object, indent),
            ObjectKind::Rectangle | ObjectKind::Ellipse | ObjectKind::Triangle => {
                self.render_shape(out, object, indent);
            }
            ObjectKind::Image | ObjectKind::QrCode | ObjectKind::ImageSequence => {
                self.render_image(out, object, indent);
            }
            ObjectKind::Ticker => self.render_ticker(out, object, indent),
            ObjectKind::Unknown => {}
        }
    }

    fn box_style(&mut self, object: &GtObject) -> String {
        let mut style = format!(
            "left:{:.3}px;top:{:.3}px;width:{:.3}px;height:{:.3}px;",
            object.location.x, object.location.y, object.dimensions.x, object.dimensions.y
        );
        style.push_str(&self.effect_css(object));
        style.push_str(&self.animation_css(&object.name));
        if !object.visible {
            style.push_str("visibility:hidden;");
        }
        style
    }

    fn effect_css(&mut self, object: &GtObject) -> String {
        let mut css = String::new();
        let mut transforms = Vec::new();
        if let Some(rotate) = object.rotate {
            transforms.push(format!("rotate({rotate:.3}deg)"));
        }
        if let Some(skew) = &object.effects.skew {
            transforms.push(format!("skew({:.3}deg,{:.3}deg)", skew.x, skew.y));
        }
        if object.effects.flip_x {
            transforms.push("scaleX(-1)".to_string());
        }
        if object.effects.flip_y {
            transforms.push("scaleY(-1)".to_string());
        }
        if object
            .geometry
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("Cube"))
        {
            transforms.push("perspective(800px) rotateY(18deg) rotateX(8deg)".to_string());
        }
        if !transforms.is_empty() {
            css.push_str(&format!("transform:{};", transforms.join(" ")));
        }
        if let Some(opacity) = object.opacity {
            let value = if opacity > 1.0 {
                opacity / 100.0
            } else {
                opacity
            };
            css.push_str(&format!("opacity:{value:.3};"));
        }
        if let Some(shadow) = &object.effects.shadow {
            let blur = shadow.blur.unwrap_or(8.0);
            let color = shadow
                .color
                .map(|color| color.to_css())
                .unwrap_or_else(|| "rgba(0,0,0,0.55)".to_string());
            css.push_str(&format!("filter:drop-shadow(0 0 {blur:.3}px {color});"));
        }
        if let Some(crop) = &object.effects.crop {
            css.push_str(&crop_css(crop.range.as_deref()));
            if let Some(feather) = &crop.feather
                && feather.split(',').any(|part| {
                    part.trim()
                        .parse::<f64>()
                        .is_ok_and(|value| value.abs() > 0.01)
                })
            {
                css.push_str("mask-mode:luminance;");
            }
        }
        if let Some(mask_name) = object.effects.mask.clone()
            && let Some(url) = self.mask_url(&mask_name)
        {
            let (pos, size) = self.mask_layout(object, &mask_name);
            css.push_str(&format!(
                "mask-image:url({url});mask-repeat:no-repeat;mask-size:{size};mask-position:{pos};-webkit-mask-image:url({url});-webkit-mask-repeat:no-repeat;-webkit-mask-size:{size};-webkit-mask-position:{pos};"
            ));
        }
        if object.effects.reflection {
            css.push_str(
                "-webkit-box-reflect:below 4px linear-gradient(transparent,rgba(0,0,0,0.35));",
            );
        }
        if let Some(mode) = &object.effects.compositing {
            match normalize(mode).as_str() {
                "additive" => css.push_str("mix-blend-mode:plus-lighter;"),
                "replace" => css.push_str("isolation:isolate;mix-blend-mode:normal;"),
                _ => {}
            }
        }
        css
    }

    fn animation_css(&self, name: &str) -> String {
        let Some(anims) = self.animations_by_name.get(name) else {
            return String::new();
        };
        let mut parts = Vec::new();
        for animation in anims {
            if animation.kind == "None" || animation.kind == "ImageSequence" {
                continue;
            }
            if let Some(spec) = animation_spec(animation, self.continuous) {
                parts.push(spec);
            }
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("animation:{};", parts.join(","))
        }
    }

    fn render_text(&mut self, out: &mut String, object: &GtObject, indent: usize) {
        let pad = "  ".repeat(indent);
        let mut style = self.box_style(object);
        style.push_str(&text_style_css(&object.style));
        if let FillKind::Solid { color } = object.fill.kind {
            style.push_str(&format!("color:{};", color.to_css()));
        }
        let text = object.text.as_deref().unwrap_or("");
        let display = if object.style.auto_upper_case {
            text.to_uppercase()
        } else {
            text.to_string()
        };
        out.push_str(&format!(
            r#"{pad}<div class="gt-object gt-text" data-gt-name="{name}" data-gt-type="{kind}" style="{style}">{content}</div>"#,
            name = esc_attr(&object.name),
            kind = object.kind.as_str(),
            style = esc_attr(&style),
            content = html_escape::encode_text(&display),
        ));
        out.push('\n');
    }

    fn render_shape(&mut self, out: &mut String, object: &GtObject, indent: usize) {
        let pad = "  ".repeat(indent);
        let style = self.box_style(object);
        let (fill, defs) = self.paint_svg(&object.fill, object);
        let (stroke, stroke_width) = stroke_css(&object.stroke, object);
        let inner = match object.kind {
            ObjectKind::Rectangle => {
                let radius = object.radius.unwrap_or(0.0);
                format!(
                    r#"<rect x="0" y="0" width="100%" height="100%" rx="{radius:.3}" ry="{radius:.3}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width:.3}"/>"#
                )
            }
            ObjectKind::Ellipse => format!(
                r#"<ellipse cx="50%" cy="50%" rx="50%" ry="50%" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width:.3}"/>"#
            ),
            ObjectKind::Triangle => format!(
                r#"<polygon points="50,0 100,100 0,100" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width:.3}" vector-effect="non-scaling-stroke"/>"#
            ),
            _ => String::new(),
        };
        let view_box = if object.kind == ObjectKind::Triangle {
            r#" viewBox="0 0 100 100" preserveAspectRatio="none""#
        } else {
            ""
        };
        out.push_str(&format!(
            r#"{pad}<div class="gt-object" data-gt-name="{name}" data-gt-type="{kind}" style="{style}"><svg class="gt-shape"{view_box}>{defs}{inner}</svg></div>"#,
            name = esc_attr(&object.name),
            kind = object.kind.as_str(),
            style = esc_attr(&style),
        ));
        out.push('\n');
    }

    fn render_image(&mut self, out: &mut String, object: &GtObject, indent: usize) {
        let pad = "  ".repeat(indent);
        let mut style = self.box_style(object);
        let fit = size_mode_css(object.size_mode.as_deref());
        let frames = object
            .image_source
            .as_deref()
            .map(|source| self.sequence_urls(source))
            .unwrap_or_default();
        if frames.is_empty() {
            out.push_str(&format!(
                r#"{pad}<div class="gt-object gt-image" data-gt-name="{name}" data-gt-type="{kind}" style="{style}"></div>"#,
                name = esc_attr(&object.name),
                kind = object.kind.as_str(),
                style = esc_attr(&style),
            ));
            out.push('\n');
            return;
        }
        if frames.len() == 1 {
            out.push_str(&format!(
                r#"{pad}<div class="gt-object gt-image" data-gt-name="{name}" data-gt-type="{kind}" style="{style}"><img alt="" src="{src}" style="{fit}"></div>"#,
                name = esc_attr(&object.name),
                kind = object.kind.as_str(),
                style = esc_attr(&style),
                src = esc_attr(&frames[0]),
                fit = esc_attr(&fit),
            ));
            out.push('\n');
            return;
        }
        let duration = self
            .animations_by_name
            .get(&object.name)
            .and_then(|anims| {
                anims
                    .iter()
                    .find(|animation| animation.kind == "ImageSequence")
                    .and_then(|animation| animation.duration.as_deref())
            })
            .and_then(|value| value.trim().parse::<f64>().ok())
            .unwrap_or_else(|| f64::from(frames.len() as u32) / 8.0)
            .max(0.05);
        let step = 100.0 / frames.len() as f64;
        let mut inner = String::new();
        let id = sanitize_id(&object.name);
        for (index, src) in frames.iter().enumerate() {
            let start = step * index as f64;
            let end = (step * (index as f64 + 1.0)).min(100.0);
            self.seq_css.push_str(&format!(
                "@keyframes gt-seq-{id}-{index} {{ 0% {{ opacity: 0; }} {start:.3}% {{ opacity: 1; }} {end:.3}% {{ opacity: 0; }} 100% {{ opacity: 0; }} }}\n"
            ));
            inner.push_str(&format!(
                r#"<img class="gt-image-frame" alt="" src="{src}" style="{fit}position:absolute;left:0;top:0;animation:gt-seq-{id}-{index} {duration:.3}s linear infinite;">"#,
                src = esc_attr(src),
                fit = esc_attr(&fit),
            ));
        }
        style.push_str("overflow:hidden;");
        out.push_str(&format!(
            r#"{pad}<div class="gt-object gt-image" data-gt-name="{name}" data-gt-type="{kind}" style="{style}">{inner}</div>"#,
            name = esc_attr(&object.name),
            kind = object.kind.as_str(),
            style = esc_attr(&style),
        ));
        out.push('\n');
    }

    fn render_ticker(&mut self, out: &mut String, object: &GtObject, indent: usize) {
        let pad = "  ".repeat(indent);
        let mut style = self.box_style(object);
        style.push_str(&text_style_css(&object.style));
        if let FillKind::Solid { color } = object.fill.kind {
            style.push_str(&format!("color:{};", color.to_css()));
        }
        let speed = object.ticker_speed.unwrap_or(2.0).max(0.1);
        let px_per_sec = speed * 30.0;
        let distance = object.dimensions.x.max(1.0) * 2.0;
        let duration = distance / px_per_sec;
        let direction = object.ticker_direction.as_deref().unwrap_or("Left");
        let keyframe = match normalize(direction).as_str() {
            "right" => "gt-ticker-right",
            "top" => "gt-ticker-top",
            "bottom" => "gt-ticker-bottom",
            _ => "gt-ticker-left",
        };
        let text = object.text.as_deref().unwrap_or("");
        let display = if object.style.auto_upper_case {
            text.to_uppercase()
        } else {
            text.to_string()
        };
        out.push_str(&format!(
            r#"{pad}<div class="gt-object gt-ticker" data-gt-name="{name}" data-gt-type="Ticker" style="{style}"><div class="gt-ticker-track gt-text" style="animation:{keyframe} {duration:.3}s linear infinite;">{content}</div></div>"#,
            name = esc_attr(&object.name),
            style = esc_attr(&style),
            content = html_escape::encode_text(&display),
        ));
        out.push('\n');
    }

    fn paint_svg(&mut self, fill: &Fill, object: &GtObject) -> (String, String) {
        match &fill.kind {
            FillKind::Solid { color } => (esc_attr(&color.to_css()), String::new()),
            FillKind::Transparent | FillKind::Unsupported { .. } => {
                ("none".to_string(), String::new())
            }
            FillKind::LinearGradient { angle, wrap, stops } => {
                self.gradient_id += 1;
                let id = format!("gt-lg-{}", self.gradient_id);
                let rad = angle.to_radians();
                let x1 = 50.0 - rad.cos() * 50.0;
                let y1 = 50.0 - rad.sin() * 50.0;
                let x2 = 50.0 + rad.cos() * 50.0;
                let y2 = 50.0 + rad.sin() * 50.0;
                let mut stops_xml = String::new();
                for stop in stops {
                    stops_xml.push_str(&format!(
                        r#"<stop offset="{:.3}" stop-color="{}"/>"#,
                        stop.offset,
                        esc_attr(&stop.color.to_css())
                    ));
                }
                let defs = format!(
                    r#"<defs><linearGradient id="{id}" x1="{x1:.3}%" y1="{y1:.3}%" x2="{x2:.3}%" y2="{y2:.3}%" spreadMethod="{spread}">{stops_xml}</linearGradient></defs>"#,
                    spread = spread(wrap.as_deref()),
                );
                (format!("url(#{id})"), defs)
            }
            FillKind::RadialGradient { wrap, stops } => {
                self.gradient_id += 1;
                let id = format!("gt-rg-{}", self.gradient_id);
                let mut stops_xml = String::new();
                for stop in stops {
                    stops_xml.push_str(&format!(
                        r#"<stop offset="{:.3}" stop-color="{}"/>"#,
                        stop.offset,
                        esc_attr(&stop.color.to_css())
                    ));
                }
                let defs = format!(
                    r#"<defs><radialGradient id="{id}" cx="50%" cy="50%" r="50%" spreadMethod="{spread}">{stops_xml}</radialGradient></defs>"#,
                    spread = spread(wrap.as_deref()),
                );
                (format!("url(#{id})"), defs)
            }
            FillKind::Picture {
                source, size_mode, ..
            } => {
                let Some(url) = self.image_url(source) else {
                    self.warnings.push(
                        Warning::new(
                            "unsupported.image.source",
                            "picture fill source was not found",
                        )
                        .with_object(&object.name),
                    );
                    return ("none".to_string(), String::new());
                };
                self.gradient_id += 1;
                let id = format!("gt-pat-{}", self.gradient_id);
                let units = match size_mode.as_deref().map(normalize).as_deref() {
                    Some("original") | Some("tile") => "userSpaceOnUse",
                    _ => "objectBoundingBox",
                };
                let (w, h, x, y) = if units == "objectBoundingBox" {
                    ("1", "1", "0", "0")
                } else {
                    ("100", "100", "0", "0")
                };
                let defs = format!(
                    r#"<defs><pattern id="{id}" patternUnits="{units}" width="{w}" height="{h}" x="{x}" y="{y}"><image href="{url}" width="100%" height="100%" preserveAspectRatio="none"/></pattern></defs>"#
                );
                (format!("url(#{id})"), defs)
            }
        }
    }

    fn image_url(&mut self, source: &str) -> Option<String> {
        if let Some(url) = self.asset_urls.get(source) {
            return Some(url.clone());
        }
        let bytes = self.package.lookup_bytes(source)?;
        let url = self.emit_asset(source, bytes);
        self.asset_urls.insert(source.to_string(), url.clone());
        Some(url)
    }

    fn sequence_urls(&mut self, source: &str) -> Vec<String> {
        let entries = self.package.sequence_entries(source);
        if entries.is_empty() {
            return self.image_url(source).into_iter().collect();
        }
        let mut urls = Vec::new();
        for entry in entries {
            if let Some(bytes) = self.package.files.get(&entry) {
                let url = self.emit_asset(&entry, bytes);
                urls.push(url);
            } else if let Some(url) = self.image_url(&entry) {
                urls.push(url);
            }
        }
        if urls.is_empty() {
            self.image_url(source).into_iter().collect()
        } else {
            urls
        }
    }

    fn emit_asset(&mut self, source: &str, bytes: &[u8]) -> String {
        let (ext, mime) = sniff_image(bytes);
        if self.options.embed_assets {
            return format!("data:{mime};base64,{}", base64_encode(bytes));
        }
        let name = asset_file_name(source, ext);
        let relative = format!("assets/{name}");
        if !self
            .assets
            .iter()
            .any(|asset| asset.relative_path == relative)
        {
            self.assets.push(OutputAsset {
                relative_path: relative.clone(),
                bytes: bytes.to_vec(),
            });
        }
        relative
    }

    fn mask_url(&mut self, name: &str) -> Option<String> {
        let source = self.objects_by_name.get(name)?.image_source.clone()?;
        self.image_url(&source)
    }

    fn mask_layout(&self, object: &GtObject, mask_name: &str) -> (String, String) {
        let Some(mask) = self.objects_by_name.get(mask_name) else {
            return ("0 0".to_string(), "100% 100%".to_string());
        };
        let x = mask.location.x - object.location.x;
        let y = mask.location.y - object.location.y;
        (
            format!("{x:.3}px {y:.3}px"),
            format!("{:.3}px {:.3}px", mask.dimensions.x, mask.dimensions.y),
        )
    }
}

fn select_storyboard<'a>(
    document: &'a GtDocument,
    options: &ConvertOptions,
) -> Option<&'a Storyboard> {
    let wanted = normalize(&options.storyboard);
    document.storyboards.iter().find(|storyboard| {
        normalize(effective_storyboard_type(
            storyboard.storyboard_type.as_deref(),
        )) == wanted
    })
}

fn animation_spec(animation: &Animation, continuous: bool) -> Option<String> {
    let name = match animation.kind.as_str() {
        "Fade" => {
            if animation.reversed {
                "gt-fade-out"
            } else {
                "gt-fade-in"
            }
        }
        "Reveal" | "Wipe" => match animation.direction.as_deref().map(normalize).as_deref() {
            Some("right") => "gt-reveal-right",
            Some("top") => "gt-reveal-top",
            Some("bottom") => "gt-reveal-bottom",
            Some("center")
                if animation
                    .center_axis
                    .as_deref()
                    .is_some_and(|axis| axis.eq_ignore_ascii_case("Y")) =>
            {
                "gt-reveal-center-y"
            }
            Some("center") => "gt-reveal-center-x",
            _ => "gt-reveal-left",
        },
        "Fly" | "Move" => match animation.direction.as_deref().map(normalize).as_deref() {
            Some("right") => "gt-fly-right",
            Some("top") => "gt-fly-top",
            Some("left") => "gt-fly-left",
            _ => "gt-fly-bottom",
        },
        "ZoomFade" | "Zoom" => "gt-zoomfade-in",
        "Scale" => "gt-scale-in",
        "Rotate" | "Spin" => "gt-spin",
        "Flip" => "gt-reveal-center-x",
        _ => return None,
    };
    let duration = animation
        .duration
        .as_deref()
        .and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or(0.5);
    let delay = animation
        .delay
        .as_deref()
        .and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    let timing = interpolation_css(animation.interpolation.as_deref(), animation.kind.as_str());
    let reverse_already_baked = name == "gt-fade-out" || name == "gt-fade-in";
    let direction = if animation.reversed && !reverse_already_baked {
        "reverse"
    } else {
        "normal"
    };
    let iterate = if continuous { "infinite" } else { "1" };
    Some(format!(
        "{name} {duration:.3}s {timing} {delay:.3}s {iterate} {direction} both"
    ))
}

fn interpolation_css(raw: Option<&str>, _kind: &str) -> String {
    match raw.map(normalize).as_deref() {
        Some("line") | Some("linear") => "linear".to_string(),
        Some("cubiceasingin") => "cubic-bezier(0.42,0,1,1)".to_string(),
        Some("cubiceasingout") => "cubic-bezier(0,0,0.58,1)".to_string(),
        Some("cubiceasinginout") => "cubic-bezier(0.42,0,0.58,1)".to_string(),
        Some("bouncein") => "cubic-bezier(0.215,0.61,0.355,1)".to_string(),
        Some("bounceout") => "cubic-bezier(0.215,0.61,0.355,1)".to_string(),
        _ => "ease".to_string(),
    }
}

fn crop_css(range: Option<&str>) -> String {
    let Some(range) = range else {
        return String::new();
    };
    let parts: Vec<f64> = range
        .split(',')
        .map(|part| part.trim().parse().unwrap_or(0.0))
        .collect();
    if parts.len() < 4 {
        return String::new();
    }
    let left = parts[0].clamp(0.0, 1.0);
    let top = parts[1].clamp(0.0, 1.0);
    let right = parts[2].clamp(0.0, 1.0);
    let bottom = parts[3].clamp(0.0, 1.0);
    format!(
        "clip-path:inset({:.3}% {:.3}% {:.3}% {:.3}%);",
        top * 100.0,
        (1.0 - right) * 100.0,
        (1.0 - bottom) * 100.0,
        left * 100.0
    )
}

fn size_mode_css(mode: Option<&str>) -> String {
    match mode.map(normalize).as_deref() {
        Some("stretch") | Some("fill") => "object-fit:fill;".to_string(),
        Some("normal") => "object-fit:none;object-position:0 0;".to_string(),
        Some("centered") | Some("center") => {
            "object-fit:contain;object-position:center;".to_string()
        }
        _ => "object-fit:contain;object-position:0 0;".to_string(),
    }
}

fn stroke_css(stroke: &Stroke, _object: &GtObject) -> (String, f64) {
    match &stroke.fill.kind {
        FillKind::Solid { color } => (esc_attr(&color.to_css()), stroke.thickness.unwrap_or(1.0)),
        _ => ("none".to_string(), 0.0),
    }
}

fn text_style_css(style: &TextStyle) -> String {
    let mut css = String::new();
    if let Some(family) = &style.font_family {
        css.push_str(&format!(
            "font-family:\"{}\",sans-serif;",
            family.replace('"', "")
        ));
    } else {
        css.push_str("font-family:sans-serif;");
    }
    if let Some(size) = style.font_size {
        css.push_str(&format!("font-size:{size:.3}px;"));
    }
    if let Some(weight) = &style.font_weight {
        css.push_str(&format!("font-weight:{};", map_weight(weight)));
    }
    if let Some(stretch) = &style.font_stretch {
        css.push_str(&format!("font-stretch:{};", map_stretch(stretch)));
    }
    if style.italic
        || style
            .font_style
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("Italic"))
    {
        css.push_str("font-style:italic;");
    }
    if style.underline || style.strikethrough {
        let mut decorations = Vec::new();
        if style.underline {
            decorations.push("underline");
        }
        if style.strikethrough {
            decorations.push("line-through");
        }
        css.push_str(&format!("text-decoration:{};", decorations.join(" ")));
    }
    let justify = match style.text_align.as_deref().map(normalize).as_deref() {
        Some("center") => "center",
        Some("right") => "flex-end",
        _ => "flex-start",
    };
    let align = match style.vertical_align.as_deref().map(normalize).as_deref() {
        Some("center") => "center",
        Some("bottom") => "flex-end",
        _ => "flex-start",
    };
    css.push_str(&format!("justify-content:{justify};align-items:{align};"));
    if style.rtl {
        css.push_str("direction:rtl;");
    }
    match style.word_wrapping.as_deref().map(normalize).as_deref() {
        Some("nowrap") | Some("no-wrap") => css.push_str("white-space:nowrap;"),
        _ => css.push_str("white-space:pre-wrap;"),
    }
    if let Some(spacing) = style.line_spacing {
        css.push_str(&format!("line-height:{spacing:.3};"));
    }
    css
}

fn map_weight(weight: &str) -> String {
    match normalize(weight).as_str() {
        "bold" => "700".to_string(),
        "regular" | "normal" => "400".to_string(),
        "light" => "300".to_string(),
        "semibold" | "semi-bold" | "demibold" => "600".to_string(),
        other => other.to_string(),
    }
}

fn map_stretch(stretch: &str) -> String {
    match normalize(stretch).as_str() {
        "condensed" => "condensed".to_string(),
        "expanded" => "expanded".to_string(),
        other => other.to_string(),
    }
}

fn spread(wrap: Option<&str>) -> &'static str {
    match wrap.map(normalize).as_deref() {
        Some("wrap") | Some("repeat") => "repeat",
        Some("clamp") | Some("pad") => "pad",
        _ => "reflect",
    }
}

fn asset_file_name(source: &str, ext: &str) -> String {
    let file_name = Path::new(source)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(source);
    let mut name = file_name.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_");
    if Path::new(&name).extension().is_none() {
        name.push('.');
        name.push_str(ext);
    }
    if name.is_empty() {
        format!("asset.{ext}")
    } else {
        name
    }
}

fn sanitize_id(name: &str) -> String {
    name.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn esc_attr(value: &str) -> String {
    html_escape::encode_double_quoted_attribute(value).into_owned()
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(n >> 18) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
