//! Own points, drawing intent and children stay on stable family roots.
use crate::{
    Color, ContinuationStep, LayoutAnchor, LiveContinuation, LiveProgram, LiveSession,
    LiveSessionError, ManimRotationPivot, Mobject, MobjectFamily, Scene, SemanticVec3,
};

pub struct FamilyRootPoints {
    group: MobjectFamily,
    vector: MobjectFamily,
    left: Mobject,
    right: Mobject,
    left_seed: Mobject,
    right_seed: Mobject,
    alignment_peer: MobjectFamily,
    alignment_leaf: Mobject,
    left_source_points: Vec<noon_core::SemanticVec3>,
    left_points: Vec<noon_core::SemanticVec3>,
    right_points: Vec<(f64, f64)>,
    stage: u8,
}

impl LiveContinuation for FamilyRootPoints {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.stage == 3 {
            return Ok(ContinuationStep::Finished);
        }
        let step = (|| -> Result<_, LiveSessionError> {
            match self.stage {
                0 => {}
                1 => {
                    live.clear_family_points(&self.group)?;
                    live.clear_family_points(&self.vector)?;
                    live.align_path_points(
                        &LayoutAnchor::from(&self.vector),
                        &LayoutAnchor::from(&self.alignment_peer),
                    )?;
                }
                2 => {
                    // Replay the same authored data and child presentation;
                    // clearing the roots did not undo their family transforms.
                    live.become_mobject(&self.left, &self.left_seed, Default::default())?;
                    live.become_mobject(&self.right, &self.right_seed, Default::default())?;
                    self.left_points.clone_from(&self.left_source_points);
                    live.set_family_points_3d(&self.group, &self.left_points)?;
                    live.stretch(
                        &LayoutAnchor::from(&self.group),
                        2.0,
                        crate::LayoutDimension::Depth,
                        ManimRotationPivot::Point3d(SemanticVec3::ZERO),
                    )?;
                    live.shift_family_3d(&self.group, SemanticVec3::new(0.0, 0.0, 0.25))?;
                    for point in &mut self.left_points {
                        point.z = point.z * 2.0 + 0.25;
                    }
                    let anchor = LayoutAnchor::from(&self.group);
                    let extent = live.dimension_length(&anchor, crate::LayoutDimension::Depth)?;
                    live.rescale_to_fit_with_pivot(
                        &anchor,
                        2.0,
                        crate::LayoutDimension::Depth,
                        true,
                        ManimRotationPivot::Point3d(SemanticVec3::ZERO),
                    )?;
                    live.rescale_to_fit_with_pivot(
                        &anchor,
                        4.0,
                        crate::LayoutDimension::Depth,
                        false,
                        ManimRotationPivot::Point3d(SemanticVec3::new(-2.0, 0.0, 0.0)),
                    )?;
                    adjust_fitting_reference(&mut self.left_points, extent);
                    assert_eq!(
                        live.dimension_length(&anchor, crate::LayoutDimension::Depth)?,
                        4.0
                    );
                    live.append_family_points_f64(&self.vector, &self.right_points)?;
                    live.align_path_points(
                        &LayoutAnchor::from(&self.vector),
                        &LayoutAnchor::from(&self.alignment_leaf),
                    )?;
                    let rotated = LayoutAnchor::from(&self.vector);
                    live.rotate_layout(&rotated, 0.37, ManimRotationPivot::Point(2.0, 0.0))?;
                    live.stretch(
                        &rotated,
                        1.1234567890123,
                        crate::LayoutDimension::Width,
                        ManimRotationPivot::Point3d(SemanticVec3::new(2.0, 0.0, 0.0)),
                    )?;
                    live.scale_layout_3d(
                        &rotated,
                        SemanticVec3::new(0.9, 1.1, 1.25),
                        ManimRotationPivot::Point3d(SemanticVec3::new(2.0, 0.0, 0.5)),
                    )?;
                }
                _ => unreachable!("three held phases"),
            }
            let expected = if self.stage == 1 { 0 } else { 16 };
            assert_eq!(self.group.get_num_points()?, expected);
            assert_eq!(
                self.group.own_path_query()?.points_3d()?,
                if self.stage == 1 {
                    Vec::new()
                } else {
                    self.left_points.clone()
                }
            );
            assert_eq!(
                self.vector.get_num_points()?,
                if self.stage == 1 { 0 } else { 32 }
            );
            assert_eq!(self.group.member_count()?, 1);
            assert_eq!(self.vector.member_count()?, 1);
            self.stage += 1;
            live.wait_segment(0.25).map(ContinuationStep::Await)
        })();
        step.map_err(|error| error.to_string())
    }
}

// Expected authoring data: depth first reaches two, then a uniform fit
// about (-2, 0, 0) reaches four and keeps the demonstration visible.
fn adjust_fitting_reference(points: &mut [SemanticVec3], extent: f64) {
    assert!(extent > 0.0);
    for point in points {
        point.x = (point.x + 2.0) * 2.0 - 2.0;
        point.y *= 2.0;
        point.z *= 4.0 / extent;
    }
}

