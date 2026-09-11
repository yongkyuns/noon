//! Manim-compatible dependent scaling for retained Arrow families.
//!
//! Arrow remains ordinary semantic Line/path leaves. This module owns only the
//! class-specific dependency rule: scaling changes the shaft and tips together,
//! preserves immutable tip resources when requested, and recaps shaft width from
//! the constructor policy retained on the semantic shaft role.

use crate::{AuthoringError, ManimArrow, Mobject};
use noon_core::{
    SemanticArrowShaftRole, SemanticMutationTransaction, SemanticNodeId, SemanticObjectRole,
    SemanticObjectState, SemanticStore, StoredGeometry,
};

/// A failure specific to retained Arrow dependency semantics.
#[derive(Clone, Debug, PartialEq)]
pub enum ArrowScaleError {
    Authoring(AuthoringError),
    InvalidFamilyTopology {
        family: SemanticNodeId,
    },
    InvalidComponentRole {
        node: SemanticNodeId,
        expected: &'static str,
    },
}

impl std::fmt::Display for ArrowScaleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(error) => error.fmt(formatter),
            Self::InvalidFamilyTopology { family } => write!(
                formatter,
                "Arrow family {}:{} does not contain exactly one shaft, one end tip, and at most one start tip",
                family.slot(),
                family.generation()
            ),
            Self::InvalidComponentRole { node, expected } => write!(
                formatter,
                "Arrow component {}:{} is not the expected {expected}",
                node.slot(),
                node.generation()
            ),
        }
    }
}

impl std::error::Error for ArrowScaleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Authoring(error) => Some(error),
            Self::InvalidFamilyTopology { .. } | Self::InvalidComponentRole { .. } => None,
        }
    }
}

impl From<AuthoringError> for ArrowScaleError {
    fn from(value: AuthoringError) -> Self {
        Self::Authoring(value)
    }
}

impl ManimArrow {
    /// Match pinned ManimCE v0.21 `Arrow.scale` for straight retained Arrows.
    ///
    /// With `scale_tips=false`, the public Arrow endpoints scale about the Arrow
    /// center while existing tip geometry keeps its size and is only moved/rotated
    /// into place. With `scale_tips=true`, all component leaves scale together.
    /// Both modes recap the shaft from the original constructor stroke width held
    /// by the authoritative semantic shaft role. All changed leaves publish in one
    /// semantic transaction and unrelated scene state is untouched.
    pub fn scale(&self, factor: f64, scale_tips: bool) -> Result<(), ArrowScaleError> {
        let factor = crate::integration::authoring_render_f64("arrow scale factor", factor)?;
        let policy = self.validate_scale_components()?;
        let endpoints = self.manim_endpoints()?;
        let start = endpoints.start;
        let end = endpoints.end;
        let old_length = distance(start, end);

        // Pinned ManimCE returns immediately for a zero-length Arrow.
        if old_length == 0.0 {
            return Ok(());
        }

        let previous_shaft = self.shaft().state()?;
        let previous_end_tip = self.end_tip().state()?;
        let previous_start_tip = self.start_tip().map(Mobject::state).transpose()?;
        let store = self.family().integration_store();

        let (next_shaft, next_end_tip, next_start_tip) = {
            let store_ref = store.borrow();
            if scale_tips {
                self.scaled_with_tips(
                    &store_ref,
                    policy,
                    factor,
                    old_length,
                    &previous_shaft,
                    &previous_end_tip,
                    previous_start_tip.as_ref(),
                )?
            } else {
                self.scaled_preserving_tips(
                    policy,
                    factor,
                    start,
                    end,
                    &previous_shaft,
                    &previous_end_tip,
                    previous_start_tip.as_ref(),
                )?
            }
        };

        let mut transaction = SemanticMutationTransaction::new();
        crate::semantic_mobject::stage_state_changes(
            &mut transaction,
            self.shaft().node_id(),
            &previous_shaft,
            &next_shaft,
        );
        crate::semantic_mobject::stage_state_changes(
            &mut transaction,
            self.end_tip().node_id(),
            &previous_end_tip,
            &next_end_tip,
        );
        if let (Some(handle), Some(previous), Some(next)) = (
            self.start_tip(),
            previous_start_tip.as_ref(),
            next_start_tip.as_ref(),
        ) {
            crate::semantic_mobject::stage_state_changes(
                &mut transaction,
                handle.node_id(),
                previous,
                next,
            );
        }
        transaction
            .apply(&mut store.borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)?;
        Ok(())
    }

