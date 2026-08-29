use std::collections::BTreeMap;
use std::io::BufRead;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::error::{Error, Result};
use crate::model::{
    Animation, Bounding, Color, Fill, FillKind, GtDocument, GtObject, Layer, LayerChild,
    Storyboard, UnknownNode, Vec3,
};

const KNOWN_OBJECT_ATTRS: &[&str] = &[
    "Name",
    "Dimensions",
    "Location",
    "Text",
    "FontFamily",
    "FontSize",
    "FontWeight",
    "FontStyle",
    "TextAlign",
    "VerticalAlign",
    "WordWrapping",
    "IgnoreOverhang",
    "LineSpacing",
    "AutoSize",
    "DataFlags",
    "Italic",
    "Underline",
    "Strikethrough",
    "AutoUpperCase",
    "RTL",
    "Locked",
    "Rotate",
    "Rotation",
    "Radius",
    "Opacity",
    "Thickness",
];

pub fn parse_document(xml: &str) -> Result<GtDocument> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut parser = Parser { reader };
    parser.parse()
}

struct Parser<R> {
    reader: Reader<R>,
}

impl<R: BufRead> Parser<R> {
    fn parse(&mut self) -> Result<GtDocument> {
        loop {
            match self.read()? {
                Event::Start(start) if local_name(&start) == "Composition" => {
                    return self.parse_root_composition(&start, false);
                }
                Event::Empty(start) if local_name(&start) == "Composition" => {
                    return self.parse_root_composition(&start, true);
                }
                Event::Eof => return Err(Error::MissingComposition),
                _ => {}
            }
        }
    }

    fn parse_root_composition(
        &mut self,
        start: &BytesStart<'_>,
        empty: bool,
    ) -> Result<GtDocument> {
        let attrs = attributes(start)?;
        let mut document = GtDocument {
            width: parse_f64(attrs.get("Width")).unwrap_or(0.0),
            height: parse_f64(attrs.get("Height")).unwrap_or(0.0),
            layers: Vec::new(),
            storyboards: Vec::new(),
            unknown_children: Vec::new(),
            extra_attrs: leftover_attrs(&attrs, &["Width", "Height"]),
            warnings: Vec::new(),
            asset_names: Vec::new(),
        };
        if empty {
            return Ok(document);
        }
        loop {
            match self.read()? {
                Event::Start(child) => match local_name(&child).as_str() {
                    "Layer" => document.layers.push(self.parse_layer(&child, false)?),
                    "Storyboard" => document
                        .storyboards
                        .push(self.parse_storyboard(&child, false)?),
                    other => {
                        document
                            .unknown_children
                            .push(self.parse_unknown(&child, false, other)?);
                    }
                },
                Event::Empty(child) => match local_name(&child).as_str() {
                    "Layer" => document.layers.push(self.parse_layer(&child, true)?),
                    "Storyboard" => document
                        .storyboards
                        .push(self.parse_storyboard(&child, true)?),
                    other => {
                        document
                            .unknown_children
                            .push(self.parse_unknown(&child, true, other)?);
                    }
                },
                Event::End(end) if end_name(&end) == "Composition" => break,
                Event::Eof => return Err(Error::UnexpectedEof("Composition")),
                _ => {}
            }
        }
        Ok(document)
    }

