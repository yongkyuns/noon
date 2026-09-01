use std::collections::BTreeMap;

use noon_core::{ObjectId, SceneDefinition, Style, Vec2};
use noon_ir::{
    ObjectSpec, OrderedTextObjectSpec, SceneDocument, SceneSpec, SceneSpecError, TextSpec,
};

use crate::{
    materialize_retained_tracks, RetainedAuthoringDocument, RetainedAuthoringTextObject,
    RetainedTextAuthoringSpec, RetainedTextBackendSpec, RetainedTrackAuthoringSpec,
    RetainedTrackMaterializationError,
};

/// Adapt the current geometry document + retained-text sidecar into the canonical
/// `noon_ir::SceneSpec` established by #391.
///
/// The compatibility-only text `order` field is consumed here and disappears into
/// `SceneSpec::objects`; transform/style move onto the ordinary `ObjectSpec` and
/// retained animation tracks join the ordinary `TrackDefinition` domain. Existing
/// producers and the old mixed retained runtime remain supported while #367 migrates
/// consumers incrementally.
pub fn retained_authoring_scene_spec(
    legacy: &SceneDefinition,
    retained: RetainedAuthoringDocument,
    retained_tracks: Vec<RetainedTrackAuthoringSpec>,
) -> Result<SceneSpec, RetainedAuthoringSceneSpecError> {
    retained
        .validate()
        .map_err(RetainedAuthoringSceneSpecError::RetainedDocument)?;

    let scale_factors = retained_scale_factors(&retained.objects);
    let tracks = materialize_retained_tracks(legacy.tracks(), retained_tracks, &scale_factors)?;
    let ordered_text = retained
        .objects
        .into_iter()
        .map(ordered_text_object)
        .collect::<Result<Vec<_>, _>>()?;

    let legacy = SceneDocument::from_scene(legacy);
    let mut spec = SceneSpec::from_legacy_with_ordered_text(&legacy, ordered_text)?;
    spec.tracks = tracks;
    spec.validate()?;
    Ok(spec)
}

#[derive(Debug)]
pub enum RetainedAuthoringSceneSpecError {
    RetainedDocument(String),
    TrackMaterialization(RetainedTrackMaterializationError),
    SceneSpec(SceneSpecError),
}

