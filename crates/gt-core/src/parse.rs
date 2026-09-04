use std::collections::BTreeMap;
use std::io::BufRead;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::error::{Error, Result};
use crate::model::{
    Animation, Bounding, Color, CropEffect, Fill, FillKind, GradientStop, GtDocument, GtObject,
    Layer, LayerChild, Point2, ShadowEffect, Storyboard, UnknownNode, Vec3, angle_to_points,
    top_left_from_anchor,
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
    "Anchor",
    "TextEffect",
    "StrokeThickness",
    "Style",
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
    "Visible",
    "SizeMode",
    "Geometry",
    "Speed",
    "Direction",
    "Type",
    "CompositingMode",
    "Compositing",
    "Source",
    "FontStretch",
    "TextWordWrapping",
    "FlipX",
    "FlipY",
    "TextureFlip",
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
            visible: attrs
                .get("Visible")
                .map(|value| parse_bool(Some(value)))
                .unwrap_or(true),
            inner_width: None,
            inner_height: None,
            objects: Vec::new(),
            extra_attrs: leftover_attrs(
                &attrs,
                &["Name", "Location", "Dimensions", "Locked", "Visible"],
            ),
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
        if tag == "Composition" {
            let inner = attributes(start)?;
            layer.inner_width = parse_f64(inner.get("Width")).or(layer.inner_width);
            layer.inner_height = parse_f64(inner.get("Height")).or(layer.inner_height);
        }
        if empty {
            return Ok(());
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Composition" {
                        let inner = attributes(&child)?;
                        layer.inner_width = parse_f64(inner.get("Width"));
                        layer.inner_height = parse_f64(inner.get("Height"));
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
            object.location = top_left_from_anchor(
                &object.location,
                &object.dimensions,
                object.anchor.as_deref(),
            );
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
        object.location = top_left_from_anchor(
            &object.location,
            &object.dimensions,
            object.anchor.as_deref(),
        );
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
        let bitmap_tag = format!("{object_tag}.Bitmap");
        let effects_tag = format!("{object_tag}.Effects");
        let crop_tag = format!("{object_tag}.Crop");
        let mask_tag = format!("{object_tag}.Mask");
        let template_tag = format!("{object_tag}.Template");
        let transform_tag = format!("{object_tag}.Transform");
        let stroke_style_tag = format!("{object_tag}.StrokeStyle");
        if child_tag == fill_tag || child_tag == "Fill" {
            object.fill = self.parse_fill(start, empty, child_tag)?;
        } else if child_tag == stroke_tag || child_tag == "Stroke" {
            object.stroke.fill = self.parse_fill(start, empty, child_tag)?;
        } else if child_tag == bounding_tag || child_tag == "Bounding" {
            if let Some(bounding) = self.parse_bounding(start, empty, child_tag)? {
                object.bounding = Some(bounding);
            }
        } else if child_tag == bitmap_tag || child_tag == "Bitmap" {
            self.parse_bitmap(object, start, empty, child_tag)?;
        } else if child_tag == effects_tag || child_tag == "Effects" {
            self.parse_effects(object, start, empty, child_tag)?;
        } else if child_tag == crop_tag || child_tag == "Crop" {
            self.parse_crop(object, start, empty, child_tag)?;
        } else if child_tag == mask_tag || child_tag == "Mask" {
            self.parse_mask(object, start, empty, child_tag)?;
        } else if child_tag == template_tag || child_tag == "Template" {
            self.parse_template(object, start, empty, child_tag)?;
        } else if child_tag == transform_tag || child_tag == "Transform" {
            self.parse_transform(object, start, empty, child_tag)?;
        } else if child_tag == stroke_style_tag || child_tag == "StrokeStyle" {
            self.parse_stroke_style(object, start, empty, child_tag)?;
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
        if let Some(fill) = found {
            return Ok(fill);
        }
        if extras.len() == 1
            && let Some(fill) = fill_from_node(&extras[0])
        {
            return Ok(fill);
        }
        if extras.is_empty() {
            return Ok(Fill::transparent());
        }
        Ok(Fill {
            kind: FillKind::Unsupported {
                detail: "non-solid fill or stroke".to_string(),
                node: UnknownNode {
                    tag: tag.to_string(),
                    attributes: BTreeMap::new(),
                    children: extras,
                    text: None,
                },
            },
        })
    }

    fn parse_bitmap(
        &mut self,
        object: &mut GtObject,
        start: &BytesStart<'_>,
        empty: bool,
        tag: &str,
    ) -> Result<()> {
        if tag == "Bitmap" {
            take_bitmap_source(object, &attributes(start)?);
            if !empty {
                self.skip_to_end(tag)?;
            }
            return Ok(());
        }
        if empty {
            return Ok(());
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Bitmap" {
                        take_bitmap_source(object, &attributes(&child)?);
                    }
                    self.skip_to_end(&child_tag)?;
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Bitmap" {
                        take_bitmap_source(object, &attributes(&child)?);
                    }
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("Bitmap")),
                _ => {}
            }
        }
        Ok(())
    }

    fn parse_effects(
        &mut self,
        object: &mut GtObject,
        _start: &BytesStart<'_>,
        empty: bool,
        tag: &str,
    ) -> Result<()> {
        if empty {
            return Ok(());
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    let node = self.parse_unknown(&child, false, &child_tag)?;
                    apply_effect_node(object, &node);
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    let node = self.parse_unknown(&child, true, &child_tag)?;
                    apply_effect_node(object, &node);
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("Effects")),
                _ => {}
            }
        }
        Ok(())
    }

    fn parse_crop(
        &mut self,
        object: &mut GtObject,
        start: &BytesStart<'_>,
        empty: bool,
        tag: &str,
    ) -> Result<()> {
        apply_crop_attrs(object, &attributes(start)?);
        if empty {
            return Ok(());
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Crop" {
                        apply_crop_attrs(object, &attributes(&child)?);
                    }
                    self.skip_to_end(&child_tag)?;
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Crop" {
                        apply_crop_attrs(object, &attributes(&child)?);
                    }
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("Crop")),
                _ => {}
            }
        }
        Ok(())
    }

    fn parse_mask(
        &mut self,
        object: &mut GtObject,
        start: &BytesStart<'_>,
        empty: bool,
        tag: &str,
    ) -> Result<()> {
        take_mask(object, &attributes(start)?);
        if empty {
            return Ok(());
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Mask" {
                        take_mask(object, &attributes(&child)?);
                    }
                    self.skip_to_end(&child_tag)?;
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Mask" {
                        take_mask(object, &attributes(&child)?);
                    }
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("Mask")),
                _ => {}
            }
        }
        Ok(())
    }

    fn parse_template(
        &mut self,
        object: &mut GtObject,
        _start: &BytesStart<'_>,
        empty: bool,
        tag: &str,
    ) -> Result<()> {
        if empty {
            return Ok(());
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    if is_object_tag(&child_tag) {
                        let nested = self.parse_object(&child, false)?;
                        object.ticker_template = Some(object_to_template(&nested));
                        if object.text.is_none() {
                            object.text = nested.text.clone();
                        }
                        if object.style.font_family.is_none() {
                            object.style.font_family = nested.style.font_family.clone();
                        }
                        if object.style.font_size.is_none() {
                            object.style.font_size = nested.style.font_size;
                        }
                        if matches!(object.fill.kind, FillKind::Transparent) {
                            object.fill = nested.fill.clone();
                        }
                    } else {
                        object.ticker_template =
                            Some(self.parse_unknown(&child, false, &child_tag)?);
                    }
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    if is_object_tag(&child_tag) {
                        let nested = self.parse_object(&child, true)?;
                        object.ticker_template = Some(object_to_template(&nested));
                        if object.text.is_none() {
                            object.text = nested.text.clone();
                        }
                    }
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("Template")),
                _ => {}
            }
        }
        Ok(())
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
            data_name: attrs.get("DataName").cloned(),
            animations: Vec::new(),
            extra_attrs: leftover_attrs(&attrs, &["Type", "Reverse", "Reversed", "DataName"]),
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
            reversed: parse_bool(attrs.get("Reverse").or(attrs.get("Reversed"))),
            center_axis: attrs.get("CenterAxis").cloned(),
            speed: attrs.get("Speed").cloned(),
            muted: false,
            extra_attrs: leftover_attrs(
                &attrs,
                &[
                    "Object",
                    "Duration",
                    "Delay",
                    "Interpolation",
                    "Direction",
                    "Reverse",
                    "Reversed",
                    "CenterAxis",
                    "Speed",
                ],
            ),
        })
    }

    fn parse_transform(
        &mut self,
        object: &mut GtObject,
        start: &BytesStart<'_>,
        empty: bool,
        tag: &str,
    ) -> Result<()> {
        apply_transform(object, &attributes(start)?);
        if empty {
            return Ok(());
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Transform" {
                        apply_transform(object, &attributes(&child)?);
                    }
                    self.skip_to_end(&child_tag)?;
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "Transform" {
                        apply_transform(object, &attributes(&child)?);
                    }
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("Transform")),
                _ => {}
            }
        }
        Ok(())
    }

    fn parse_stroke_style(
        &mut self,
        object: &mut GtObject,
        start: &BytesStart<'_>,
        empty: bool,
        tag: &str,
    ) -> Result<()> {
        take_dash(object, &attributes(start)?);
        if empty {
            return Ok(());
        }
        loop {
            match self.read()? {
                Event::Start(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "StrokeStyle" {
                        take_dash(object, &attributes(&child)?);
                    }
                    self.skip_to_end(&child_tag)?;
                }
                Event::Empty(child) => {
                    let child_tag = local_name(&child);
                    if child_tag == "StrokeStyle" {
                        take_dash(object, &attributes(&child)?);
                    }
                }
                Event::End(end) if end_name(&end) == tag => break,
                Event::Eof => return Err(Error::UnexpectedEof("StrokeStyle")),
                _ => {}
            }
        }
        Ok(())
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
    if let Some(fill) = fill_from_node(&node) {
        *found = Some(fill);
        return;
    }
    extras.push(node);
}

