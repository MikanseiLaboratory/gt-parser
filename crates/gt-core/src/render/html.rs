use crate::model::{
    Fill, FillKind, GtDocument, GtObject, Layer, LayerChild, ObjectKind, Stroke, TextStyle,
};

pub fn render(document: &GtDocument) -> String {
    let mut body = String::new();
    body.push_str(&format!(
        r#"<div class="gt-stage" style="width:{:.3}px;height:{:.3}px;">"#,
        document.width, document.height
    ));
    body.push('\n');
    for layer in &document.layers {
        render_layer(&mut body, layer, 1);
    }
    body.push_str("</div>\n");

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>GT Title</title>
<style>
html, body {{
  margin: 0;
  background: transparent;
}}
.gt-stage {{
  position: relative;
  overflow: hidden;
  background: transparent;
}}
.gt-layer, .gt-object {{
  position: absolute;
  box-sizing: border-box;
}}
.gt-text {{
  display: flex;
  overflow: hidden;
}}
.gt-shape {{
  display: block;
  overflow: visible;
}}
</style>
</head>
<body>
{body}</body>
</html>
"#
    )
}

fn render_layer(out: &mut String, layer: &Layer, indent: usize) {
    let pad = "  ".repeat(indent);
    out.push_str(&format!(
        r#"{pad}<div class="gt-layer" data-gt-name="{name}" data-gt-type="Layer" style="left:{x:.3}px;top:{y:.3}px;width:{w:.3}px;height:{h:.3}px;">"#,
        name = esc_attr(&layer.name),
        x = layer.location.x,
        y = layer.location.y,
        w = layer.dimensions.x,
        h = layer.dimensions.y,
    ));
    out.push('\n');
    for child in &layer.objects {
        match child {
            LayerChild::Layer(nested) => render_layer(out, nested, indent + 1),
            LayerChild::Object(object) => render_object(out, object, indent + 1),
        }
    }
    out.push_str(&format!("{pad}</div>\n"));
}

fn render_object(out: &mut String, object: &GtObject, indent: usize) {
    if !object.kind.phase1_renderable() {
        return;
    }
    match object.kind {
        ObjectKind::TextBlock => render_text(out, object, indent),
        ObjectKind::Rectangle | ObjectKind::Ellipse | ObjectKind::Triangle => {
            render_shape(out, object, indent);
        }
        _ => {}
    }
}

fn render_text(out: &mut String, object: &GtObject, indent: usize) {
    let pad = "  ".repeat(indent);
    let mut style = box_style(object);
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
        r#"{pad}<div class="gt-object gt-text" data-gt-name="{name}" data-gt-type="TextBlock" style="{style}">{content}</div>"#,
        name = esc_attr(&object.name),
        style = esc_attr(&style),
        content = html_escape::encode_text(&display),
    ));
    out.push('\n');
}

fn render_shape(out: &mut String, object: &GtObject, indent: usize) {
    let pad = "  ".repeat(indent);
    let style = box_style(object);
    let fill = paint_css(&object.fill);
    let (stroke, stroke_width) = stroke_css(&object.stroke);
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
        r#"{pad}<svg class="gt-object gt-shape" data-gt-name="{name}" data-gt-type="{kind}" style="{style}"{view_box}>{inner}</svg>"#,
        name = esc_attr(&object.name),
        kind = object.kind.as_str(),
        style = esc_attr(&style),
    ));
    out.push('\n');
}

fn box_style(object: &GtObject) -> String {
    format!(
        "left:{:.3}px;top:{:.3}px;width:{:.3}px;height:{:.3}px;",
        object.location.x, object.location.y, object.dimensions.x, object.dimensions.y
    )
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
        "semibold" | "semi-bold" => "600".to_string(),
        other => other.to_string(),
    }
}

fn paint_css(fill: &Fill) -> String {
    match fill.kind {
        FillKind::Solid { color } => color.to_css(),
        FillKind::Transparent | FillKind::Unsupported { .. } => "none".to_string(),
    }
}

fn stroke_css(stroke: &Stroke) -> (String, f64) {
    match stroke.fill.kind {
        FillKind::Solid { color } => (color.to_css(), stroke.thickness.unwrap_or(1.0)),
        _ => ("none".to_string(), 0.0),
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn esc_attr(value: &str) -> String {
    html_escape::encode_double_quoted_attribute(value).into_owned()
}
