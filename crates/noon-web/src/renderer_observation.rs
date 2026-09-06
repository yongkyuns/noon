use noon::{CallbackRendererDirtyClassification, CommittedCallbackRendererObservation};
use noon_core::{ObjectId, Style, Transform2D};
use noon_render_wgpu::{
    PackedStyle, PackedTransform, RenderPrimitive, RetainedDrawStats, RetainedPreparedObjectKind,
    RetainedPreparedObjectObservation, RetainedPreparedObjectOutcome, RetainedUploadStats,
    UploadWrite,
};
use serde::{Deserialize, Serialize};

use crate::{RetainedExecutionFrameMirror, TransportSlotId};

pub const RENDERER_OBSERVATION_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RendererDirtyClassification {
    All,
    Added,
    Updated,
    Removed,
    Unchanged,
}

impl From<CallbackRendererDirtyClassification> for RendererDirtyClassification {
    fn from(value: CallbackRendererDirtyClassification) -> Self {
        match value {
            CallbackRendererDirtyClassification::All => Self::All,
            CallbackRendererDirtyClassification::Added => Self::Added,
            CallbackRendererDirtyClassification::Updated => Self::Updated,
            CallbackRendererDirtyClassification::Removed => Self::Removed,
            CallbackRendererDirtyClassification::Unchanged => Self::Unchanged,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RendererObservationPublication {
    pub session: u32,
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RendererCommittedObjectObservation {
    pub runtime: String,
    pub callback_sequence: String,
    pub scene_revision: String,
    pub execution_revision: String,
    pub frame_epoch: String,
    pub semantic_slot: u32,
    pub semantic_generation: u32,
    pub object: ObjectId,
    pub frame_index: usize,
    pub time: f64,
    pub transform: Transform2D,
    pub style: Style,
    pub presence: bool,
    pub dirty: RendererDirtyClassification,
}

impl From<CommittedCallbackRendererObservation> for RendererCommittedObjectObservation {
    fn from(value: CommittedCallbackRendererObservation) -> Self {
        let token = value.token();
        let publication = value.publication();
        let target = value.target();
        Self {
            runtime: token.runtime().get().to_string(),
            callback_sequence: token.sequence().get().to_string(),
            scene_revision: publication.scene_revision().get().to_string(),
            execution_revision: publication.execution_revision().get().to_string(),
            frame_epoch: publication.frame_epoch().get().to_string(),
            semantic_slot: target.slot(),
            semantic_generation: target.generation(),
            object: value.execution_object(),
            frame_index: value.frame_index(),
            time: value.time(),
            transform: value.transform(),
            style: value.style(),
            presence: value.presence(),
            dirty: value.dirty().into(),
        }
    }
}

/// One opt-in target pinned to the exact retained transport publication that
/// carries its callback-published runtime row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RendererObservationRequest {
    pub schema_version: u32,
    pub publication: RendererObservationPublication,
    pub slot: TransportSlotId,
    pub committed: RendererCommittedObjectObservation,
}

impl RendererObservationRequest {
    pub fn from_callback_publication(
        transport_session: u32,
        transport_sequence: u64,
        committed: CommittedCallbackRendererObservation,
    ) -> Self {
        Self {
            schema_version: RENDERER_OBSERVATION_VERSION,
            publication: RendererObservationPublication {
                session: transport_session,
                sequence: transport_sequence,
            },
            slot: committed.execution_slot().into(),
            committed: committed.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RendererMirroredObjectObservation {
    pub object: ObjectId,
    pub frame_index: usize,
    pub slot: TransportSlotId,
    pub time: f64,
    pub transform: Transform2D,
    pub style: Style,
    pub presence: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RendererPreparedKind {
    Geometry,
    Text,
    Mixed,
}

impl From<RetainedPreparedObjectKind> for RendererPreparedKind {
    fn from(value: RetainedPreparedObjectKind) -> Self {
        match value {
            RetainedPreparedObjectKind::Geometry => Self::Geometry,
            RetainedPreparedObjectKind::Text => Self::Text,
            RetainedPreparedObjectKind::Mixed => Self::Mixed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RendererPreparedPrimitive {
    Circle,
    Rectangle,
    Line,
    Path,
}

impl RendererPreparedPrimitive {
    const fn upload_buffer(self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::Rectangle => "rectangle",
            Self::Line => "line",
            Self::Path => "path_instance",
        }
    }
}

impl From<RenderPrimitive> for RendererPreparedPrimitive {
    fn from(value: RenderPrimitive) -> Self {
        match value {
            RenderPrimitive::Circle => Self::Circle,
            RenderPrimitive::Rectangle => Self::Rectangle,
            RenderPrimitive::Line => Self::Line,
            RenderPrimitive::Path { .. } | RenderPrimitive::MegaPath { .. } => Self::Path,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RendererPackedTransformObservation {
    pub translation: [f32; 2],
    pub scale: [f32; 2],
    pub rotation: f32,
}

impl From<PackedTransform> for RendererPackedTransformObservation {
    fn from(value: PackedTransform) -> Self {
        Self {
            translation: value.translation,
            scale: value.scale,
            rotation: value.rotation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RendererPackedStyleObservation {
    pub fill: [f32; 4],
    pub stroke: [f32; 4],
    pub stroke_width: f32,
    pub opacity: f32,
    pub fill_enabled: u32,
    pub stroke_enabled: u32,
}

impl From<PackedStyle> for RendererPackedStyleObservation {
    fn from(value: PackedStyle) -> Self {
        Self {
            fill: value.fill,
            stroke: value.stroke,
            stroke_width: value.stroke_width,
            opacity: value.opacity,
            fill_enabled: value.fill_enabled,
            stroke_enabled: value.stroke_enabled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RendererPreparedObjectObservation {
    pub kind: RendererPreparedKind,
    pub primitive: Option<RendererPreparedPrimitive>,
    pub instance_index: Option<usize>,
    pub instance_start: Option<usize>,
    pub instance_end: Option<usize>,
    pub transform: Option<RendererPackedTransformObservation>,
    pub style: Option<RendererPackedStyleObservation>,
    pub instance_dirty: bool,
    pub render_item_start: Option<usize>,
    pub render_item_end: Option<usize>,
    pub render_item_count: usize,
    pub glyph_item_count: usize,
    pub submission_membership: bool,
    pub full_rebuilds: usize,
    pub instances_repacked: usize,
}

impl From<RetainedPreparedObjectObservation> for RendererPreparedObjectObservation {
    fn from(value: RetainedPreparedObjectObservation) -> Self {
        let geometry = value.geometry;
        Self {
            kind: value.kind.into(),
            primitive: geometry.map(|geometry| geometry.primitive.into()),
            instance_index: geometry.map(|geometry| geometry.instance_index),
            instance_start: geometry.map(|geometry| geometry.instance_start),
            instance_end: geometry.map(|geometry| geometry.instance_end),
            transform: geometry.map(|geometry| geometry.transform.into()),
            style: geometry.map(|geometry| geometry.style.into()),
            instance_dirty: geometry.is_some_and(|geometry| geometry.instance_dirty),
            render_item_start: value.render_item_start,
            render_item_end: value.render_item_end,
            render_item_count: value.render_item_count,
            glyph_item_count: value.glyph_item_count,
            submission_membership: value.submission_membership,
            full_rebuilds: value.full_rebuilds,
            instances_repacked: value.instances_repacked,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RendererUploadWriteObservation {
    pub buffer: &'static str,
    pub instance_start: usize,
    pub instance_end: usize,
    pub byte_offset: u64,
    pub byte_length: usize,
    pub payload_hash: u64,
}

impl From<&UploadWrite> for RendererUploadWriteObservation {
    fn from(value: &UploadWrite) -> Self {
        Self {
            buffer: value.buffer,
            instance_start: value.instance_range.start,
            instance_end: value.instance_range.end,
            byte_offset: value.byte_offset,
            byte_length: value.byte_length,
            payload_hash: value.payload_hash,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RendererUploadObservation {
    pub target_write: Option<RendererUploadWriteObservation>,
    pub geometry_bytes_uploaded: usize,
    pub text_bytes_uploaded: usize,
    pub buffer_reallocations: usize,
    pub instance_generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct RendererDrawObservation {
    pub submission_membership: bool,
    pub geometry_draw_calls: usize,
    pub geometry_instances_drawn: usize,
    pub text_draw_calls: usize,
    pub text_instances_drawn: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct RendererPresentationObservation {
    pub surface_status: &'static str,
    pub submit_called: bool,
    pub present_called: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum RendererObservationOutcome {
    Presented {
        schema_version: u32,
        backend: &'static str,
        publication: RendererObservationPublication,
        committed: RendererCommittedObjectObservation,
        mirrored: RendererMirroredObjectObservation,
        prepared: RendererPreparedObjectObservation,
        upload: RendererUploadObservation,
        draw: RendererDrawObservation,
        presentation: RendererPresentationObservation,
    },
    Absent {
        schema_version: u32,
        publication: RendererObservationPublication,
        slot: TransportSlotId,
    },
    StalePublication {
        schema_version: u32,
        requested: RendererObservationPublication,
        applied: Option<RendererObservationPublication>,
    },
    ResourceUnavailable {
        schema_version: u32,
        publication: RendererObservationPublication,
        slot: TransportSlotId,
        resource: &'static str,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResolvedRendererObservationTarget {
    pub request: RendererObservationRequest,
    pub mirrored: RendererMirroredObjectObservation,
}

pub(crate) fn resolve_renderer_observation_target(
    request: RendererObservationRequest,
    mirror: &RetainedExecutionFrameMirror,
) -> Result<ResolvedRendererObservationTarget, RendererObservationOutcome> {
    let applied = mirror
        .session()
        .zip(mirror.applied_sequence())
        .map(|(session, sequence)| RendererObservationPublication { session, sequence });
    if request.schema_version != RENDERER_OBSERVATION_VERSION
        || applied != Some(request.publication)
    {
        return Err(RendererObservationOutcome::StalePublication {
            schema_version: RENDERER_OBSERVATION_VERSION,
            requested: request.publication,
            applied,
        });
    }
    let Some(frame_index) = mirror.frame_index_for_slot(request.slot) else {
        return Err(RendererObservationOutcome::Absent {
            schema_version: RENDERER_OBSERVATION_VERSION,
            publication: request.publication,
            slot: request.slot,
        });
    };
    let Some(frame) = mirror.frame() else {
        return Err(RendererObservationOutcome::Absent {
            schema_version: RENDERER_OBSERVATION_VERSION,
            publication: request.publication,
            slot: request.slot,
        });
    };
    let Some(object) = frame.objects.get(frame_index) else {
        return Err(RendererObservationOutcome::Absent {
            schema_version: RENDERER_OBSERVATION_VERSION,
            publication: request.publication,
            slot: request.slot,
        });
    };
    if object.id != request.committed.object {
        return Err(RendererObservationOutcome::Absent {
            schema_version: RENDERER_OBSERVATION_VERSION,
            publication: request.publication,
            slot: request.slot,
        });
    }
    Ok(ResolvedRendererObservationTarget {
        mirrored: RendererMirroredObjectObservation {
            object: object.id,
            frame_index,
            slot: request.slot,
            time: frame.time,
            transform: object.transform,
            style: object.style,
            presence: frame.is_present(frame_index),
        },
        request,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn finish_renderer_observation(
    target: ResolvedRendererObservationTarget,
    prepared: Result<RetainedPreparedObjectObservation, RetainedPreparedObjectOutcome>,
    geometry_writes: &[UploadWrite],
    upload: RetainedUploadStats,
    draw: RetainedDrawStats,
    instance_generation: u64,
    backend: &'static str,
    surface_status: &'static str,
) -> RendererObservationOutcome {
    if !target.mirrored.presence {
        return RendererObservationOutcome::Absent {
            schema_version: RENDERER_OBSERVATION_VERSION,
            publication: target.request.publication,
            slot: target.request.slot,
        };
    }
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(RetainedPreparedObjectOutcome::Absent) => {
            return RendererObservationOutcome::Absent {
                schema_version: RENDERER_OBSERVATION_VERSION,
                publication: target.request.publication,
                slot: target.request.slot,
            };
        }
        Err(RetainedPreparedObjectOutcome::Unsupported(_)) => {
            return RendererObservationOutcome::ResourceUnavailable {
                schema_version: RENDERER_OBSERVATION_VERSION,
                publication: target.request.publication,
                slot: target.request.slot,
                resource: "unsupported_render_primitive",
            };
        }
        Err(RetainedPreparedObjectOutcome::VisibilityProjectionUnavailable) => {
            return RendererObservationOutcome::ResourceUnavailable {
                schema_version: RENDERER_OBSERVATION_VERSION,
                publication: target.request.publication,
                slot: target.request.slot,
                resource: "visibility_submission_membership",
            };
        }
        Err(RetainedPreparedObjectOutcome::FamilyProjectionUnavailable) => {
            return RendererObservationOutcome::ResourceUnavailable {
                schema_version: RENDERER_OBSERVATION_VERSION,
                publication: target.request.publication,
                slot: target.request.slot,
                resource: "family_projection_mapping",
            };
        }
    };
    if matches!(
        prepared.kind,
        RetainedPreparedObjectKind::Text | RetainedPreparedObjectKind::Mixed
    ) {
        return RendererObservationOutcome::ResourceUnavailable {
            schema_version: RENDERER_OBSERVATION_VERSION,
            publication: target.request.publication,
            slot: target.request.slot,
            resource: "text_upload_ranges",
        };
    }
    let geometry = prepared
        .geometry
        .expect("geometry-only observation has one instance");
    let primitive: RendererPreparedPrimitive = geometry.primitive.into();
    if primitive == RendererPreparedPrimitive::Path {
        return RendererObservationOutcome::ResourceUnavailable {
            schema_version: RENDERER_OBSERVATION_VERSION,
            publication: target.request.publication,
            slot: target.request.slot,
            resource: "path_draw_instance_mapping",
        };
    }
    let target_write = geometry_writes.iter().find(|write| {
        write.buffer == primitive.upload_buffer()
            && write.instance_range.contains(&geometry.instance_index)
    });
    if geometry.instance_dirty && target_write.is_none() {
        return RendererObservationOutcome::ResourceUnavailable {
            schema_version: RENDERER_OBSERVATION_VERSION,
            publication: target.request.publication,
            slot: target.request.slot,
            resource: "geometry_upload_write",
        };
    }
    let submission_membership = prepared.submission_membership;
    RendererObservationOutcome::Presented {
        schema_version: RENDERER_OBSERVATION_VERSION,
        backend,
        publication: target.request.publication,
        committed: target.request.committed,
        mirrored: target.mirrored,
        prepared: prepared.into(),
        upload: RendererUploadObservation {
            target_write: target_write.map(Into::into),
            geometry_bytes_uploaded: upload.geometry.bytes_uploaded,
            text_bytes_uploaded: upload.text.bytes_uploaded,
            buffer_reallocations: upload.geometry.buffer_reallocations,
            instance_generation,
        },
        draw: RendererDrawObservation {
            submission_membership,
            geometry_draw_calls: draw.geometry.draw_calls,
            geometry_instances_drawn: draw.geometry.instances_drawn,
            text_draw_calls: draw.text.draw_calls,
            text_instances_drawn: draw.text.instances_drawn,
        },
        presentation: RendererPresentationObservation {
            surface_status,
            submit_called: true,
            present_called: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{Camera2DState, GeometryRef, ObjectContentRef, Vec2};
    use noon_render_wgpu::{DrawStats, PreparedGeometryObjectObservation, UploadStats};

    use crate::{
        RetainedExecutionDeltaEnvelope, RetainedTransportApplyOutcome,
        RetainedTransportObjectState, TransportObjectContent, RETAINED_EXECUTION_TRANSPORT_CHANNEL,
        RETAINED_EXECUTION_TRANSPORT_VERSION,
    };

    use super::*;

    fn request(sequence: u64, slot: TransportSlotId) -> RendererObservationRequest {
        RendererObservationRequest {
            schema_version: RENDERER_OBSERVATION_VERSION,
            publication: RendererObservationPublication {
                session: 7,
                sequence,
            },
            slot,
            committed: RendererCommittedObjectObservation {
                runtime: "9".into(),
                callback_sequence: "3".into(),
                scene_revision: "1".into(),
                execution_revision: "1".into(),
                frame_epoch: "4".into(),
                semantic_slot: 2,
                semantic_generation: 0,
                object: ObjectId::new(21),
                frame_index: 0,
                time: 1.5,
                transform: Transform2D {
                    translation: Vec2::new(2.0, -1.0),
                    ..Transform2D::IDENTITY
                },
                style: Style::default(),
                presence: true,
                dirty: RendererDirtyClassification::Updated,
            },
        }
    }

    fn mirror() -> RetainedExecutionFrameMirror {
        let mut mirror = RetainedExecutionFrameMirror::default();
        let slot = TransportSlotId {
            slot: 4,
            generation: 2,
        };
        let transform = Transform2D {
            translation: Vec2::new(2.0, -1.0),
            ..Transform2D::IDENTITY
        };
        let delta = RetainedExecutionDeltaEnvelope {
            channel: RETAINED_EXECUTION_TRANSPORT_CHANNEL.into(),
            protocol_version: RETAINED_EXECUTION_TRANSPORT_VERSION,
            session: 7,
            sequence: 0,
            snapshot: true,
            time: 1.5,
            camera: Camera2DState::default(),
            objects: vec![RetainedTransportObjectState {
                slot,
                order: 0,
                object: ObjectId::new(21),
                content: TransportObjectContent::from(&ObjectContentRef::Geometry(
                    GeometryRef::circle(1.0),
                )),
                transform,
                style: Style::default(),
                appearance: 1.0,
                text_bounds: None,
                presence: true,
                reveal: 1.0,
                morph: 0.0,
                render_geometry: None,
                render_transform: None,
                render_geometry_resource: None,
            }],
        };
        assert_eq!(
            mirror.apply(delta).unwrap().0,
            RetainedTransportApplyOutcome::Applied
        );
        mirror
    }

    #[test]
    fn observation_target_requires_exact_publication_and_slot_generation() {
        let mirror = mirror();
        let slot = TransportSlotId {
            slot: 4,
            generation: 2,
        };
        let resolved = resolve_renderer_observation_target(request(0, slot), &mirror).unwrap();
        assert_eq!(resolved.mirrored.slot, slot);
        assert_eq!(resolved.mirrored.frame_index, 0);
        assert_eq!(
            resolved.mirrored.transform.translation,
            Vec2::new(2.0, -1.0)
        );

        assert!(matches!(
            resolve_renderer_observation_target(request(1, slot), &mirror),
            Err(RendererObservationOutcome::StalePublication { .. })
        ));
        let mut foreign_session = request(0, slot);
        foreign_session.publication.session = 8;
        assert!(matches!(
            resolve_renderer_observation_target(foreign_session, &mirror),
            Err(RendererObservationOutcome::StalePublication { .. })
        ));
        assert!(matches!(
            resolve_renderer_observation_target(
                request(
                    0,
                    TransportSlotId {
                        slot: 4,
                        generation: 1,
                    },
                ),
                &mirror,
            ),
            Err(RendererObservationOutcome::Absent { .. })
        ));
    }

    #[test]
    fn presented_observation_preserves_exact_local_upload_and_draw_evidence() {
        let mirror = mirror();
        let slot = TransportSlotId {
            slot: 4,
            generation: 2,
        };
        let target = resolve_renderer_observation_target(request(0, slot), &mirror).unwrap();
        let prepared = RetainedPreparedObjectObservation {
            object: ObjectId::new(21),
            kind: RetainedPreparedObjectKind::Geometry,
            geometry: Some(PreparedGeometryObjectObservation {
                object: ObjectId::new(21),
                primitive: RenderPrimitive::Circle,
                instance_index: 3,
                instance_start: 3,
                instance_end: 4,
                transform: PackedTransform::from(target.mirrored.transform),
                style: PackedStyle::from(target.mirrored.style),
                instance_dirty: true,
                submission_membership: Some(true),
            }),
            render_item_start: Some(7),
            render_item_end: Some(8),
            render_item_count: 1,
            glyph_item_count: 0,
            submission_membership: true,
            full_rebuilds: 0,
            instances_repacked: 1,
        };
        let writes = [UploadWrite {
            buffer: "circle",
            instance_range: 3..4,
            byte_offset: 240,
            byte_length: 80,
            payload_hash: 17,
        }];
        let outcome = finish_renderer_observation(
            target,
            Ok(prepared),
            &writes,
            RetainedUploadStats {
                geometry: UploadStats {
                    bytes_uploaded: 80,
                    buffer_reallocations: 0,
                },
                text: Default::default(),
            },
            RetainedDrawStats {
                geometry: DrawStats {
                    draw_calls: 1,
                    instances_drawn: 1,
                },
                text: Default::default(),
            },
            4,
            "WebGPU",
            "success",
        );

        let RendererObservationOutcome::Presented {
            prepared,
            upload,
            draw,
            presentation,
            ..
        } = outcome
        else {
            panic!("an exact retained geometry observation must be publishable");
        };
        assert_eq!(
            (prepared.instance_start, prepared.instance_end),
            (Some(3), Some(4))
        );
        assert_eq!(
            (prepared.render_item_start, prepared.render_item_end),
            (Some(7), Some(8))
        );
        assert_eq!(prepared.full_rebuilds, 0);
        assert_eq!(prepared.instances_repacked, 1);
        let target_write = upload.target_write.unwrap();
        assert_eq!(
            (target_write.instance_start, target_write.instance_end),
            (3, 4)
        );
        assert_eq!(target_write.byte_offset, 240);
        assert!(draw.submission_membership);
        assert!(presentation.submit_called);
        assert!(presentation.present_called);
    }
}