fn fill_from_node(node: &UnknownNode) -> Option<Fill> {
    match node.tag.as_str() {
        "Brush" => Some(fill_from_brush(node)),
        "LinearGradientBrush" | "LinearGradient" => {
            let angle = parse_f64(node.attributes.get("Angle")).unwrap_or(0.0);
            let (start, end) = angle_to_points(angle);
            Some(Fill {
                kind: FillKind::LinearGradient {
                    start,
                    end,
                    wrap: node
                        .attributes
                        .get("Wrap")
                        .cloned()
                        .or_else(|| node.attributes.get("WrapX").cloned()),
                    stops: gradient_stops(node),
                },
            })
        }
        "RadialGradientBrush" | "RadialGradient" => Some(Fill {
            kind: FillKind::RadialGradient {
                wrap: node
                    .attributes
                    .get("Wrap")
                    .cloned()
                    .or_else(|| node.attributes.get("WrapX").cloned()),
                stops: gradient_stops(node),
            },
        }),
        "Picture" | "PictureFill" | "ImageBrush" => {
            let source = node
                .attributes
                .get("Source")
                .or_else(|| node.attributes.get("ImageSource"))
                .cloned()?;
            Some(Fill {
                kind: FillKind::Picture {
                    source,
                    size_mode: node
                        .attributes
                        .get("SizeMode")
                        .or_else(|| node.attributes.get("Stretch"))
                        .cloned(),
                    extra: leftover_attrs(
                        &node.attributes,
                        &["Source", "ImageSource", "SizeMode", "Stretch"],
                    ),
                },
            })
        }
        _ => None,
    }
}

