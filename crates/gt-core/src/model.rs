use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::warn::Warning;

pub const MAX_ANIMATIONS_PER_OBJECT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let mut parts = raw.split(',').map(|part| part.trim().parse().ok());
        Some(Self {
            x: parts.next()??,
            y: parts.next()??,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

    pub fn format_xyz(&self) -> String {
        format!(
            "{},{},{}",
            trim_float(self.x),
            trim_float(self.y),
            trim_float(self.z)
        )
    }
}

impl Default for Vec3 {
    fn default() -> Self {
        Self::zero()
    }
}

pub fn trim_float(value: f64) -> String {
    let text = format!("{value:.10}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-" {
        "0".to_string()
    } else {
        text.to_string()
    }
}

pub fn angle_to_points(angle_deg: f64) -> (Point2, Point2) {
    let rad = angle_deg.to_radians();
    (
        Point2::new(0.5 - rad.cos() * 0.5, 0.5 - rad.sin() * 0.5),
        Point2::new(0.5 + rad.cos() * 0.5, 0.5 + rad.sin() * 0.5),
    )
}

pub fn anchor_fractions(anchor: Option<&str>) -> (f64, f64) {
    match anchor.unwrap_or("TopLeft") {
        "TopCenter" => (0.5, 0.0),
        "TopRight" => (1.0, 0.0),
        "MiddleLeft" => (0.0, 0.5),
        "MiddleCenter" | "Center" => (0.5, 0.5),
        "MiddleRight" => (1.0, 0.5),
        "BottomLeft" => (0.0, 1.0),
        "BottomCenter" => (0.5, 1.0),
        "BottomRight" => (1.0, 1.0),
        _ => (0.0, 0.0),
    }
}

pub fn top_left_from_anchor(location: &Vec3, dimensions: &Vec3, anchor: Option<&str>) -> Vec3 {
    let (fx, fy) = anchor_fractions(anchor);
    Vec3 {
        x: location.x - fx * dimensions.x,
        y: location.y - fy * dimensions.y,
        z: location.z,
    }
}

pub fn anchor_point_from_top_left(
    location: &Vec3,
    dimensions: &Vec3,
    anchor: Option<&str>,
) -> Vec3 {
    let (fx, fy) = anchor_fractions(anchor);
    Vec3 {
        x: location.x + fx * dimensions.x,
        y: location.y + fy * dimensions.y,
        z: location.z,
    }
}

/// GT color stored as #AARRGGBB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    pub fn to_argb(self) -> String {
        format!("#{:02X}{:02X}{:02X}{:02X}", self.a, self.r, self.g, self.b)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UnknownNode {
    pub tag: String,
    pub attributes: BTreeMap<String, String>,
    pub children: Vec<UnknownNode>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bounding {
    pub object: String,
    pub padding: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FillKind {
    Solid {
        color: Color,
    },
    Transparent,
    LinearGradient {
        start: Point2,
        end: Point2,
        wrap: Option<String>,
        stops: Vec<GradientStop>,
    },
    RadialGradient {
        wrap: Option<String>,
        stops: Vec<GradientStop>,
    },
    Picture {
        source: String,
        size_mode: Option<String>,
        extra: BTreeMap<String, String>,
    },
    Unsupported {
        detail: String,
        node: UnknownNode,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub kind: FillKind,
}

impl Default for Fill {
    fn default() -> Self {
        Self::transparent()
    }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub fill: Fill,
    pub thickness: Option<f64>,
    #[serde(default)]
    pub dash_style: Option<String>,
}

impl Default for Stroke {
    fn default() -> Self {
        Self::none()
    }
}

impl Stroke {
    pub fn none() -> Self {
        Self {
            fill: Fill::transparent(),
            thickness: None,
            dash_style: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
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
    pub font_stretch: Option<String>,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub auto_upper_case: bool,
    pub rtl: bool,
    #[serde(default)]
    pub text_effect: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub color: Color,
    pub offset: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShadowEffect {
    pub mode: Option<String>,
    pub blur: Option<f64>,
    pub color: Option<Color>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CropEffect {
    pub range: Option<String>,
    pub feather: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ObjectEffects {
    pub shadow: Option<ShadowEffect>,
    pub crop: Option<CropEffect>,
    pub mask: Option<String>,
    pub skew: Option<Vec3>,
    pub reflection: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    pub compositing: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
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
    ImageSequence,
    #[default]
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
            "ImageSequence" => Self::ImageSequence,
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
            Self::ImageSequence => "ImageSequence",
            Self::Unknown => "Unknown",
        }
    }

    pub fn is_shape(self) -> bool {
        matches!(self, Self::Rectangle | Self::Ellipse | Self::Triangle)
    }

    pub fn phase1_renderable(self) -> bool {
        self.renders_html()
    }

    pub fn renders_html(self) -> bool {
        !matches!(self, Self::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GtObject {
    pub kind: ObjectKind,
    pub tag: String,
    pub name: String,
    pub location: Vec3,
    pub dimensions: Vec3,
    pub anchor: Option<String>,
    pub data_flags: Option<String>,
    pub locked: bool,
    pub text: Option<String>,
    pub style: TextStyle,
    pub fill: Fill,
    pub stroke: Stroke,
    pub bounding: Option<Bounding>,
    pub rotate: Option<f64>,
    pub rotate_xyz: Option<Vec3>,
    pub radius: Option<f64>,
    pub opacity: Option<f64>,
    #[serde(default = "default_true")]
    pub visible: bool,
    pub size_mode: Option<String>,
    pub geometry: Option<String>,
    pub image_source: Option<String>,
    #[serde(default)]
    pub bitmap_position: Option<f64>,
    #[serde(default)]
    pub rect_style: Option<String>,
    pub effects: ObjectEffects,
    pub ticker_speed: Option<f64>,
    pub ticker_direction: Option<String>,
    pub ticker_kind: Option<String>,
    #[serde(default)]
    pub ticker_template: Option<UnknownNode>,
    pub extra_attrs: BTreeMap<String, String>,
    pub unknown_children: Vec<UnknownNode>,
}

impl Default for GtObject {
    fn default() -> Self {
        Self::new("Unknown")
    }
}

impl GtObject {
    pub fn new(tag: &str) -> Self {
        Self {
            kind: ObjectKind::from_tag(tag),
            tag: tag.to_string(),
            name: String::new(),
            location: Vec3::zero(),
            dimensions: Vec3::zero(),
            anchor: None,
            data_flags: None,
            locked: false,
            text: None,
            style: TextStyle::default(),
            fill: Fill::transparent(),
            stroke: Stroke::none(),
            bounding: None,
            rotate: None,
            rotate_xyz: None,
            radius: None,
            opacity: None,
            visible: true,
            size_mode: None,
            geometry: None,
            image_source: None,
            bitmap_position: None,
            rect_style: None,
            effects: ObjectEffects::default(),
            ticker_speed: None,
            ticker_direction: None,
            ticker_kind: None,
            ticker_template: None,
            extra_attrs: BTreeMap::new(),
            unknown_children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub name: String,
    pub location: Vec3,
    pub dimensions: Vec3,
    pub locked: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub inner_width: Option<f64>,
    #[serde(default)]
    pub inner_height: Option<f64>,
    pub objects: Vec<LayerChild>,
    pub extra_attrs: BTreeMap<String, String>,
    pub unknown_children: Vec<UnknownNode>,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            name: String::new(),
            location: Vec3::zero(),
            dimensions: Vec3::zero(),
            locked: false,
            visible: true,
            inner_width: None,
            inner_height: None,
            objects: Vec::new(),
            extra_attrs: BTreeMap::new(),
            unknown_children: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum LayerChild {
    Object(Box<GtObject>),
    Layer(Layer),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Animation {
    pub kind: String,
    pub object: Option<String>,
    pub duration: Option<String>,
    pub delay: Option<String>,
    pub interpolation: Option<String>,
    pub direction: Option<String>,
    pub reversed: bool,
    pub center_axis: Option<String>,
    #[serde(default)]
    pub speed: Option<String>,
    #[serde(default)]
    pub muted: bool,
    pub extra_attrs: BTreeMap<String, String>,
}

impl Animation {
    pub fn delay_secs(&self) -> f64 {
        self.delay
            .as_deref()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0.0)
    }

    pub fn duration_secs(&self) -> f64 {
        self.duration
            .as_deref()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(1.0)
    }

    pub fn speed_value(&self) -> f64 {
        self.speed
            .as_deref()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(1.0)
    }

    pub fn is_placeholder(&self) -> bool {
        self.kind.eq_ignore_ascii_case("None")
    }

    pub fn is_continuous_type(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "RotateContinuous" | "FillOffset" | "StrokeOffset" | "Blink" | "ImageSequenceLoop"
        )
    }

    pub fn end_secs(&self) -> f64 {
        if self.is_continuous_type() || self.is_placeholder() {
            self.delay_secs()
        } else {
            self.delay_secs() + self.duration_secs()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Storyboard {
    pub storyboard_type: Option<String>,
    pub reversed: bool,
    pub data_name: Option<String>,
    pub animations: Vec<Animation>,
    pub extra_attrs: BTreeMap<String, String>,
    pub unknown_children: Vec<UnknownNode>,
}

impl Storyboard {
    pub fn effective_type(&self) -> &str {
        match self.storyboard_type.as_deref() {
            None | Some("") => "TransitionIn",
            Some(value) => value,
        }
    }

    pub fn plays_rewound(&self) -> bool {
        matches!(self.effective_type(), "TransitionOut" | "DataChangeIn")
    }

    pub fn is_continuous(&self) -> bool {
        self.effective_type().eq_ignore_ascii_case("Continuous")
    }

    pub fn is_scoped(&self) -> bool {
        self.data_name
            .as_deref()
            .is_some_and(|value| !value.is_empty())
    }

    pub fn duration(&self) -> f64 {
        self.animations
            .iter()
            .filter(|animation| !animation.is_placeholder())
            .map(Animation::end_secs)
            .fold(0.0, f64::max)
    }

    pub fn earliest_start(&self) -> f64 {
        self.animations
            .iter()
            .filter(|animation| !animation.is_placeholder())
            .map(Animation::delay_secs)
            .fold(0.0, f64::min)
    }

    pub fn counted_animations_for(&self, object: &str) -> usize {
        self.animations
            .iter()
            .filter(|animation| {
                !animation.is_placeholder()
                    && animation
                        .object
                        .as_deref()
                        .is_some_and(|name| name.eq_ignore_ascii_case(object))
            })
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectReport {
    pub width: f64,
    pub height: f64,
    pub layers: Vec<LayerSummary>,
    pub storyboards: Vec<StoryboardSummary>,
    pub warnings: Vec<Warning>,
    pub asset_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectSummary {
    pub name: String,
    pub kind: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoryboardSummary {
    pub storyboard_type: Option<String>,
    pub animation_count: usize,
}

pub fn flatten_objects(layer: &Layer) -> Vec<&GtObject> {
    let mut out = Vec::new();
    collect_objects(layer, &mut out);
    out
}

pub fn flatten_objects_mut(layer: &mut Layer) -> Vec<&mut GtObject> {
    let mut out = Vec::new();
    collect_objects_mut(layer, &mut out);
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

fn collect_objects_mut<'a>(layer: &'a mut Layer, out: &mut Vec<&'a mut GtObject>) {
    for child in &mut layer.objects {
        match child {
            LayerChild::Object(object) => out.push(object),
            LayerChild::Layer(nested) => collect_objects_mut(nested, out),
        }
    }
}

impl GtObject {
    pub fn top_left(&self) -> Vec3 {
        top_left_from_anchor(&self.location, &self.dimensions, self.anchor.as_deref())
    }

    pub fn opacity_value(&self) -> f64 {
        match self.opacity {
            Some(value) if value > 1.0 => value / 100.0,
            Some(value) => value,
            None => 1.0,
        }
    }
}