    fn validate_scale_components(&self) -> Result<SemanticArrowShaftRole, ArrowScaleError> {
        self.family().validate()?;
        let shaft = self.shaft().state()?;
        let policy = match shaft.role() {
            SemanticObjectRole::ArrowShaft(policy) => policy,
            _ => {
                return Err(ArrowScaleError::InvalidComponentRole {
                    node: self.shaft().node_id(),
                    expected: "Arrow shaft",
                })
            }
        };
        if self.end_tip().state()?.role() != SemanticObjectRole::ArrowEndTip {
            return Err(ArrowScaleError::InvalidComponentRole {
                node: self.end_tip().node_id(),
                expected: "Arrow end tip",
            });
        }
        if let Some(start_tip) = self.start_tip() {
            if start_tip.state()?.role() != SemanticObjectRole::ArrowStartTip {
                return Err(ArrowScaleError::InvalidComponentRole {
                    node: start_tip.node_id(),
                    expected: "Arrow start tip",
                });
            }
        }

        let expected = if self.start_tip().is_some() { 3 } else { 2 };
        let members = self
            .family()
            .integration_store()
            .borrow()
            .semantic_family_members_checked(self.family().node_id())
            .map_err(AuthoringError::from)?;
        let topology_matches = members.len() == expected
            && members.first() == Some(&self.shaft().node_id())
            && members.get(1) == Some(&self.end_tip().node_id())
            && self
                .start_tip()
                .is_none_or(|tip| members.get(2) == Some(&tip.node_id()));
        if !topology_matches {
            return Err(ArrowScaleError::InvalidFamilyTopology {
                family: self.family().node_id(),
            });
        }
        Ok(policy)
    }

    fn scaled_with_tips(
        &self,
        store: &SemanticStore,
        policy: SemanticArrowShaftRole,
        factor: f64,
        old_length: f64,
        previous_shaft: &SemanticObjectState,
        previous_end_tip: &SemanticObjectState,
        previous_start_tip: Option<&SemanticObjectState>,
    ) -> Result<
        (
            SemanticObjectState,
            SemanticObjectState,
            Option<SemanticObjectState>,
        ),
        ArrowScaleError,
    > {
        let bounds = self
            .family()
            .layout_bounds()?
            .ok_or(AuthoringError::MissingLayoutBounds(self.family().node_id()))?;
        let pivot = (
            (bounds.min_x + bounds.max_x) * 0.5,
            (bounds.min_y + bounds.max_y) * 0.5,
        );

        let scale_one =
            |previous: &SemanticObjectState| -> Result<SemanticObjectState, ArrowScaleError> {
                let old_center = crate::semantic_mobject::state_center(store, previous)?;
                let target_center = (
                    pivot.0 + (old_center.0 - pivot.0) * factor,
                    pivot.1 + (old_center.1 - pivot.1) * factor,
                );
                let mut next = previous.clone();
                crate::semantic_mobject::scale_state_about_center(
                    store,
                    &mut next,
                    factor,
                    factor,
                    target_center,
                )?;
                Ok(next)
            };

        let mut shaft = scale_one(previous_shaft)?;
        shaft.style.stroke_width = policy
            .initial_stroke_width()
            .min(policy.max_stroke_width_to_length_ratio() * old_length * factor.abs());
        let end_tip = scale_one(previous_end_tip)?;
        let start_tip = previous_start_tip.map(scale_one).transpose()?;
        Ok((shaft, end_tip, start_tip))
    }

