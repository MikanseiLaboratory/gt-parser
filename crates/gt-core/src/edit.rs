use crate::error::{Error, Result};
use crate::model::{Animation, GtDocument, MAX_ANIMATIONS_PER_OBJECT, Storyboard};

pub fn add_storyboard(
    document: &mut GtDocument,
    storyboard_type: Option<String>,
    data_name: Option<String>,
) -> Result<usize> {
    let ty = storyboard_type.unwrap_or_else(|| "TransitionIn".to_string());
    let data = data_name.unwrap_or_default();
    if document.storyboards.iter().any(|storyboard| {
        storyboard.effective_type().eq_ignore_ascii_case(&ty)
            && storyboard.data_name.clone().unwrap_or_default() == data
    }) {
        return Err(Error::Invalid(format!(
            "storyboard {ty} already exists for '{data}'"
        )));
    }
    document.storyboards.push(Storyboard {
        storyboard_type: if ty.eq_ignore_ascii_case("TransitionIn") {
            None
        } else {
            Some(ty)
        },
        reversed: false,
        data_name: if data.is_empty() { None } else { Some(data) },
        animations: Vec::new(),
        extra_attrs: Default::default(),
        unknown_children: Vec::new(),
    });
    Ok(document.storyboards.len() - 1)
}

pub fn add_animation(
    document: &mut GtDocument,
    storyboard_index: usize,
    object: &str,
    kind: Option<&str>,
) -> Result<usize> {
    let storyboard = document
        .storyboards
        .get_mut(storyboard_index)
        .ok_or_else(|| Error::Invalid("storyboard index out of range".into()))?;
    if storyboard.counted_animations_for(object) >= MAX_ANIMATIONS_PER_OBJECT {
        return Err(Error::Invalid(format!(
            "object '{object}' already has {MAX_ANIMATIONS_PER_OBJECT} animations"
        )));
    }
    let continuous = storyboard.is_continuous();
    let kind = kind.unwrap_or(if continuous {
        "RotateContinuous"
    } else {
        "Fade"
    });
    storyboard.animations.push(Animation {
        kind: kind.to_string(),
        object: Some(object.to_string()),
        duration: if matches!(
            kind,
            "RotateContinuous" | "FillOffset" | "StrokeOffset" | "Blink" | "ImageSequenceLoop"
        ) {
            None
        } else {
            Some("1".to_string())
        },
        delay: None,
        interpolation: if continuous {
            None
        } else {
            Some("CubicEasingInOut".to_string())
        },
        direction: None,
        reversed: false,
        center_axis: None,
        speed: if continuous {
            Some("1".to_string())
        } else {
            None
        },
        muted: false,
        extra_attrs: Default::default(),
    });
    Ok(storyboard.animations.len() - 1)
}

pub fn set_animation(
    document: &mut GtDocument,
    storyboard_index: usize,
    animation_index: usize,
    patch: AnimationPatch,
) -> Result<()> {
    let storyboard = document
        .storyboards
        .get_mut(storyboard_index)
        .ok_or_else(|| Error::Invalid("storyboard index out of range".into()))?;
    if animation_index >= storyboard.animations.len() {
        return Err(Error::Invalid("animation index out of range".into()));
    }
    if let Some(object) = patch.object.clone() {
        let current = storyboard.animations[animation_index]
            .object
            .clone()
            .unwrap_or_default();
        if object != current
            && storyboard.counted_animations_for(&object) >= MAX_ANIMATIONS_PER_OBJECT
        {
            return Err(Error::Invalid(format!(
                "object '{object}' already has {MAX_ANIMATIONS_PER_OBJECT} animations"
            )));
        }
    }
    let animation = &mut storyboard.animations[animation_index];
    if let Some(kind) = patch.kind {
        animation.kind = kind;
    }
    if let Some(object) = patch.object {
        animation.object = Some(object);
    }
    if let Some(delay) = patch.delay {
        animation.delay = Some(delay);
    }
    if let Some(duration) = patch.duration {
        animation.duration = Some(duration);
    }
    if let Some(interpolation) = patch.interpolation {
        animation.interpolation = Some(interpolation);
    }
    if let Some(direction) = patch.direction {
        animation.direction = Some(direction);
    }
    if let Some(reversed) = patch.reversed {
        animation.reversed = reversed;
    }
    if let Some(center_axis) = patch.center_axis {
        animation.center_axis = Some(center_axis);
    }
    if let Some(speed) = patch.speed {
        animation.speed = Some(speed);
    }
    if let Some(muted) = patch.muted {
        animation.muted = muted;
    }
    Ok(())
}

pub fn delete_animation(
    document: &mut GtDocument,
    storyboard_index: usize,
    animation_index: usize,
) -> Result<Animation> {
    let storyboard = document
        .storyboards
        .get_mut(storyboard_index)
        .ok_or_else(|| Error::Invalid("storyboard index out of range".into()))?;
    if animation_index >= storyboard.animations.len() {
        return Err(Error::Invalid("animation index out of range".into()));
    }
    Ok(storyboard.animations.remove(animation_index))
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AnimationPatch {
    pub kind: Option<String>,
    pub object: Option<String>,
    pub delay: Option<String>,
    pub duration: Option<String>,
    pub interpolation: Option<String>,
    pub direction: Option<String>,
    pub reversed: Option<bool>,
    pub center_axis: Option<String>,
    pub speed: Option<String>,
    pub muted: Option<bool>,
}
