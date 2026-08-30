use crate::ConvertOptions;
use crate::model::{
    FillKind, GtDocument, GtObject, Layer, LayerChild, ObjectKind, Vec3, flatten_objects,
};
use crate::warn::Warning;

pub fn resolve_bounding(document: &mut GtDocument) {
    let snapshots: Vec<(String, Vec3, Vec3)> = document
        .layers
        .iter()
        .flat_map(flatten_objects)
        .map(|object| {
            (
                object.name.clone(),
                object.location.clone(),
                object.dimensions.clone(),
            )
        })
        .collect();
    for layer in &mut document.layers {
        resolve_layer_bounding(layer, &snapshots);
    }
}

fn resolve_layer_bounding(layer: &mut Layer, snapshots: &[(String, Vec3, Vec3)]) {
    for child in &mut layer.objects {
        match child {
            LayerChild::Layer(nested) => resolve_layer_bounding(nested, snapshots),
            LayerChild::Object(object) => {
                if let Some(bounding) = object.bounding.clone()
                    && let Some((_, location, dimensions)) = snapshots
                        .iter()
                        .find(|(name, _, _)| name == &bounding.object)
                {
                    let (left, top, right, bottom) = parse_padding(&bounding.padding);
                    object.location.x = location.x - left;
                    object.location.y = location.y - top;
                    object.dimensions.x = dimensions.x + left + right;
                    object.dimensions.y = dimensions.y + top + bottom;
                }
            }
        }
    }
}

fn parse_padding(raw: &str) -> (f64, f64, f64, f64) {
    let parts: Vec<f64> = raw
        .split(',')
        .map(|part| part.trim().parse().unwrap_or(0.0))
        .collect();
    match parts.as_slice() {
        [one] => (*one, *one, *one, *one),
        [h, v] => (*h, *v, *h, *v),
        [left, top, right, bottom] => (*left, *top, *right, *bottom),
        _ => (0.0, 0.0, 0.0, 0.0),
    }
}

pub fn collect_warnings(document: &GtDocument, options: &ConvertOptions) -> Vec<Warning> {
    let mut warnings = document.warnings.clone();
    for node in &document.unknown_children {
        warnings.push(Warning::new(
            "unsupported.root_child",
            format!("root child <{}> is not converted", node.tag),
        ));
    }
    let selected = normalize(&options.storyboard);
    let mut matched_storyboard = false;
    for storyboard in &document.storyboards {
        let ty = effective_storyboard_type(storyboard.storyboard_type.as_deref());
        if normalize(ty) == selected {
            matched_storyboard = true;
        }
        match normalize(ty).as_str() {
            "transitionin" | "transitionout" | "continuous" => {}
            "datachangein" | "datachangeout" => {
                warnings.push(Warning::new(
                    "unsupported.storyboard.datachange",
                    format!("storyboard {ty} is kept in IR for Wasm runtime (stage 6)"),
                ));
            }
            other => {
                warnings.push(Warning::new(
                    "unsupported.storyboard",
                    format!("storyboard {other} is parsed but not selected for HTML"),
                ));
            }
        }
        for animation in &storyboard.animations {
            if !supported_animation(&animation.kind) {
                warnings.push(Warning::new(
                    format!(
                        "unsupported.animation.{}",
                        animation.kind.to_ascii_lowercase()
                    ),
                    format!(
                        "animation <{}> is recorded but has no dedicated CSS mapping",
                        animation.kind
                    ),
                ));
            }
        }
    }
    if !document.storyboards.is_empty() && !matched_storyboard {
        warnings.push(Warning::new(
            "unsupported.storyboard.missing",
            format!(
                "no storyboard named '{}' was found; static layout is used",
                options.storyboard
            ),
        ));
    }
    for layer in &document.layers {
        walk_layer(layer, &mut warnings);
    }
    warnings
}

pub fn effective_storyboard_type(raw: Option<&str>) -> &str {
    match raw {
        None | Some("") => "TransitionIn",
        Some(value) => value,
    }
}

