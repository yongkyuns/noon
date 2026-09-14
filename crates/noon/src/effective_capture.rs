//! Coherent effective-state capture shared by Scene-owned live operations.
use crate::{AuthoringError, ExecutionSession, Mobject, UnsupportedAuthoringOperation};
use noon_core::{Color, SemanticObjectState, SemanticPaint, SemanticStore, SemanticStyle, Style};
use std::{cell::RefCell, rc::Rc};

/// Capture one object's coherent effective state when it is reachable in
/// the supplied execution, otherwise preserve its authored detached state.
/// Unsupported derived presentation remains fail-closed.
pub(crate) fn capture_mobject_state(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    source: &Mobject,
) -> Result<SemanticObjectState, AuthoringError> {
    let mut state = source.state()?;
    if !state.signal_bindings().is_empty() {
        return Err(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::CaptureReactiveBinding,
        ));
    }
    if execution.semantic_object_is_reachable(source.node_id()) {
        let store = store.borrow();
        let observed = execution.effective_semantic_object(&store, source.node_id())?;
        if !observed.authored_content_layout_applicable() {
            return Err(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::CaptureRenderOverride,
            ));
        }
        preserve_or_capture_f32(
            &mut state.transform.translation.x,
            observed.object.transform.translation.x,
        );
        preserve_or_capture_f32(
            &mut state.transform.translation.y,
            observed.object.transform.translation.y,
        );
        preserve_or_capture_f32(
            &mut state.transform.scale.x,
            observed.object.transform.scale.x,
        );
        preserve_or_capture_f32(
            &mut state.transform.scale.y,
            observed.object.transform.scale.y,
        );
        preserve_or_capture_f32(
            &mut state.transform.rotation_z,
            observed.object.transform.rotation,
        );
        state.set_z_index(observed.object.z_index);
        // Runtime lifecycle appearance is deliberately independent of semantic style opacity,
        // while a detached authored snapshot has no separate appearance channel. The renderer
        // composes them multiplicatively, so fold that same normalized multiplier into the
        // captured object opacity without copying lifecycle ownership or membership semantics.
        let mut effective_style = observed.object.style;
        effective_style.opacity *= observed.object.appearance.clamp(0.0, 1.0);
        state.style = target_style_from_effective(&state.style, effective_style)?;
    }
    Ok(state)
}

/// Reconstruct the supported authored target style directly from one
/// effective runtime row while preserving authored precision when lowering
/// did not change the value.
pub(crate) fn target_style_from_effective(
    authored: &SemanticStyle,
    effective: Style,
) -> Result<SemanticStyle, AuthoringError> {
    if matches!(authored.fill.as_ref(), Some(SemanticPaint::Resource(_)))
        || matches!(authored.stroke.as_ref(), Some(SemanticPaint::Resource(_)))
    {
        return Err(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::CaptureResourcePaint,
        ));
    }
    let (fill, fill_opacity) =
        if lowered_solid_color(authored.fill.as_ref(), authored.fill_opacity) == effective.fill {
            (authored.fill.clone(), authored.fill_opacity)
        } else {
            (effective.fill.map(SemanticPaint::Solid), 1.0)
        };
    let (stroke, stroke_opacity) =
        if lowered_solid_color(authored.stroke.as_ref(), authored.stroke_opacity)
            == effective.stroke
        {
            (authored.stroke.clone(), authored.stroke_opacity)
        } else {
            (effective.stroke.map(SemanticPaint::Solid), 1.0)
        };
    Ok(SemanticStyle {
        fill,
        fill_opacity,
        stroke,
        stroke_opacity,
        stroke_width: if authored.stroke_width as f32 == effective.stroke_width {
            authored.stroke_width
        } else {
            f64::from(effective.stroke_width)
        },
        stroke_width_mode: effective.stroke_width_mode,
        stroke_join: effective.stroke_join,
        stroke_cap: effective.stroke_cap,
        object_opacity: if authored.object_opacity as f32 == effective.opacity {
            authored.object_opacity
        } else {
            f64::from(effective.opacity)
        },
    })
}

fn lowered_solid_color(paint: Option<&SemanticPaint>, opacity: f64) -> Option<Color> {
    let SemanticPaint::Solid(color) = paint? else {
        return None;
    };
    Some(Color {
        alpha: (f64::from(color.alpha) * f64::from(opacity as f32)) as f32,
        ..*color
    })
}

fn preserve_or_capture_f32(authored: &mut f64, effective: f32) {
    if *authored as f32 != effective {
        *authored = f64::from(effective);
    }
}
