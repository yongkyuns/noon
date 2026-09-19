//! Shared authored declaration for a retained 2D inset view.
//!
//! Both handles remain ordinary semantic rectangles. Activation adds them to the
//! scene foreground and installs one typed display-to-camera-frame relation in
//! the same semantic transaction.

use std::rc::Rc;

use noon_core::{
    Color, SemanticInset2DViewRole, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticObjectRole, SemanticObjectState, SemanticPaint, SemanticStyle, SemanticTransform2_5D,
    SemanticVec3, StoredGeometry, Vec2, DEFAULT_FRAME_HEIGHT, DEFAULT_FRAME_WIDTH,
    DEFAULT_MOBJECT_TO_EDGE_BUFFER,
};

use crate::{AuthoringError, Mobject, MobjectTarget, Scene, SceneMembershipRequest};

const MANIM_CAIRO_LINE_WIDTH_MULTIPLE: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomedSceneOptions {
    pub display_height: f64,
    pub display_width: f64,
    pub display_center: Option<Vec2>,
    pub display_corner: Vec2,
    pub display_corner_buff: f64,
    pub camera_frame_start: Vec2,
    pub zoom_factor: f64,
    pub camera_frame_stroke_width: f64,
    pub image_frame_stroke_width: f64,
    pub capture_own_display: bool,
}

impl Default for ZoomedSceneOptions {
    fn default() -> Self {
        Self {
            display_height: 3.0,
            display_width: 3.0,
            display_center: None,
            display_corner: noon_core::UR,
            display_corner_buff: f64::from(DEFAULT_MOBJECT_TO_EDGE_BUFFER),
            camera_frame_start: Vec2::ZERO,
            zoom_factor: 0.15,
            camera_frame_stroke_width: 2.0,
            image_frame_stroke_width: 3.0,
            capture_own_display: false,
        }
    }
}

impl ZoomedSceneOptions {
    fn validate(self) -> Result<Self, AuthoringError> {
        for (name, value) in [
            ("zoomed_display_height", self.display_height),
            ("zoomed_display_width", self.display_width),
            ("zoom_factor", self.zoom_factor),
        ] {
            if !value.is_finite() {
                return Err(AuthoringError::InvalidRenderNumber {
                    name: name.into(),
                    value,
                });
            }
            if value <= 0.0 {
                return Err(AuthoringError::NonPositiveNumber {
                    name: name.into(),
                    value,
                });
            }
        }
        for (name, value) in [
            ("zoomed_display_corner_buff", self.display_corner_buff),
            (
                "zoomed_camera_frame_stroke_width",
                self.camera_frame_stroke_width,
            ),
            ("image_frame_stroke_width", self.image_frame_stroke_width),
        ] {
            if !value.is_finite() {
                return Err(AuthoringError::InvalidRenderNumber {
                    name: name.into(),
                    value,
                });
            }
            if value < 0.0 {
                return Err(AuthoringError::NegativeStrokeWidth(value));
            }
        }
        let finite = |point: Vec2| point.x.is_finite() && point.y.is_finite();
        if !finite(self.camera_frame_start)
            || !finite(self.display_corner)
            || self.display_center.is_some_and(|center| !finite(center))
        {
            return Err(AuthoringError::NonFiniteTransform);
        }
        if self.display_center.is_none() && self.display_corner == Vec2::ZERO {
            return Err(AuthoringError::ZeroDirection);
        }
        Ok(self)
    }