fn supported_animation(kind: &str) -> bool {
    matches!(
        kind,
        "Reveal"
            | "Fade"
            | "Fly"
            | "ZoomFade"
            | "Move"
            | "Scale"
            | "Rotate"
            | "Zoom"
            | "Wipe"
            | "Spin"
            | "Flip"
            | "ImageSequence"
            | "None"
    )
}

fn walk_layer(layer: &Layer, warnings: &mut Vec<Warning>) {
    for node in &layer.unknown_children {
        warnings.push(
            Warning::new(
                "unsupported.layer_child",
                format!("layer child <{}> is not converted", node.tag),
            )
            .with_object(&layer.name),
        );
    }
    for child in &layer.objects {
        match child {
            LayerChild::Layer(nested) => walk_layer(nested, warnings),
            LayerChild::Object(object) => warn_object(object, warnings),
        }
    }
}

fn warn_object(object: &GtObject, warnings: &mut Vec<Warning>) {
    if !object.kind.renders_html() {
        warnings.push(
            Warning::new(
                format!(
                    "unsupported.object.{}",
                    object.kind.as_str().to_ascii_lowercase()
                ),
                format!(
                    "<{}> '{}' is not converted to HTML",
                    object.tag, object.name
                ),
            )
            .with_object(&object.name),
        );
    }
    if object.kind == ObjectKind::Text3D {
        warnings.push(
            Warning::new(
                "approximate.text3d",
                "Text3D is approximated as 2D text; pixel match is not a goal",
            )
            .with_object(&object.name),
        );
    }
    if object.kind == ObjectKind::QrCode && object.image_source.is_none() {
        warnings.push(
            Warning::new(
                "unsupported.qr.generation",
                "QR object has no embedded image; a generator crate was not added",
            )
            .with_object(&object.name),
        );
    }
    if object
        .geometry
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("Cube"))
    {
        warnings.push(
            Warning::new(
                "approximate.cube",
                "Cube geometry is approximated in CSS 3D; pixel match is not a goal",
            )
            .with_object(&object.name),
        );
    }
    match &object.fill.kind {
        FillKind::Unsupported { detail, .. } => {
            warnings.push(Warning::new("unsupported.fill", detail).with_object(&object.name));
        }
        FillKind::Picture { extra, .. } if !extra.is_empty() => {
            for key in extra.keys() {
                warnings.push(
                    Warning::new(
                        "unsupported.picture_fill.attribute",
                        format!("picture fill attribute {key} is not applied"),
                    )
                    .with_object(&object.name),
                );
            }
        }
        _ => {}
    }
    if let FillKind::Unsupported { detail, .. } = &object.stroke.fill.kind {
        warnings.push(Warning::new("unsupported.stroke", detail).with_object(&object.name));
    }
    if object.kind == ObjectKind::Image && object.image_source.is_none() {
        warnings.push(
            Warning::new("unsupported.image.source", "Image has no Bitmap Source")
                .with_object(&object.name),
        );
    }
    if object.effects.reflection {
        warnings.push(
            Warning::new(
                "approximate.reflection",
                "Reflection is approximated with -webkit-box-reflect",
            )
            .with_object(&object.name),
        );
    }
    if let Some(mode) = &object.effects.compositing {
        let lower = normalize(mode);
        if lower == "replace" || lower == "additive" {
            warnings.push(
                Warning::new(
                    "approximate.compositing",
                    format!("Compositing {mode} is approximated with mix-blend-mode"),
                )
                .with_object(&object.name),
            );
        }
    }
    if object
        .style
        .auto_size
        .as_deref()
        .is_some_and(|value| !value.eq_ignore_ascii_case("Fixed"))
    {
        warnings.push(
            Warning::new(
                "unsupported.auto_size",
                format!(
                    "AutoSize={} is not applied as live layout",
                    object.style.auto_size.as_deref().unwrap_or("")
                ),
            )
            .with_object(&object.name),
        );
    }
    for node in &object.unknown_children {
        warnings.push(
            Warning::new(
                "unsupported.object_child",
                format!("object child <{}> is not converted", node.tag),
            )
            .with_object(&object.name),
        );
    }
    for key in object.extra_attrs.keys() {
        warnings.push(
            Warning::new(
                "unsupported.attribute",
                format!("attribute {key} is preserved but not rendered"),
            )
            .with_object(&object.name),
        );
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
