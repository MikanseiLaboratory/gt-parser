use crate::model::{FillKind, GtDocument, GtObject, Layer, LayerChild, ObjectKind};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DataField {
    pub name: String,
    pub object: String,
    pub kind: String,
    pub event_source: bool,
}

pub fn list_fields(document: &GtDocument) -> Vec<DataField> {
    let mut fields = Vec::new();
    for layer in &document.layers {
        for child in &layer.objects {
            if let LayerChild::Object(object) = child {
                fields.extend(fields_for_object(object));
            }
        }
    }
    fields.reverse();
    fields
}

pub fn is_known_field(document: &GtDocument, name: &str) -> bool {
    list_fields(document).iter().any(|field| field.name == name)
}

fn fields_for_object(object: &GtObject) -> Vec<DataField> {
    let hidden = has_flag(object, "Hidden") || has_flag(object, "NoEvents");
    let event_source = !hidden;
    match object.kind {
        ObjectKind::TextBlock | ObjectKind::Text3D => vec![DataField {
            name: format!("{}.Text", object.name),
            object: object.name.clone(),
            kind: "Text".to_string(),
            event_source,
        }],
        ObjectKind::Image | ObjectKind::QrCode | ObjectKind::ImageSequence => vec![DataField {
            name: format!("{}.Source", object.name),
            object: object.name.clone(),
            kind: "Source".to_string(),
            event_source,
        }],
        ObjectKind::Ticker => ticker_fields(object, event_source),
        ObjectKind::Rectangle | ObjectKind::Ellipse => {
            let kind = if matches!(object.fill.kind, FillKind::Picture { .. }) {
                "Fill.Bitmap"
            } else {
                "Fill.Color"
            };
            let event_source = !hidden && object.data_flags.is_some();
            vec![DataField {
                name: format!("{}.{kind}", object.name),
                object: object.name.clone(),
                kind: kind.to_string(),
                event_source,
            }]
        }
        _ => Vec::new(),
    }
}

fn ticker_fields(object: &GtObject, event_source: bool) -> Vec<DataField> {
    if let Some(template) = &object.ticker_template
        && !template.tag.eq_ignore_ascii_case("TextBlock")
        && !template.tag.eq_ignore_ascii_case("Text3D")
    {
        return template
            .children
            .iter()
            .filter_map(|child| {
                let name = child.attributes.get("Name")?.clone();
                let suffix = if child.tag.eq_ignore_ascii_case("Image") {
                    "Source"
                } else {
                    "Text"
                };
                Some(DataField {
                    name: format!("{}.{name}.{suffix}", object.name),
                    object: object.name.clone(),
                    kind: suffix.to_string(),
                    event_source,
                })
            })
            .collect();
    }
    vec![DataField {
        name: format!("{}.Text", object.name),
        object: object.name.clone(),
        kind: "Text".to_string(),
        event_source,
    }]
}

fn has_flag(object: &GtObject, flag: &str) -> bool {
    object
        .data_flags
        .as_deref()
        .unwrap_or("")
        .split(',')
        .any(|part| part.trim().eq_ignore_ascii_case(flag))
}

pub fn set_field(document: &mut GtDocument, field: &str, value: &str) -> bool {
    let Some((object_name, rest)) = field.split_once('.') else {
        return false;
    };
    for layer in &mut document.layers {
        if set_field_in_layer(layer, object_name, rest, value) {
            return true;
        }
    }
    false
}

fn set_field_in_layer(layer: &mut Layer, object_name: &str, rest: &str, value: &str) -> bool {
    for child in &mut layer.objects {
        match child {
            LayerChild::Object(object) if object.name.eq_ignore_ascii_case(object_name) => {
                return apply_field(object, rest, value);
            }
            LayerChild::Layer(nested) => {
                if set_field_in_layer(nested, object_name, rest, value) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn apply_field(object: &mut GtObject, rest: &str, value: &str) -> bool {
    match rest {
        "Text" => {
            object.text = Some(value.to_string());
            true
        }
        "Source" => {
            object.image_source = Some(value.to_string());
            true
        }
        "Fill.Color" => {
            if let Some(color) = crate::model::Color::parse(value) {
                object.fill = crate::model::Fill::solid(color);
                true
            } else {
                false
            }
        }
        "Fill.Bitmap" => {
            object.fill.kind = FillKind::Picture {
                source: value.to_string(),
                size_mode: None,
                extra: Default::default(),
            };
            true
        }
        _ => false,
    }
}
