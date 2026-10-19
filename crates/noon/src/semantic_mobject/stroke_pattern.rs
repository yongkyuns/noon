//! Pattern authoring over ordinary analytic lines and exact object tracks.
use super::*;
use noon_core::{
    CompositionTimeMap, SemanticObjectTrackProperty, SemanticObjectTrackValues, StrokePattern,
    TrackTiming,
};

impl Mobject {
    /// Attach a retained pattern without constructing individual dash objects.
    /// This first profile supports planar analytic Lines, not arbitrary curves.
    pub fn set_stroke_pattern(&mut self, pattern: Option<StrokePattern>) -> Result<(), String> {
        self.validate().map_err(|e| e.to_string())?;
        let mut state = self.state().map_err(|e| e.to_string())?;
        if let Some(pattern) = pattern {
            pattern.validate().map_err(|e| e.to_string())?;
            if !matches!(
                &state.content,
                SemanticObjectContent::Geometry(StoredGeometry::Line { .. })
            ) {
                return Err("periodic strokes currently require an analytic Line".into());
            }
            // The retained Line renderer uses fill slots for pattern parameters.
            // Reject a missing stroke rather than rendering those slots as color.
            if state.style.stroke.is_none() || state.style.stroke_width <= 0.0 {
                return Err(
                    "periodic strokes require an enabled stroke and positive width".into(),
                );
            }
        }
        state.style.stroke_pattern = pattern;
        self.commit_state(state).map_err(|e| e.to_string())
    }

    pub fn stroke_pattern(&self) -> Result<Option<StrokePattern>, AuthoringError> {
        Ok(self.state()?.style.stroke_pattern)
    }
}

impl crate::Scene {
    /// Declare flow at the authored cursor for a finite duration. Signed speed
    /// uses local scene units per second. At completion the phase freezes.
    /// The cursor is unchanged; use ordinary waits for subsequent declarations.
    /// Include the returned leaf in the ordinary initial animation root.
    ///
    /// This first profile is pre-bootstrap and owns a full object track. It does
    /// not imply an independent live phase channel or concurrent conflicting
    /// transforms. Use flow_stroke_to for coordinated geometry and phase.
    pub fn flow_stroke(
        &mut self,
        line: &Mobject,
        speed: f32,
        duration: f64,
    ) -> Result<crate::DeclaredAnimation, String> {
        self.flow_stroke_to(line, line, speed, duration)
    }

