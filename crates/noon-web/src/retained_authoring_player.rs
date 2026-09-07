use noon::RetainedScene;
use noon_core::{
    Camera2DState, FontResourceArena, GeometryResourceArena, ObjectId, TextResourceArena,
};
use noon_runtime::{EvaluationError, FrameState, SceneInstance};

use crate::{
    retained_scene_spec_runtime::{CanonicalRetainedAuthoringScene, MixedRetainedAuthoringError},
    RetainedExecutionDeltaEncoder, RetainedExecutionDeltaEnvelope, RetainedExecutionTransportError,
    RetainedResourceBundle, RetainedResourceTransportError,
};

/// Deterministic execution owner for one mixed retained scene.
///
/// Compatibility authoring inputs may still be normalized before construction, but
/// runtime evaluation consumes one [`SceneInstance`]. The resource arenas stay
/// next to it so renderer preparation can resolve text/font/vector resources without
/// putting those payloads on the Python or per-frame execution wire.
#[derive(Clone, Debug)]
pub struct RetainedAuthoringPlayer {
    scene: RetainedScene,
    runtime: SceneInstance,
    encoder: RetainedExecutionDeltaEncoder,
    resource_bundle: Vec<u8>,
    camera_object: Option<ObjectId>,
    snapshot_sent: bool,
}

impl RetainedAuthoringPlayer {
    pub(crate) fn new(
        materialized: CanonicalRetainedAuthoringScene,
        session: u32,
    ) -> Result<Self, RetainedAuthoringPlayerError> {
        let camera_object = materialized.camera_object();
        let compiled = materialized.compile()?;
        let scene = materialized.into_scene();
        let render_geometries =
            crate::retained_resource_transport::compiled_render_geometries(&compiled);
        let mut bundle = RetainedResourceBundle::capture(
            scene
                .objects()
                .iter()
                .filter_map(|object| object.content.text()),
            scene.texts(),
            scene.geometries(),
            scene.fonts(),
        )?;
        let preparations =
            crate::retained_resource_transport::compiled_render_geometry_preparations(
                &compiled,
                &render_geometries,
            )?;
        bundle.set_render_geometries(session, render_geometries.clone(), preparations);
        let resource_bundle = bundle.encode_binary()?;
        let runtime = SceneInstance::new(compiled);
        Ok(Self {
            scene,
            runtime,
            encoder: RetainedExecutionDeltaEncoder::with_render_geometries(
                session,
                render_geometries,
            ),
            resource_bundle,
            camera_object,
            snapshot_sent: false,
        })
    }

    pub const fn scene(&self) -> &RetainedScene {
        &self.scene
    }

    pub fn frame(&self) -> &FrameState {
        self.runtime.frame()
    }

    pub const fn camera_object(&self) -> Option<ObjectId> {
        self.camera_object
    }

    pub fn resource_bundle_bytes(&self) -> &[u8] {
        &self.resource_bundle
    }

    pub const fn texts(&self) -> &TextResourceArena {
        self.scene.texts()
    }

    pub const fn geometries(&self) -> &GeometryResourceArena {
        self.scene.geometries()
    }

    pub const fn fonts(&self) -> &FontResourceArena {
        self.scene.fonts()
    }

    /// Evaluate one absolute scene time and encode the renderer-facing retained delta.
    ///
    /// The first call always emits a complete snapshot. Forward evaluation then emits
    /// only dirty objects. A backward seek invalidates the retained runtime frame and
    /// therefore naturally becomes a complete retained snapshot without changing the
    /// session or object/resource identities.
    pub fn evaluate_delta(
        &mut self,
        time: f64,
    ) -> Result<Option<RetainedExecutionDeltaEnvelope>, RetainedAuthoringPlayerError> {
        self.runtime.evaluate(time)?;
        let camera = self.camera_state()?;
        let changes = self.runtime.take_frame_changes();
        if !self.snapshot_sent {
            let delta = self.encoder.encode_snapshot(self.runtime.frame(), camera)?;
            self.snapshot_sent = true;
            return Ok(Some(delta));
        }
        Ok(self
            .encoder
            .encode_incremental(self.runtime.frame(), &changes, camera)?)
    }

    fn camera_state(&self) -> Result<Camera2DState, RetainedAuthoringPlayerError> {
        let Some(camera_object) = self.camera_object else {
            return Ok(Camera2DState::default());
        };
        let object = self
            .runtime
            .frame()
            .objects
            .iter()
            .find(|object| object.id == camera_object)
            .ok_or(RetainedAuthoringPlayerError::InvalidCameraObject(
                camera_object,
            ))?;
        let geometry =
            object
                .geometry()
                .ok_or(RetainedAuthoringPlayerError::InvalidCameraObject(
                    camera_object,
                ))?;
        Camera2DState::from_frame_object(geometry, object.transform).ok_or(
            RetainedAuthoringPlayerError::InvalidCameraObject(camera_object),
        )
    }
}

#[derive(Debug)]
pub enum RetainedAuthoringPlayerError {
    Authoring(MixedRetainedAuthoringError),
    Resource(RetainedResourceTransportError),
    Evaluation(EvaluationError),
    Transport(RetainedExecutionTransportError),
    InvalidCameraObject(ObjectId),
}