    fn parse_layer(&mut self, start: &BytesStart<'_>, empty: bool) -> Result<Layer> {
        let attrs = attributes(start)?;
        let mut layer = Layer {
            name: attrs.get("Name").cloned().unwrap_or_default(),
            location: attrs
                .get("Location")
                .map(|value| Vec3::parse(value))
                .unwrap_or_else(Vec3::zero),
            dimensions: attrs
                .get("Dimensions")
                .map(|value| Vec3::parse(value))
                .unwrap_or_else(Vec3::zero),
            locked: parse_bool(attrs.get("Locked")),
            objects: Vec::new(),
            extra_attrs: leftover_attrs(&attrs, &["Name", "Location", "Dimensions", "Locked"]),
            unknown_children: Vec::new(),
        };
        if empty {
            return Ok(layer);
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let tag = local_name(&child);
                    if tag == "Layer.Composition" || tag == "Composition" {
                        self.parse_layer_composition(&mut layer, &child, false)?;
                    } else if tag == "Layer" {
                        layer
                            .objects
                            .push(LayerChild::Layer(self.parse_layer(&child, false)?));
                    } else {
                        layer
                            .unknown_children
                            .push(self.parse_unknown(&child, false, &tag)?);
                    }
                }
                Event::Empty(child) => {
                    let tag = local_name(&child);
                    if tag == "Layer" {
                        layer
                            .objects
                            .push(LayerChild::Layer(self.parse_layer(&child, true)?));
                    } else {
                        layer
                            .unknown_children
                            .push(self.parse_unknown(&child, true, &tag)?);
                    }
                }
                Event::End(end) if end_name(&end) == "Layer" => break,
                Event::Eof => return Err(Error::UnexpectedEof("Layer")),
                _ => {}
            }
        }
        Ok(layer)
    }

    fn parse_layer_composition(
        &mut self,
        layer: &mut Layer,
        start: &BytesStart<'_>,
        empty: bool,
    ) -> Result<()> {
        let tag = local_name(start);
        if empty {
            return Ok(());
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Composition" {
                        self.parse_layer_composition(layer, &child, false)?;
                    } else if child_tag == "Layer" {
                        layer
                            .objects
                            .push(LayerChild::Layer(self.parse_layer(&child, false)?));
                    } else if is_object_tag(&child_tag) {
                        layer.objects.push(LayerChild::Object(Box::new(
                            self.parse_object(&child, false)?,
                        )));
                    } else {
                        layer
                            .unknown_children
                            .push(self.parse_unknown(&child, false, &child_tag)?);
                    }
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Layer" {
                        layer
                            .objects
                            .push(LayerChild::Layer(self.parse_layer(&child, true)?));
                    } else if is_object_tag(&child_tag) {
                        layer.objects.push(LayerChild::Object(Box::new(
                            self.parse_object(&child, true)?,
                        )));
                    } else {
                        layer
                            .unknown_children
                            .push(self.parse_unknown(&child, true, &child_tag)?);
                    }
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("Layer.Composition")),
                _ => {}
            }
        }
        Ok(())
    }

    fn parse_object(&mut self, start: &BytesStart<'_>, empty: bool) -> Result<GtObject> {
        let tag = local_name(start);
        let attrs = attributes(start)?;
        let mut object = GtObject::new(&tag);
        apply_object_attrs(&mut object, &attrs);
        if empty {
            return Ok(object);
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    self.consume_object_child(&mut object, &tag, &child, false, &child_tag)?;
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    self.consume_object_child(&mut object, &tag, &child, true, &child_tag)?;
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("object")),
                _ => {}
            }
        }
        Ok(object)
    }

    fn consume_object_child(
        &mut self,
        object: &mut GtObject,
        object_tag: &str,
        start: &BytesStart<'_>,
        empty: bool,
        child_tag: &str,
    ) -> Result<()> {
        let fill_tag = format!("{object_tag}.Fill");
        let stroke_tag = format!("{object_tag}.Stroke");
        let bounding_tag = format!("{object_tag}.Bounding");
        if child_tag == fill_tag || child_tag == "Fill" {
            object.fill = self.parse_fill(start, empty, child_tag)?;
        } else if child_tag == stroke_tag || child_tag == "Stroke" {
            object.stroke.fill = self.parse_fill(start, empty, child_tag)?;
        } else if child_tag == bounding_tag || child_tag == "Bounding" {
            if let Some(bounding) = self.parse_bounding(start, empty, child_tag)? {
                object.bounding = Some(bounding);
            }
        } else {
            object
                .unknown_children
                .push(self.parse_unknown(start, empty, child_tag)?);
        }
        Ok(())
    }

    fn parse_fill(&mut self, _start: &BytesStart<'_>, empty: bool, tag: &str) -> Result<Fill> {
        if empty {
            return Ok(Fill::transparent());
        }
        let mut found = None;
        let mut extras = Vec::new();
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    let node = self.parse_unknown(&child, false, &child_tag)?;
                    consider_fill_node(&mut found, &mut extras, node);
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    let node = self.parse_unknown(&child, true, &child_tag)?;
                    consider_fill_node(&mut found, &mut extras, node);
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("Fill")),
                _ => {}
            }
        }
        if extras.is_empty() {
            Ok(found.unwrap_or_else(Fill::transparent))
        } else {
            let mut children = extras;
            if let Some(Fill {
                kind: FillKind::Solid { color },
            }) = found
            {
                children.insert(
                    0,
                    UnknownNode {
                        tag: "Brush".to_string(),
                        attributes: BTreeMap::from([(
                            "Color".to_string(),
                            format!(
                                "#{:02X}{:02X}{:02X}{:02X}",
                                color.a, color.r, color.g, color.b
                            ),
                        )]),
                        children: Vec::new(),
                        text: None,
                    },
                );
            }
            Ok(Fill {
                kind: FillKind::Unsupported {
                    detail: "non-solid fill or stroke".to_string(),
                    node: UnknownNode {
                        tag: tag.to_string(),
                        attributes: BTreeMap::new(),
                        children,
                        text: None,
                    },
                },
            })
        }
    }

    fn parse_bounding(
        &mut self,
        start: &BytesStart<'_>,
        empty: bool,
        tag: &str,
    ) -> Result<Option<Bounding>> {
        let mut found = bounding_from_attrs(&attributes(start)?);
        if empty {
            return Ok(found);
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Bounding" {
                        let attrs = attributes(&child)?;
                        if found.is_none() {
                            found = bounding_from_attrs(&attrs);
                        }
                    }
                    self.skip_to_end(&child_tag)?;
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Bounding" {
                        let attrs = attributes(&child)?;
                        if found.is_none() {
                            found = bounding_from_attrs(&attrs);
                        }
                    }
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("Bounding")),
                _ => {}
            }
        }
        Ok(found)
    }

    fn parse_storyboard(&mut self, start: &BytesStart<'_>, empty: bool) -> Result<Storyboard> {
        let attrs = attributes(start)?;
        let mut storyboard = Storyboard {
            storyboard_type: attrs.get("Type").cloned(),
            reversed: parse_bool(attrs.get("Reverse").or(attrs.get("Reversed"))),
            animations: Vec::new(),
            extra_attrs: leftover_attrs(&attrs, &["Type", "Reverse", "Reversed"]),
            unknown_children: Vec::new(),
        };
        if empty {
            return Ok(storyboard);
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let tag = local_name(&child);
                    if tag == "Storyboard.Animations" || tag == "Animations" {
                        self.parse_animations(&mut storyboard, &child, false)?;
                    } else if is_animation_tag(&tag) {
                        storyboard
                            .animations
                            .push(self.parse_animation(&child, false, &tag)?);
                    } else {
                        storyboard
                            .unknown_children
                            .push(self.parse_unknown(&child, false, &tag)?);
                    }
                }
                Event::Empty(child) => {
                    let tag = local_name(&child);
                    if is_animation_tag(&tag) {
                        storyboard
                            .animations
                            .push(self.parse_animation(&child, true, &tag)?);
                    } else {
                        storyboard
                            .unknown_children
                            .push(self.parse_unknown(&child, true, &tag)?);
                    }
                }
                Event::End(end) if end_name(&end) == "Storyboard" => break,
                Event::Eof => return Err(Error::UnexpectedEof("Storyboard")),
                _ => {}
            }
        }
        Ok(storyboard)
    }

    fn parse_animations(
        &mut self,
        storyboard: &mut Storyboard,
        start: &BytesStart<'_>,
        empty: bool,
    ) -> Result<()> {
        let tag = local_name(start);
        if empty {
            return Ok(());
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    if is_animation_tag(&child_tag) {
                        storyboard
                            .animations
                            .push(self.parse_animation(&child, false, &child_tag)?);
                    } else {
                        storyboard
                            .unknown_children
                            .push(self.parse_unknown(&child, false, &child_tag)?);
                    }
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    if is_animation_tag(&child_tag) {
                        storyboard
                            .animations
                            .push(self.parse_animation(&child, true, &child_tag)?);
                    } else {
                        storyboard
                            .unknown_children
                            .push(self.parse_unknown(&child, true, &child_tag)?);
                    }
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("Storyboard.Animations")),
                _ => {}
            }
        }
        Ok(())
    }

    fn parse_animation(
        &mut self,
        start: &BytesStart<'_>,
        empty: bool,
        tag: &str,
    ) -> Result<Animation> {
        let attrs = attributes(start)?;
        if !empty {
            self.skip_to_end(tag)?;
        }
        Ok(Animation {
            kind: tag.to_string(),
            object: attrs.get("Object").cloned(),
            duration: attrs.get("Duration").cloned(),
            delay: attrs.get("Delay").cloned(),
            interpolation: attrs.get("Interpolation").cloned(),
            direction: attrs.get("Direction").cloned(),
            extra_attrs: leftover_attrs(
                &attrs,
                &["Object", "Duration", "Delay", "Interpolation", "Direction"],
            ),
        })
    }

    fn parse_unknown(
        &mut self,
        start: &BytesStart<'_>,
        empty: bool,
        tag: &str,
    ) -> Result<UnknownNode> {
        let attributes = attributes(start)?;
        if empty {
            return Ok(UnknownNode {
                tag: tag.to_string(),
                attributes,
                children: Vec::new(),
                text: None,
            });
        }
        let mut children = Vec::new();
        let mut text = String::new();
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    children.push(self.parse_unknown(&child, false, &child_tag)?);
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    children.push(self.parse_unknown(&child, true, &child_tag)?);
                }
                Event::Text(t) => {
                    text.push_str(&t.xml10_content());
                }
                Event::CData(cdata) => {
                    text.push_str(&cdata.xml10_content());
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("unknown")),
                _ => {}
            }
        }
        Ok(UnknownNode {
            tag: tag.to_string(),
            attributes,
            children,
            text: if text.trim().is_empty() {
                None
            } else {
                Some(text)
            },
        })
    }

    fn skip_to_end(&mut self, _tag: &str) -> Result<()> {
        let mut depth = 1;
        loop {
            match self.read()? {
                Event::Start(_) => depth += 1,
                Event::End(_) => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                Event::Eof => return Err(Error::UnexpectedEof("skip")),
                _ => {}
            }
        }
    }

    fn read(&mut self) -> Result<Event<'static>> {
        let mut buf = Vec::new();
        let event = self.reader.read_event_into(&mut buf)?;
        Ok(event.into_owned())
    }
}