    fn scaled_preserving_tips(
        &self,
        policy: SemanticArrowShaftRole,
        factor: f64,
        start: (f64, f64),
        end: (f64, f64),
        previous_shaft: &SemanticObjectState,
        previous_end_tip: &SemanticObjectState,
        previous_start_tip: Option<&SemanticObjectState>,
    ) -> Result<
        (
            SemanticObjectState,
            SemanticObjectState,
            Option<SemanticObjectState>,
        ),
        ArrowScaleError,
    > {
        let pivot = midpoint(start, end);
        let new_start = scale_point_about(start, pivot, factor);
        let new_end = scale_point_about(end, pivot, factor);
        let new_length = distance(new_start, new_end);

        let shaft_endpoints = self.shaft().manim_line_endpoints()?;
        let end_tip_length = distance(end, shaft_endpoints.end);
        let start_tip_length = previous_start_tip
            .map(|_| distance(start, shaft_endpoints.start))
            .unwrap_or(0.0);

        let mut next_end_tip = previous_end_tip.clone();
        let mut next_start_tip = previous_start_tip.cloned();
        let (shaft_start, shaft_end) = if new_length == 0.0 {
            reposition_tip(
                &mut next_end_tip,
                end,
                current_tip_direction(shaft_endpoints.end, end),
                new_end,
                None,
            )?;
            if let Some(next) = next_start_tip.as_mut() {
                reposition_tip(
                    next,
                    start,
                    current_tip_direction(shaft_endpoints.start, start),
                    new_start,
                    None,
                )?;
            }
            (new_start, new_end)
        } else {
            let direction = unit_direction(new_start, new_end);
            reposition_tip(
                &mut next_end_tip,
                end,
                current_tip_direction(shaft_endpoints.end, end),
                new_end,
                Some(direction),
            )?;
            let end_base = (
                new_end.0 - direction.0 * end_tip_length,
                new_end.1 - direction.1 * end_tip_length,
            );

            let start_base = if let Some(next) = next_start_tip.as_mut() {
                // Manim re-adds the end tip first. The start-tip tangent therefore
                // uses the line from the public start to the newly shortened end.
                let tangent = unit_direction_or(new_start, end_base, direction);
                let start_direction = (-tangent.0, -tangent.1);
                reposition_tip(
                    next,
                    start,
                    current_tip_direction(shaft_endpoints.start, start),
                    new_start,
                    Some(start_direction),
                )?;
                (
                    new_start.0 + tangent.0 * start_tip_length,
                    new_start.1 + tangent.1 * start_tip_length,
                )
            } else {
                new_start
            };
            (start_base, end_base)
        };

        let mut next_shaft = previous_shaft.clone();
        set_line_world_endpoints(
            &mut next_shaft,
            shaft_start,
            shaft_end,
            "scaled arrow shaft",
        )?;
        next_shaft.style.stroke_width = policy
            .initial_stroke_width()
            .min(policy.max_stroke_width_to_length_ratio() * new_length);

        Ok((next_shaft, next_end_tip, next_start_tip))
    }
}

fn distance(left: (f64, f64), right: (f64, f64)) -> f64 {
    (right.0 - left.0).hypot(right.1 - left.1)
}

fn midpoint(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
    ((left.0 + right.0) * 0.5, (left.1 + right.1) * 0.5)
}

fn scale_point_about(point: (f64, f64), pivot: (f64, f64), factor: f64) -> (f64, f64) {
    (
        pivot.0 + (point.0 - pivot.0) * factor,
        pivot.1 + (point.1 - pivot.1) * factor,
    )
}

fn unit_direction(start: (f64, f64), end: (f64, f64)) -> (f64, f64) {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length = dx.hypot(dy);
    debug_assert!(length > 0.0);
    (dx / length, dy / length)
}

fn unit_direction_or(start: (f64, f64), end: (f64, f64), fallback: (f64, f64)) -> (f64, f64) {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length = dx.hypot(dy);
    if length == 0.0 {
        fallback
    } else {
        (dx / length, dy / length)
    }
}

fn current_tip_direction(base: (f64, f64), apex: (f64, f64)) -> Option<(f64, f64)> {
    let dx = apex.0 - base.0;
    let dy = apex.1 - base.1;
    let length = dx.hypot(dy);
    (length > 0.0).then_some((dx / length, dy / length))
}

