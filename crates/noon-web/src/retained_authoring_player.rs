use noon::RetainedScene;
use noon_core::{
    Camera2DState, FontResourceArena, GeometryResourceArena, ObjectId, TextResourceArena,
};
use noon_runtime::{EvaluationError, RetainedFrameState, RetainedSceneInstance};

use crate::{
    MixedRetainedAuthoringError, MixedRetainedAuthoringScene, RetainedExecutionDeltaEncoder,
    RetainedExecutionDeltaEnvelope, RetainedExecutionTransportError, RetainedResourceBundle,
    RetainedResourceTransportError,
};

/// Deterministic execution owner for one mixed retained scene.
///
/// Compatibility authoring inputs may still be normalized before construction, but
/// runtime evaluation consumes one [`RetainedSceneInstance`]. The resource arenas stay
/// next to it so renderer preparation can resolve text/font/vector resources without
/// putting those payloads on the Python or per-frame execution wire.
#[derive(Clone, Debug)]
pub struct RetainedAuthoringPlayer {
    scene: RetainedScene,
    runtime: RetainedSceneInstance,
    encoder: RetainedExecutionDeltaEncoder,
    resource_bundle: Vec<u8>,
    camera_object: Option<ObjectId>,
    snapshot_sent: bool,
}

impl RetainedAuthoringPlayer {
    pub fn from_json(
        legacy_scene_json: &str,
        retained_document_json: &str,
        session: u32,
    ) -> Result<Self, RetainedAuthoringPlayerError> {
        let mixed =
            MixedRetainedAuthoringScene::from_json(legacy_scene_json, retained_document_json)?;
        Self::new(mixed, session)
    }

    pub fn new(
        mixed: MixedRetainedAuthoringScene,
        session: u32,
    ) -> Result<Self, RetainedAuthoringPlayerError> {
        let camera_object = mixed.camera_object();
        let compiled = mixed.compile()?;
        let scene = mixed.into_scene();
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
        let runtime = RetainedSceneInstance::new(compiled);
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

    pub fn frame(&self) -> &RetainedFrameState {
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
    use noon_core::{
        GeometryRef, ObjectContentRef, Property, RateFunction, SceneDefinition, TextSourceKind,
        TrackTiming, TrackValues, Vec2,
    };

    use crate::{
        RetainedAuthoringDocument, RetainedAuthoringTextObject, RetainedTextAuthoringSpec,
        RetainedTrackAuthoringSpec, RetainedTypstAuthoringSpec, TransportObjectContent,
    };

    use super::*;

    fn text_document(
        source: &str,
        math: bool,
        order: u32,
        object: ObjectId,
    ) -> RetainedAuthoringDocument {
        RetainedAuthoringDocument::new(vec![RetainedAuthoringTextObject {
            object,
            order,
            text: RetainedTypstAuthoringSpec::new(source, math, 48.0).unwrap(),
        }])
        .unwrap()
    }

    fn native_text_document(
        source: &str,
        order: u32,
        object: ObjectId,
    ) -> RetainedAuthoringDocument {
        RetainedAuthoringDocument::new(vec![RetainedAuthoringTextObject {
            object,
            order,
            text: RetainedTextAuthoringSpec::native(
                source,
                noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY,
                48.0,
                -1.0,
            )
            .unwrap(),
        }])
        .unwrap()
    }

    #[test]
    fn first_frame_is_one_geometry_text_geometry_snapshot_with_live_resources() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        let square = legacy.add(GeometryRef::rectangle(0.5, 0.5));
        let text_id = ObjectId::new(1_u64 << 52);
        let mixed = MixedRetainedAuthoringScene::from_parts(
            &legacy,
            text_document("*Hello*", false, 1, text_id),
        )
        .unwrap();
        let mut player = RetainedAuthoringPlayer::new(mixed, 17).unwrap();

        let delta = player.evaluate_delta(0.0).unwrap().unwrap();
        assert!(delta.snapshot);
        assert_eq!(delta.sequence, 0);
        assert_eq!(
            delta
                .objects
                .iter()
                .map(|object| object.object)
                .collect::<Vec<_>>(),
            vec![circle, text_id, square]
        );
        assert!(matches!(
            delta.objects[0].content,
            TransportObjectContent::Geometry { .. }
        ));
        let TransportObjectContent::Text { text } = delta.objects[1].content else {
            panic!("middle retained object must stay text-backed");
        };
        let retained_handle = player.scene().objects()[1].content.text().unwrap();
        assert_eq!(text.id, retained_handle.id.get());
        assert_eq!(text.version, retained_handle.version);
        assert_eq!(
            player.texts().get(retained_handle).unwrap().kind,
            TextSourceKind::Typst
        );
        assert!(!player.fonts().is_empty());
        let bundle = RetainedResourceBundle::decode_binary(player.resource_bundle_bytes()).unwrap();
        assert_eq!(bundle.text_count(), 1);
        assert!(bundle.font_count() >= 1);
    }

    #[test]
    fn retained_native_text_scale_track_evaluates_without_replacing_resource_identity() {
        let legacy = SceneDefinition::new();
        let text_id = ObjectId::new(1_u64 << 52);
        let base_scale = noon::NATIVE_POINT_TO_SCENE_SCALE;
        let track = RetainedTrackAuthoringSpec::new(
            text_id,
            Property::Scale,
            TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::ZERO,
            },
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        );
        let mixed = MixedRetainedAuthoringScene::from_parts_with_tracks(
            &legacy,
            native_text_document("Shrink", 0, text_id),
            vec![track],
        )
        .unwrap();
        let mut player = RetainedAuthoringPlayer::new(mixed, 21).unwrap();
        let text_handle = player.scene().objects()[0].content.text().unwrap();

        let initial = player.evaluate_delta(0.0).unwrap().unwrap();
        assert!(initial.snapshot);
        let midpoint = player.evaluate_delta(0.5).unwrap().unwrap();
        assert!(!midpoint.snapshot);
        assert_eq!(midpoint.objects.len(), 1);
        assert_eq!(midpoint.objects[0].object, text_id);
        assert!((midpoint.objects[0].transform.scale.x - base_scale * 0.5).abs() < 1.0e-6);
        assert!((midpoint.objects[0].transform.scale.y - base_scale * 0.5).abs() < 1.0e-6);
        let TransportObjectContent::Text { text } = midpoint.objects[0].content else {
            panic!("scaled retained Text must stay text-backed");
        };
        assert_eq!(text.id, text_handle.id.get());
        assert_eq!(text.version, text_handle.version);

        let endpoint = player.evaluate_delta(1.0).unwrap().unwrap();
        assert_eq!(endpoint.objects.len(), 1);
        assert_eq!(endpoint.objects[0].transform.scale, Vec2::ZERO);
        assert_eq!(
            player.scene().objects()[0].content,
            ObjectContentRef::Text(text_handle)
        );
    }