fn fill_from_brush(node: &UnknownNode) -> Fill {
    let brush_type = node
        .attributes
        .get("Type")
        .map(|value| value.as_str())
        .unwrap_or("Solid");
    match brush_type {
        "LinearGradient" => {
            let (default_start, default_end) = angle_to_points(0.0);
            Fill {
                kind: FillKind::LinearGradient {
                    start: node
                        .attributes
                        .get("StartPoint")
                        .and_then(|value| Point2::parse(value))
                        .unwrap_or(default_start),
                    end: node
                        .attributes
                        .get("EndPoint")
                        .and_then(|value| Point2::parse(value))
                        .unwrap_or(default_end),
                    wrap: node
                        .attributes
                        .get("WrapX")
                        .cloned()
                        .or_else(|| node.attributes.get("Wrap").cloned()),
                    stops: gradient_stops(node),
                },
            }
        }
        "RadialGradient" => Fill {
            kind: FillKind::RadialGradient {
                wrap: node
                    .attributes
                    .get("WrapX")
                    .cloned()
                    .or_else(|| node.attributes.get("Wrap").cloned()),
                stops: gradient_stops(node),
            },
        },
        "Bitmap" => {
            let source = bitmap_source_from_node(node).unwrap_or_default();
            Fill {
                kind: FillKind::Picture {
                    source,
                    size_mode: node.attributes.get("SizeMode").cloned(),
                    extra: BTreeMap::new(),
                },
            }
        }
        _ => node
            .attributes
            .get("Color")
            .and_then(|value| Color::parse(value))
            .map(Fill::solid)
            .unwrap_or_else(Fill::transparent),
    }
}