fn reposition_tip(
    state: &mut SemanticObjectState,
    old_apex: (f64, f64),
    old_direction: Option<(f64, f64)>,
    new_apex: (f64, f64),
    new_direction: Option<(f64, f64)>,
) -> Result<(), AuthoringError> {
    if let (Some(old), Some(new)) = (old_direction, new_direction) {
        let angle = new.1.atan2(new.0) - old.1.atan2(old.0);
        let ((translation_x, translation_y), rotation) =
            crate::semantic_mobject::rotate_affine_about_point(
                (state.transform.translation.x, state.transform.translation.y),
                state.transform.rotation_z,
                angle,
                old_apex,
            )?;
        state.transform.translation.x = translation_x;
        state.transform.translation.y = translation_y;
        state.transform.rotation_z = rotation;
    }
    state.transform.translation.x += new_apex.0 - old_apex.0;
    state.transform.translation.y += new_apex.1 - old_apex.1;
    state
        .transform
        .translation
        .lower_xy_f32()
        .map_err(AuthoringError::from)?;
    Ok(())
}

fn set_line_world_endpoints(
    state: &mut SemanticObjectState,
    start: (f64, f64),
    end: (f64, f64),
    name: &str,
) -> Result<(), AuthoringError> {
    let start = inverse_transform_point(state.transform, start, name)?;
    let end = inverse_transform_point(state.transform, end, name)?;
    state.content = StoredGeometry::Line {
        start: lower_point(&format!("{name} start"), start)?,
        end: lower_point(&format!("{name} end"), end)?,
    }
    .into();
    Ok(())
}

fn inverse_transform_point(
    transform: noon_core::SemanticTransform2_5D,
    point: (f64, f64),
    name: &str,
) -> Result<(f64, f64), AuthoringError> {
    let translated_x = point.0 - transform.translation.x;
    let translated_y = point.1 - transform.translation.y;
    let (sin, cos) = transform.rotation_z.sin_cos();
    let rotated_x = translated_x * cos + translated_y * sin;
    let rotated_y = -translated_x * sin + translated_y * cos;
    checked_point(
        name,
        if transform.scale.x == 0.0 {
            0.0
        } else {
            rotated_x / transform.scale.x
        },
        if transform.scale.y == 0.0 {
            0.0
        } else {
            rotated_y / transform.scale.y
        },
    )
}

fn checked_point(name: &str, x: f64, y: f64) -> Result<(f64, f64), AuthoringError> {
    crate::integration::authoring_render_f64(&format!("{name}.x"), x)?;
    crate::integration::authoring_render_f64(&format!("{name}.y"), y)?;
    Ok((x, y))
}