impl std::fmt::Display for RetainedAuthoringPlayerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(error) => error.fmt(formatter),
            Self::Resource(error) => error.fmt(formatter),
            Self::Evaluation(error) => error.fmt(formatter),
            Self::Transport(error) => error.fmt(formatter),
            Self::InvalidCameraObject(object) => write!(
                formatter,
                "retained camera object {} is missing or not a supported 2D frame",
                object.get()
            ),
        }
    }
}

impl std::error::Error for RetainedAuthoringPlayerError {}

impl From<MixedRetainedAuthoringError> for RetainedAuthoringPlayerError {
    fn from(value: MixedRetainedAuthoringError) -> Self {
        Self::Authoring(value)
    }
}

impl From<RetainedResourceTransportError> for RetainedAuthoringPlayerError {
    fn from(value: RetainedResourceTransportError) -> Self {
        Self::Resource(value)
    }
}

impl From<EvaluationError> for RetainedAuthoringPlayerError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

impl From<RetainedExecutionTransportError> for RetainedAuthoringPlayerError {
    fn from(value: RetainedExecutionTransportError) -> Self {
        Self::Transport(value)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use noon_core::{
        CompositionTimeMap, ObjectContentRef, Property, RateFunction, TrackDefinition, TrackId,
        TrackTiming, TrackValues, Vec2,
    };

    use crate::{CanonicalAuthoringScene, TransportObjectContent};

    use super::*;

    fn player(
        scene: &noon::Scene,
        bindings: Vec<(ObjectId, noon::Mobject)>,
        tracks: Vec<TrackDefinition>,
        camera_object: Option<ObjectId>,
        session: u32,
    ) -> RetainedAuthoringPlayer {
        let mut context = CanonicalAuthoringScene::with_store(Rc::clone(scene.store()));
        for (id, object) in bindings {
            context.bind_mobject(id, &object).unwrap();
        }
        let spec = context.finalize(tracks, Vec::new(), camera_object).unwrap();
        RetainedAuthoringPlayer::new(
            CanonicalRetainedAuthoringScene::from_scene_spec(spec).unwrap(),
            session,
        )
        .unwrap()
    }

    fn position_track(object: ObjectId, duration: f64) -> TrackDefinition {
        TrackDefinition {
            id: TrackId::new(0),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(2.0, 0.0),
            },
            timing: TrackTiming::new(0.0, duration, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        }
    }

    #[test]
    fn first_frame_preserves_canonical_geometry_text_geometry_order_and_resources() {
        let circle = ObjectId::new(1);
        let text = ObjectId::new(2);
        let square = ObjectId::new(3);
        let scene = noon::Scene::new();
        let circle_handle = scene.circle(0.25).unwrap();
        let text_handle = scene.text(noon::Text::new("Hello")).unwrap();
        let square_handle = scene.square(0.5).unwrap();
        let mut player = player(
            &scene,
            vec![
                (circle, circle_handle),
                (text, text_handle),
                (square, square_handle),
            ],
            Vec::new(),
            None,
            17,
        );

        let delta = player.evaluate_delta(0.0).unwrap().unwrap();
        assert!(delta.snapshot);
        assert_eq!(
            delta
                .objects
                .iter()
                .map(|object| object.object)
                .collect::<Vec<_>>(),
            vec![circle, text, square]
        );
        assert!(matches!(
            delta.objects[0].content,
            TransportObjectContent::Geometry { .. }
        ));
        assert!(matches!(
            delta.objects[1].content,
            TransportObjectContent::Text { .. }
        ));
        assert_eq!(
            RetainedResourceBundle::decode_binary(player.resource_bundle_bytes())
                .unwrap()
                .text_count(),
            1
        );
    }

    #[test]
    fn forward_and_backward_evaluation_preserve_text_identity() {
        let circle = ObjectId::new(1);
        let text = ObjectId::new(2);
        let scene = noon::Scene::new();
        let circle_handle = scene.circle(0.25).unwrap();
        let authored_text = scene.text(noon::Text::new("stable")).unwrap();
        let mut player = player(
            &scene,
            vec![(circle, circle_handle), (text, authored_text)],
            vec![position_track(circle, 1.0)],
            None,
            18,
        );
        let text_handle = player.scene().objects()[1].content.text().unwrap();

        player.evaluate_delta(0.0).unwrap().unwrap();
        let forward = player.evaluate_delta(0.5).unwrap().unwrap();
        assert!(!forward.snapshot);
        assert_eq!(forward.objects.len(), 1);
        assert_eq!(forward.objects[0].object, circle);
        let rewind = player.evaluate_delta(0.25).unwrap().unwrap();
        assert!(rewind.snapshot);
        assert_eq!(
            player.scene().objects()[1].content,
            ObjectContentRef::Text(text_handle)
        );
    }

    #[test]
    fn camera_is_derived_from_the_same_evaluated_object_stream() {
        let camera = ObjectId::new(1);
        let mut scene = noon::Scene::new();
        let camera_handle = scene.camera_frame().unwrap();
        let mut player = player(
            &scene,
            vec![(camera, camera_handle)],
            vec![position_track(camera, 1.0)],
            Some(camera),
            20,
        );

        let delta = player.evaluate_delta(0.5).unwrap().unwrap();
        assert_eq!(delta.camera.center, Vec2::new(1.0, 0.0));
        assert!((delta.camera.height - 8.0).abs() < 1.0e-6);
    }
}