fn bitmap_source_from_node(node: &UnknownNode) -> Option<String> {
    if let Some(source) = node.attributes.get("Source") {
        return Some(source.clone());
    }
    for child in &node.children {
        if (child.tag == "Brush.Bitmap" || child.tag == "Bitmap")
            && let Some(source) = bitmap_source_from_node(child)
        {
            return Some(source);
        }
    }
    None
}

fn gradient_stops(node: &UnknownNode) -> Vec<GradientStop> {
    let mut stops = Vec::new();
    collect_stops(node, &mut stops);
    if stops.is_empty() {
        stops.push(GradientStop {
            color: Color::transparent(),
            offset: 0.0,
        });
        stops.push(GradientStop {
            color: Color::transparent(),
            offset: 1.0,
        });
    }
    stops
}

fn collect_stops(node: &UnknownNode, stops: &mut Vec<GradientStop>) {
    if (node.tag == "GradientStop" || node.tag == "Stop")
        && let Some(color) = node
            .attributes
            .get("Color")
            .and_then(|value| Color::parse(value))
    {
        let offset = node
            .attributes
            .get("Offset")
            .or_else(|| node.attributes.get("Position"))
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0.0);
        stops.push(GradientStop { color, offset });
    }
    for child in &node.children {
        collect_stops(child, stops);
    }
}

fn take_bitmap_source(object: &mut GtObject, attrs: &BTreeMap<String, String>) {
    if let Some(source) = attrs.get("Source").cloned() {
        object.image_source = Some(source);
    }
    if let Some(position) = parse_f64(attrs.get("Position")) {
        object.bitmap_position = Some(position);
    }
}

fn apply_transform(object: &mut GtObject, attrs: &BTreeMap<String, String>) {
    if let Some(raw) = attrs.get("Rotate").or(attrs.get("Rotation")) {
        let parsed = Vec3::parse(raw);
        object.rotate_xyz = Some(parsed);
        object.rotate = Some(parsed.z.to_degrees());
    }
}

fn take_dash(object: &mut GtObject, attrs: &BTreeMap<String, String>) {
    if let Some(dash) = attrs.get("DashStyle").cloned() {
        object.stroke.dash_style = Some(dash);
    }
}

fn object_to_template(object: &GtObject) -> UnknownNode {
    let mut attributes = BTreeMap::new();
    if !object.name.is_empty() {
        attributes.insert("Name".to_string(), object.name.clone());
    }
    if let Some(text) = &object.text {
        attributes.insert("Text".to_string(), text.clone());
    }
    UnknownNode {
        tag: object.tag.clone(),
        attributes,
        children: object.unknown_children.clone(),
        text: None,
    }
}