    fn display_position(self) -> Vec2 {
        self.display_center.unwrap_or_else(|| {
            let direction = Vec2::new(
                self.display_corner.x.signum(),
                self.display_corner.y.signum(),
            );
            Vec2::new(
                direction.x
                    * (DEFAULT_FRAME_WIDTH * 0.5
                        - self.display_width as f32 * 0.5
                        - self.display_corner_buff as f32),
                direction.y
                    * (DEFAULT_FRAME_HEIGHT * 0.5
                        - self.display_height as f32 * 0.5
                        - self.display_corner_buff as f32),
            )
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ZoomedView {
    camera_frame: Mobject,
    display: Mobject,
    capture_own_display: bool,
}

impl ZoomedView {
    pub fn camera_frame(&self) -> &Mobject {
        &self.camera_frame
    }

    pub fn display(&self) -> &Mobject {
        &self.display
    }

    /// Current authored camera-to-display height ratio.
    pub fn zoom_factor(&self) -> Result<f64, AuthoringError> {
        let display_height = self.display.height()?;
        if !display_height.is_finite() || display_height <= 0.0 {
            return Err(AuthoringError::NonPositiveNumber {
                name: "zoomed_display_height".into(),
                value: display_height,
            });
        }
        Ok(self.camera_frame.height()? / display_height)
    }
}

impl Scene {
    /// Create the two detached ordinary objects for a Manim-compatible zoomed view.
    pub fn zoomed_view(
        &mut self,
        options: ZoomedSceneOptions,
    ) -> Result<ZoomedView, AuthoringError> {
        let options = options.validate()?;
        let mut frame = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(options.display_width as f32, options.display_height as f32),
        });
        frame.transform = SemanticTransform2_5D {
            translation: SemanticVec3::from_vec2(options.camera_frame_start),
            scale: SemanticVec3::new(options.zoom_factor, options.zoom_factor, 1.0),
            ..SemanticTransform2_5D::default()
        };
        frame.style = SemanticStyle {
            fill: None,
            stroke: Some(SemanticPaint::Solid(Color::WHITE)),
            stroke_width: options.camera_frame_stroke_width * MANIM_CAIRO_LINE_WIDTH_MULTIPLE,
            ..SemanticStyle::default()
        };

        let mut display = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(options.display_width as f32, options.display_height as f32),
        });
        display.transform.translation = SemanticVec3::from_vec2(options.display_position());
        display.style = SemanticStyle {
            fill: Some(SemanticPaint::Solid(Color::BLACK)),
            fill_opacity: 1.0,
            stroke: Some(SemanticPaint::Solid(Color::WHITE)),
            stroke_width: options.image_frame_stroke_width * MANIM_CAIRO_LINE_WIDTH_MULTIPLE,
            ..SemanticStyle::default()
        };

        let mut transaction = SemanticMutationTransaction::new();
        let frame = transaction.create_node(SemanticNodeCreation::object(frame));
        let display = transaction.create_node(SemanticNodeCreation::object(display));
        let result = self.apply_semantic_transaction(transaction)?;
        let frame = result
            .resolve(frame)
            .ok_or(AuthoringError::UnresolvedCreatedNode(frame))?;
        let display = result
            .resolve(display)
            .ok_or(AuthoringError::UnresolvedCreatedNode(display))?;
        Ok(ZoomedView {
            camera_frame: Mobject::from_node(Rc::clone(self.integration_store()), frame)?,
            display: Mobject::from_node(Rc::clone(self.integration_store()), display)?,
            capture_own_display: options.capture_own_display,
        })
    }

    /// Atomically register the inset relation and add both ordinary handles to foreground.
    pub fn activate_zooming(&mut self, view: &ZoomedView) -> Result<(), AuthoringError> {
        let transaction = self.prepare_zooming_activation(view)?;
        self.apply_semantic_transaction(transaction)?;
        Ok(())
    }