    #[test]
    fn forward_evaluation_emits_only_dirty_geometry_and_keeps_text_identity_stable() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        legacy
            .animate_position(
                circle,
                Vec2::ZERO,
                Vec2::new(2.0, 0.0),
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .unwrap();
        let text_id = ObjectId::new(1_u64 << 52);
        let mixed = MixedRetainedAuthoringScene::from_parts(
            &legacy,
            text_document("stable", false, 1, text_id),
        )
        .unwrap();
        let mut player = RetainedAuthoringPlayer::new(mixed, 18).unwrap();
        let text_handle = player.scene().objects()[1].content.text().unwrap();

        player.evaluate_delta(0.0).unwrap().unwrap();
        let delta = player.evaluate_delta(0.5).unwrap().unwrap();
        assert!(!delta.snapshot);
        assert_eq!(delta.objects.len(), 1);
        assert_eq!(delta.objects[0].object, circle);
        assert!(matches!(
            delta.objects[0].content,
            TransportObjectContent::Geometry { .. }
        ));
        assert_eq!(
            player.scene().objects()[1].content,
            ObjectContentRef::Text(text_handle)
        );
    }

    #[test]
    fn backward_evaluation_reissues_snapshot_without_changing_text_resource_handle() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        legacy
            .animate_position(
                circle,
                Vec2::ZERO,
                Vec2::new(2.0, 0.0),
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .unwrap();
        let text_id = ObjectId::new(1_u64 << 52);
        let mixed = MixedRetainedAuthoringScene::from_parts(
            &legacy,
            text_document("seek", false, 1, text_id),
        )
        .unwrap();
        let mut player = RetainedAuthoringPlayer::new(mixed, 19).unwrap();
        let text_handle = player.scene().objects()[1].content.text().unwrap();

        player.evaluate_delta(0.0).unwrap().unwrap();
        player.evaluate_delta(0.75).unwrap().unwrap();
        let rewind = player.evaluate_delta(0.25).unwrap().unwrap();
        assert!(rewind.snapshot);
        assert_eq!(rewind.sequence, 2);
        let TransportObjectContent::Text { text } = rewind.objects[1].content else {
            panic!("rewind must preserve retained text content identity");
        };
        assert_eq!(text.id, text_handle.id.get());
        assert_eq!(text.version, text_handle.version);
    }

    #[test]
    fn retained_player_derives_camera_from_the_same_evaluated_object_stream() {
        let mut legacy = SceneDefinition::new();
        let camera = legacy.add(GeometryRef::rectangle(14.0, 8.0));
        assert!(legacy.set_camera_object(camera));
        legacy
            .animate_position(
                camera,
                Vec2::ZERO,
                Vec2::new(4.0, -2.0),
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .unwrap();
        let mixed = MixedRetainedAuthoringScene::from_parts(
            &legacy,
            RetainedAuthoringDocument::new(Vec::new()).unwrap(),
        )
        .unwrap();
        let mut player = RetainedAuthoringPlayer::new(mixed, 20).unwrap();

        let delta = player.evaluate_delta(0.5).unwrap().unwrap();
        assert_eq!(delta.camera.center, Vec2::new(2.0, -1.0));
        assert!((delta.camera.height - 8.0).abs() < 1.0e-6);
    }
}