fn apply_effect_node(object: &mut GtObject, node: &UnknownNode) {
    let effect_type = node
        .attributes
        .get("Type")
        .map(|value| value.as_str())
        .unwrap_or(node.tag.as_str());
    match effect_type {
        "Shadow" | "Glow" => {
            object.effects.shadow = Some(ShadowEffect {
                mode: node.attributes.get("Mode").cloned(),
                blur: parse_f64(
                    node.attributes
                        .get("BlurAmount")
                        .or(node.attributes.get("Blur")),
                ),
                color: node
                    .attributes
                    .get("Color")
                    .and_then(|value| Color::parse(value)),
            });
        }
        "Skew" => {
            object.effects.skew = Some(
                node.attributes
                    .get("Angle")
                    .map(|value| Vec3::parse(value))
                    .unwrap_or_else(Vec3::zero),
            );
        }
        "Reflection" => object.effects.reflection = true,
        "TextureFlip" | "Flip" => {
            let value = node
                .attributes
                .get("Value")
                .or_else(|| node.attributes.get("Axis"))
                .cloned()
                .unwrap_or_default();
            let lower = value.to_ascii_lowercase();
            object.effects.flip_x = object.effects.flip_x
                || lower.contains("horizontal")
                || lower == "x"
                || lower == "h"
                || parse_bool(node.attributes.get("FlipX"));
            object.effects.flip_y = object.effects.flip_y
                || lower.contains("vertical")
                || lower == "y"
                || lower == "v"
                || parse_bool(node.attributes.get("FlipY"));
        }
        _ if node.tag == "Effect" => {
            object.unknown_children.push(node.clone());
        }
        _ => object.unknown_children.push(node.clone()),
    }
}

fn apply_crop_attrs(object: &mut GtObject, attrs: &BTreeMap<String, String>) {
    if attrs.is_empty() {
        return;
    }
    object.effects.crop = Some(CropEffect {
        range: attrs.get("Range").cloned(),
        feather: attrs.get("Feather").cloned(),
    });
}

fn take_mask(object: &mut GtObject, attrs: &BTreeMap<String, String>) {
    if let Some(name) = attrs.get("Object").cloned() {
        object.effects.mask = Some(name);
    }
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
    object.style.word_wrapping = attrs
        .get("WordWrapping")
        .or(attrs.get("TextWordWrapping"))
        .cloned();
    object.style.ignore_overhang = attrs.get("IgnoreOverhang").cloned();
    object.style.line_spacing = parse_f64(attrs.get("LineSpacing"));
    object.style.auto_size = attrs.get("AutoSize").cloned();
    object.style.font_stretch = attrs.get("FontStretch").cloned();
    object.style.italic = parse_bool(attrs.get("Italic"));
    object.style.underline = parse_bool(attrs.get("Underline"));
    object.style.strikethrough = parse_bool(attrs.get("Strikethrough"));
    object.style.text_effect = attrs.get("TextEffect").cloned();
    object.style.auto_upper_case = parse_bool(attrs.get("AutoUpperCase"))
        || object
            .style
            .text_effect
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("Uppercase"));
    object.style.rtl = parse_bool(attrs.get("RTL"));
    object.anchor = attrs.get("Anchor").cloned();
    object.data_flags = attrs.get("DataFlags").cloned();
    object.locked = parse_bool(attrs.get("Locked"));
    object.rect_style = attrs.get("Style").cloned();
    if let Some(raw) = attrs.get("Rotate").or(attrs.get("Rotation")) {
        let parsed = Vec3::parse(raw);
        if raw.split(',').count() >= 3 {
            object.rotate_xyz = Some(parsed);
            object.rotate = Some(parsed.z.to_degrees());
        } else {
            object.rotate = Some(parsed.x);
        }
    }
    object.radius = parse_f64(attrs.get("Radius"));
    object.opacity = parse_f64(attrs.get("Opacity"));
    object.visible = attrs
        .get("Visible")
        .map(|value| parse_bool(Some(value)))
        .unwrap_or(true);
    object.size_mode = attrs.get("SizeMode").cloned();
    object.geometry = attrs.get("Geometry").cloned();
    object.ticker_speed = parse_f64(attrs.get("Speed"));
    object.ticker_direction = attrs.get("Direction").cloned();
    if object.kind == crate::model::ObjectKind::Ticker {
        object.ticker_kind = attrs.get("Type").cloned();
    }
    object.effects.compositing = attrs
        .get("CompositingMode")
        .or(attrs.get("Compositing"))
        .cloned();
    object.effects.flip_x = parse_bool(attrs.get("FlipX"));
    object.effects.flip_y = parse_bool(attrs.get("FlipY"));
    if let Some(source) = attrs.get("Source") {
        object.image_source = Some(source.clone());
    }
    if let Some(thickness) = parse_f64(attrs.get("StrokeThickness").or(attrs.get("Thickness"))) {
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
    !tag.contains('.')
        && !matches!(
            tag,
            "Composition" | "Layer" | "Storyboard" | "Storyboard.Animations" | "Animations"
        )
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