    /// Prepare the one authoritative relation/membership edit for cold or live publication.
    pub fn prepare_zooming_activation(
        &self,
        view: &ZoomedView,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        if !Rc::ptr_eq(
            view.camera_frame.integration_store(),
            self.integration_store(),
        ) || !Rc::ptr_eq(view.display.integration_store(), self.integration_store())
        {
            return Err(AuthoringError::ForeignStore);
        }
        view.camera_frame.validate()?;
        view.display.validate()?;
        let targets = [
            MobjectTarget::Object(view.camera_frame()),
            MobjectTarget::Object(view.display()),
        ];
        let mut transaction = crate::scene_membership::prepare_scene_membership(
            self.integration_store(),
            self.root(),
            SceneMembershipRequest::AddForeground(&targets),
        )?;
        transaction.replace_role(
            view.display.node_id(),
            SemanticObjectRole::Inset2DView(
                SemanticInset2DViewRole::new(view.camera_frame.node_id())
                    .capture_own_display(view.capture_own_display),
            ),
        );
        Ok(transaction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnimationOptions, RateFunction};

    #[test]
    fn declaration_is_detached_and_activation_is_one_atomic_foreground_publication() {
        let mut scene = Scene::new();
        let view = scene.zoomed_view(ZoomedSceneOptions::default()).unwrap();
        assert!(scene
            .integration_store()
            .borrow()
            .node(scene.root())
            .unwrap()
            .members()
            .is_empty());

        let before = scene.revision();
        scene.activate_zooming(&view).unwrap();
        assert_eq!(scene.revision().get(), before.get() + 1);
        assert_eq!(
            scene.foreground_mobjects().unwrap(),
            [view.camera_frame().node_id(), view.display().node_id()]
        );
        let views = scene
            .integration_store()
            .borrow()
            .inset_2d_views()
            .collect::<Vec<_>>();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].0, view.display().node_id());
        assert_eq!(views[0].1.camera_frame, view.camera_frame().node_id());
    }

    #[test]
    fn effective_view_uses_one_frame_and_rejects_rotated_display() {
        let mut scene = Scene::new();
        let view = scene.zoomed_view(ZoomedSceneOptions::default()).unwrap();
        scene.activate_zooming(&view).unwrap();
        let session = scene.execution_session().unwrap();
        let state = session.inset_2d_views().unwrap()[0];
        assert_eq!(state.camera.center, Vec2::ZERO);
        assert!((state.zoom_factor() - 0.15).abs() < 1.0e-6);
        assert_eq!(state.display_size, Vec2::new(3.0, 3.0));

        let display_id = session
            .execution_object_id(view.display().node_id())
            .unwrap();
        let mut display = view.display().clone();
        display.set_rotation(0.125).unwrap();
        let session = scene.execution_session().unwrap();
        assert_eq!(
            session.inset_2d_views(),
            Err(crate::ExecutionSessionInset2DError::InvalidDisplay { object: display_id })
        );
    }

    #[test]
    fn camera_animation_and_direct_seek_derive_the_effective_inset_from_one_frame() {
        let mut scene = Scene::new();
        let view = scene.zoomed_view(ZoomedSceneOptions::default()).unwrap();
        scene.activate_zooming(&view).unwrap();
        let mut target = view.camera_frame().target_editor().unwrap();
        target.set_translation(2.0, -1.0).unwrap();
        target.scale(2.0, 2.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        let segment = scene
            .live(&mut session)
            .declare_and_activate_transform_to(
                view.camera_frame(),
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        scene
            .live(&mut session)
            .advance_segment_to(segment, 0.5)
            .unwrap();
        let midpoint = session.inset_2d_views().unwrap()[0];
        assert_eq!(midpoint.camera.center, Vec2::new(1.0, -0.5));
        assert!((midpoint.zoom_factor() - 0.225).abs() < 1.0e-6);

        session.seek(0.25).unwrap();
        let rewind = session.inset_2d_views().unwrap()[0];
        assert_eq!(rewind.camera.center, Vec2::new(0.5, -0.25));
        assert!((rewind.zoom_factor() - 0.1875).abs() < 1.0e-6);
        session.advance_to(1.0).unwrap();
        let endpoint = session.inset_2d_views().unwrap()[0];
        assert_eq!(endpoint.camera.center, Vec2::new(2.0, -1.0));
        assert!((endpoint.zoom_factor() - 0.3).abs() < 1.0e-6);
    }

    #[test]
    fn invalid_relation_rolls_back_and_camera_retirement_cleans_the_relation() {
        let mut scene = Scene::new();
        let view = scene.zoomed_view(ZoomedSceneOptions::default()).unwrap();
        scene.activate_zooming(&view).unwrap();
        let before = scene.revision();
        let mut invalid = SemanticMutationTransaction::new();
        invalid.replace_role(
            view.display().node_id(),
            SemanticObjectRole::Inset2DView(SemanticInset2DViewRole::new(view.display().node_id())),
        );
        assert!(scene.apply_semantic_transaction(invalid).is_err());
        assert_eq!(scene.revision(), before);
        assert_eq!(
            scene.integration_store().borrow().inset_2d_views().count(),
            1
        );

        let mut retire = SemanticMutationTransaction::new();
        retire.remove_node(view.camera_frame().node_id());
        scene.apply_semantic_transaction(retire).unwrap();
        let store = scene.integration_store().borrow();
        assert_eq!(store.inset_2d_views().count(), 0);
        assert_eq!(
            store
                .semantic_object_state_checked(view.display().node_id())
                .unwrap()
                .role(),
            SemanticObjectRole::Ordinary
        );
    }
}