fn consider_fill_node(found: &mut Option<Fill>, extras: &mut Vec<UnknownNode>, node: UnknownNode) {
    if node.tag == "Brush"
        && node.children.is_empty()
        && let Some(color) = node
            .attributes
            .get("Color")
            .and_then(|value| Color::parse(value))
    {
        *found = Some(Fill::solid(color));
        return;
    }
    extras.push(node);
}

fn bounding_from_attrs(attrs: &BTreeMap<String, String>) -> Option<Bounding> {
    attrs.get("Object").map(|object| Bounding {
        object: object.clone(),
        padding: attrs
            .get("Padding")
            .cloned()
            .unwrap_or_else(|| "0,0,0,0".to_string()),
    })
}

fn apply_object_attrs(object: &mut GtObject, attrs: &BTreeMap<String, String>) {
    if let Some(name) = attrs.get("Name") {
        object.name = name.clone();
    }
    if let Some(location) = attrs.get("Location") {
        object.location = Vec3::parse(location);
    }
    if let Some(dimensions) = attrs.get("Dimensions") {
        object.dimensions = Vec3::parse(dimensions);
    }
    if let Some(text) = attrs.get("Text") {
        object.text = Some(text.clone());
    }
    object.style.font_family = attrs.get("FontFamily").cloned();
    object.style.font_size = parse_f64(attrs.get("FontSize"));
    object.style.font_weight = attrs.get("FontWeight").cloned();
    object.style.font_style = attrs.get("FontStyle").cloned();
    object.style.text_align = attrs.get("TextAlign").cloned();
    object.style.vertical_align = attrs.get("VerticalAlign").cloned();
    object.style.word_wrapping = attrs.get("WordWrapping").cloned();
    object.style.ignore_overhang = attrs.get("IgnoreOverhang").cloned();
    object.style.line_spacing = parse_f64(attrs.get("LineSpacing"));
    object.style.auto_size = attrs.get("AutoSize").cloned();
    object.style.italic = parse_bool(attrs.get("Italic"));
    object.style.underline = parse_bool(attrs.get("Underline"));
    object.style.strikethrough = parse_bool(attrs.get("Strikethrough"));
    object.style.auto_upper_case = parse_bool(attrs.get("AutoUpperCase"));
    object.style.rtl = parse_bool(attrs.get("RTL"));
    object.rotate = parse_f64(attrs.get("Rotate").or(attrs.get("Rotation")));
    object.radius = parse_f64(attrs.get("Radius"));
    object.opacity = parse_f64(attrs.get("Opacity"));
    if let Some(thickness) = parse_f64(attrs.get("Thickness")) {
        object.stroke.thickness = Some(thickness);
    }
    object.extra_attrs = leftover_attrs(attrs, KNOWN_OBJECT_ATTRS);
}