    /// Coordinate phase and the connection's geometric/affine target in one
    /// evaluated track. This reuses exact object snapshots, not a new router.
    pub fn flow_stroke_to(
        &mut self,
        line: &Mobject,
        target: &Mobject,
        speed: f32,
        duration: f64,
    ) -> Result<crate::DeclaredAnimation, String> {
        self.require_object(line).map_err(|e| e.to_string())?;
        self.require_object(target).map_err(|e| e.to_string())?;
        let source_state = line.state().map_err(|e| e.to_string())?;
        let target_state = target.state().map_err(|e| e.to_string())?;
        for state in [&source_state, &target_state] {
            if !matches!(
                &state.content,
                SemanticObjectContent::Geometry(StoredGeometry::Line { .. })
            ) {
                return Err("continuous stroke flow requires analytic Line endpoints".into());
            }
            if state.style.stroke.is_none() || state.style.stroke_width <= 0.0 {
                return Err("continuous stroke flow requires stroked Line endpoints".into());
            }
        }
        if self.effective(line).is_ok() {
            return Err("flow_stroke must be declared before execution bootstrap".into());
        }
        if !speed.is_finite()
            || !duration.is_finite()
            || duration <= 0.0
            || !(self.time() + duration).is_finite()
        {
            return Err("stroke flow speed must be finite and duration positive/finite".into());
        }
        let pattern = source_state
            .style
            .stroke_pattern
            .ok_or("flow_stroke requires a declared stroke pattern")?;
        pattern.validate().map_err(|e| e.to_string())?;
        let end = f64::from(pattern.phase()) + f64::from(speed) * duration;
        if !end.is_finite() || !(end as f32).is_finite() {
            return Err("stroke flow terminal phase is not representable".into());
        }
        let terminal = pattern.with_phase(end as f32).map_err(|e| e.to_string())?;
        // Endpoint snapshots, track declaration and base state share one atomic
        // transaction. No failed operation can leak copies or a partial track.
        let mut terminal_state = target_state;
        terminal_state.style.stroke_pattern = Some(terminal);
        let mut transaction = SemanticMutationTransaction::new();
        let from = transaction.create_node(SemanticNodeCreation::object(source_state.clone()));
        let to = transaction.create_node(SemanticNodeCreation::object(terminal_state.clone()));
        let animation = transaction.create_object_property_track(
            line.node_id(),
            SemanticObjectTrackProperty::Transform,
            SemanticObjectTrackValues::Object {
                from: from.into(),
                to: to.into(),
            },
            TrackTiming::new(self.time(), duration, noon_core::RateFunction::Linear),
            CompositionTimeMap::identity(),
        );
        stage_state_changes(
            &mut transaction,
            line.node_id(),
            &source_state,
            &terminal_state,
        );
        let result = self
            .apply_semantic_transaction(transaction)
            .map_err(|e| e.to_string())?;
        Ok(crate::DeclaredAnimation::new(
            Rc::clone(self.integration_store()),
            result
                .resolve(animation)
                .expect("committed animation has one identity"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{AnimationOptions, SemanticAnimationIntent};

    fn session(scene: &crate::Scene, flow: &crate::DeclaredAnimation) -> crate::ExecutionSession {
        let root = scene
            .declare_animation(
                SemanticAnimationIntent::Composition {
                    kind: noon_core::SemanticAnimationCompositionKind::Parallel,
                    children: vec![flow.node_id()],
                },
                AnimationOptions::new(),
            )
            .unwrap();
        scene.execution_session_with_animation_root(&root).unwrap()
    }

    #[test]
    fn stroke_pattern_phase_uses_normal_timeline_and_repeated_seek() {
        let mut scene = crate::Scene::new();
        let mut line = scene.line((-3.0, 0.0), (3.0, 0.0)).unwrap();
        line.set_stroke_pattern(Some(StrokePattern::dashed(0.3, 0.2).unwrap()))
            .unwrap();
        scene.add(&line).unwrap();
        let flow = scene.flow_stroke(&line, 1.0, 4.0).unwrap();
        scene.wait(4.0).unwrap();
        let mut execution = session(&scene, &flow);
        let identity = execution.frame().objects[0].id;
        for time in [0.0, 0.125, 0.5, 1.125, 4.0, 0.125, 3.0, 0.0] {
            let frame = execution.seek(time).unwrap();
            assert_eq!(
                frame.objects.len(),
                1,
                "dashes must not become scene objects"
            );
            assert_eq!(frame.objects[0].id, identity);
            assert_eq!(
                frame.objects[0].style.stroke_pattern.unwrap().phase(),
                time as f32
            );
        }
    }

    #[test]
    fn stroke_pattern_target_motion_and_phase_share_one_sample() {
        let mut scene = crate::Scene::new();
        let mut line = scene.line((-3.0, 0.0), (3.0, 0.0)).unwrap();
        line.set_stroke_pattern(Some(StrokePattern::dotted(0.4).unwrap()))
            .unwrap();
        scene.add(&line).unwrap();
        let target = scene.line((-3.0, 0.0), (3.0, 2.0)).unwrap();
        let flow = scene.flow_stroke_to(&line, &target, -0.5, 2.0).unwrap();
        scene.wait(2.0).unwrap();
        let mut execution = session(&scene, &flow);
        let row = &execution.seek(1.0).unwrap().objects[0];
        assert_eq!(row.style.stroke_pattern.unwrap().phase(), -0.5);
        match row.geometry().unwrap() {
            GeometryRef::Line { start, end } => {
                assert_eq!(*start, Vec2::new(-3.0, 0.0));
                assert_eq!(*end, Vec2::new(3.0, 1.0));
            }
            _ => panic!("expected retained analytic Line"),
        }
    }

    #[test]
    fn stroke_pattern_rejects_invalid_requests_without_mutation() {
        let mut scene = crate::Scene::new();
        let mut circle = scene.circle(1.0).unwrap();
        assert!(circle
            .set_stroke_pattern(Some(StrokePattern::dotted(0.3).unwrap()))
            .is_err());
        let mut line = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
        line.set_stroke_pattern(Some(StrokePattern::dashed(0.2, 0.2).unwrap()))
            .unwrap();
        scene.add(&line).unwrap();
        let revision = scene.revision();
        assert!(scene.flow_stroke(&line, f32::NAN, 1.0).is_err());
        assert!(scene.flow_stroke(&line, 1.0, 0.0).is_err());
        assert_eq!(scene.revision(), revision);
    }

    #[test]
    fn missing_stroke_or_width_never_exposes_packed_pattern_as_paint() {
        let mut scene = crate::Scene::new();
        let pattern = StrokePattern::dashed(0.25, 0.25).unwrap();

        let mut fill_only = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
        fill_only.disable_stroke().unwrap();
        assert!(fill_only.set_stroke_pattern(Some(pattern)).is_err());
        assert!(fill_only.stroke_pattern().unwrap().is_none());

        let mut zero_width = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
        zero_width.set_stroke_width(0.0).unwrap();
        assert!(zero_width.set_stroke_pattern(Some(pattern)).is_err());

        let mut source = scene.line((-2.0, 0.0), (2.0, 0.0)).unwrap();
        source.set_stroke_pattern(Some(pattern)).unwrap();
        scene.add(&source).unwrap();
        let mut target = scene.line((-2.0, 0.0), (2.0, 1.0)).unwrap();
        target.disable_stroke().unwrap();
        let before = scene.revision();
        assert!(scene.flow_stroke_to(&source, &target, 1.0, 2.0).is_err());
        assert_eq!(scene.revision(), before);

        // A formerly valid patterned Line can later lose its stroke through
        // normal authored styling. Lowering must reject that combination.
        source.disable_stroke().unwrap();
        assert!(scene.execution_session().is_err());
    }
}