impl std::fmt::Display for RetainedAuthoringSceneSpecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RetainedDocument(error) => formatter.write_str(error),
            Self::TrackMaterialization(error) => error.fmt(formatter),
            Self::SceneSpec(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetainedAuthoringSceneSpecError {}

impl From<RetainedTrackMaterializationError> for RetainedAuthoringSceneSpecError {
    fn from(value: RetainedTrackMaterializationError) -> Self {
        Self::TrackMaterialization(value)
    }
}

impl From<SceneSpecError> for RetainedAuthoringSceneSpecError {
    fn from(value: SceneSpecError) -> Self {
        Self::SceneSpec(value)
    }
}

fn ordered_text_object(
    object: RetainedAuthoringTextObject,
) -> Result<OrderedTextObjectSpec, SceneSpecError> {
    let RetainedAuthoringTextObject {
        object: id,
        order,
        text,
    } = object;
    let RetainedTextAuthoringSpec {
        source,
        backend,
        font_size,
        transform,
        color,
        opacity,
    } = text;

    let text = match backend {
        RetainedTextBackendSpec::Native {
            font_family,
            line_spacing,
        } => TextSpec::native_plain(source, font_family, font_size, line_spacing),
        RetainedTextBackendSpec::Typst { math: false } => TextSpec::typst(source, font_size),
        RetainedTextBackendSpec::Typst { math: true } => TextSpec::math_typst(source, font_size),
    };
    let mut object = ObjectSpec::text(id, text);
    object.transform = transform;
    object.style = Style {
        fill: Some(color),
        stroke: None,
        stroke_width: 0.0,
        opacity,
        ..Style::default()
    };
    OrderedTextObjectSpec::new(order, object)
}

fn retained_scale_factors(objects: &[RetainedAuthoringTextObject]) -> BTreeMap<ObjectId, Vec2> {
    objects
        .iter()
        .map(|object| {
            let factor = match &object.text.backend {
                RetainedTextBackendSpec::Native { .. } => noon::NATIVE_POINT_TO_SCENE_SCALE,
                RetainedTextBackendSpec::Typst { .. } => {
                    object.text.font_size * noon::SCALE_FACTOR_PER_FONT_POINT
                }
            };
            (object.object, Vec2::new(factor, factor))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use noon_core::{
        Color, GeometryRef, ObjectId, Property, RateFunction, TrackTiming, TrackValues,
        Transform2D, Vec2,
    };
    use noon_ir::{ObjectSpecContent, TextSpecKind, TextSpecOptions};

    use super::*;
    use crate::MixedRetainedAuthoringScene;

    fn native_spec(source: &str) -> RetainedTextAuthoringSpec {
        RetainedTextAuthoringSpec::native(source, "DejaVu Sans Mono", 48.0, 0.5).unwrap()
    }

    fn retained_document(
        object: ObjectId,
        order: u32,
        text: RetainedTextAuthoringSpec,
    ) -> RetainedAuthoringDocument {
        RetainedAuthoringDocument::new(vec![RetainedAuthoringTextObject {
            object,
            order,
            text,
        }])
        .unwrap()
    }

    #[test]
    fn retained_native_text_adapts_into_existing_scene_spec_contract() {
        let mut legacy = SceneDefinition::new();
        let circle = legacy.add(GeometryRef::circle(0.25));
        let square = legacy.add(GeometryRef::rectangle(0.5, 0.5));
        let text_id = ObjectId::new(1_u64 << 52);
        let mut text = native_spec("A\nB");
        text.transform = Transform2D {
            translation: Vec2::new(1.0, -2.0),
            rotation: 0.25,
            scale: Vec2::new(1.5, 0.75),
        };
        text.set_color(Color::rgba(0.2, 0.4, 0.8, 1.0)).unwrap();
        text.set_opacity(0.6).unwrap();

        let spec =
            retained_authoring_scene_spec(&legacy, retained_document(text_id, 1, text), Vec::new())
                .unwrap();

        assert_eq!(
            spec.objects
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>(),
            vec![circle, text_id, square]
        );
        let ObjectSpecContent::Text(text) = &spec.objects[1].content else {
            panic!("middle object must be canonical source-level text");
        };
        assert_eq!(text.kind, TextSpecKind::Plain);
        assert_eq!(text.source, "A\nB");
        assert!(matches!(
            &text.options,
            TextSpecOptions::NativePlain { font_family, line_spacing }
                if font_family == "DejaVu Sans Mono" && *line_spacing == 0.5
        ));
        assert_eq!(spec.objects[1].transform.translation, Vec2::new(1.0, -2.0));
        assert_eq!(spec.objects[1].transform.rotation, 0.25);
        assert_eq!(spec.objects[1].transform.scale, Vec2::new(1.5, 0.75));
        assert_eq!(
            spec.objects[1].style.fill,
            Some(Color::rgba(0.2, 0.4, 0.8, 1.0))
        );
        assert_eq!(spec.objects[1].style.opacity, 0.6);
    }

    #[test]
    fn retained_tracks_join_canonical_track_domain_with_existing_normalization() {
        let legacy = SceneDefinition::new();
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = retained_document(text_id, 0, native_spec("Shrink"));
        let track = RetainedTrackAuthoringSpec::new(
            text_id,
            Property::Scale,
            TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::ZERO,
            },
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        );

        let spec = retained_authoring_scene_spec(&legacy, retained, vec![track]).unwrap();
        assert_eq!(spec.tracks.len(), 1);
        assert_eq!(spec.tracks[0].object, text_id);
        assert_eq!(
            spec.tracks[0].values,
            TrackValues::Vec2 {
                from: Vec2::new(
                    noon::NATIVE_POINT_TO_SCENE_SCALE,
                    noon::NATIVE_POINT_TO_SCENE_SCALE,
                ),
                to: Vec2::ZERO,
            }
        );
    }

    #[test]
    fn scene_spec_adapter_matches_current_split_path_identity_tracks_and_camera() {
        let mut legacy = SceneDefinition::new();
        let camera = legacy.add(GeometryRef::rectangle(14.0, 8.0));
        let geometry = legacy.add(GeometryRef::circle(0.5));
        assert!(legacy.set_camera_object(camera));
        let text_id = ObjectId::new(1_u64 << 52);
        let retained = retained_document(text_id, 1, native_spec("middle"));
        let track = RetainedTrackAuthoringSpec::new(
            text_id,
            Property::Opacity,
            TrackValues::Scalar {
                from: 1.0,
                to: 0.25,
            },
            TrackTiming::new(0.0, 0.5, RateFunction::Linear),
        );

        let canonical =
            retained_authoring_scene_spec(&legacy, retained.clone(), vec![track.clone()]).unwrap();
        let current =
            MixedRetainedAuthoringScene::from_parts_with_tracks(&legacy, retained, vec![track])
                .unwrap();

        assert_eq!(
            canonical
                .objects
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>(),
            current
                .scene()
                .objects()
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(canonical.tracks, current.tracks());
        assert_eq!(canonical.camera_object, current.camera_object());
        assert_eq!(canonical.objects[0].id, camera);
        assert_eq!(canonical.objects[2].id, geometry);
    }

    #[test]
    fn typst_backends_map_without_backend_specific_payloads() {
        let legacy = SceneDefinition::new();
        for (math, kind) in [
            (false, TextSpecKind::Typst),
            (true, TextSpecKind::MathTypst),
        ] {
            let id = ObjectId::new((1_u64 << 52) + u64::from(math));
            let retained = retained_document(
                id,
                0,
                RetainedTextAuthoringSpec::new("x^2", math, 72.0).unwrap(),
            );
            let spec = retained_authoring_scene_spec(&legacy, retained, Vec::new()).unwrap();
            let ObjectSpecContent::Text(text) = &spec.objects[0].content else {
                panic!("retained Typst must remain source-level text");
            };
            assert_eq!(text.kind, kind);
            assert!(matches!(text.options, TextSpecOptions::Default));
            let json = spec.to_json().unwrap();
            for forbidden in ["glyph", "font_bytes", "svg", "atlas"] {
                assert!(!json.contains(forbidden));
            }
        }
    }
}