fn leftover_attrs(attrs: &BTreeMap<String, String>, known: &[&str]) -> BTreeMap<String, String> {
    attrs
        .iter()
        .filter(|(key, _)| !known.iter().any(|known| known.eq_ignore_ascii_case(key)))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn is_object_tag(tag: &str) -> bool {
    !tag.contains('.')
        && !matches!(
            tag,
            "Composition" | "Layer" | "Storyboard" | "Brush" | "Bounding" | "Fill" | "Stroke"
        )
}

fn is_animation_tag(tag: &str) -> bool {
    matches!(
        tag,
        "Reveal" | "Fade" | "Move" | "Scale" | "Rotate" | "Flip" | "Wipe" | "Fly" | "Zoom" | "Spin"
    ) || (!tag.contains('.') && tag.ends_with("Animation"))
}

fn parse_f64(value: Option<&String>) -> Option<f64> {
    value.and_then(|value| value.trim().parse().ok())
}

fn parse_bool(value: Option<&String>) -> bool {
    value
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes"
            )
        })
        .unwrap_or(false)
}

fn attributes(start: &BytesStart<'_>) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    for attr in start.attributes() {
        let attr = attr?;
        let key = attr.key.as_ref().to_string();
        let value = attr
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)?
            .into_owned();
        map.insert(key, value);
    }
    Ok(map)
}

fn local_name(start: &BytesStart<'_>) -> String {
    start.local_name().as_ref().to_string()
}

fn end_name(end: &quick_xml::events::BytesEnd<'_>) -> String {
    end.local_name().as_ref().to_string()
}
