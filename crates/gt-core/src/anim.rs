use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::{CropEffect, GtDocument, Layer, LayerChild, Storyboard};
use crate::resolve::effective_storyboard_type;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineSegment {
    pub storyboard_index: usize,
    pub offset: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimOverride {
    pub opacity_mul: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub scale_anchor_x: f64,
    pub scale_anchor_y: f64,
    pub rotate_x: f64,
    pub rotate_y: f64,
    pub rotate_z: f64,
    pub hidden: bool,
    pub has_crop: bool,
    pub crop_x0: f64,
    pub crop_y0: f64,
    pub crop_x1: f64,
    pub crop_y1: f64,
    pub feather_scale: f64,
    pub sequence_position: Option<f64>,
}

impl Default for AnimOverride {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimOverride {
    fn new() -> Self {
        Self {
            opacity_mul: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_anchor_x: 0.5,
            scale_anchor_y: 0.5,
            rotate_x: 0.0,
            rotate_y: 0.0,
            rotate_z: 0.0,
            hidden: false,
            has_crop: false,
            crop_x0: 0.0,
            crop_y0: 0.0,
            crop_x1: 1.0,
            crop_y1: 1.0,
            feather_scale: 1.0,
            sequence_position: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AnimationFrame {
    pub time: f64,
    pub objects: BTreeMap<String, AnimOverride>,
    pub layers: BTreeMap<String, AnimOverride>,
}

pub fn evaluate_storyboard(document: &GtDocument, index: usize, time: f64) -> AnimationFrame {
    let mut frame = AnimationFrame {
        time,
        ..AnimationFrame::default()
    };
    if let Some(storyboard) = document.storyboards.get(index) {
        accumulate(
            &mut frame,
            document,
            storyboard,
            time,
            storyboard.plays_rewound(),
        );
    }
    frame
}

pub fn evaluate_segments(
    document: &GtDocument,
    segments: &[TimelineSegment],
    time: f64,
) -> AnimationFrame {
    let mut frame = AnimationFrame {
        time,
        ..AnimationFrame::default()
    };
    for segment in segments {
        let Some(storyboard) = document.storyboards.get(segment.storyboard_index) else {
            continue;
        };
        let local = time - segment.offset;
        if local < storyboard.earliest_start() {
            continue;
        }
        accumulate(
            &mut frame,
            document,
            storyboard,
            local,
            storyboard.plays_rewound(),
        );
    }
    frame
}

pub fn combined_in_out_segments(document: &GtDocument, hold: f64) -> Option<Vec<TimelineSegment>> {
    let inn = document.storyboards.iter().position(|storyboard| {
        effective_storyboard_type(storyboard.storyboard_type.as_deref()) == "TransitionIn"
    })?;
    let out = document.storyboards.iter().position(|storyboard| {
        effective_storyboard_type(storyboard.storyboard_type.as_deref()) == "TransitionOut"
    })?;
    let in_end = document.storyboards[inn].duration();
    Some(vec![
        TimelineSegment {
            storyboard_index: inn,
            offset: 0.0,
        },
        TimelineSegment {
            storyboard_index: out,
            offset: in_end + hold,
        },
    ])
}

fn accumulate(
    frame: &mut AnimationFrame,
    document: &GtDocument,
    storyboard: &Storyboard,
    time: f64,
    rewound: bool,
) {
    for animation in &storyboard.animations {
        if animation.is_placeholder() || animation.muted {
            continue;
        }
        let Some(name) = animation.object.as_deref() else {
            continue;
        };
        let reversed = animation.reversed != rewound;
        let elapsed = time - animation.delay_secs();
        let duration = animation.duration_secs().max(1e-6);
        let raw = if animation.is_continuous_type() {
            0.0
        } else {
            (elapsed / duration).clamp(0.0, 1.0)
        };
        let eased = ease(animation.interpolation.as_deref(), raw);
        let rest = if reversed { 1.0 - eased } else { eased };
        for (is_layer, target_name, box_rect, crop) in resolve_targets(document, name) {
            apply(
                frame,
                document,
                animation.kind.as_str(),
                animation.direction.as_deref(),
                animation.center_axis.as_deref(),
                animation.speed_value(),
                is_layer,
                &target_name,
                box_rect,
                crop.as_ref(),
                raw,
                rest,
                reversed,
                elapsed,
                time,
                animation.delay_secs(),
            );
        }
    }
}

type TargetBox = (f64, f64, f64, f64);

fn resolve_targets(
    document: &GtDocument,
    name: &str,
) -> Vec<(bool, String, TargetBox, Option<CropEffect>)> {
    let mut out = Vec::new();
    for layer in &document.layers {
        if layer.name.eq_ignore_ascii_case(name) {
            let w = if layer.dimensions.x > 0.0 {
                layer.dimensions.x
            } else {
                layer.inner_width.unwrap_or(document.width)
            };
            let h = if layer.dimensions.y > 0.0 {
                layer.dimensions.y
            } else {
                layer.inner_height.unwrap_or(document.height)
            };
            out.push((
                true,
                layer.name.clone(),
                (layer.location.x, layer.location.y, w, h),
                None,
            ));
        }
        collect_named_objects(layer, name, &mut out);
    }
    out
}

fn collect_named_objects(
    layer: &Layer,
    name: &str,
    out: &mut Vec<(bool, String, TargetBox, Option<CropEffect>)>,
) {
    for child in &layer.objects {
        match child {
            LayerChild::Object(object) if object.name.eq_ignore_ascii_case(name) => {
                out.push((
                    false,
                    object.name.clone(),
                    (
                        layer.location.x + object.location.x,
                        layer.location.y + object.location.y,
                        object.dimensions.x,
                        object.dimensions.y,
                    ),
                    object.effects.crop.clone(),
                ));
            }
            LayerChild::Layer(nested) => collect_named_objects(nested, name, out),
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply(
    frame: &mut AnimationFrame,
    document: &GtDocument,
    kind: &str,
    direction: Option<&str>,
    center_axis: Option<&str>,
    speed: f64,
    is_layer: bool,
    name: &str,
    box_rect: TargetBox,
    crop: Option<&CropEffect>,
    raw: f64,
    rest: f64,
    reversed: bool,
    elapsed: f64,
    time: f64,
    delay: f64,
) {
    let slot = if is_layer {
        frame.layers.entry(name.to_string()).or_default()
    } else {
        frame.objects.entry(name.to_string()).or_default()
    };
    let (left, top, width, height) = box_rect;
    let dir = direction.unwrap_or(if kind == "Scroll" { "Bottom" } else { "Left" });
    let doc_w = document.width;
    let doc_h = document.height;
    match kind {
        "Hidden" => slot.hidden = true,
        "Fade" => slot.opacity_mul *= rest,
        "Zoom" => apply_scale(slot, rest, rest, 0.5, 0.5),
        "ZoomFade" => {
            let scale = 2.0 - rest;
            apply_scale(slot, scale, scale, 0.5, 0.5);
            slot.opacity_mul *= rest;
        }
        "Fly" | "Move" => {
            let (dx, dy) = fly_from(dir, left, top, width, height, doc_w, doc_h);
            slot.offset_x += dx * (1.0 - rest);
            slot.offset_y += dy * (1.0 - rest);
        }
        "Bounce" => {
            let (dx, _) = fly_from(dir, left, top, width, height, doc_w, doc_h);
            slot.offset_x += dx * (1.0 - rest);
            let bounce = if reversed {
                bounce_in(raw)
            } else {
                bounce_out(raw)
            };
            let bounce_rest = if reversed { 1.0 - bounce } else { bounce };
            slot.offset_y += -height * (1.0 / 3.0) * (1.0 - bounce_rest);
        }
        "Expand" => {
            let (collapse_x, collapse_y, ax, ay) = expand_anchor(dir);
            apply_scale(
                slot,
                if collapse_x { rest } else { 1.0 },
                if collapse_y { rest } else { 1.0 },
                ax,
                ay,
            );
        }
        "Reveal" | "Wipe" => {
            let to = resting_crop(crop);
            let from = reveal_from(dir, center_axis);
            slot.has_crop = true;
            slot.crop_x0 = lerp(from.0, to.0, rest);
            slot.crop_y0 = lerp(from.1, to.1, rest);
            slot.crop_x1 = lerp(from.2, to.2, rest);
            slot.crop_y1 = lerp(from.3, to.3, rest);
            slot.feather_scale = if dir.eq_ignore_ascii_case("Center") {
                rest
            } else {
                1.0
            };
        }
        "Rotate" | "Spin" => {
            let turn = std::f64::consts::TAU * (1.0 - rest);
            apply_rotate(slot, dir, turn);
        }
        "Scroll" => {
            let (fx, fy, tx, ty) = scroll_path(dir, left, top, width, height, doc_w, doc_h);
            slot.offset_x += lerp(fx, tx, rest) - left;
            slot.offset_y += lerp(fy, ty, rest) - top;
        }
        "RotateContinuous" => {
            let turn = std::f64::consts::TAU * speed * elapsed.max(0.0);
            apply_rotate(slot, dir, turn);
        }
        "ImageSequence" => slot.sequence_position = Some(rest),
        "ImageSequenceLoop" => {
            let loop_len = 1.0_f64.max(1e-6);
            let phase = (time - delay) / loop_len;
            slot.sequence_position = Some(if time < delay {
                0.0
            } else {
                phase - phase.floor()
            });
        }
        "Scale" => apply_scale(slot, rest, rest, 0.5, 0.5),
        _ => {}
    }
}

fn apply_scale(slot: &mut AnimOverride, sx: f64, sy: f64, ax: f64, ay: f64) {
    slot.scale_x *= sx;
    slot.scale_y *= sy;
    slot.scale_anchor_x = ax;
    slot.scale_anchor_y = ay;
}

fn apply_rotate(slot: &mut AnimOverride, dir: &str, turn: f64) {
    match dir {
        "Top" => slot.rotate_y += turn,
        "Bottom" => slot.rotate_y -= turn,
        "Left" => slot.rotate_x += turn,
        "Right" => slot.rotate_x -= turn,
        "TopLeft" => {
            slot.rotate_y += turn;
            slot.rotate_x += turn;
        }
        "TopRight" => {
            slot.rotate_y += turn;
            slot.rotate_x -= turn;
        }
        "BottomLeft" => {
            slot.rotate_y -= turn;
            slot.rotate_x += turn;
        }
        "BottomRight" => {
            slot.rotate_y -= turn;
            slot.rotate_x -= turn;
        }
        "Center" => slot.rotate_z += turn,
        _ => {}
    }
}

fn fly_from(dir: &str, x: f64, y: f64, w: f64, h: f64, doc_w: f64, doc_h: f64) -> (f64, f64) {
    let (sx, sy) = match dir {
        "Top" => (x, -h),
        "Bottom" => (x, doc_h),
        "Left" => (-w, y),
        "Right" => (doc_w, y),
        "TopLeft" => (-w, -h),
        "TopRight" => (doc_w, -h),
        "BottomLeft" => (-w, doc_h),
        "BottomRight" => (doc_w, doc_h),
        _ => (x, y),
    };
    (sx - x, sy - y)
}

fn expand_anchor(dir: &str) -> (bool, bool, f64, f64) {
    match dir {
        "Top" => (false, true, 0.5, 0.0),
        "Bottom" => (false, true, 0.5, 1.0),
        "Left" => (true, false, 0.0, 0.5),
        "Right" => (true, false, 1.0, 0.5),
        "TopLeft" => (true, true, 0.0, 0.0),
        "TopRight" => (true, true, 1.0, 0.0),
        "BottomLeft" => (true, true, 0.0, 1.0),
        "BottomRight" => (true, true, 1.0, 1.0),
        _ => (true, true, 0.5, 0.5),
    }
}

fn reveal_from(dir: &str, axis: Option<&str>) -> (f64, f64, f64, f64) {
    match dir {
        "Top" => (0.0, 0.0, 1.0, 0.0),
        "Bottom" => (0.0, 1.0, 1.0, 1.0),
        "Right" => (1.0, 0.0, 1.0, 1.0),
        "TopLeft" => (0.0, 0.0, 0.0, 0.0),
        "TopRight" => (1.0, 0.0, 1.0, 0.0),
        "BottomLeft" => (0.0, 1.0, 0.0, 1.0),
        "BottomRight" => (1.0, 1.0, 1.0, 1.0),
        "Center" => match axis.unwrap_or("Both") {
            "X" => (0.5, 0.0, 0.5, 1.0),
            "Y" => (0.0, 0.5, 1.0, 0.5),
            _ => (0.5, 0.5, 0.5, 0.5),
        },
        _ => (0.0, 0.0, 0.0, 1.0),
    }
}

fn resting_crop(crop: Option<&CropEffect>) -> (f64, f64, f64, f64) {
    let Some(crop) = crop.and_then(|crop| crop.range.as_deref()) else {
        return (0.0, 0.0, 1.0, 1.0);
    };
    let parts: Vec<f64> = crop
        .split(',')
        .map(|part| part.trim().parse().unwrap_or(0.0))
        .collect();
    if parts.len() >= 4 {
        (parts[0], parts[1], parts[2], parts[3])
    } else {
        (0.0, 0.0, 1.0, 1.0)
    }
}

fn scroll_path(
    dir: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    doc_w: f64,
    doc_h: f64,
) -> (f64, f64, f64, f64) {
    match dir {
        "Top" => (x, doc_h, x, -h),
        "Left" => (doc_w, y, -w, y),
        "Right" => (-w, y, doc_w, y),
        _ => (x, -h, x, doc_h),
    }
}

fn lerp(from: f64, to: f64, t: f64) -> f64 {
    from + (to - from) * t
}

fn ease(name: Option<&str>, t: f64) -> f64 {
    match name.unwrap_or("Linear") {
        "CubicEasingIn" => 1.0 - (1.0 - t).powi(3),
        "CubicEasingOut" => t.powi(3),
        "CubicEasingInOut" => {
            if t < 0.5 {
                4.0 * t.powi(3)
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        "BounceIn" => bounce_in(t),
        "BounceOut" => bounce_out(t),
        _ => t,
    }
}

fn bounce_out(t: f64) -> f64 {
    let n1 = 7.5625;
    let d1 = 2.75;
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        let t = t - 1.5 / d1;
        n1 * t * t + 0.75
    } else if t < 2.5 / d1 {
        let t = t - 2.25 / d1;
        n1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / d1;
        n1 * t * t + 0.984375
    }
}

fn bounce_in(t: f64) -> f64 {
    1.0 - bounce_out(1.0 - t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Animation, GtDocument, GtObject, Layer, LayerChild, ObjectKind, Vec3};

    fn doc_with_rect() -> GtDocument {
        let mut object = GtObject::new("Rectangle");
        object.name = "Box".to_string();
        object.location = Vec3 {
            x: 100.0,
            y: 200.0,
            z: 0.0,
        };
        object.dimensions = Vec3 {
            x: 50.0,
            y: 40.0,
            z: 0.0,
        };
        object.kind = ObjectKind::Rectangle;
        GtDocument {
            width: 1920.0,
            height: 1080.0,
            layers: vec![Layer {
                name: "Layer 1".to_string(),
                location: Vec3::zero(),
                dimensions: Vec3 {
                    x: 1920.0,
                    y: 1080.0,
                    z: 0.0,
                },
                locked: false,
                visible: true,
                inner_width: Some(1920.0),
                inner_height: Some(1080.0),
                objects: vec![LayerChild::Object(Box::new(object))],
                extra_attrs: Default::default(),
                unknown_children: Vec::new(),
            }],
            storyboards: vec![Storyboard {
                storyboard_type: None,
                reversed: false,
                data_name: None,
                animations: vec![Animation {
                    kind: "Fly".to_string(),
                    object: Some("Box".to_string()),
                    duration: Some("1".to_string()),
                    delay: None,
                    interpolation: Some("Linear".to_string()),
                    direction: Some("Top".to_string()),
                    reversed: false,
                    center_axis: None,
                    speed: None,
                    muted: false,
                    extra_attrs: Default::default(),
                }],
                extra_attrs: Default::default(),
                unknown_children: Vec::new(),
            }],
            unknown_children: Vec::new(),
            extra_attrs: Default::default(),
            warnings: Vec::new(),
            asset_names: Vec::new(),
        }
    }

    #[test]
    fn fly_top_starts_above() {
        let document = doc_with_rect();
        let frame = evaluate_storyboard(&document, 0, 0.0);
        let over = frame.objects.get("Box").unwrap();
        assert!((over.offset_y + 240.0).abs() < 0.01);
    }

    #[test]
    fn fade_midpoint_is_half() {
        let mut document = doc_with_rect();
        document.storyboards[0].animations[0].kind = "Fade".to_string();
        document.storyboards[0].animations[0].direction = None;
        let frame = evaluate_storyboard(&document, 0, 0.5);
        let over = frame.objects.get("Box").unwrap();
        assert!((over.opacity_mul - 0.5).abs() < 0.01);
    }

    #[test]
    fn reveal_left_starts_closed() {
        let mut document = doc_with_rect();
        document.storyboards[0].animations[0].kind = "Reveal".to_string();
        document.storyboards[0].animations[0].direction = Some("Left".to_string());
        let frame = evaluate_storyboard(&document, 0, 0.0);
        let over = frame.objects.get("Box").unwrap();
        assert!(over.has_crop);
        assert!((over.crop_x1 - 0.0).abs() < 0.01);
    }

    #[test]
    fn transition_out_rewinds_fade() {
        let mut document = doc_with_rect();
        document.storyboards[0].storyboard_type = Some("TransitionOut".to_string());
        document.storyboards[0].animations[0].kind = "Fade".to_string();
        document.storyboards[0].animations[0].direction = None;
        let frame = evaluate_storyboard(&document, 0, 0.0);
        let over = frame.objects.get("Box").unwrap();
        assert!((over.opacity_mul - 1.0).abs() < 0.01);
    }

    #[test]
    fn fly_right_starts_offscreen() {
        let mut document = doc_with_rect();
        document.storyboards[0].animations[0].direction = Some("Right".to_string());
        let frame = evaluate_storyboard(&document, 0, 0.0);
        let over = frame.objects.get("Box").unwrap();
        assert!((over.offset_x - (1920.0 - 100.0)).abs() < 0.01);
    }

    #[test]
    fn reveal_center_x_starts_as_line() {
        let mut document = doc_with_rect();
        document.storyboards[0].animations[0].kind = "Reveal".to_string();
        document.storyboards[0].animations[0].direction = Some("Center".to_string());
        document.storyboards[0].animations[0].center_axis = Some("X".to_string());
        let frame = evaluate_storyboard(&document, 0, 0.0);
        let over = frame.objects.get("Box").unwrap();
        assert!(over.has_crop);
        assert!((over.crop_x0 - 0.5).abs() < 0.01);
        assert!((over.crop_x1 - 0.5).abs() < 0.01);
    }

    #[test]
    fn combined_in_hold_out_does_not_apply_out_during_in() {
        let mut document = doc_with_rect();
        document.storyboards[0].animations[0].kind = "Fade".to_string();
        document.storyboards[0].animations[0].direction = None;
        document.storyboards.push(Storyboard {
            storyboard_type: Some("TransitionOut".to_string()),
            reversed: false,
            data_name: None,
            animations: vec![Animation {
                kind: "Fade".to_string(),
                object: Some("Box".to_string()),
                duration: Some("1".to_string()),
                delay: None,
                interpolation: Some("Linear".to_string()),
                direction: None,
                reversed: false,
                center_axis: None,
                speed: None,
                muted: false,
                extra_attrs: Default::default(),
            }],
            extra_attrs: Default::default(),
            unknown_children: Vec::new(),
        });
        let segments = combined_in_out_segments(&document, 2.0).unwrap();
        let during_in = evaluate_segments(&document, &segments, 0.5);
        assert!((during_in.objects.get("Box").unwrap().opacity_mul - 0.5).abs() < 0.01);
        let during_hold = evaluate_segments(&document, &segments, 2.0);
        assert!((during_hold.objects.get("Box").unwrap().opacity_mul - 1.0).abs() < 0.01);
        let during_out = evaluate_segments(&document, &segments, 3.0);
        assert!((during_out.objects.get("Box").unwrap().opacity_mul - 1.0).abs() < 0.01);
    }
}
