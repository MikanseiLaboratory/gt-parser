use std::collections::BTreeMap;

use serde::Serialize;

use crate::warn::Warning;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }

    pub fn parse(raw: &str) -> Self {
        let mut parts = raw
            .split(',')
            .map(|part| part.trim().parse().unwrap_or(0.0));
        Self {
            x: parts.next().unwrap_or(0.0),
            y: parts.next().unwrap_or(0.0),
            z: parts.next().unwrap_or(0.0),
        }
    }
}

/// GT color stored as #AARRGGBB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Color {
    pub a: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub fn transparent() -> Self {
        Self {
            a: 0,
            r: 0,
            g: 0,
            b: 0,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let hex = raw.trim().trim_start_matches('#');
        match hex.len() {
            6 => {
                let n = u32::from_str_radix(hex, 16).ok()?;
                Some(Self {
                    a: 255,
                    r: ((n >> 16) & 0xFF) as u8,
                    g: ((n >> 8) & 0xFF) as u8,
                    b: (n & 0xFF) as u8,
                })
            }
            8 => {
                let n = u32::from_str_radix(hex, 16).ok()?;
                Some(Self {
                    a: ((n >> 24) & 0xFF) as u8,
                    r: ((n >> 16) & 0xFF) as u8,
                    g: ((n >> 8) & 0xFF) as u8,
                    b: (n & 0xFF) as u8,
                })
            }
            _ => None,
        }
    }

    pub fn is_transparent(self) -> bool {
        self.a == 0
    }

    pub fn to_css(self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!(
                "rgba({}, {}, {}, {:.3})",
                self.r,
                self.g,
                self.b,
                f64::from(self.a) / 255.0
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnknownNode {
    pub tag: String,
    pub attributes: BTreeMap<String, String>,
    pub children: Vec<UnknownNode>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bounding {
    pub object: String,
    pub padding: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FillKind {
    Solid { color: Color },
    Transparent,
    Unsupported { detail: String, node: UnknownNode },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fill {
    pub kind: FillKind,
}

impl Fill {
    pub fn solid(color: Color) -> Self {
        if color.is_transparent() {
            Self {
                kind: FillKind::Transparent,
            }
        } else {
            Self {
                kind: FillKind::Solid { color },
            }
        }
    }

    pub fn transparent() -> Self {
        Self {
            kind: FillKind::Transparent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stroke {
    pub fill: Fill,
    pub thickness: Option<f64>,
}

impl Stroke {
    pub fn none() -> Self {
        Self {
            fill: Fill::transparent(),
            thickness: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct TextStyle {
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub font_weight: Option<String>,
    pub font_style: Option<String>,
    pub text_align: Option<String>,
    pub vertical_align: Option<String>,
    pub word_wrapping: Option<String>,
    pub ignore_overhang: Option<String>,
    pub line_spacing: Option<f64>,
    pub auto_size: Option<String>,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub auto_upper_case: bool,
    pub rtl: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum ObjectKind {
    TextBlock,
    Rectangle,
    Ellipse,
    Triangle,
    Image,
    Ticker,
    Text3D,
    QrCode,
    Unknown,
}

impl ObjectKind {
    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "TextBlock" => Self::TextBlock,
            "Rectangle" => Self::Rectangle,
            "Ellipse" => Self::Ellipse,
            "Triangle" => Self::Triangle,
            "Image" => Self::Image,
            "Ticker" => Self::Ticker,
            "Text3D" | "TextBlock3D" => Self::Text3D,
            "QrCode" | "QRCode" | "QR" => Self::QrCode,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TextBlock => "TextBlock",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Triangle => "Triangle",
            Self::Image => "Image",
            Self::Ticker => "Ticker",
            Self::Text3D => "Text3D",
            Self::QrCode => "QrCode",
            Self::Unknown => "Unknown",
        }
    }

    pub fn is_shape(self) -> bool {
        matches!(self, Self::Rectangle | Self::Ellipse | Self::Triangle)
    }

    pub fn phase1_renderable(self) -> bool {
        matches!(
            self,
            Self::TextBlock | Self::Rectangle | Self::Ellipse | Self::Triangle
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GtObject {
    pub kind: ObjectKind,
    pub tag: String,
    pub name: String,
    pub location: Vec3,
    pub dimensions: Vec3,
    pub text: Option<String>,
    pub style: TextStyle,
    pub fill: Fill,
    pub stroke: Stroke,
    pub bounding: Option<Bounding>,
    pub rotate: Option<f64>,
    pub radius: Option<f64>,
    pub opacity: Option<f64>,
    pub extra_attrs: BTreeMap<String, String>,
    pub unknown_children: Vec<UnknownNode>,
}

impl GtObject {
    pub fn new(tag: &str) -> Self {
        Self {
            kind: ObjectKind::from_tag(tag),
            tag: tag.to_string(),
            name: String::new(),
            location: Vec3::zero(),
            dimensions: Vec3::zero(),
            text: None,
            style: TextStyle::default(),
            fill: Fill::transparent(),
            stroke: Stroke::none(),
            bounding: None,
            rotate: None,
            radius: None,
            opacity: None,
            extra_attrs: BTreeMap::new(),
            unknown_children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Layer {
    pub name: String,
    pub location: Vec3,
    pub dimensions: Vec3,
    pub locked: bool,
    pub objects: Vec<LayerChild>,
    pub extra_attrs: BTreeMap<String, String>,
    pub unknown_children: Vec<UnknownNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum LayerChild {
    Object(Box<GtObject>),
    Layer(Layer),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Animation {
    pub kind: String,
    pub object: Option<String>,
    pub duration: Option<String>,
    pub delay: Option<String>,
    pub interpolation: Option<String>,
    pub direction: Option<String>,
    pub extra_attrs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Storyboard {
    pub storyboard_type: Option<String>,
    pub reversed: bool,
    pub animations: Vec<Animation>,
    pub extra_attrs: BTreeMap<String, String>,
    pub unknown_children: Vec<UnknownNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GtDocument {
    pub width: f64,
    pub height: f64,
    pub layers: Vec<Layer>,
    pub storyboards: Vec<Storyboard>,
    pub unknown_children: Vec<UnknownNode>,
    pub extra_attrs: BTreeMap<String, String>,
    pub warnings: Vec<Warning>,
    pub asset_names: Vec<String>,
}

impl GtDocument {
    pub fn inspect_report(&self) -> InspectReport {
        InspectReport {
            width: self.width,
            height: self.height,
            layers: self.layers.iter().map(LayerSummary::from_layer).collect(),
            storyboards: self
                .storyboards
                .iter()
                .map(|storyboard| StoryboardSummary {
                    storyboard_type: storyboard.storyboard_type.clone(),
                    animation_count: storyboard.animations.len(),
                })
                .collect(),
            warnings: self.warnings.clone(),
            asset_names: self.asset_names.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InspectReport {
    pub width: f64,
    pub height: f64,
    pub layers: Vec<LayerSummary>,
    pub storyboards: Vec<StoryboardSummary>,
    pub warnings: Vec<Warning>,
    pub asset_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LayerSummary {
    pub name: String,
    pub objects: Vec<ObjectSummary>,
}

impl LayerSummary {
    fn from_layer(layer: &Layer) -> Self {
        Self {
            name: layer.name.clone(),
            objects: flatten_objects(layer)
                .into_iter()
                .map(|object| ObjectSummary {
                    name: object.name.clone(),
                    kind: object.kind.as_str().to_string(),
                    text: object.text.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObjectSummary {
    pub name: String,
    pub kind: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoryboardSummary {
    pub storyboard_type: Option<String>,
    pub animation_count: usize,
}

pub fn flatten_objects(layer: &Layer) -> Vec<&GtObject> {
    let mut out = Vec::new();
    collect_objects(layer, &mut out);
    out
}

fn collect_objects<'a>(layer: &'a Layer, out: &mut Vec<&'a GtObject>) {
    for child in &layer.objects {
        match child {
            LayerChild::Object(object) => out.push(object),
            LayerChild::Layer(nested) => collect_objects(nested, out),
        }
    }
}