/// The same compiled Rust continuation feeds native capture and direct WASM.
pub fn program() -> Result<LiveProgram<FamilyRootPoints>, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut left = scene.circle(0.2)?;
        let mut right = scene.circle(0.2)?;
        left.shift(-2.0, 0.0)?;
        right.shift(2.0, 0.0)?;
        for child in [&mut left, &mut right] {
            child.set_fill(1.0, 1.0, 0.0, 1.0)?;
            child.set_stroke_width(0.0)?;
        }
        let left_seed = left.copy_handle()?;
        let right_seed = right.copy_handle()?;
        let mut left_square = scene.square(1.2)?;
        let mut right_square = scene.square(1.2)?;
        left_square.shift(-2.0, 0.0)?;
        right_square.shift(2.0, 0.0)?;
        let mut left_points = left_square
            .path_query()?
            .points_3d()?
            .into_iter()
            .enumerate()
            .map(|(index, mut point)| {
                point.z = (index as f64 + 1.0) / 32.0;
                point
            })
            .collect::<Vec<_>>();
        let left_source_points = left_points.clone();
        let right_points = right_square.path_query()?.points()?;
        let group = scene.family_with_presentation(&[(&left).into()], 0.0, Color::WHITE, false)?;
        let vector = scene.family_with_presentation(&[(&right).into()], 0.0, Color::WHITE, true)?;
        let alignment_peer = scene.family_with_presentation(&[], 0.0, Color::WHITE, true)?;
        let alignment_leaf = scene.circle(0.7)?;
        LayoutAnchor::from(&vector).align_path_points(&LayoutAnchor::from(&alignment_peer))?;
        group.set_points_3d(&left_points)?;
        LayoutAnchor::from(&group).stretch(
            2.0,
            crate::LayoutDimension::Depth,
            ManimRotationPivot::Point3d(SemanticVec3::ZERO),
        )?;
        group.shift_3d(SemanticVec3::new(0.0, 0.0, 0.25))?;
        for point in &mut left_points {
            point.z = point.z * 2.0 + 0.25;
        }
        let anchor = LayoutAnchor::from(&group);
        let extent = anchor.dimension_length(crate::LayoutDimension::Depth)?;
        anchor.rescale_to_fit_with_pivot(
            2.0,
            crate::LayoutDimension::Depth,
            true,
            ManimRotationPivot::Point3d(SemanticVec3::ZERO),
        )?;
        anchor.rescale_to_fit_with_pivot(
            4.0,
            crate::LayoutDimension::Depth,
            false,
            ManimRotationPivot::Point3d(SemanticVec3::new(-2.0, 0.0, 0.0)),
        )?;
        adjust_fitting_reference(&mut left_points, extent);
        assert_eq!(anchor.dimension_length(crate::LayoutDimension::Depth)?, 4.0);
        vector.append_points_f64(&right_points[..8])?;
        vector.append_points_f64(&right_points[8..])?;
        let mut vector_root = vector
            .own_object()?
            .ok_or_else(|| std::io::Error::other("materialized family has no own payload"))?;
        vector_root.set_fill(0.0, 0.0, 1.0, 1.0)?;
        vector_root.set_stroke_width(0.0)?;
        LayoutAnchor::from(&vector).align_path_points(&LayoutAnchor::from(&alignment_leaf))?;
        let rotated = LayoutAnchor::from(&vector);
        rotated.rotate(0.37, ManimRotationPivot::Point(2.0, 0.0))?;
        rotated.stretch(
            1.1234567890123,
            crate::LayoutDimension::Width,
            ManimRotationPivot::Point3d(SemanticVec3::new(2.0, 0.0, 0.0)),
        )?;
        rotated.scale_3d(
            SemanticVec3::new(0.9, 1.1, 1.25),
            ManimRotationPivot::Point3d(SemanticVec3::new(2.0, 0.0, 0.5)),
        )?;
        scene.add_many(&[(&group).into(), (&vector).into()])?;
        Ok(scene.into_live_program(FamilyRootPoints {
            group,
            vector,
            left,
            right,
            left_seed,
            right_seed,
            alignment_peer,
            alignment_leaf,
            left_source_points,
            left_points,
            right_points,
            stage: 0,
        })?)
    };
    build().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn shared_root_points_continue_through_held_clear_and_restore_phases() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        for end in [0.25, 0.5, 0.75] {
            assert!(matches!(
                program.resume().unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            match program.drive_to(&mut callbacks, end).unwrap() {
                LiveProgramStatus::PublicationPending(expected) => {
                    let context = program.take_renderer_publication().context();
                    assert_eq!(context, expected);
                    program.admit_publication(context).unwrap();
                }
                LiveProgramStatus::ReadyToResume => {}
                status => panic!("root-point completion: {status:?}"),
            }
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }

    #[test]
    fn public_live_family_points_reject_foreign_identity_without_publication() {
        let mut scene = Scene::new();
        let local = scene.family(&[]).unwrap();
        scene.add_many(&[(&local).into()]).unwrap();
        let foreign = Scene::new().family(&[]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let context = session.publication_context();
        let revision = scene.revision();
        let mut live = scene.live(&mut session);
        assert!(live.set_family_points_f64(&foreign, &[(0.0, 0.0)]).is_err());
        assert!(live.clear_family_points(&foreign).is_err());
        assert_eq!(session.publication_context(), context);
        assert_eq!(scene.revision(), revision);
        assert!(local.own_object().unwrap().is_none());
    }
}
