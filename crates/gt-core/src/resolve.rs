use crate::model::{FillKind, GtDocument, GtObject, Layer, LayerChild, ObjectKind};
use crate::warn::Warning;

pub fn collect_warnings(document: &GtDocument) -> Vec<Warning> {
    let mut warnings = document.warnings.clone();
    for node in &document.unknown_children {
        warnings.push(Warning::new(
            "unsupported.root_child",
            format!("root child <{}> is not converted in phase 1", node.tag),
        ));
    }
    for storyboard in &document.storyboards {
        warnings.push(Warning::new(
            "unsupported.storyboard",
            format!(
                "storyboard {} is parsed but not rendered in phase 1",
                storyboard.storyboard_type.as_deref().unwrap_or("(untyped)")
            ),
        ));
    }
    for layer in &document.layers {
        walk_layer(layer, &mut warnings);
    }
    warnings
}

fn walk_layer(layer: &Layer, warnings: &mut Vec<Warning>) {
    for node in &layer.unknown_children {
        warnings.push(
            Warning::new(
                "unsupported.layer_child",
                format!("layer child <{}> is not converted in phase 1", node.tag),
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
    if !object.kind.phase1_renderable() {
        warnings.push(
            Warning::new(
                format!(
                    "unsupported.object.{}",
                    object.kind.as_str().to_ascii_lowercase()
                ),
                format!(
                    "<{}> '{}' is not converted to HTML in phase 1",
                    object.tag, object.name
                ),
            )
            .with_object(&object.name),
        );
    }
    match &object.fill.kind {
        FillKind::Unsupported { detail, .. } => {
            warnings.push(Warning::new("unsupported.fill", detail).with_object(&object.name));
        }
        FillKind::Solid { .. } | FillKind::Transparent => {}
    }
    if let FillKind::Unsupported { detail, .. } = &object.stroke.fill.kind {
        warnings.push(Warning::new("unsupported.stroke", detail).with_object(&object.name));
    }
    if object.bounding.is_some() {
        warnings.push(
            Warning::new(
                "unsupported.bounding",
                "Bounding is recorded but not resolved in phase 1",
            )
            .with_object(&object.name),
        );
    }
    if object.rotate.is_some() {
        warnings.push(
            Warning::new(
                "unsupported.rotate",
                "Rotate is recorded but not applied in phase 1",
            )
            .with_object(&object.name),
        );
    }
    if object.radius.is_some() && object.kind != ObjectKind::Rectangle {
        warnings.push(
            Warning::new(
                "unsupported.radius",
                "Radius is recorded but only rectangles will use it in a later phase",
            )
            .with_object(&object.name),
        );
    }
    if object.opacity.is_some() {
        warnings.push(
            Warning::new(
                "unsupported.opacity",
                "Opacity is recorded but not applied in phase 1",
            )
            .with_object(&object.name),
        );
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
                    "AutoSize={} is not applied in phase 1",
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
                format!("object child <{}> is not converted in phase 1", node.tag),
            )
            .with_object(&object.name),
        );
    }
    for key in object.extra_attrs.keys() {
        warnings.push(
            Warning::new(
                "unsupported.attribute",
                format!("attribute {key} is preserved but not rendered in phase 1"),
            )
            .with_object(&object.name),
        );
    }
}