fn lower_point(name: &str, point: (f64, f64)) -> Result<noon_core::Vec2, AuthoringError> {
    crate::semantic_mobject::authoring_xy_f64(point.0, point.1)
        .and_then(|value| value.lower_xy_f32().map_err(AuthoringError::from))
        .map_err(|error| match error {
            AuthoringError::VectorLowering(_) => AuthoringError::InvalidRenderNumber {
                name: name.to_owned(),
                value: point.0.abs().max(point.1.abs()),
            },
            other => other,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ManimArrowOptions, Scene, DEFAULT_ARROW_STROKE_WIDTH_RATIO};
    use noon_core::SemanticObjectContent;
    use std::rc::Rc;

    fn resource_handle(state: &SemanticObjectState) -> noon_core::GeometryResourceHandle {
        let SemanticObjectContent::Geometry(StoredGeometry::Resource(handle)) = state.content else {
            panic!("arrow tip must remain a retained geometry resource");
        };
        handle
    }

    #[test]
    fn scale_preserves_tip_resource_and_recovers_authored_stroke_cap() {
        let scene = Scene::new();
        let mut options = ManimArrowOptions::arrow(0.0, 0.0, 0.4, 0.0).unwrap();
        options.set_buff(0.0).unwrap();
        let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
        let tip_resource = resource_handle(&arrow.end_tip().state().unwrap());
        let before_revision = scene.integration_store().borrow().scene_revision();

        arrow.scale(0.5, false).unwrap();
        let scaled_length = arrow.manim_length().unwrap();
        assert!((scaled_length - 0.2).abs() < 1e-6);
        assert_eq!(
            resource_handle(&arrow.end_tip().state().unwrap()),
            tip_resource
        );
        assert!(
            (arrow.shaft().state().unwrap().style.stroke_width
                - DEFAULT_ARROW_STROKE_WIDTH_RATIO * scaled_length)
                .abs()
                < 1e-12
        );
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before_revision.checked_next().unwrap()
        );

        arrow.scale(10.0, false).unwrap();
        assert!((arrow.manim_length().unwrap() - 2.0).abs() < 1e-5);
        assert_eq!(
            resource_handle(&arrow.end_tip().state().unwrap()),
            tip_resource
        );
        assert!((arrow.shaft().state().unwrap().style.stroke_width - 0.06).abs() < 1e-12);
    }

    #[test]
    fn scale_tips_true_scales_tip_with_family_in_one_publication() {
        let scene = Scene::new();
        let mut options = ManimArrowOptions::arrow(-1.0, 0.0, 1.0, 0.0).unwrap();
        options.set_buff(0.0).unwrap();
        let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
        let tip_resource = resource_handle(&arrow.end_tip().state().unwrap());
        let tip_scale = arrow.end_tip().state().unwrap().transform.scale;
        let before_revision = scene.integration_store().borrow().scene_revision();

        arrow.scale(2.0, true).unwrap();

        let after = arrow.end_tip().state().unwrap();
        assert_eq!(resource_handle(&after), tip_resource);
        assert!((after.transform.scale.x - tip_scale.x * 2.0).abs() < 1e-12);
        assert!((after.transform.scale.y - tip_scale.y * 2.0).abs() < 1e-12);
        assert!((arrow.manim_length().unwrap() - 4.0).abs() < 1e-5);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before_revision.checked_next().unwrap()
        );
    }

    #[test]
    fn negative_scale_preserves_tip_size_and_reverses_public_direction() {
        let scene = Scene::new();
        let mut options = ManimArrowOptions::arrow(-1.0, 0.0, 1.0, 0.0).unwrap();
        options.set_buff(0.0).unwrap();
        let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
        let before_tip_scale = arrow.end_tip().state().unwrap().transform.scale;

        arrow.scale(-1.0, false).unwrap();

        let endpoints = arrow.manim_endpoints().unwrap();
        assert!((endpoints.start.0 - 1.0).abs() < 1e-5);
        assert!((endpoints.end.0 + 1.0).abs() < 1e-5);
        assert_eq!(
            arrow.end_tip().state().unwrap().transform.scale,
            before_tip_scale
        );
        assert!((arrow.manim_angle().unwrap() - std::f64::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn double_arrow_preserves_both_tip_resources() {
        let scene = Scene::new();
        let mut options = ManimArrowOptions::double_arrow(-2.0, 0.0, 2.0, 0.0).unwrap();
        options.set_buff(0.0).unwrap();
        let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
        let end_resource = resource_handle(&arrow.end_tip().state().unwrap());
        let start_resource = resource_handle(&arrow.start_tip().unwrap().state().unwrap());

        arrow.scale(0.5, false).unwrap();

        assert_eq!(
            resource_handle(&arrow.end_tip().state().unwrap()),
            end_resource
        );
        assert_eq!(
            resource_handle(&arrow.start_tip().unwrap().state().unwrap()),
            start_resource
        );
        assert!((arrow.manim_length().unwrap() - 2.0).abs() < 1e-5);
    }

    #[test]
    fn zero_length_scale_is_a_noop_like_manim() {
        let scene = Scene::new();
        let arrow = ManimArrow::create(
            Rc::clone(scene.integration_store()),
            ManimArrowOptions::arrow(1.0, 2.0, 1.0, 2.0).unwrap(),
        )
        .unwrap();
        let before_revision = scene.integration_store().borrow().scene_revision();
        let before_shaft = arrow.shaft().state().unwrap();

        arrow.scale(3.0, false).unwrap();

        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before_revision
        );
        assert_eq!(arrow.shaft().state().unwrap(), before_shaft);
    }

    #[test]
    fn scale_rejects_invalid_input_atomically_and_keeps_unrelated_state() {
        let scene = Scene::new();
        let arrow = ManimArrow::create(
            Rc::clone(scene.integration_store()),
            ManimArrowOptions::arrow(-1.0, 0.0, 1.0, 0.0).unwrap(),
        )
        .unwrap();
        let unrelated = scene.circle(0.5).unwrap();
        let unrelated_before = unrelated.state().unwrap();
        let before_revision = scene
            .integration_store()
            .borrow()
            .scene_revision();

        assert!(arrow.scale(f64::NAN, false).is_err());

        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before_revision
        );
        assert_eq!(unrelated.state().unwrap(), unrelated_before);
    }
}
