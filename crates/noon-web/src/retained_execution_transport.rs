use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use noon_core::{
    Camera2DState, Color, EffectRadius, EffectUpdate, GeometryRef, Glow, GlowSource,
    Inset2DViewState, ObjectContentRef, ObjectId, PublicationContext, Rect, SemanticNodeId,
    SemanticProjection3D, SemanticRotation3D, SemanticSpatialMaterial, SemanticVec3,
    SemanticWorldTransform3D, Style, TextResourceHandle, Transform2D,
};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};
use serde::{Deserialize, Serialize};

use crate::{TransportImageResourceHandle, TransportImageSampling, TransportSlotId};

mod composed;
pub(crate) mod incremental_render_resources;
use composed::PublishedComposedSource;

/// Resource-aware execution channel for the retained geometry/text runtime.
///
/// Object content and family-plan semantic bindings are explicit so geometry and
/// text share the source identity/order stream across a genuine worker boundary.
pub const RETAINED_EXECUTION_TRANSPORT_CHANNEL: &str = "noon.execution.retained";
// V26 carries ordered effects, composed source plans, seeded masks and checked
// HatchPaint at the genuine execution/render worker boundary. Older envelopes reject.
pub const RETAINED_EXECUTION_TRANSPORT_VERSION: u32 = 26;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportGlowSource {
    Painted,
    Silhouette,
}

impl From<GlowSource> for TransportGlowSource {
    fn from(source: GlowSource) -> Self {
        match source {
            GlowSource::Painted => Self::Painted,
            GlowSource::Silhouette => Self::Silhouette,
        }
    }
}

impl From<TransportGlowSource> for GlowSource {
    fn from(source: TransportGlowSource) -> Self {
        match source {
            TransportGlowSource::Painted => Self::Painted,
            TransportGlowSource::Silhouette => Self::Silhouette,
        }
    }
}

/// Wire-only effective treatment for a true cross-worker boundary. This is
/// never a second in-process authored or mutable effect representation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransportEffectState {
    pub attachment: SemanticNodeId,
    #[serde(flatten)]
    pub definition: TransportEffectDefinition,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TransportEffectDefinition {
    Glow {
        color: Color,
        radius: EffectRadius,
        intensity: f64,
        source: TransportGlowSource,
    },
    Blur {
        radius: EffectRadius,
    },
    Shadow {
        color: Color,
        radius: EffectRadius,
        offset: noon_core::EffectOffset,
    },
    NoiseDissolve {
        seed: u32,
        softness: f64,
        progress: f64,
    },
}
impl From<&noon_compile::CompiledEffect> for TransportEffectState {
    fn from(effect: &noon_compile::CompiledEffect) -> Self {
        let definition = match effect.definition {
            noon_core::EffectDefinition::Glow(effect) => TransportEffectDefinition::Glow {
                color: effect.color(),
                radius: effect.radius(),
                intensity: effect.intensity(),
                source: effect.source().into(),
            },
            noon_core::EffectDefinition::Shadow(v) => TransportEffectDefinition::Shadow {
                color: v.color(),
                radius: v.radius(),
                offset: v.offset(),
            },
            noon_core::EffectDefinition::Blur(blur) => TransportEffectDefinition::Blur {
                radius: blur.radius(),
            },
            noon_core::EffectDefinition::NoiseDissolve(value) => {
                TransportEffectDefinition::NoiseDissolve {
                    seed: value.seed(),
                    softness: value.softness(),
                    progress: value.progress(),
                }
            }
        };
        Self {
            attachment: effect.attachment,
            definition,
        }
    }
}
impl TransportEffectState {
    fn to_compiled(self) -> Option<noon_compile::CompiledEffect> {
        let definition = match self.definition {
            TransportEffectDefinition::Glow {
                color,
                radius,
                intensity,
                source,
            } => Glow::new(
                EffectUpdate::default()
                    .color(color)
                    .radius(radius)
                    .intensity(intensity)
                    .source(source.into()),
            )
            .ok()?
            .into(),
            TransportEffectDefinition::Shadow {
                color,
                radius,
                offset,
            } => noon_core::Shadow::new(
                EffectUpdate::default()
                    .color(color)
                    .radius(radius)
                    .offset(offset),
            )
            .ok()?
            .into(),
            TransportEffectDefinition::Blur { radius } => {
                noon_core::Blur::new(EffectUpdate::default().radius(radius))
                    .ok()?
                    .into()
            }
            TransportEffectDefinition::NoiseDissolve {
                seed,
                softness,
                progress,
            } => noon_core::NoiseDissolve::new(seed, softness)
                .ok()?
                .with_progress(progress)
                .ok()?
                .into(),
        };
        Some(noon_compile::CompiledEffect {
            attachment: self.attachment,
            definition,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransportTextResourceHandle {
    pub arena: u64,
    pub id: u64,
    pub version: u64,
}

impl TransportTextResourceHandle {
    pub(crate) const fn from_source_handle(value: TextResourceHandle) -> Self {
        Self {
            arena: value.arena,
            id: value.id.get(),
            version: value.version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransportObjectContent {
    Composed {
        source: crate::TransportComposedSourceHandle,
    },
    Image {
        image: TransportImageResourceHandle,
        sampling: TransportImageSampling,
    },
    Geometry {
        geometry: GeometryRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resource: Option<crate::TransportGeometryResourceHandle>,
    },
    Text {
        text: TransportTextResourceHandle,
    },
}

/// Exact f64 spatial declaration carried only across the Python-worker boundary.
/// It is reconstructed into the ordinary compiled spatial row on receipt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransportSpatialState {
    pub draw_kind: noon_compile::CompiledSpatialDrawKind,
    pub composition_domain: noon_core::SemanticSpatialCompositionDomain,
    pub fixed_orientation_center: Option<[f64; 3]>,
    pub translation: [f64; 3],
    pub rotation_wxyz: [f64; 4],
    pub scale: [f64; 3],
    pub camera_projection: Option<TransportProjection3D>,
    pub material: TransportSpatialMaterial,
    pub point_light: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cairo_path_appearance: Option<Box<noon_compile::CompiledCairoPathAppearance>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransportProjection3D {
    Perspective {
        vertical_fov_radians: f64,
        near: f64,
        far: f64,
    },
    Orthographic {
        height: f64,
        near: f64,
        far: f64,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportSpatialMaterial {
    #[default]
    Unlit,
    PointLit,
    CairoSurface,
    CairoPath,
}

impl TransportSpatialState {
    fn from_compiled(value: &noon_compile::CompiledSpatialState) -> Self {
        let world = value.world;
        let camera_projection = value.camera_projection.map(|projection| match projection {
            SemanticProjection3D::Perspective {
                vertical_fov_radians,
                near,
                far,
            } => TransportProjection3D::Perspective {
                vertical_fov_radians,
                near,
                far,
            },
            SemanticProjection3D::Orthographic { height, near, far } => {
                TransportProjection3D::Orthographic { height, near, far }
            }
        });
        Self {
            draw_kind: value.draw_kind,
            composition_domain: value.composition_domain,
            fixed_orientation_center: value.fixed_orientation_center.map(|v| [v.x, v.y, v.z]),
            translation: [
                world.translation.x,
                world.translation.y,
                world.translation.z,
            ],
            rotation_wxyz: world.rotation.components(),
            scale: [world.scale.x, world.scale.y, world.scale.z],
            camera_projection,
            material: match value.material {
                SemanticSpatialMaterial::Unlit => TransportSpatialMaterial::Unlit,
                SemanticSpatialMaterial::PointLit => TransportSpatialMaterial::PointLit,
                SemanticSpatialMaterial::CairoSurface => TransportSpatialMaterial::CairoSurface,
                SemanticSpatialMaterial::CairoPath => TransportSpatialMaterial::CairoPath,
            },
            point_light: value.point_light,
            cairo_path_appearance: value.cairo_path_appearance.clone(),
        }
    }

    fn is_valid(&self) -> bool {
        use noon_compile::CompiledSpatialDrawKind as Draw;
        use noon_core::SemanticSpatialCompositionDomain as Domain;
        if (self.material == TransportSpatialMaterial::CairoPath)
            != self.cairo_path_appearance.is_some()
            || self
                .cairo_path_appearance
                .as_deref()
                .is_some_and(|appearance| {
                    self.draw_kind != Draw::Planar
                        || self.composition_domain != Domain::World
                        || !appearance.is_valid()
                        || (appearance.gradient_direction.is_some()
                            && appearance.world_family_bounds.is_none())
                })
            || (self.camera_projection.is_some() || self.point_light)
                && self.composition_domain != Domain::World
            || self.draw_kind == Draw::Mesh && self.composition_domain != Domain::World
            || self.material == TransportSpatialMaterial::PointLit && self.draw_kind != Draw::Mesh
            || self.material == TransportSpatialMaterial::CairoSurface
                && self.draw_kind != Draw::Mesh
            || self.material == TransportSpatialMaterial::PointLit
                && self.scale.into_iter().any(|component| component == 0.0)
            || self.camera_projection.is_some() && self.point_light
            || (self.camera_projection.is_some() || self.point_light)
                && (self.draw_kind != Draw::Planar
                    || self.scale != [1.0; 3]
                    || self.material != TransportSpatialMaterial::Unlit)
            || (self.composition_domain == Domain::FixedOrientation)
                != self.fixed_orientation_center.is_some()
            || self
                .fixed_orientation_center
                .is_some_and(|v| !v.into_iter().all(f64::is_finite))
        {
            return false;
        }
        let [w, x, y, z] = self.rotation_wxyz;
        let Some(rotation) = SemanticRotation3D::from_validated_components(w, x, y, z) else {
            return false;
        };
        SemanticWorldTransform3D::new(
            SemanticVec3::new(
                self.translation[0],
                self.translation[1],
                self.translation[2],
            ),
            rotation,
            SemanticVec3::new(self.scale[0], self.scale[1], self.scale[2]),
        )
        .is_some()
            && self
                .camera_projection
                .is_none_or(|projection| match projection {
                    TransportProjection3D::Perspective {
                        vertical_fov_radians,
                        near,
                        far,
                    } => SemanticProjection3D::Perspective {
                        vertical_fov_radians,
                        near,
                        far,
                    }
                    .is_valid(),
                    TransportProjection3D::Orthographic { height, near, far } => {
                        SemanticProjection3D::Orthographic { height, near, far }.is_valid()
                    }
                })
    }

    fn to_compiled(&self) -> Option<noon_compile::CompiledSpatialState> {
        if !self.is_valid() {
            return None;
        }
        let [w, x, y, z] = self.rotation_wxyz;
        let rotation = SemanticRotation3D::from_validated_components(w, x, y, z)?;
        let world = SemanticWorldTransform3D::new(
            SemanticVec3::new(
                self.translation[0],
                self.translation[1],
                self.translation[2],
            ),
            rotation,
            SemanticVec3::new(self.scale[0], self.scale[1], self.scale[2]),
        )?;
        let camera_projection = self.camera_projection.map(|projection| match projection {
            TransportProjection3D::Perspective {
                vertical_fov_radians,
                near,
                far,
            } => SemanticProjection3D::Perspective {
                vertical_fov_radians,
                near,
                far,
            },
            TransportProjection3D::Orthographic { height, near, far } => {
                SemanticProjection3D::Orthographic { height, near, far }
            }
        });
        Some(noon_compile::CompiledSpatialState {
            draw_kind: self.draw_kind,
            composition_domain: self.composition_domain,
            spatial_anchor_family: None,
            fixed_orientation_center: self
                .fixed_orientation_center
                .map(|v| SemanticVec3::new(v[0], v[1], v[2])),
            world,
            camera_projection,
            camera_profile: None,
            camera_motions: None,
            material: match self.material {
                TransportSpatialMaterial::Unlit => SemanticSpatialMaterial::Unlit,
                TransportSpatialMaterial::PointLit => SemanticSpatialMaterial::PointLit,
                TransportSpatialMaterial::CairoSurface => SemanticSpatialMaterial::CairoSurface,
                TransportSpatialMaterial::CairoPath => SemanticSpatialMaterial::CairoPath,
            },
            point_light: self.point_light,
            cairo_path_appearance: self.cairo_path_appearance.clone(),
        })
    }
}

impl TryFrom<&ObjectContentRef> for TransportObjectContent {
    type Error = RetainedExecutionTransportError;
    fn try_from(value: &ObjectContentRef) -> Result<Self, Self::Error> {
        Ok(match value {
            // Bare conversion has no session/sequence-qualified resource key.
            // Only the resource-aware encoder can bind a composed source.
            ObjectContentRef::Composed(_) => {
                return Err(RetainedExecutionTransportError::UnsupportedComposedSource)
            }
            ObjectContentRef::Image(image) => Self::Image {
                image: image.resource().into(),
                sampling: image.sampling().into(),
            },
            ObjectContentRef::Geometry(geometry) => Self::Geometry {
                geometry: geometry.clone(),
                resource: None,
            },
            ObjectContentRef::Text(text) => Self::Text {
                text: TransportTextResourceHandle::from_source_handle(*text),
            },
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedTransportObjectState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_paint: Option<Arc<noon_core::SemanticPaint>>,
    pub draw_enabled: bool,
    pub slot: TransportSlotId,
    pub order: u32,
    pub object: ObjectId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<TransportEffectState>,
    /// Effective layer of this published row, not its painter rank.
    pub z_index: f64,
    pub content: TransportObjectContent,
    pub transform: Transform2D,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spatial: Option<TransportSpatialState>,
    pub style: Style,
    pub appearance: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_bounds: Option<Rect>,
    pub presence: bool,
    pub reveal: f32,
    pub morph: f32,
    pub render_geometry: Option<GeometryRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_transform: Option<Transform2D>,
    /// Generation-qualified arena reference, encoded as a decimal string so JS
    /// metadata parsing cannot round a high generation before Rust receives it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "render_geometry_id_json"
    )]
    pub render_geometry_resource: Option<u64>,
}

/// Existing-row update for hot fields in dense, nonstructural frames.
/// The slot generation and object identity must match the installed row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedTransportObjectPatch {
    pub slot: TransportSlotId,
    pub object: ObjectId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Style>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub morph: Option<f32>,
    /// Move an already active derived frame without repeating its linear part.
    /// Absence leaves it unchanged; other override changes require a complete row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_translation: Option<noon_core::Vec2>,
}

mod render_geometry_id_json {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(id: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error> {
        id.map(|id| id.to_string()).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<u64>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|id| id.parse().map_err(serde::de::Error::custom))
            .transpose()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainedPainterOrderDelta {
    pub start: u32,
    pub end: u32,
    pub slots: Vec<TransportSlotId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedExecutionDeltaEnvelope {
    pub channel: String,
    pub protocol_version: u32,
    pub session: u32,
    pub sequence: u64,
    /// Exact runtime publication authority associated with these effective rows.
    pub publication_context: PublicationContext,
    pub snapshot: bool,
    pub time: f64,
    #[serde(default)]
    pub camera: Camera2DState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inset_2d_views: Vec<Inset2DViewState>,
    pub objects: Vec<RetainedTransportObjectState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_patches: Vec<RetainedTransportObjectPatch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_slots: Vec<TransportSlotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub painter_order: Option<RetainedPainterOrderDelta>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedTransportApplyOutcome {
    Applied,
    DroppedStale,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedExecutionTransportError {
    UnsupportedComposedSource,
    UnknownComposedSource(crate::TransportComposedSourceHandle),
    InvalidComposedSourceHandle(TransportSlotId),
    InvalidComposedSource(noon_compile::CompiledComposedSourceError),
    UnknownImageResource(TransportImageResourceHandle),
    ImageRenderGeometry(TransportSlotId),
    InvalidChannel(String),
    UnsupportedVersion(u32),
    InvalidTime(f64),
    SequenceExhausted,
    SessionRequiresSnapshot {
        session: u32,
        sequence: u64,
    },
    SequenceGap {
        expected: u64,
        actual: u64,
    },
    IncrementalBeforeSnapshot,
    StructuralChangeRequiresSnapshot,
    FrameShapeMismatch,
    InvalidObjectIndex(usize),
    InvalidEffectState(TransportSlotId),
    InvalidSpatialState(TransportSlotId),
    MultipleCamera3D,
    InvalidZIndex(TransportSlotId),
    InvalidOrder(u32),
    DuplicateSlot(TransportSlotId),
    DuplicateObject(ObjectId),
    UnknownSlot(TransportSlotId),
    InvalidObjectPatch(TransportSlotId),
    SlotIdentityChanged(TransportSlotId),
    TextRenderGeometry(TransportSlotId),
    InvalidRenderGeometryResource(u64),
    AmbiguousRenderGeometry(TransportSlotId),
    MissingCompiledRenderResource(TransportSlotId),
    InvalidRenderTransform(TransportSlotId),
    InvalidInset2DView(ObjectId),
    StalePublicationContext {
        received: PublicationContext,
        installed: PublicationContext,
    },
    UnknownTextResource(TransportTextResourceHandle),
    UnknownGeometryResource(crate::TransportGeometryResourceHandle),
    InvalidGeometryResourceReference(TransportSlotId),
}

impl std::fmt::Display for RetainedExecutionTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownComposedSource(handle) => write!(formatter, "unknown immutable composed source {handle:?}"),
            Self::InvalidComposedSourceHandle(slot) => write!(formatter, "invalid composed source provenance or owner profile at {slot:?}"),
            Self::InvalidComposedSource(error) => error.fmt(formatter),
            Self::UnsupportedComposedSource => write!(formatter,"composed source conversion requires a publication resource key"),
            Self::InvalidEffectState(slot) => write!(
                formatter,
                "retained worker slot {}:{} contains invalid or unsupported effect stack",
                slot.slot, slot.generation
            ),
            Self::InvalidSpatialState(slot) => write!(
                formatter,
                "retained slot {}:{} has invalid spatial state",
                slot.slot, slot.generation
            ),
            Self::MultipleCamera3D => formatter.write_str(
                "retained execution snapshot contains more than one effective Camera3D row",
            ),
            Self::StalePublicationContext {
                received,
                installed,
            } => write!(
                formatter,
                "retained publication context {received:?} precedes installed context {installed:?}",
            ),
            Self::UnknownImageResource(handle) => {
                write!(formatter, "unknown raster image resource {handle:?}")
            }
            Self::ImageRenderGeometry(slot) => write!(
                formatter,
                "image slot {slot:?} cannot carry vector reveal, morph, or render geometry"
            ),
            Self::InvalidChannel(channel) => {
                write!(
                    formatter,
                    "invalid retained execution transport channel {channel:?}"
                )
            }
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "unsupported retained execution transport version {version}"
            ),
            Self::InvalidTime(time) => write!(formatter, "invalid retained frame time {time}"),
            Self::SequenceExhausted => {
                formatter.write_str("retained execution transport sequence exhausted")
            }
            Self::SessionRequiresSnapshot { session, sequence } => write!(
                formatter,
                "retained execution session {session} must begin with snapshot sequence 0, got {sequence}"
            ),
            Self::SequenceGap { expected, actual } => write!(
                formatter,
                "retained execution delta sequence gap: expected {expected}, got {actual}"
            ),
            Self::IncrementalBeforeSnapshot => formatter
                .write_str("retained execution transport requires a snapshot before incrementals"),
            Self::StructuralChangeRequiresSnapshot => formatter
                .write_str("retained execution structural changes require a complete snapshot"),
            Self::FrameShapeMismatch => formatter
                .write_str("retained frame object/property arrays have inconsistent lengths"),
            Self::InvalidObjectIndex(index) => {
                write!(formatter, "invalid retained frame object index {index}")
            }
            Self::InvalidZIndex(slot) => write!(
                formatter,
                "retained slot {}:{} has a non-finite z-index",
                slot.slot, slot.generation
            ),
            Self::InvalidOrder(order) => {
                write!(formatter, "invalid retained execution render order {order}")
            }
            Self::DuplicateSlot(slot) => write!(
                formatter,
                "duplicate retained execution slot {}:{}",
                slot.slot, slot.generation
            ),
            Self::DuplicateObject(object) => write!(
                formatter,
                "duplicate retained execution object {}",
                object.get()
            ),
            Self::UnknownSlot(slot) => write!(
                formatter,
                "unknown retained execution slot {}:{}",
                slot.slot, slot.generation
            ),
            Self::InvalidObjectPatch(slot) => write!(
                formatter,
                "invalid retained execution patch for slot {}:{}",
                slot.slot, slot.generation
            ),
            Self::SlotIdentityChanged(slot) => write!(
                formatter,
                "retained execution slot {}:{} changed object identity without a snapshot",
                slot.slot, slot.generation
            ),
            Self::TextRenderGeometry(slot) => write!(
                formatter,
                "retained text slot {}:{} cannot carry transient render geometry",
                slot.slot, slot.generation
            ),
            Self::InvalidRenderGeometryResource(index) => write!(
                formatter,
                "unknown retained render geometry resource {index} in this session"
            ),
            Self::AmbiguousRenderGeometry(slot) => write!(
                formatter,
                "retained slot {}:{} carries both inline and resource geometry",
                slot.slot, slot.generation
            ),
            Self::MissingCompiledRenderResource(slot) => write!(
                formatter,
                "retained slot {}:{} has an unregistered compiled render geometry",
                slot.slot, slot.generation
            ),
            Self::InvalidRenderTransform(slot) => write!(
                formatter,
                "retained slot {}:{} has an invalid render transform",
                slot.slot, slot.generation
            ),
            Self::InvalidInset2DView(display) => write!(
                formatter,
                "retained inset view for display {} is invalid or references a missing object",
                display.get()
            ),
            Self::UnknownTextResource(handle) => write!(
                formatter,
                "unknown retained transport text resource {}@{}",
                handle.id, handle.version
            ),
            Self::UnknownGeometryResource(handle) => write!(
                formatter,
                "unknown retained geometry resource {}@{}",
                handle.id, handle.version
            ),
            Self::InvalidGeometryResourceReference(slot) => write!(
                formatter,
                "retained slot {}:{} has an invalid external geometry resource reference",
                slot.slot, slot.generation
            ),
        }
    }
}

impl std::error::Error for RetainedExecutionTransportError {}

#[derive(Clone, Debug)]
pub struct RetainedExecutionDeltaEncoder {
    session: u32,
    next_sequence: u64,
    initialized: bool,
    snapshot_orders: Vec<Option<u32>>,
    render_geometries: Option<Arc<[Arc<GeometryRef>]>>,
    render_geometry_indices: Option<HashMap<usize, u64>>,
    published_render_geometries: Vec<Option<Arc<GeometryRef>>>,
    published_composed_sources: Vec<Option<PublishedComposedSource>>,
}

impl RetainedExecutionDeltaEncoder {
    pub(crate) const fn session(&self) -> u32 {
        self.session
    }

    pub const fn new(session: u32) -> Self {
        Self {
            session,
            next_sequence: 0,
            initialized: false,
            snapshot_orders: Vec::new(),
            render_geometries: None,
            render_geometry_indices: None,
            published_render_geometries: Vec::new(),
            published_composed_sources: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_render_geometries(
        session: u32,
        geometries: Arc<[Arc<GeometryRef>]>,
    ) -> Self {
        let indices = geometries
            .iter()
            .enumerate()
            .map(|(index, geometry)| {
                (
                    Arc::as_ptr(geometry) as usize,
                    u64::try_from(index).expect("compiled geometry table exceeds u64"),
                )
            })
            .collect();
        Self {
            render_geometries: Some(geometries),
            render_geometry_indices: Some(indices),
            ..Self::new(session)
        }
    }

    fn transport_object(
        &self,
        frame: &FrameState,
        index: usize,
        reuse_inline_geometry: bool,
    ) -> Result<RetainedTransportObjectState, RetainedExecutionTransportError> {
        let resource = frame
            .render_geometries
            .get(index)
            .and_then(Option::as_ref)
            .and_then(|geometry| {
                self.render_geometry_indices
                    .as_ref()
                    .and_then(|indices| indices.get(&(Arc::as_ptr(geometry) as usize)))
                    .copied()
            });
        debug_assert!(resource.is_none_or(|index| {
            self.render_geometries
                .as_ref()
                .is_some_and(|items| (index as usize) < items.len())
        }));
        let mut object = transport_object(
            frame,
            index,
            resource,
            self.composed_source_handle(frame, index),
        )?;
        let render_geometry = render_geometry_identity(frame, index);
        if reuse_inline_geometry
            && resource.is_none()
            && object.render_transform.is_some()
            && object.render_geometry.is_some()
            && self
                .published_render_geometries
                .get(index)
                .and_then(Option::as_ref)
                .zip(render_geometry.as_ref())
                .is_some_and(|(published, current)| Arc::ptr_eq(published, current))
        {
            object.render_geometry = None;
        }
        if self.render_geometry_indices.is_some()
            && object.render_transform.is_some()
            && object.render_geometry_resource.is_none()
        {
            return Err(
                RetainedExecutionTransportError::MissingCompiledRenderResource(object.slot),
            );
        }
        Ok(object)
    }

    pub fn encode_snapshot_with_context(
        &mut self,
        frame: &FrameState,
        camera: Camera2DState,
        publication_context: PublicationContext,
    ) -> Result<RetainedExecutionDeltaEnvelope, RetainedExecutionTransportError> {
        self.encode_snapshot_indices_with_context(
            frame,
            camera,
            publication_context,
            0..frame.objects.len(),
        )
    }

    pub fn encode_snapshot_indices_with_context(
        &mut self,
        frame: &FrameState,
        camera: Camera2DState,
        publication_context: PublicationContext,
        indices: impl IntoIterator<Item = usize>,
    ) -> Result<RetainedExecutionDeltaEnvelope, RetainedExecutionTransportError> {
        validate_frame_shape(frame)?;
        validate_time(frame.time)?;
        let indices = indices.into_iter().collect::<Vec<_>>();
        let mut orders = vec![None; frame.objects.len()];
        let mut render_geometries = vec![None; frame.objects.len()];
        let mut composed_sources = vec![None; frame.objects.len()];
        let objects = indices
            .into_iter()
            .enumerate()
            .map(|(order, index)| {
                let mut object = self.transport_object(frame, index, false)?;
                if orders[index].is_some() {
                    return Err(RetainedExecutionTransportError::DuplicateSlot(object.slot));
                }
                object.order = u32::try_from(order)
                    .map_err(|_| RetainedExecutionTransportError::InvalidObjectIndex(index))?;
                orders[index] = Some(object.order);
                render_geometries[index] = render_geometry_identity(frame, index);
                composed_sources[index] = composed::published_source(frame, index, &object.content);
                Ok(object)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let sequence = self.take_sequence()?;
        self.initialized = true;
        self.snapshot_orders = orders;
        self.published_render_geometries = render_geometries;
        self.published_composed_sources = composed_sources;
        Ok(RetainedExecutionDeltaEnvelope {
            channel: RETAINED_EXECUTION_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: RETAINED_EXECUTION_TRANSPORT_VERSION,
            session: self.session,
            sequence,
            publication_context,
            snapshot: true,
            time: frame.time,
            camera,
            inset_2d_views: Vec::new(),
            objects,
            object_patches: Vec::new(),
            removed_slots: Vec::new(),
            painter_order: None,
        })
    }

    pub fn encode_incremental_with_context(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        camera: Camera2DState,
        publication_context: PublicationContext,
    ) -> Result<Option<RetainedExecutionDeltaEnvelope>, RetainedExecutionTransportError> {
        self.encode_incremental_inner(frame, changes, camera, publication_context, None)
    }

    pub fn encode_incremental_with_painter_order_and_context(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        camera: Camera2DState,
        publication_context: PublicationContext,
        painter_order: &[u32],
    ) -> Result<Option<RetainedExecutionDeltaEnvelope>, RetainedExecutionTransportError> {
        self.encode_incremental_inner(
            frame,
            changes,
            camera,
            publication_context,
            Some(painter_order),
        )
    }

    fn encode_incremental_inner(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        camera: Camera2DState,
        publication_context: PublicationContext,
        painter_order: Option<&[u32]>,
    ) -> Result<Option<RetainedExecutionDeltaEnvelope>, RetainedExecutionTransportError> {
        validate_frame_shape(frame)?;
        validate_time(frame.time)?;
        if !self.initialized {
            return Err(RetainedExecutionTransportError::IncrementalBeforeSnapshot);
        }
        if changes.is_structural() && painter_order.is_none() {
            return Err(RetainedExecutionTransportError::StructuralChangeRequiresSnapshot);
        }
        if changes.is_all() {
            let indices = self
                .snapshot_orders
                .iter()
                .enumerate()
                .filter_map(|(index, order)| order.map(|_| index))
                .collect::<Vec<_>>();
            return self
                .encode_snapshot_indices_with_context(frame, camera, publication_context, indices)
                .map(Some);
        }
        if changes.is_empty() {
            return Ok(None);
        }
        let final_live_indices = painter_order
            .map(|order| {
                order
                    .iter()
                    .map(|&index| index as usize)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let removed_rows = changes
            .removed_indices()
            .iter()
            .copied()
            .map(|index| transport_slot(frame, index).map(|slot| (index, slot)))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|(index, _)| !final_live_indices.contains(index))
            .collect::<Vec<_>>();
        let excluded_indices = removed_rows
            .iter()
            .map(|(index, _)| *index)
            .collect::<HashSet<_>>();
        let removed_rows = removed_rows
            .into_iter()
            .filter(|(index, _)| {
                self.snapshot_orders
                    .get(*index)
                    .copied()
                    .flatten()
                    .is_some()
            })
            .collect::<Vec<_>>();
        let removed_slots = removed_rows
            .iter()
            .map(|(_, slot)| *slot)
            .collect::<Vec<_>>();
        let (order_updates, painter_order) = if let Some(order) = painter_order {
            let delta = changes
                .painter_order_range()
                .map(|range| {
                    let end = range.end.min(order.len());
                    let mut seen = HashSet::with_capacity(end.saturating_sub(range.start));
                    let mut updates = Vec::with_capacity(end.saturating_sub(range.start));
                    let slots = order[range.start.min(end)..end]
                        .iter()
                        .enumerate()
                        .map(|(offset, &index)| {
                            let index = index as usize;
                            if !seen.insert(index) {
                                return Err(RetainedExecutionTransportError::InvalidObjectIndex(
                                    index,
                                ));
                            }
                            let rank = u32::try_from(range.start + offset).map_err(|_| {
                                RetainedExecutionTransportError::InvalidObjectIndex(index)
                            })?;
                            updates.push((index, rank));
                            transport_slot(frame, index)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok::<_, RetainedExecutionTransportError>((
                        updates,
                        RetainedPainterOrderDelta {
                            start: u32::try_from(range.start).map_err(|_| {
                                RetainedExecutionTransportError::InvalidObjectIndex(range.start)
                            })?,
                            end: u32::try_from(range.end).map_err(|_| {
                                RetainedExecutionTransportError::InvalidObjectIndex(range.end)
                            })?,
                            slots,
                        },
                    ))
                })
                .transpose()?;
            match delta {
                Some((updates, delta)) => (updates, Some(delta)),
                None => (Vec::new(), None),
            }
        } else {
            (Vec::new(), None)
        };
        let order_update_map = order_updates.iter().copied().collect::<HashMap<_, _>>();
        let objects = changes
            .object_indices()
            .iter()
            .copied()
            .filter(|index| !excluded_indices.contains(index))
            .map(|index| {
                let mut object = self.transport_object(frame, index, true)?;
                object.order = order_update_map
                    .get(&index)
                    .copied()
                    .or_else(|| self.snapshot_orders.get(index).copied().flatten())
                    .ok_or(RetainedExecutionTransportError::UnknownSlot(object.slot))?;
                Ok(object)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let sequence = self.take_sequence()?;
        self.commit_composed_sources(frame, &objects, &excluded_indices);
        for (index, _) in &removed_rows {
            if let Some(order) = self.snapshot_orders.get_mut(*index) {
                *order = None;
            }
        }
        if self.snapshot_orders.len() < frame.objects.len() {
            self.snapshot_orders.resize(frame.objects.len(), None);
        }
        for (index, rank) in order_updates {
            self.snapshot_orders[index] = Some(rank);
        }
        if self.published_render_geometries.len() < frame.objects.len() {
            self.published_render_geometries
                .resize(frame.objects.len(), None);
        }
        for &index in changes.object_indices() {
            if !excluded_indices.contains(&index) {
                self.published_render_geometries[index] = render_geometry_identity(frame, index);
            }
        }
        for &index in &excluded_indices {
            if let Some(geometry) = self.published_render_geometries.get_mut(index) {
                *geometry = None;
            }
        }
        Ok(Some(RetainedExecutionDeltaEnvelope {
            channel: RETAINED_EXECUTION_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: RETAINED_EXECUTION_TRANSPORT_VERSION,
            session: self.session,
            sequence,
            publication_context,
            snapshot: false,
            time: frame.time,
            camera,
            inset_2d_views: Vec::new(),
            objects,
            object_patches: Vec::new(),
            removed_slots,
            painter_order,
        }))
    }

    fn take_sequence(&mut self) -> Result<u64, RetainedExecutionTransportError> {
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(RetainedExecutionTransportError::SequenceExhausted)?;
        Ok(sequence)
    }
}

#[derive(Clone, Debug, Default)]
pub struct RetainedExecutionFrameMirror {
    session: Option<u32>,
    next_sequence: u64,
    slots: Vec<TransportSlotId>,
    slot_indices: HashMap<TransportSlotId, usize>,
    object_indices: HashMap<ObjectId, usize>,
    render_geometries: Vec<crate::retained_resource_transport::RenderGeometrySlot>,
    resource_session: Option<u32>,
    image_handles: HashMap<TransportImageResourceHandle, noon_core::RasterImageContentRef>,
    text_handles: HashMap<TransportTextResourceHandle, TextResourceHandle>,
    geometry_handles:
        HashMap<crate::TransportGeometryResourceHandle, noon_core::GeometryResourceHandle>,
    composed_sources: HashMap<crate::TransportComposedSourceHandle, Arc<noon_core::ComposedSource>>,
    composed_dependencies: noon_compile::ComposedDependencies,
    composed_handles: HashMap<ObjectId, crate::TransportComposedSourceHandle>,
    #[cfg(test)]
    last_composed_validation: noon_compile::ComposedValidationStats,
    camera: Camera2DState,
    publication_context: PublicationContext,
    inset_2d_views: Vec<Inset2DViewState>,
    frame: Option<FrameState>,
    painter_order: Vec<u32>,
    painter_ranks: Vec<Option<u32>>,
}

struct PreparedPainterOrder {
    range: std::ops::Range<usize>,
    old_end: usize,
    segment: Vec<u32>,
}

impl noon_runtime::ObjectIndexLookup for RetainedExecutionFrameMirror {
    fn object_index(&self, object: ObjectId) -> Option<usize> {
        self.object_indices.get(&object).copied()
    }
}

impl RetainedExecutionFrameMirror {
    pub(crate) fn extend_installed_image_handles(
        &mut self,
        additions: &HashMap<TransportImageResourceHandle, noon_core::RasterImageContentRef>,
    ) {
        self.image_handles
            .extend(additions.iter().map(|(&key, &value)| (key, value)));
    }
    pub(crate) fn remove_installed_image_handles<'a>(
        &mut self,
        handles: impl IntoIterator<Item = &'a TransportImageResourceHandle>,
    ) {
        for handle in handles {
            self.image_handles.remove(handle);
        }
    }
    pub(crate) fn extend_installed_geometry_handles(
        &mut self,
        additions: &HashMap<
            crate::TransportGeometryResourceHandle,
            noon_core::GeometryResourceHandle,
        >,
    ) {
        self.geometry_handles
            .extend(additions.iter().map(|(&key, &value)| (key, value)));
    }
    pub(crate) fn remove_installed_geometry_handles<'a>(
        &mut self,
        handles: impl IntoIterator<Item = &'a crate::TransportGeometryResourceHandle>,
    ) {
        for handle in handles {
            self.geometry_handles.remove(handle);
        }
    }
    pub(crate) fn with_installed_resources(
        session: Option<u32>,
        geometries: &[crate::retained_resource_transport::RenderGeometrySlot],
        text_handles: HashMap<TransportTextResourceHandle, TextResourceHandle>,
    ) -> Self {
        Self {
            resource_session: session,
            render_geometries: geometries.to_vec(),
            text_handles,
            ..Self::default()
        }
    }

    pub(crate) fn extend_installed_text_handles(
        &mut self,
        additions: &HashMap<TransportTextResourceHandle, TextResourceHandle>,
    ) {
        self.text_handles.extend(
            additions
                .iter()
                .map(|(&transport, &local)| (transport, local)),
        );
    }

    pub(crate) fn remove_installed_text_handles<'a>(
        &mut self,
        handles: impl IntoIterator<Item = &'a TransportTextResourceHandle>,
    ) {
        for handle in handles {
            self.text_handles.remove(handle);
        }
    }

    pub(crate) fn resolve_fill_paint(
        &self,
        paint: Option<&Arc<noon_core::SemanticPaint>>,
    ) -> Result<Option<Arc<noon_core::SemanticPaint>>, RetainedExecutionTransportError> {
        let Some(paint) = paint else {
            return Ok(None);
        };
        let Some(image) = paint.as_image() else {
            return Ok(Some(paint.clone()));
        };
        let transport = image.resource().into();
        let local = self.image_handles.get(&transport).ok_or(
            RetainedExecutionTransportError::UnknownImageResource(transport),
        )?;
        Ok(Some(Arc::new(noon_core::SemanticPaint::Image(
            std::sync::Arc::new(noon_core::ImagePaint::new(
                local.resource(),
                image.mapping(),
                image.sampling(),
            )),
        ))))
    }

    fn resolve_content(
        &self,
        content: &TransportObjectContent,
    ) -> Result<ObjectContentRef, RetainedExecutionTransportError> {
        match content {
            TransportObjectContent::Composed { source } => self
                .composed_sources
                .get(source)
                .cloned()
                .map(ObjectContentRef::Composed)
                .ok_or(RetainedExecutionTransportError::UnknownComposedSource(
                    *source,
                )),
            TransportObjectContent::Image { image, sampling } => self
                .image_handles
                .get(image)
                .map(|content| ObjectContentRef::Image(content.with_sampling((*sampling).into())))
                .ok_or(RetainedExecutionTransportError::UnknownImageResource(
                    *image,
                )),
            TransportObjectContent::Geometry { geometry, resource } => {
                if let Some(resource) = resource {
                    let local = self.geometry_handles.get(resource).ok_or(
                        RetainedExecutionTransportError::UnknownGeometryResource(*resource),
                    )?;
                    Ok(ObjectContentRef::Geometry(GeometryRef::External(local.id)))
                } else {
                    Ok(ObjectContentRef::Geometry(geometry.clone()))
                }
            }
            TransportObjectContent::Text { text } => self
                .text_handles
                .get(text)
                .copied()
                .map(ObjectContentRef::Text)
                .ok_or(RetainedExecutionTransportError::UnknownTextResource(*text)),
        }
    }

    fn resolve_render_geometry(
        &self,
        object: &RetainedTransportObjectState,
        session: u32,
    ) -> Result<Option<Arc<GeometryRef>>, RetainedExecutionTransportError> {
        match object.render_geometry_resource {
            Some(index) if self.resource_session == Some(session) => {
                let (slot, generation) =
                    crate::retained_resource_transport::render_geometry_parts(index);
                self.render_geometries
                    .get(slot as usize)
                    .filter(|entry| entry.generation == generation)
                    .and_then(|entry| entry.geometry.clone())
                    .map(Some)
                    .ok_or(RetainedExecutionTransportError::InvalidRenderGeometryResource(index))
            }
            Some(index) => {
                Err(RetainedExecutionTransportError::InvalidRenderGeometryResource(index))
            }
            None => Ok(object.render_geometry.clone().map(Arc::new)),
        }
    }
    pub fn frame(&self) -> Option<&FrameState> {
        self.frame.as_ref()
    }

    pub const fn session(&self) -> Option<u32> {
        self.session
    }

    pub fn applied_sequence(&self) -> Option<u64> {
        self.session.and_then(|_| self.next_sequence.checked_sub(1))
    }

    pub fn frame_index_for_slot(&self, slot: TransportSlotId) -> Option<usize> {
        self.slot_indices.get(&slot).copied()
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn has_composed_source_rows(&self) -> bool {
        !self.composed_handles.is_empty()
    }

    pub(crate) fn frame_index_for_object(&self, object: ObjectId) -> Option<usize> {
        self.object_indices.get(&object).copied()
    }

    pub(crate) fn resolve_transport_object_state(
        &self,
        object: &RetainedTransportObjectState,
    ) -> Result<FrameObjectState, RetainedExecutionTransportError> {
        validate_object_state(object)?;
        let content = self.resolve_content(&object.content)?;
        frame_object(
            object,
            content,
            self.resolve_fill_paint(object.fill_paint.as_ref())?,
        )
    }

    pub const fn camera(&self) -> Camera2DState {
        self.camera
    }

    /// Exact source runtime context of the currently installed worker frame.
    pub const fn publication_context(&self) -> PublicationContext {
        self.publication_context
    }

    pub fn inset_2d_views(&self) -> &[Inset2DViewState] {
        &self.inset_2d_views
    }

    /// Resolve the effective Camera3D from the transported semantic row.
    /// The separate `camera` envelope field remains the 2D inspection/HUD view.
    pub fn camera_3d_object(&self) -> Option<&FrameObjectState> {
        self.frame.as_ref()?.objects.iter().find(|object| {
            object
                .spatial
                .as_deref()
                .is_some_and(|state| state.camera_projection.is_some())
        })
    }

    pub fn painter_order(&self) -> &[u32] {
        &self.painter_order
    }

    pub fn apply(
        &mut self,
        delta: RetainedExecutionDeltaEnvelope,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), RetainedExecutionTransportError>
    {
        validate_envelope_header(&delta)?;

        match self.session {
            None => {
                if !delta.snapshot || delta.sequence != 0 {
                    return Err(RetainedExecutionTransportError::SessionRequiresSnapshot {
                        session: delta.session,
                        sequence: delta.sequence,
                    });
                }
            }
            Some(session) if session != delta.session => {
                if !delta.snapshot || delta.sequence != 0 {
                    return Err(RetainedExecutionTransportError::SessionRequiresSnapshot {
                        session: delta.session,
                        sequence: delta.sequence,
                    });
                }
            }
            Some(_) if delta.sequence < self.next_sequence => {
                return Ok((
                    RetainedTransportApplyOutcome::DroppedStale,
                    FrameChanges::default(),
                ));
            }
            Some(_) if delta.sequence != self.next_sequence => {
                return Err(RetainedExecutionTransportError::SequenceGap {
                    expected: self.next_sequence,
                    actual: delta.sequence,
                });
            }
            Some(_) => {}
        }

        self.validate_inset_2d_views(&delta)?;
        self.validate_composed_handles(&delta)?;

        let next_sequence = delta
            .sequence
            .checked_add(1)
            .ok_or(RetainedExecutionTransportError::SequenceExhausted)?;
        if self.session == Some(delta.session)
            && publication_context_is_stale(delta.publication_context, self.publication_context)
        {
            return Err(RetainedExecutionTransportError::StalePublicationContext {
                received: delta.publication_context,
                installed: self.publication_context,
            });
        }
        let changes = if delta.snapshot {
            self.apply_snapshot(&delta)?;
            FrameChanges::all()
        } else {
            self.apply_incremental(&delta)?
        };
        self.commit_composed_handles(&delta);
        self.session = Some(delta.session);
        self.next_sequence = next_sequence;
        self.publication_context = delta.publication_context;
        self.camera = delta.camera;
        self.inset_2d_views = delta.inset_2d_views.clone();
        if let Some(frame) = &mut self.frame {
            frame.time = delta.time;
        }
        Ok((RetainedTransportApplyOutcome::Applied, changes))
    }

    fn validate_inset_2d_views(
        &self,
        delta: &RetainedExecutionDeltaEnvelope,
    ) -> Result<(), RetainedExecutionTransportError> {
        let mut live = if delta.snapshot {
            HashSet::new()
        } else {
            self.object_indices.keys().copied().collect()
        };
        if !delta.snapshot {
            for slot in &delta.removed_slots {
                if let Some(index) = self.slot_indices.get(slot) {
                    if let Some(object) = self
                        .frame
                        .as_ref()
                        .and_then(|frame| frame.objects.get(*index))
                    {
                        live.remove(&object.id);
                    }
                }
            }
        }
        live.extend(delta.objects.iter().map(|object| object.object));
        let mut displays = HashSet::new();
        for view in &delta.inset_2d_views {
            let finite = [
                view.camera.center.x,
                view.camera.center.y,
                view.camera.height,
                view.display_center.x,
                view.display_center.y,
                view.display_size.x,
                view.display_size.y,
                view.display_stroke_width,
            ]
            .into_iter()
            .all(f32::is_finite);
            if !finite
                || view.camera.height <= 0.0
                || view.display_size.x <= 0.0
                || view.display_size.y <= 0.0
                || view.display_stroke_width < 0.0
                || view.camera_bounds().is_none()
                || !live.contains(&view.camera_frame)
                || !live.contains(&view.display)
                || !displays.insert(view.display)
            {
                return Err(RetainedExecutionTransportError::InvalidInset2DView(
                    view.display,
                ));
            }
        }
        Ok(())
    }

    fn apply_snapshot(
        &mut self,
        delta: &RetainedExecutionDeltaEnvelope,
    ) -> Result<(), RetainedExecutionTransportError> {
        if let Some(patch) = delta.object_patches.first() {
            return Err(RetainedExecutionTransportError::InvalidObjectPatch(
                patch.slot,
            ));
        }
        let mut objects = delta.objects.clone();
        objects.sort_by_key(|object| object.order);
        let mut seen_slots = HashSet::with_capacity(objects.len());
        let mut seen_objects = HashSet::with_capacity(objects.len());
        for (index, object) in objects.iter().enumerate() {
            let expected = u32::try_from(index)
                .map_err(|_| RetainedExecutionTransportError::InvalidOrder(object.order))?;
            if object.order != expected {
                return Err(RetainedExecutionTransportError::InvalidOrder(object.order));
            }
            if !seen_slots.insert(object.slot) {
                return Err(RetainedExecutionTransportError::DuplicateSlot(object.slot));
            }
            if !seen_objects.insert(object.object) {
                return Err(RetainedExecutionTransportError::DuplicateObject(
                    object.object,
                ));
            }
            validate_object_state(object)?;
            if object.render_transform.is_some()
                && object.render_geometry.is_none()
                && object.render_geometry_resource.is_none()
            {
                return Err(RetainedExecutionTransportError::InvalidRenderTransform(
                    object.slot,
                ));
            }
        }
        validate_single_camera(objects.iter().map(|object| {
            (
                object.object,
                object
                    .spatial
                    .as_ref()
                    .is_some_and(|state| state.camera_projection.is_some()),
            )
        }))?;

        let render_geometries = objects
            .iter()
            .map(|object| self.resolve_render_geometry(object, delta.session))
            .collect::<Result<Vec<_>, _>>()?;
        let frame_objects = objects
            .iter()
            .map(|object| {
                self.resolve_content(&object.content).and_then(|content| {
                    frame_object(
                        object,
                        content,
                        self.resolve_fill_paint(object.fill_paint.as_ref())?,
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let composed_dependencies = self.preflight_composed_snapshot(&frame_objects, &objects)?;
        let slots = objects.iter().map(|object| object.slot).collect::<Vec<_>>();
        let slot_indices = slots
            .iter()
            .copied()
            .enumerate()
            .map(|(index, slot)| (slot, index))
            .collect();
        let object_indices = objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.object, index))
            .collect();
        self.slots = slots;
        self.slot_indices = slot_indices;
        self.object_indices = object_indices;
        self.composed_dependencies = composed_dependencies;
        self.painter_order = (0..objects.len() as u32).collect();
        self.painter_ranks = (0..objects.len() as u32).map(Some).collect();
        self.frame = Some(FrameState {
            family_animations: vec![None; objects.len()],
            family_animation_plan_indices: vec![None; objects.len()],
            time: delta.time,
            objects: frame_objects,
            presences: objects.iter().map(|object| object.presence).collect(),
            reveals: objects.iter().map(|object| object.reveal).collect(),
            morphs: objects.iter().map(|object| object.morph).collect(),
            render_geometries,
            render_transforms: objects
                .iter()
                .map(|object| object.render_transform)
                .collect(),
        });
        Ok(())
    }

    fn apply_incremental(
        &mut self,
        delta: &RetainedExecutionDeltaEnvelope,
    ) -> Result<FrameChanges, RetainedExecutionTransportError> {
        let frame = self
            .frame
            .as_ref()
            .ok_or(RetainedExecutionTransportError::IncrementalBeforeSnapshot)?;
        let mut updates = Vec::with_capacity(delta.objects.len());
        let mut seen_slots = HashSet::with_capacity(delta.objects.len());
        let mut added_slot_indices = HashMap::new();
        let mut added_objects = HashSet::new();
        let mut next_slot_count = self.slots.len();
        for object in &delta.objects {
            if !seen_slots.insert(object.slot) {
                return Err(RetainedExecutionTransportError::DuplicateSlot(object.slot));
            }
            validate_object_state(object)?;
            let content = self.resolve_content(&object.content)?;
            let (index, added) = if let Some(&index) = self.slot_indices.get(&object.slot) {
                let current = &frame.objects[index];
                if current.id != object.object {
                    return Err(RetainedExecutionTransportError::SlotIdentityChanged(
                        object.slot,
                    ));
                }
                // Typed resource resolution above already validates new content.
                // Content replacement does not change this retained object's identity.
                (index, false)
            } else {
                if self.object_indices.contains_key(&object.object)
                    || !added_objects.insert(object.object)
                {
                    return Err(RetainedExecutionTransportError::DuplicateObject(
                        object.object,
                    ));
                }
                let index = next_slot_count;
                next_slot_count += 1;
                added_slot_indices.insert(object.slot, index);
                (index, true)
            };
            let geometry = if object.render_transform.is_some()
                && object.render_geometry.is_none()
                && object.render_geometry_resource.is_none()
            {
                if added {
                    return Err(RetainedExecutionTransportError::InvalidRenderTransform(
                        object.slot,
                    ));
                }
                Some(
                    frame
                        .render_geometries
                        .get(index)
                        .cloned()
                        .flatten()
                        .ok_or(RetainedExecutionTransportError::InvalidRenderTransform(
                            object.slot,
                        ))?,
                )
            } else {
                self.resolve_render_geometry(object, delta.session)?
            };
            let fill_paint = self.resolve_fill_paint(object.fill_paint.as_ref())?;
            updates.push(composed::ResolvedRow {
                index,
                added,
                wire: object,
                geometry,
                state: frame_object(object, content, fill_paint)?,
            });
        }
        let mut seen_removed = HashSet::with_capacity(delta.removed_slots.len());
        let _validated_removed_indices = delta
            .removed_slots
            .iter()
            .map(|slot| {
                if !seen_removed.insert(*slot) || seen_slots.contains(slot) {
                    return Err(RetainedExecutionTransportError::DuplicateSlot(*slot));
                }
                self.slot_indices
                    .get(slot)
                    .copied()
                    .ok_or(RetainedExecutionTransportError::UnknownSlot(*slot))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Validate the complete patch set before changing any installed row.
        let mut patch_updates = Vec::with_capacity(delta.object_patches.len());
        for patch in &delta.object_patches {
            if !seen_slots.insert(patch.slot) || seen_removed.contains(&patch.slot) {
                return Err(RetainedExecutionTransportError::DuplicateSlot(patch.slot));
            }
            let index = self
                .slot_indices
                .get(&patch.slot)
                .copied()
                .ok_or(RetainedExecutionTransportError::UnknownSlot(patch.slot))?;
            if frame.objects[index].id != patch.object {
                return Err(RetainedExecutionTransportError::SlotIdentityChanged(
                    patch.slot,
                ));
            }
            if frame.objects[index].content.composed_source().is_some()
                && (patch
                    .transform
                    .is_some_and(|transform| transform != Transform2D::IDENTITY)
                    || patch.style.is_some_and(|style| {
                        style.fill.is_some()
                            || style.stroke.is_some()
                            || !style.opacity.is_finite()
                            || !(0.0..=1.0).contains(&style.opacity)
                    })
                    || patch.morph.is_some_and(|morph| morph != 0.0)
                    || patch.render_translation.is_some())
            {
                return Err(
                    RetainedExecutionTransportError::InvalidComposedSourceHandle(patch.slot),
                );
            }
            if patch.transform.is_none()
                && patch.style.is_none()
                && patch.morph.is_none()
                && patch.render_translation.is_none()
            {
                return Err(RetainedExecutionTransportError::InvalidObjectPatch(
                    patch.slot,
                ));
            }
            if frame.objects[index].content.image().is_some()
                && patch.morph.is_some_and(|morph| morph != 0.0)
            {
                return Err(RetainedExecutionTransportError::ImageRenderGeometry(
                    patch.slot,
                ));
            }
            if let Some(translation) = patch.render_translation {
                validate_render_translation(patch.slot, translation)?;
                if frame.render_transforms[index].is_none()
                    || frame.render_geometries[index].is_none()
                {
                    return Err(RetainedExecutionTransportError::InvalidRenderTransform(
                        patch.slot,
                    ));
                }
            }
            patch_updates.push((index, patch));
        }

        let painter_update = self.validate_painter_order_delta(
            delta.painter_order.as_ref(),
            &added_slot_indices,
            &seen_removed,
            &seen_slots,
        )?;
        let composed_update = self.preflight_composed_incremental(
            &updates,
            &_validated_removed_indices,
            painter_update.as_ref(),
        )?;
        let mut camera_rows = frame
            .objects
            .iter()
            .enumerate()
            .filter(|(index, _)| !removed_indices_contains_slot(&self.slots, &seen_removed, *index))
            .map(|(_, object)| {
                (
                    object.id,
                    object
                        .spatial
                        .as_deref()
                        .is_some_and(|state| state.camera_projection.is_some()),
                )
            })
            .collect::<HashMap<_, _>>();
        for object in &delta.objects {
            camera_rows.insert(
                object.object,
                object
                    .spatial
                    .as_ref()
                    .is_some_and(|state| state.camera_projection.is_some()),
            );
        }
        validate_single_camera(camera_rows)?;
        let added_indices = updates
            .iter()
            .filter_map(|row| {
                (row.added
                    || self
                        .painter_ranks
                        .get(row.index)
                        .copied()
                        .flatten()
                        .is_none())
                .then_some(row.index)
            })
            .collect::<Vec<_>>();
        let removed_indices = _validated_removed_indices;
        let frame = self.frame.as_mut().expect("validated retained frame");
        let mut changed = Vec::with_capacity(updates.len() + patch_updates.len());
        for composed::ResolvedRow {
            index,
            added: is_added,
            wire: object,
            geometry,
            state,
        } in updates
        {
            if is_added {
                debug_assert_eq!(index, frame.objects.len());
                self.slots.push(object.slot);
                self.slot_indices.insert(object.slot, index);
                self.object_indices.insert(object.object, index);
                frame.objects.push(state);
                frame.presences.push(object.presence);
                frame.reveals.push(object.reveal);
                frame.morphs.push(object.morph);
                frame.render_geometries.push(geometry);
                frame.render_transforms.push(object.render_transform);
            } else {
                frame.objects[index] = state;
                frame.presences[index] = object.presence;
                frame.reveals[index] = object.reveal;
                frame.morphs[index] = object.morph;
                frame.render_geometries[index] = geometry;
                frame.render_transforms[index] = object.render_transform;
            }
            changed.push(index);
        }
        for (index, patch) in patch_updates {
            if let Some(transform) = patch.transform {
                frame.objects[index].transform = transform;
            }
            if let Some(style) = patch.style {
                frame.objects[index].style = style;
            }
            if let Some(morph) = patch.morph {
                frame.morphs[index] = morph;
            }
            if let Some(translation) = patch.render_translation {
                frame.render_transforms[index]
                    .as_mut()
                    .expect("validated active render frame")
                    .translation = translation;
            }
            changed.push(index);
        }
        for &index in &removed_indices {
            if frame.objects[index].content.composed_source().is_some() {
                // A retired row retains its identity for re-entry, but must not
                // retain the immutable member allocation after its last root.
                frame.objects[index].content = ObjectContentRef::Geometry(GeometryRef::circle(0.0));
                frame.objects[index].effects = None;
            }
            frame.presences[index] = false;
            frame.render_geometries[index] = None;
            frame.render_transforms[index] = None;
        }
        let painter_range = painter_update.map(
            |PreparedPainterOrder {
                 range,
                 old_end,
                 segment,
             }| {
                for &index in &self.painter_order[range.start..old_end] {
                    self.painter_ranks[index as usize] = None;
                }
                if self.painter_ranks.len() < frame.objects.len() {
                    self.painter_ranks.resize(frame.objects.len(), None);
                }
                let next_end = range.start + segment.len();
                self.painter_order.splice(range.start..old_end, segment);
                for (rank, &index) in self.painter_order[range.start..next_end].iter().enumerate() {
                    self.painter_ranks[index as usize] = Some((range.start + rank) as u32);
                }
                range
            },
        );
        let mut changes = FrameChanges::with_structure(changed, added_indices, removed_indices);
        if let Some(range) = painter_range {
            changes = changes.with_painter_order(range);
        }
        #[cfg(test)]
        {
            self.last_composed_validation = composed_update.stats;
        }
        composed_update.commit(&mut self.composed_dependencies);
        Ok(changes)
    }

    fn validate_painter_order_delta(
        &self,
        delta: Option<&RetainedPainterOrderDelta>,
        added_slot_indices: &HashMap<TransportSlotId, usize>,
        removed: &HashSet<TransportSlotId>,
        updated: &HashSet<TransportSlotId>,
    ) -> Result<Option<PreparedPainterOrder>, RetainedExecutionTransportError> {
        if delta.is_none() {
            if !removed.is_empty() || !added_slot_indices.is_empty() {
                return Err(RetainedExecutionTransportError::StructuralChangeRequiresSnapshot);
            }
            return Ok(None);
        }
        let delta = delta.expect("checked painter delta");
        let start = delta.start as usize;
        let end = delta.end as usize;
        if end < start {
            return Err(RetainedExecutionTransportError::InvalidOrder(delta.end));
        }
        let old_end = end.min(self.painter_order.len());
        if start > old_end {
            return Err(RetainedExecutionTransportError::InvalidOrder(delta.start));
        }
        let old_segment = self.painter_order[start..old_end]
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        let mut segment = Vec::with_capacity(delta.slots.len());
        let mut seen = HashSet::with_capacity(delta.slots.len());
        for slot in &delta.slots {
            if removed.contains(slot) || !seen.insert(*slot) {
                return Err(RetainedExecutionTransportError::DuplicateSlot(*slot));
            }
            let index = self
                .slot_indices
                .get(slot)
                .or_else(|| added_slot_indices.get(slot))
                .copied()
                .ok_or(RetainedExecutionTransportError::UnknownSlot(*slot))?;
            segment.push(index as u32);
        }
        let segment_set = segment.iter().copied().collect::<HashSet<_>>();
        if removed.iter().any(|slot| {
            self.slot_indices
                .get(slot)
                .is_none_or(|index| !old_segment.contains(&(*index as u32)))
        }) {
            return Err(RetainedExecutionTransportError::InvalidOrder(delta.end));
        }
        if old_segment.iter().any(|index| {
            !segment_set.contains(index) && !removed.contains(&self.slots[*index as usize])
        }) {
            return Err(RetainedExecutionTransportError::InvalidOrder(delta.end));
        }
        let mut added = added_slot_indices
            .values()
            .map(|&index| index as u32)
            .collect::<HashSet<_>>();
        for slot in updated {
            if let Some(&index) = self.slot_indices.get(slot) {
                if self.painter_ranks.get(index).copied().flatten().is_none() {
                    added.insert(index as u32);
                }
            }
        }
        if segment
            .iter()
            .any(|index| !old_segment.contains(index) && !added.contains(index))
            || added.iter().any(|index| !segment_set.contains(index))
        {
            return Err(RetainedExecutionTransportError::InvalidOrder(delta.end));
        }
        Ok(Some(PreparedPainterOrder {
            range: start..end,
            old_end,
            segment,
        }))
    }
}

fn validate_envelope_header(
    delta: &RetainedExecutionDeltaEnvelope,
) -> Result<(), RetainedExecutionTransportError> {
    if delta.channel != RETAINED_EXECUTION_TRANSPORT_CHANNEL {
        return Err(RetainedExecutionTransportError::InvalidChannel(
            delta.channel.clone(),
        ));
    }
    if delta.protocol_version != RETAINED_EXECUTION_TRANSPORT_VERSION {
        return Err(RetainedExecutionTransportError::UnsupportedVersion(
            delta.protocol_version,
        ));
    }
    validate_time(delta.time)
}

fn validate_time(time: f64) -> Result<(), RetainedExecutionTransportError> {
    if time.is_finite() {
        Ok(())
    } else {
        Err(RetainedExecutionTransportError::InvalidTime(time))
    }
}

fn validate_frame_shape(frame: &FrameState) -> Result<(), RetainedExecutionTransportError> {
    let count = frame.objects.len();
    if frame.presences.len() == count
        && frame.reveals.len() == count
        && frame.morphs.len() == count
        && frame.render_geometries.len() == count
        && frame.render_transforms.len() == count
    {
        Ok(())
    } else {
        Err(RetainedExecutionTransportError::FrameShapeMismatch)
    }
}

fn render_geometry_identity(frame: &FrameState, index: usize) -> Option<Arc<GeometryRef>> {
    frame.render_transforms.get(index).copied().flatten()?;
    frame.render_geometries.get(index).cloned().flatten()
}

fn transport_slot(
    frame: &FrameState,
    index: usize,
) -> Result<TransportSlotId, RetainedExecutionTransportError> {
    if index >= frame.objects.len() {
        return Err(RetainedExecutionTransportError::InvalidObjectIndex(index));
    }
    Ok(TransportSlotId {
        slot: u32::try_from(index)
            .map_err(|_| RetainedExecutionTransportError::InvalidObjectIndex(index))?,
        generation: 0,
    })
}

fn transport_object(
    frame: &FrameState,
    index: usize,
    render_geometry_resource: Option<u64>,
    composed_source: Option<crate::TransportComposedSourceHandle>,
) -> Result<RetainedTransportObjectState, RetainedExecutionTransportError> {
    let object = frame
        .objects
        .get(index)
        .ok_or(RetainedExecutionTransportError::InvalidObjectIndex(index))?;
    let slot_index = u32::try_from(index)
        .map_err(|_| RetainedExecutionTransportError::InvalidObjectIndex(index))?;
    let state = RetainedTransportObjectState {
        fill_paint: object.fill_paint.clone(),
        draw_enabled: object.draw_enabled,
        slot: TransportSlotId {
            slot: slot_index,
            generation: 0,
        },
        order: slot_index,
        object: object.id,
        effects: object
            .effects
            .as_ref()
            .map(|stack| {
                stack
                    .iter()
                    .map(|effect| TransportEffectState::from(&effect))
                    .collect()
            })
            .unwrap_or_default(),
        z_index: object.z_index,
        content: match composed_source {
            Some(source) => TransportObjectContent::Composed { source },
            None => (&object.content).try_into()?,
        },
        transform: object.transform,
        spatial: object
            .spatial
            .as_deref()
            .map(TransportSpatialState::from_compiled),
        style: object.style,
        appearance: object.appearance,
        text_bounds: object.text_bounds,
        presence: frame.presences[index],
        reveal: frame.reveals[index],
        morph: frame.morphs[index],
        render_geometry: if render_geometry_resource.is_none() {
            frame.render_geometries[index].as_deref().cloned()
        } else {
            None
        },
        render_transform: frame.render_transforms[index],
        render_geometry_resource,
    };
    // Resource-aware encoding binds versioned source handles after this typed
    // row is captured. Validate the complete reference at the wire boundary.
    validate_object_fields(&state)?;
    Ok(state)
}

fn validate_object_state(
    object: &RetainedTransportObjectState,
) -> Result<(), RetainedExecutionTransportError> {
    if let TransportObjectContent::Geometry { geometry, resource } = &object.content {
        match (geometry, resource) {
            (GeometryRef::External(id), Some(handle)) if handle.id == id.get() => {}
            (GeometryRef::External(_), _) | (_, Some(_)) => {
                return Err(
                    RetainedExecutionTransportError::InvalidGeometryResourceReference(object.slot),
                );
            }
            (_, None) => {}
        }
    }
    validate_object_fields(object)
}

fn validate_object_fields(
    object: &RetainedTransportObjectState,
) -> Result<(), RetainedExecutionTransportError> {
    composed::validate_owner_fields(object)?;
    noon_compile::validate_fill_paint(object.object, object.fill_paint.as_deref())
        .map_err(|_| RetainedExecutionTransportError::InvalidObjectPatch(object.slot))?;
    if let Some(hatch) = object
        .fill_paint
        .as_deref()
        .and_then(noon_core::SemanticPaint::as_hatch)
    {
        let invalid = || RetainedExecutionTransportError::InvalidObjectPatch(object.slot);
        if object.spatial.is_some()
            || object.reveal != 1.0
            || object.morph != 0.0
            || object.render_geometry.is_some()
            || object.render_geometry_resource.is_some()
        {
            return Err(invalid());
        }
        match &object.content {
            TransportObjectContent::Geometry { geometry, .. } => match geometry {
                GeometryRef::Circle { .. }
                | GeometryRef::Rectangle { .. }
                | GeometryRef::VectorPath(_) => {
                    noon_compile::validate_hatch_bounds(
                        hatch,
                        geometry.local_bounds().ok_or_else(invalid)?,
                    )
                    .map_err(|_| invalid())?;
                }
                // The installed-resource boundary validates these bounds before
                // publishing the frame or consuming its sequence.
                GeometryRef::External(_) => {}
                GeometryRef::Line { .. } => return Err(invalid()),
            },
            _ => return Err(invalid()),
        }
    }
    if object.effects.len() > noon_core::MAX_EFFECT_STACK_LEN
        || object.effects.iter().enumerate().any(|(index, effect)| {
            effect.to_compiled().is_none()
                || object.effects[..index]
                    .iter()
                    .any(|previous| previous.attachment == effect.attachment)
        })
        || (!object.effects.is_empty()
            && (!glow_content_is_supported(&object.content)
                || (matches!(&object.content, TransportObjectContent::Image { .. })
                    && !noon_core::SemanticImageContent::supports_render_style(&object.style))
                || object.spatial.is_some()
                || !object.reveal.is_finite()
                || !(0.0..=1.0).contains(&object.reveal)
                || object.morph != 0.0))
    {
        return Err(RetainedExecutionTransportError::InvalidEffectState(
            object.slot,
        ));
    }
    if !object.z_index.is_finite() {
        return Err(RetainedExecutionTransportError::InvalidZIndex(object.slot));
    }
    if let Some(transform) = object.render_transform {
        validate_render_transform(object.slot, transform)?;
    }
    if object
        .spatial
        .as_ref()
        .is_some_and(|spatial| !spatial.is_valid())
    {
        return Err(RetainedExecutionTransportError::InvalidSpatialState(
            object.slot,
        ));
    }
    if object.render_geometry.is_some() && object.render_geometry_resource.is_some() {
        return Err(RetainedExecutionTransportError::AmbiguousRenderGeometry(
            object.slot,
        ));
    }
    if matches!(&object.content, TransportObjectContent::Image { .. })
        && (object.render_geometry.is_some()
            || object.render_geometry_resource.is_some()
            || object.render_transform.is_some()
            || object.reveal != 1.0
            || object.morph != 0.0
            || object.text_bounds.is_some())
    {
        return Err(RetainedExecutionTransportError::ImageRenderGeometry(
            object.slot,
        ));
    }
    if matches!(&object.content, TransportObjectContent::Text { .. })
        && (object.render_geometry.is_some()
            || object.render_geometry_resource.is_some()
            || object.render_transform.is_some())
    {
        return Err(RetainedExecutionTransportError::TextRenderGeometry(
            object.slot,
        ));
    }
    Ok(())
}

fn glow_content_is_supported(content: &TransportObjectContent) -> bool {
    match content {
        TransportObjectContent::Composed { .. } => true,
        TransportObjectContent::Geometry { geometry, resource } => match geometry {
            GeometryRef::Circle { radius } => {
                resource.is_none() && radius.is_finite() && *radius > 0.0
            }
            GeometryRef::Rectangle { size } => {
                resource.is_none()
                    && size.x.is_finite()
                    && size.y.is_finite()
                    && size.x > 0.0
                    && size.y > 0.0
            }
            GeometryRef::Line { start, end } => {
                resource.is_none()
                    && start.x.is_finite()
                    && start.y.is_finite()
                    && end.x.is_finite()
                    && end.y.is_finite()
                    && (start.x != end.x || start.y != end.y)
            }
            GeometryRef::VectorPath(path) => {
                resource.is_none() && path.is_finite() && path.has_drawable_segments()
            }
            // A typed row is validated before the versioned source-resource
            // table binds this handle. Full id/version validation follows in
            // validate_object_state at the receiver boundary.
            GeometryRef::External(_) => true,
        },
        TransportObjectContent::Text { .. } => true,
        // The existing resource installation resolves the immutable pixels and
        // dimensions before committing the row. Image style, reveal and morph
        // admission remain independently checked by validate_object_fields.
        TransportObjectContent::Image { .. } => true,
    }
}

fn validate_single_camera(
    rows: impl IntoIterator<Item = (ObjectId, bool)>,
) -> Result<(), RetainedExecutionTransportError> {
    let mut camera = None;
    for (object, is_camera) in rows {
        if is_camera && camera.replace(object).is_some() {
            return Err(RetainedExecutionTransportError::MultipleCamera3D);
        }
    }
    Ok(())
}

fn validate_render_transform(
    slot: TransportSlotId,
    transform: Transform2D,
) -> Result<(), RetainedExecutionTransportError> {
    validate_render_translation(slot, transform.translation)?;
    if !transform.scale.x.is_finite()
        || !transform.scale.y.is_finite()
        || !transform.rotation.is_finite()
    {
        return Err(RetainedExecutionTransportError::InvalidRenderTransform(
            slot,
        ));
    }
    Ok(())
}

fn validate_render_translation(
    slot: TransportSlotId,
    translation: noon_core::Vec2,
) -> Result<(), RetainedExecutionTransportError> {
    if !translation.x.is_finite() || !translation.y.is_finite() {
        return Err(RetainedExecutionTransportError::InvalidRenderTransform(
            slot,
        ));
    }
    Ok(())
}

fn publication_context_is_stale(
    received: PublicationContext,
    installed: PublicationContext,
) -> bool {
    received.frame_epoch().get() < installed.frame_epoch().get()
        || (received.frame_epoch() == installed.frame_epoch() && received != installed)
}

fn removed_indices_contains_slot(
    slots: &[TransportSlotId],
    removed: &HashSet<TransportSlotId>,
    index: usize,
) -> bool {
    slots.get(index).is_some_and(|slot| removed.contains(slot))
}

fn frame_object(
    object: &RetainedTransportObjectState,
    content: ObjectContentRef,
    fill_paint: Option<Arc<noon_core::SemanticPaint>>,
) -> Result<FrameObjectState, RetainedExecutionTransportError> {
    let spatial = match object.spatial.as_ref() {
        Some(spatial) => Some(spatial.to_compiled().ok_or(
            RetainedExecutionTransportError::InvalidSpatialState(object.slot),
        )?),
        None => None,
    };
    Ok(FrameObjectState {
        fill_paint,
        draw_enabled: object.draw_enabled,
        effects: if object.effects.is_empty() {
            None
        } else {
            let bindings =
                object
                    .effects
                    .iter()
                    .map(|value| {
                        value.to_compiled().ok_or(
                            RetainedExecutionTransportError::InvalidEffectState(object.slot),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
            Some(Arc::new(
                noon_compile::CompiledEffectStack::new(bindings).map_err(|_| {
                    RetainedExecutionTransportError::InvalidEffectState(object.slot)
                })?,
            ))
        },
        spatial: spatial.map(Box::new),
        z_index: object.z_index,
        id: object.object,
        content,
        transform: object.transform,
        style: object.style,
        appearance: object.appearance,
        text_bounds: object.text_bounds,
    })
}

#[cfg(test)]
mod tests {
    use noon_core::{Color, TextResourceId, Vec2};

    use super::*;

    fn test_mirror() -> RetainedExecutionFrameMirror {
        let handles = [
            TextResourceHandle {
                arena: 0,
                id: TextResourceId::new(7),
                version: 3,
            },
            TextResourceHandle {
                arena: 0,
                id: TextResourceId::new(8),
                version: 1,
            },
        ]
        .into_iter()
        .map(|handle| {
            (
                TransportTextResourceHandle::from_source_handle(handle),
                handle,
            )
        })
        .collect();
        RetainedExecutionFrameMirror::with_installed_resources(None, &[], handles)
    }

    fn test_mirror_with_render_geometries(
        session: u32,
        geometries: Arc<[Arc<GeometryRef>]>,
    ) -> RetainedExecutionFrameMirror {
        let mut mirror = test_mirror();
        mirror.resource_session = Some(session);
        mirror.render_geometries = geometries
            .iter()
            .map(
                |geometry| crate::retained_resource_transport::RenderGeometrySlot {
                    generation: 0,
                    geometry: Some(geometry.clone()),
                },
            )
            .collect();
        mirror
    }

    fn mixed_frame() -> FrameState {
        let text = TextResourceHandle {
            arena: 0,
            id: TextResourceId::new(7),
            version: 3,
        };
        FrameState {
            family_animations: vec![None; 2],
            family_animation_plan_indices: vec![None; 2],
            time: 0.0,
            objects: vec![
                FrameObjectState {
                    fill_paint: None,
                    draw_enabled: true,
                    effects: None,
                    spatial: None,
                    z_index: 0.0,
                    id: ObjectId::new(11),
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                    text_bounds: None,
                },
                FrameObjectState {
                    fill_paint: None,
                    draw_enabled: true,
                    effects: None,
                    spatial: None,
                    z_index: 0.0,
                    id: ObjectId::new(12),
                    content: ObjectContentRef::Text(text),
                    transform: Transform2D::IDENTITY,
                    style: Style {
                        fill: Some(Color::WHITE),
                        ..Style::default()
                    },
                    appearance: 1.0,
                    text_bounds: None,
                },
            ],
            presences: vec![true, true],
            reveals: vec![1.0, 1.0],
            morphs: vec![0.0, 0.0],
            render_geometries: vec![None, None],
            render_transforms: vec![None, None],
        }
    }

    #[test]
    fn invalid_composed_owner_rejects_without_consuming_sequence_or_snapshot_state() {
        let frame = mixed_frame();
        let camera = Camera2DState::default();
        let context = PublicationContext::default();
        let mut encoder = RetainedExecutionDeltaEncoder::new(73);
        let first = encoder
            .encode_snapshot_with_context(&frame, camera, context)
            .unwrap();
        assert_eq!(first.sequence, 0);
        let orders = encoder.snapshot_orders.clone();
        let geometries = encoder.published_render_geometries.clone();
        let mut unsupported = frame.clone();
        let member = unsupported.objects[0].id;
        unsupported.objects[1].content = ObjectContentRef::Composed(Arc::new(
            noon_core::ComposedSource::new([member], None).unwrap(),
        ));
        unsupported.objects[1].draw_enabled = false;
        assert_eq!(
            TransportObjectContent::try_from(&unsupported.objects[1].content),
            Err(RetainedExecutionTransportError::UnsupportedComposedSource)
        );
        assert_eq!(
            encoder.encode_snapshot_with_context(&unsupported, camera, context),
            Err(
                RetainedExecutionTransportError::InvalidComposedSourceHandle(TransportSlotId {
                    slot: 1,
                    generation: 0
                })
            )
        );
        assert_eq!(encoder.snapshot_orders, orders);
        assert_eq!(encoder.published_render_geometries, geometries);
        let next = encoder
            .encode_snapshot_with_context(&frame, camera, context)
            .unwrap();
        assert_eq!(next.sequence, 1);
        let mut mirror = test_mirror();
        mirror.apply(first).unwrap();
        mirror.apply(next).unwrap();
        assert_eq!(mirror.frame().unwrap(), &frame);
    }

    fn effect_stack(
        effect: &noon_compile::CompiledEffect,
    ) -> Arc<noon_compile::CompiledEffectStack> {
        Arc::new(noon_compile::CompiledEffectStack::new([*effect]).unwrap())
    }

    #[test]
    fn checked_gradient_survives_snapshot_replacement_and_removal_at_worker_boundary() {
        let gradient = Arc::new(
            noon_core::LinearGradient::new(
                [0.0, 0.5],
                [1.0, 0.5],
                [
                    noon_core::GradientStop::new(0.0, Color::rgba(1.0, 0.0, 0.0, 0.0)).unwrap(),
                    noon_core::GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)).unwrap(),
                ],
            )
            .unwrap(),
        );
        let mut frame = mixed_frame();
        frame.objects[0].fill_paint = Some(Arc::new(noon_core::SemanticPaint::LinearGradient(
            gradient.clone(),
        )));
        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let snapshot = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                PublicationContext::default(),
            )
            .unwrap();
        let json = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            json.matches("\"fill_paint\"").count(),
            1,
            "ordinary rows omit paint payload"
        );
        let mut mirror = test_mirror();
        mirror.apply(serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(
            mirror.frame().unwrap().objects[0]
                .fill_paint
                .as_deref()
                .and_then(noon_core::SemanticPaint::as_linear_gradient)
                .map(Arc::as_ref),
            Some(gradient.as_ref())
        );
        frame.objects[0].fill_paint = None;
        let delta = encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        mirror.apply(delta).unwrap();
        assert!(mirror.frame().unwrap().objects[0].fill_paint.is_none());
        let mut value = serde_json::to_value(gradient.as_ref()).unwrap();
        value["stops"][0]["position"] = serde_json::json!(-0.1);
        assert!(serde_json::from_value::<noon_core::LinearGradient>(value).is_err());
    }

    #[test]
    fn ordered_effects_roundtrip_full_profile_and_reject_invalid_suffix_atomically() {
        use noon_compile::{CompiledEffect, CompiledEffectStack};
        let bindings: Vec<_> = (0..noon_core::MAX_EFFECT_STACK_LEN)
            .map(|index| CompiledEffect {
                attachment: SemanticNodeId::new(90 + index as u32, u32::MAX - index as u32),
                definition: match index % 4 {
                    0 => noon_core::Blur::new(
                        EffectUpdate::default().radius(noon_core::Pixels(1.25 + index as f64)),
                    )
                    .unwrap()
                    .into(),
                    1 => Glow::new(
                        EffectUpdate::default()
                            .radius(noon_core::Pixels(2.5))
                            .intensity(0.75),
                    )
                    .unwrap()
                    .into(),
                    2 => noon_core::Shadow::new(
                        EffectUpdate::default()
                            .offset(noon_core::EffectOffset::Pixels([-2.25, 3.75])),
                    )
                    .unwrap()
                    .into(),
                    _ => noon_core::NoiseDissolve::new(u32::MAX, 0.0800000000000123)
                        .unwrap()
                        .with_progress(0.6250000000000123)
                        .unwrap()
                        .into(),
                },
            })
            .collect();
        let mut frame = mixed_frame();
        frame.objects[0].effects = Some(Arc::new(
            CompiledEffectStack::new(bindings.clone()).unwrap(),
        ));
        let mut encoder = RetainedExecutionDeltaEncoder::new(27);
        let snapshot = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                PublicationContext::default(),
            )
            .unwrap();
        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(
            json["objects"][0]["effects"].as_array().unwrap().len(),
            noon_core::MAX_EFFECT_STACK_LEN
        );
        assert!(json["objects"][0].get("effect").is_none());
        assert!(json["objects"][1].get("effects").is_none());
        let mut legacy = json.clone();
        legacy["objects"][0]["effect"] = legacy["objects"][0]["effects"][0].clone();
        assert!(serde_json::from_value::<RetainedExecutionDeltaEnvelope>(legacy).is_err());
        let mut mirror = test_mirror();
        mirror.apply(serde_json::from_value(json).unwrap()).unwrap();
        assert_eq!(
            mirror.frame().unwrap().objects[0].effects,
            frame.objects[0].effects
        );
        let mut changed = bindings;
        changed.swap(0, 7);
        changed[1].definition = EffectUpdate::default()
            .intensity(1.25)
            .apply_to(changed[1].definition)
            .unwrap();
        frame.objects[0].effects = Some(Arc::new(CompiledEffectStack::new(changed).unwrap()));
        let delta = encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(delta.objects.len(), 1);
        let before = mirror.frame().unwrap().clone();
        let sequence = mirror.next_sequence;
        let context = mirror.publication_context;
        for failure in 0..9 {
            let mut bad = delta.clone();
            // A valid first row must not publish before a bad later binding.
            let mut suffix = snapshot.objects[1].clone();
            suffix.effects = snapshot.objects[0].effects.clone();
            match failure {
                0 => {
                    suffix.effects[7].attachment = suffix.effects[0].attachment;
                }
                1 => {
                    suffix.effects.push(suffix.effects[0]);
                }
                2 => {
                    suffix.effects[7].definition = TransportEffectDefinition::Blur {
                        radius: EffectRadius::Pixels(-1.0),
                    };
                }
                _ => {
                    suffix.effects[7].definition = TransportEffectDefinition::NoiseDissolve {
                        seed: u32::MAX,
                        progress: match failure {
                            3 => -0.1,
                            4 => 1.1,
                            5 => f64::NAN,
                            _ => 0.5,
                        },
                        softness: match failure {
                            6 => -0.1,
                            7 => 1.1,
                            8 => f64::INFINITY,
                            _ => 0.08,
                        },
                    };
                }
            }
            bad.objects.push(suffix);
            assert!(matches!(
                mirror.apply(bad),
                Err(RetainedExecutionTransportError::InvalidEffectState(_))
            ));
            assert_eq!(mirror.frame().unwrap(), &before);
            assert_eq!(mirror.next_sequence, sequence);
            assert_eq!(mirror.publication_context, context);
        }
        mirror
            .apply(serde_json::from_str(&serde_json::to_string(&delta).unwrap()).unwrap())
            .unwrap();
        assert_eq!(
            mirror.frame().unwrap().objects[0].effects,
            frame.objects[0].effects
        );
        assert_eq!(mirror.frame().unwrap().objects[1], before.objects[1]);
    }

    #[test]
    fn tagged_mask_preserves_seed_and_precision_and_rejects_foreign_fields() {
        let mut frame = mixed_frame();
        let definition = noon_core::NoiseDissolve::new(u32::MAX, 0.0800000000000123)
            .unwrap()
            .with_progress(0.6250000000000123)
            .unwrap();
        let effect = Arc::new(noon_compile::CompiledEffect {
            attachment: SemanticNodeId::new(77, u32::MAX),
            definition: definition.into(),
        });
        frame.objects[0].effects = Some(effect_stack(&effect));
        let snapshot = RetainedExecutionDeltaEncoder::new(4)
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                PublicationContext::default(),
            )
            .unwrap();
        let value = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(value["protocol_version"], 26);
        let mask = &value["objects"][0]["effects"][0];
        assert_eq!(mask["kind"], "noise_dissolve");
        assert_eq!(mask["seed"], u32::MAX);
        for (field, bad) in [
            ("radius", serde_json::json!(1.0)),
            ("seed", serde_json::json!(1.5)),
            ("seed", serde_json::json!(-1)),
            ("seed", serde_json::json!(4294967296u64)),
        ] {
            let mut invalid = value.clone();
            invalid["objects"][0]["effects"][0][field] = bad;
            assert!(serde_json::from_value::<RetainedExecutionDeltaEnvelope>(invalid).is_err());
        }
        let mut mirror = test_mirror();
        mirror
            .apply(serde_json::from_value(value).unwrap())
            .unwrap();
        assert_eq!(
            mirror.frame().unwrap().objects[0].effects,
            frame.objects[0].effects
        );
    }

    #[test]
    fn hatch_worker_value_roundtrips_and_rejects_bad_profiles_without_consuming_sequence() {
        let paint = Arc::new(noon_core::SemanticPaint::Hatch(Arc::new(
            noon_core::HatchPaint::new(
                Color::RED,
                Color::rgba(0.0, 0.0, 1.0, 0.0),
                noon_core::HatchPaintOptions {
                    phase: [-0.173, 3.231],
                    cross: true,
                    ..Default::default()
                },
            )
            .unwrap(),
        )));
        let mut frame = mixed_frame();
        frame.objects[0].fill_paint = Some(paint.clone());
        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let snapshot = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                PublicationContext::default(),
            )
            .unwrap();
        let value = serde_json::to_value(&snapshot).unwrap();
        assert!(value["objects"][0]["fill_paint"]["hatch"].is_object());
        for (field, bad) in [
            ("width", serde_json::json!(2.0)),
            ("spacing", serde_json::json!(0.0)),
            ("cross", serde_json::json!(1)),
            ("unknown", serde_json::json!(true)),
        ] {
            let mut rejected = value.clone();
            rejected["objects"][0]["fill_paint"]["hatch"]["options"][field] = bad;
            assert!(serde_json::from_value::<RetainedExecutionDeltaEnvelope>(rejected).is_err());
        }
        let mut mirror = test_mirror();
        let mut obsolete = snapshot.clone();
        obsolete.protocol_version = 23;
        assert_eq!(
            mirror.apply(obsolete),
            Err(RetainedExecutionTransportError::UnsupportedVersion(23))
        );
        assert!(mirror.frame().is_none());
        mirror
            .apply(serde_json::from_value(value).unwrap())
            .unwrap();
        assert_eq!(
            mirror.frame().unwrap().objects[0].fill_paint.as_ref(),
            Some(&paint)
        );
        frame.objects[0].fill_paint = None;
        let delta = encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        let mut invalid = delta.clone();
        invalid.objects[0].fill_paint = Some(Arc::new(noon_core::SemanticPaint::Hatch(Arc::new(
            noon_core::HatchPaint::new(
                Color::RED,
                Color::BLUE,
                noon_core::HatchPaintOptions {
                    spacing: 1e-6,
                    width: 1e-7,
                    ..Default::default()
                },
            )
            .unwrap(),
        ))));
        let coherent = mirror.frame().unwrap().clone();
        assert!(mirror.apply(invalid).is_err());
        assert_eq!(mirror.frame(), Some(&coherent));
        assert_eq!(mirror.applied_sequence(), Some(0));
        mirror.apply(delta).unwrap();
        assert!(mirror.frame().unwrap().objects[0].fill_paint.is_none());
        assert_eq!(mirror.applied_sequence(), Some(1));
    }

    #[test]
    fn tagged_blur_reaches_the_worker_and_rejects_unsupported_fields() {
        let mut frame = mixed_frame();
        let effect = Arc::new(noon_compile::CompiledEffect {
            attachment: SemanticNodeId::new(77, 3),
            definition: noon_core::Blur::new(
                EffectUpdate::default().radius(noon_core::Pixels(3.250000000000123)),
            )
            .unwrap()
            .into(),
        });
        frame.objects[0].effects = Some(effect_stack(&effect));
        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let snapshot = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                PublicationContext::default(),
            )
            .unwrap();
        let value = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(value["objects"][0]["effects"][0]["kind"], "blur");
        let mut invalid = value.clone();
        invalid["objects"][0]["effects"][0]["intensity"] = serde_json::json!(1.0);
        assert!(serde_json::from_value::<RetainedExecutionDeltaEnvelope>(invalid).is_err());
        let mut mirror = test_mirror();
        mirror
            .apply(serde_json::from_value(value).unwrap())
            .unwrap();
        assert_eq!(
            mirror.frame().unwrap().objects[0]
                .effects
                .as_ref()
                .and_then(|stack| stack.get(effect.attachment)),
            Some(*effect)
        );
    }

    #[test]
    fn tagged_shadow_roundtrips_signed_subpixel_offsets_and_rejects_foreign_fields() {
        let mut frame = mixed_frame();
        let effect = Arc::new(noon_compile::CompiledEffect {
            attachment: SemanticNodeId::new(77, 3),
            definition: noon_core::Shadow::new(
                EffectUpdate::default()
                    .radius(noon_core::Pixels(3.25))
                    .offset(noon_core::EffectOffset::Pixels([-1.000000000000123, 4.5])),
            )
            .unwrap()
            .into(),
        });
        frame.objects[0].effects = Some(effect_stack(&effect));
        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let snapshot = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                PublicationContext::default(),
            )
            .unwrap();
        let value = serde_json::to_value(snapshot).unwrap();
        assert_eq!(value["objects"][0]["effects"][0]["kind"], "shadow");
        for field in ["intensity", "source"] {
            let mut invalid = value.clone();
            invalid["objects"][0]["effects"][0][field] = serde_json::json!(0.0);
            assert!(serde_json::from_value::<RetainedExecutionDeltaEnvelope>(invalid).is_err());
        }
        let mut mirror = test_mirror();
        mirror
            .apply(serde_json::from_value(value).unwrap())
            .unwrap();
        assert_eq!(
            mirror.frame().unwrap().objects[0]
                .effects
                .as_ref()
                .and_then(|stack| stack.get(effect.attachment)),
            Some(*effect)
        );
    }

    #[test]
    fn effective_glow_survives_snapshot_delta_removal_and_json_without_new_identity() {
        let mut frame = mixed_frame();
        let effect = Arc::new(noon_compile::CompiledEffect {
            attachment: noon_core::SemanticNodeId::new(77, 3),
            definition: (Glow::new(
                EffectUpdate::default()
                    .color(Color::rgba(0.25, 0.5, 0.75, 0.125))
                    .radius(noon_core::Pixels(12.5))
                    .intensity(0.5)
                    .source(GlowSource::Silhouette),
            )
            .unwrap())
            .into(),
        });
        frame.objects[0].effects = Some(effect_stack(&effect));
        let camera = Camera2DState::default();
        let context = noon_core::PublicationContext::default();
        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let snapshot = encoder
            .encode_snapshot_with_context(&frame, camera, context)
            .unwrap();
        assert_eq!(snapshot.protocol_version, 26);
        assert_eq!(snapshot.objects[0].effects[0].attachment, effect.attachment);
        assert!(snapshot.objects[1].effects.is_empty());
        let json = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            json.matches("\"effects\"").count(),
            1,
            "no-effect rows omit the wire field"
        );
        let decoded: RetainedExecutionDeltaEnvelope = serde_json::from_str(&json).unwrap();
        let mut mirror = test_mirror();
        assert_eq!(
            mirror.apply(decoded).unwrap().0,
            RetainedTransportApplyOutcome::Applied
        );
        assert_eq!(
            mirror.frame().unwrap().objects[0]
                .effects
                .as_ref()
                .and_then(|stack| stack.get(effect.attachment)),
            Some(*effect)
        );
        assert!(mirror.frame().unwrap().objects[1].effects.is_none());

        // A parameter-only publication keeps the same generational attachment.
        let changed = Arc::new(noon_compile::CompiledEffect {
            attachment: effect.attachment,
            definition: EffectUpdate::default()
                .intensity(1.25)
                .apply_to(effect.definition)
                .unwrap(),
        });
        frame.objects[0].effects = Some(effect_stack(&changed));
        let delta = encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![0]),
                camera,
                context,
            )
            .unwrap()
            .unwrap();
        assert!(!delta.snapshot);
        assert_eq!(delta.objects.len(), 1);
        let wire: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&serde_json::to_string(&delta).unwrap()).unwrap();
        mirror.apply(wire).unwrap();
        assert_eq!(
            mirror.frame().unwrap().objects[0]
                .effects
                .as_ref()
                .and_then(|stack| stack.get(changed.attachment)),
            Some(*changed)
        );

        // Removing the exact effect clears its renderer-derived scope; the old
        // generation is not recycled or replaced by a transport allocator.
        frame.objects[0].effects = None;
        let removed = encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![0]),
                camera,
                context,
            )
            .unwrap()
            .unwrap();
        mirror.apply(removed).unwrap();
        assert!(mirror.frame().unwrap().objects[0].effects.is_none());

        // Every predecessor, including the former single-binding schema,
        // rejects rather than silently discarding ordered effects.
        let next_sequence = mirror.next_sequence;
        for version in 14..26 {
            let mut old = snapshot.clone();
            old.protocol_version = version;
            assert_eq!(
                mirror.apply(old),
                Err(RetainedExecutionTransportError::UnsupportedVersion(version))
            );
            assert_eq!(mirror.next_sequence, next_sequence);
        }
    }

    #[test]
    fn invalid_worker_glow_rejects_atomically_before_snapshot_or_delta_sequence() {
        let mut frame = mixed_frame();
        let valid = Arc::new(noon_compile::CompiledEffect {
            attachment: SemanticNodeId::new(99, 7),
            definition: (Glow::default()).into(),
        });
        frame.objects[0].effects = Some(effect_stack(&valid));
        let camera = Camera2DState::default();
        let context = PublicationContext::default();
        let mut encoder = RetainedExecutionDeltaEncoder::new(9);
        let snapshot = encoder
            .encode_snapshot_with_context(&frame, camera, context)
            .unwrap();
        let mut mirror = test_mirror();

        for corrupt in [
            {
                let mut corrupt = snapshot.clone();
                if let TransportEffectDefinition::Glow { intensity, .. } =
                    &mut corrupt.objects[0].effects[0].definition
                {
                    *intensity = 9.0;
                } else {
                    panic!("Glow fixture");
                }
                corrupt
            },
            {
                let mut corrupt = snapshot.clone();
                if let TransportEffectDefinition::Glow { radius, .. } =
                    &mut corrupt.objects[0].effects[0].definition
                {
                    *radius = EffectRadius::Scene(-1.0);
                } else {
                    panic!("Glow fixture");
                }
                corrupt
            },
            {
                let mut corrupt = snapshot.clone();
                if let TransportEffectDefinition::Glow { color, .. } =
                    &mut corrupt.objects[0].effects[0].definition
                {
                    color.red = f32::NAN;
                } else {
                    panic!("Glow fixture");
                }
                corrupt
            },
            {
                let mut corrupt = snapshot.clone();
                corrupt.objects[0].reveal = 1.5;
                corrupt
            },
            {
                let mut corrupt = snapshot.clone();
                corrupt.objects[0].content = TransportObjectContent::Image {
                    image: crate::TransportImageResourceHandle {
                        arena: 1,
                        id: 1,
                        version: 0,
                    },
                    sampling: crate::TransportImageSampling::Linear,
                };
                // Images now support glow, but never vector stroke paint.
                corrupt.objects[0].style.stroke = Some(Color::WHITE);
                corrupt
            },
        ] {
            let bad_slot = corrupt
                .objects
                .iter()
                .find(|row| !row.effects.is_empty() && validate_object_fields(row).is_err())
                .unwrap()
                .slot;
            assert_eq!(
                mirror.apply(corrupt),
                Err(RetainedExecutionTransportError::InvalidEffectState(
                    bad_slot
                ))
            );
            assert!(mirror.frame().is_none());
            assert!(mirror.session().is_none());
        }

        mirror.apply(snapshot).unwrap();
        assert_eq!(
            mirror.frame().unwrap().objects[0]
                .effects
                .as_ref()
                .and_then(|stack| stack.get(valid.attachment)),
            Some(*valid)
        );
        let next_sequence = mirror.next_sequence;
        let mut delta = encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![0]),
                camera,
                context,
            )
            .unwrap()
            .unwrap();
        if let TransportEffectDefinition::Glow { intensity, .. } =
            &mut delta.objects[0].effects[0].definition
        {
            *intensity = f64::INFINITY;
        } else {
            panic!("Glow fixture");
        }
        assert_eq!(
            mirror.apply(delta),
            Err(RetainedExecutionTransportError::InvalidEffectState(
                TransportSlotId {
                    slot: 0,
                    generation: 0
                }
            ))
        );
        assert_eq!(mirror.next_sequence, next_sequence);
        assert_eq!(
            mirror.frame().unwrap().objects[0]
                .effects
                .as_ref()
                .and_then(|stack| stack.get(valid.attachment)),
            Some(*valid)
        );

        frame.objects[1].effects = Some(effect_stack(&valid));
        frame.reveals[1] = 0.5;
        frame.objects[1].style = Style {
            stroke: Some(Color::WHITE),
            ..Style::default()
        };
        // The source text handle is resolved by the separately installed text
        // resource table; row validation admits its content and partial reveal.
        assert!(encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![1]),
                camera,
                context,
            )
            .unwrap()
            .is_some());
        frame.objects[0].content = ObjectContentRef::Geometry(GeometryRef::VectorPath(
            noon_core::VectorPath::new()
                .move_to(noon_core::Vec2::ZERO)
                .line_to(noon_core::Vec2::new(1.0, 1.0)),
        ));
        frame.objects[0].style = Style {
            stroke: Some(Color::WHITE),
            ..Style::default()
        };
        frame.reveals[0] = 0.25;
        assert!(encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![0]),
                camera,
                context,
            )
            .unwrap()
            .is_some());
        frame.objects[1].content =
            ObjectContentRef::Image(noon_core::RasterImageContentRef::from_resource(
                noon_core::SemanticImageContent::new(noon_core::RasterImageResourceHandle {
                    arena: 1,
                    id: noon_core::RasterImageResourceId::new(1),
                    version: 0,
                }),
                &noon_core::RasterImageResource::from_rgba8(1, 1, [255_u8; 4]).unwrap(),
            ));
        let before_sequence = encoder.next_sequence;
        assert_eq!(
            encoder.encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![1]),
                camera,
                context
            ),
            Err(RetainedExecutionTransportError::InvalidEffectState(
                TransportSlotId {
                    slot: 1,
                    generation: 0
                }
            ))
        );
        assert_eq!(encoder.next_sequence, before_sequence);
    }

    #[test]
    fn image_glow_transport_preserves_source_and_rejects_invalid_edits_atomically() {
        use noon_core::{
            RasterImageContentRef, RasterImageResource, RasterImageResourceHandle,
            RasterImageResourceId, RasterImageSampling, SemanticImageContent,
        };

        let resource =
            RasterImageResource::from_rgba8(2, 1, [255_u8, 0, 0, 255, 0, 0, 255, 64]).unwrap();
        let handle = RasterImageResourceHandle {
            arena: 41,
            id: RasterImageResourceId::new(9),
            version: 3,
        };
        for sampling in [
            RasterImageSampling::Nearest,
            RasterImageSampling::Linear,
            RasterImageSampling::Bicubic,
        ] {
            for source in [GlowSource::Painted, GlowSource::Silhouette] {
                let image = RasterImageContentRef::from_resource(
                    SemanticImageContent::with_sampling(handle, sampling),
                    &resource,
                );
                let mut frame = mixed_frame();
                frame.objects[0].content = image.into();
                frame.objects[0].style = Style {
                    fill: Some(Color::rgba(1.0, 1.0, 1.0, 0.4)),
                    stroke_width: 0.0,
                    opacity: 0.7,
                    ..Style::default()
                };
                let effect = Arc::new(noon_compile::CompiledEffect {
                    attachment: SemanticNodeId::new(99, 7),
                    definition: (Glow::new(EffectUpdate::default().source(source)).unwrap()).into(),
                });
                frame.objects[0].effects = Some(effect_stack(&effect));
                let camera = Camera2DState::default();
                let context = PublicationContext::default();
                let mut encoder = RetainedExecutionDeltaEncoder::new(9);
                let snapshot = encoder
                    .encode_snapshot_with_context(&frame, camera, context)
                    .unwrap();
                let mut mirror = test_mirror();
                assert_eq!(
                    mirror.apply(snapshot.clone()),
                    Err(RetainedExecutionTransportError::UnknownImageResource(
                        handle.into()
                    ))
                );
                assert!(mirror.frame().is_none());
                assert!(mirror.session().is_none());
                mirror.extend_installed_image_handles(&HashMap::from([(handle.into(), image)]));
                mirror.apply(snapshot).unwrap();
                let installed = &mirror.frame().unwrap().objects[0];
                assert_eq!(installed.content.image(), Some(image));
                assert_eq!(
                    installed
                        .effects
                        .as_ref()
                        .and_then(|stack| stack.get(effect.attachment)),
                    Some(*effect)
                );
                assert_eq!(installed.style, frame.objects[0].style);

                let changed = Arc::new(noon_compile::CompiledEffect {
                    attachment: effect.attachment,
                    definition: EffectUpdate::default()
                        .intensity(1.5)
                        .apply_to(effect.definition)
                        .unwrap(),
                });
                frame.objects[0].effects = Some(effect_stack(&changed));
                let delta = encoder
                    .encode_incremental_with_context(
                        &frame,
                        &FrameChanges::objects(vec![0]),
                        camera,
                        context,
                    )
                    .unwrap()
                    .unwrap();
                let sequence = mirror.next_sequence;
                let before = mirror.frame().unwrap().clone();
                let mut recolored = delta.clone();
                recolored.objects[0].style.fill = Some(Color::RED);
                assert!(matches!(
                    mirror.apply(recolored),
                    Err(RetainedExecutionTransportError::InvalidEffectState(_))
                ));
                let mut revealed = delta.clone();
                revealed.objects[0].reveal = 0.5;
                assert!(matches!(
                    mirror.apply(revealed),
                    Err(RetainedExecutionTransportError::ImageRenderGeometry(_))
                ));
                let mut morphed = delta.clone();
                morphed.objects[0].morph = 0.5;
                assert!(matches!(
                    mirror.apply(morphed),
                    Err(RetainedExecutionTransportError::InvalidEffectState(_))
                ));
                assert_eq!(mirror.next_sequence, sequence);
                assert_eq!(mirror.frame().unwrap(), &before);
                mirror.apply(delta).unwrap();
                assert_eq!(
                    mirror.frame().unwrap().objects[0].content.image(),
                    Some(image)
                );
                assert_eq!(
                    mirror.frame().unwrap().objects[0]
                        .effects
                        .as_ref()
                        .and_then(|stack| stack.get(changed.attachment)),
                    Some(*changed)
                );
                assert_eq!(mirror.frame().unwrap().objects[1], before.objects[1]);
            }
        }
    }

    #[test]
    fn worker_transport_round_trips_effective_spatial_pose_camera_and_material() {
        let mut frame = mixed_frame();
        let rotation = noon_core::SemanticRotation3D::from_axis_angle(
            noon_core::SemanticVec3::new(1.0, 2.0, -3.0),
            0.731,
        )
        .unwrap();
        frame.objects[0].spatial = Some(Box::new(noon_compile::CompiledSpatialState {
            draw_kind: noon_compile::CompiledSpatialDrawKind::Mesh,
            composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
            spatial_anchor_family: None,
            fixed_orientation_center: None,
            world: noon_core::SemanticWorldTransform3D::new(
                noon_core::SemanticVec3::new(1.0 / 3.0, -1.0e60, 1.0e-100),
                rotation,
                noon_core::SemanticVec3::new(0.75, 2.0, 1.25),
            )
            .unwrap(),
            camera_projection: None,
            material: noon_core::SemanticSpatialMaterial::PointLit,
            point_light: false,
            cairo_path_appearance: None,
            camera_profile: None,
            camera_motions: None,
        }));
        frame.objects[1].spatial = Some(Box::new(noon_compile::CompiledSpatialState {
            draw_kind: noon_compile::CompiledSpatialDrawKind::Planar,
            composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
            spatial_anchor_family: None,
            fixed_orientation_center: None,
            world: noon_core::SemanticWorldTransform3D::new(
                noon_core::SemanticVec3::new(0.0, 0.0, 8.0),
                rotation,
                noon_core::SemanticVec3::new(1.0, 1.0, 1.0),
            )
            .unwrap(),
            camera_projection: Some(noon_core::SemanticProjection3D::Perspective {
                vertical_fov_radians: 1.0,
                near: 0.1,
                far: 30.0,
            }),
            material: noon_core::SemanticSpatialMaterial::Unlit,
            point_light: false,
            cairo_path_appearance: None,
            camera_profile: None,
            camera_motions: None,
        }));
        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let publication_context = noon_core::PublicationContext::new(
            noon_core::SceneRevision::new(9_007_199_254_740_999),
            noon_core::ExecutionRevision::new(9_007_199_254_741_001),
            noon_core::FrameEpoch::new(9_007_199_254_741_003),
        );
        let snapshot = encoder
            .encode_snapshot_with_context(&frame, Camera2DState::default(), publication_context)
            .unwrap();
        let wire = serde_json::to_vec(&snapshot).unwrap();
        let decoded: RetainedExecutionDeltaEnvelope = serde_json::from_slice(&wire).unwrap();
        let mut mirror = test_mirror();
        assert_eq!(
            mirror.apply(decoded).unwrap().0,
            RetainedTransportApplyOutcome::Applied
        );
        assert_eq!(mirror.frame().unwrap(), &frame);
        assert_eq!(mirror.publication_context(), snapshot.publication_context);
        assert_eq!(
            mirror.camera_3d_object().unwrap().camera_projection(),
            frame.objects[1].camera_projection()
        );
        assert_eq!(
            mirror.frame().unwrap().objects[0]
                .world_transform()
                .unwrap()
                .rotation
                .components(),
            rotation.components()
        );
    }

    #[test]
    fn invalid_spatial_snapshot_is_rejected_without_installing_frame() {
        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let mut delta = encoder
            .encode_snapshot_with_context(
                &mixed_frame(),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        delta.objects[0].spatial = Some(TransportSpatialState {
            draw_kind: noon_compile::CompiledSpatialDrawKind::Mesh,
            composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
            fixed_orientation_center: None,
            translation: [0.0; 3],
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            scale: [1.0; 3],
            camera_projection: Some(TransportProjection3D::Perspective {
                vertical_fov_radians: f64::NAN,
                near: 0.1,
                far: 30.0,
            }),
            material: TransportSpatialMaterial::Unlit,
            point_light: false,
            cairo_path_appearance: None,
        });
        let mut mirror = RetainedExecutionFrameMirror::default();
        assert!(matches!(
            mirror.apply(delta),
            Err(RetainedExecutionTransportError::InvalidSpatialState(_))
        ));
        assert!(mirror.frame().is_none());
    }

    #[test]
    fn worker_spatial_rows_preserve_domains_and_reject_inconsistent_roles() {
        use noon_compile::CompiledSpatialDrawKind as Draw;
        use noon_core::SemanticSpatialCompositionDomain as Domain;
        let valid = TransportSpatialState {
            draw_kind: Draw::Planar,
            composition_domain: Domain::FixedOrientation,
            fixed_orientation_center: Some([1.0 / 3.0, -2.5, 4.0]),
            translation: [0.0; 3],
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            scale: [1.0; 3],
            camera_projection: None,
            material: TransportSpatialMaterial::Unlit,
            point_light: false,
            cairo_path_appearance: None,
        };
        let wire = serde_json::to_vec(&valid).unwrap();
        let decoded: TransportSpatialState = serde_json::from_slice(&wire).unwrap();
        assert_eq!(decoded, valid);
        let compiled = decoded.to_compiled().unwrap();
        assert_eq!(compiled.composition_domain, Domain::FixedOrientation);
        assert_eq!(compiled.spatial_anchor_family, None);
        assert_eq!(compiled.camera_profile, None);
        assert!(compiled.camera_motions.is_none());
        assert_eq!(TransportSpatialState::from_compiled(&compiled), valid);

        let cairo_surface = TransportSpatialState {
            draw_kind: Draw::Mesh,
            composition_domain: Domain::World,
            fixed_orientation_center: None,
            material: TransportSpatialMaterial::CairoSurface,
            ..valid.clone()
        };
        assert!(cairo_surface.is_valid());
        let compiled = cairo_surface.to_compiled().unwrap();
        assert_eq!(
            compiled.material,
            noon_core::SemanticSpatialMaterial::CairoSurface
        );
        assert_eq!(
            TransportSpatialState::from_compiled(&compiled),
            cairo_surface
        );

        let camera = TransportSpatialState {
            composition_domain: Domain::World,
            fixed_orientation_center: None,
            camera_projection: Some(TransportProjection3D::Perspective {
                vertical_fov_radians: 1.0,
                near: 0.1,
                far: 100.0,
            }),
            ..valid.clone()
        };
        assert!(camera.is_valid());
        let invalid = [
            TransportSpatialState {
                fixed_orientation_center: None,
                ..valid.clone()
            },
            TransportSpatialState {
                fixed_orientation_center: Some([f64::NAN, 0.0, 0.0]),
                ..valid.clone()
            },
            TransportSpatialState {
                draw_kind: Draw::Mesh,
                ..valid.clone()
            },
            TransportSpatialState {
                material: TransportSpatialMaterial::PointLit,
                ..valid.clone()
            },
            TransportSpatialState {
                material: TransportSpatialMaterial::CairoSurface,
                ..valid.clone()
            },
            TransportSpatialState {
                point_light: true,
                ..valid.clone()
            },
            TransportSpatialState {
                scale: [2.0; 3],
                ..camera.clone()
            },
            TransportSpatialState {
                draw_kind: Draw::Mesh,
                ..camera.clone()
            },
            TransportSpatialState {
                material: TransportSpatialMaterial::PointLit,
                ..camera.clone()
            },
            TransportSpatialState {
                point_light: true,
                ..camera.clone()
            },
            TransportSpatialState {
                composition_domain: Domain::FixedFrame,
                ..camera.clone()
            },
        ];
        for row in invalid {
            assert!(!row.is_valid(), "invalid role/domain combination: {row:?}");
            assert!(row.to_compiled().is_none());
        }
    }

    #[test]
    fn cairo_path_worker_rows_round_trip_and_reject_incomplete_gradient_bounds_atomically() {
        use noon_compile::{
            CompiledCairoPathAppearance, CompiledSpatialDrawKind, CompiledWorldBounds3D64,
        };
        use noon_core::SemanticSpatialCompositionDomain;
        let row = TransportSpatialState {
            draw_kind: CompiledSpatialDrawKind::Planar,
            composition_domain: SemanticSpatialCompositionDomain::World,
            fixed_orientation_center: None,
            translation: [1.0 / 3.0, -2.0, 4.0],
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            scale: [1.0; 3],
            camera_projection: None,
            material: TransportSpatialMaterial::CairoPath,
            point_light: false,
            cairo_path_appearance: Some(Box::new(CompiledCairoPathAppearance {
                sheen_factor: 0.2,
                gradient_direction: Some(SemanticVec3::new(-7.0, -9.0, 10.0)),
                world_family_bounds: Some(CompiledWorldBounds3D64 {
                    min: SemanticVec3::new(-5.25, -0.175, 0.0),
                    max: SemanticVec3::new(5.25, 0.175, 0.0),
                }),
            })),
        };
        assert!(row.is_valid());
        let decoded: TransportSpatialState =
            serde_json::from_slice(&serde_json::to_vec(&row).unwrap()).unwrap();
        assert_eq!(decoded, row);
        let compiled = decoded.to_compiled().unwrap();
        assert_eq!(compiled.material, SemanticSpatialMaterial::CairoPath);
        assert_eq!(TransportSpatialState::from_compiled(&compiled), row);

        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let mut snapshot = encoder
            .encode_snapshot_with_context(
                &mixed_frame(),
                Camera2DState::default(),
                PublicationContext::default(),
            )
            .unwrap();
        snapshot.objects[0].spatial = Some(row.clone());
        let mut mirror = test_mirror();
        mirror.apply(snapshot.clone()).unwrap();
        let installed = mirror.frame().unwrap().clone();
        let mut invalid = row.clone();
        invalid
            .cairo_path_appearance
            .as_deref_mut()
            .unwrap()
            .world_family_bounds = None;
        assert!(!invalid.is_valid());
        snapshot.sequence = 1;
        snapshot.objects[0].spatial = Some(invalid);
        assert!(matches!(
            mirror.apply(snapshot),
            Err(RetainedExecutionTransportError::InvalidSpatialState(_))
        ));
        assert_eq!(mirror.frame().unwrap(), &installed);
        let mut invalid = row.clone();
        invalid
            .cairo_path_appearance
            .as_deref_mut()
            .unwrap()
            .world_family_bounds
            .as_mut()
            .unwrap()
            .min
            .x = 6.0;
        assert!(!invalid.is_valid());
        let mut invalid = row.clone();
        invalid.material = TransportSpatialMaterial::Unlit;
        assert!(!invalid.is_valid());
        let mut invalid = row.clone();
        invalid.cairo_path_appearance = None;
        assert!(!invalid.is_valid());
        let ordinary = TransportSpatialState {
            material: TransportSpatialMaterial::Unlit,
            cairo_path_appearance: None,
            ..row
        };
        let json = serde_json::to_value(ordinary).unwrap();
        assert!(json.get("cairo_path_appearance").is_none());
    }

    #[test]
    fn incremental_spatial_world_track_replaces_one_effective_row_atomically() {
        let mut start = mixed_frame();
        start.objects[0].spatial = Some(Box::new(noon_compile::CompiledSpatialState {
            draw_kind: noon_compile::CompiledSpatialDrawKind::Planar,
            composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
            spatial_anchor_family: None,
            fixed_orientation_center: None,
            world: noon_core::SemanticWorldTransform3D::IDENTITY,
            camera_projection: Some(noon_core::SemanticProjection3D::Orthographic {
                height: 8.0,
                near: 0.1,
                far: 100.0,
            }),
            material: noon_core::SemanticSpatialMaterial::Unlit,
            point_light: false,
            cairo_path_appearance: None,
            camera_profile: None,
            camera_motions: None,
        }));
        let mut encoder = RetainedExecutionDeltaEncoder::new(9);
        let start_context = noon_core::PublicationContext::new(
            noon_core::SceneRevision::new(3),
            noon_core::ExecutionRevision::new(8),
            noon_core::FrameEpoch::new(8),
        );
        let snapshot = encoder
            .encode_snapshot_with_context(&start, Camera2DState::default(), start_context)
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(snapshot).unwrap();
        assert_eq!(mirror.publication_context(), start_context);

        let mut end = start.clone();
        end.time = 0.75;
        end.objects[0].spatial = Some(Box::new(noon_compile::CompiledSpatialState {
            draw_kind: noon_compile::CompiledSpatialDrawKind::Planar,
            composition_domain: noon_core::SemanticSpatialCompositionDomain::World,
            spatial_anchor_family: None,
            fixed_orientation_center: None,
            world: noon_core::SemanticWorldTransform3D::new(
                noon_core::SemanticVec3::new(2.25, -4.5, 7.0),
                noon_core::SemanticRotation3D::from_axis_angle(
                    noon_core::SemanticVec3::new(0.0, 1.0, 0.0),
                    0.8,
                )
                .unwrap(),
                noon_core::SemanticVec3::new(1.0, 1.0, 1.0),
            )
            .unwrap(),
            camera_projection: start.objects[0].camera_projection(),
            material: noon_core::SemanticSpatialMaterial::Unlit,
            point_light: false,
            cairo_path_appearance: None,
            camera_profile: None,
            camera_motions: None,
        }));
        let end_context = noon_core::PublicationContext::new(
            noon_core::SceneRevision::new(3),
            noon_core::ExecutionRevision::new(8),
            noon_core::FrameEpoch::new(9),
        );
        let delta = encoder
            .encode_incremental_with_context(
                &end,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                end_context,
            )
            .unwrap()
            .unwrap();
        assert_eq!(delta.objects.len(), 1);
        assert!(delta.object_patches.is_empty());
        let (_, changes) = mirror.apply(delta).unwrap();
        assert_eq!(mirror.publication_context(), end_context);
        assert_eq!(changes.object_indices(), &[0]);
        assert_eq!(mirror.frame().unwrap(), &end);
        assert_eq!(
            mirror.camera_3d_object().unwrap().world_transform(),
            end.objects[0].world_transform()
        );

        let mut stale_context = encoder
            .encode_incremental_with_context(
                &end,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                start_context,
            )
            .unwrap()
            .unwrap();
        stale_context.publication_context = start_context;
        assert!(matches!(
            mirror.apply(stale_context),
            Err(RetainedExecutionTransportError::StalePublicationContext { .. })
        ));
        assert_eq!(mirror.publication_context(), end_context);

        let mut next_session = RetainedExecutionDeltaEncoder::new(10);
        let replacement_session = next_session
            .encode_snapshot_with_context(
                &end,
                Camera2DState::default(),
                noon_core::PublicationContext::new(
                    noon_core::SceneRevision::new(1),
                    noon_core::ExecutionRevision::new(1),
                    noon_core::FrameEpoch::new(1),
                ),
            )
            .unwrap();
        mirror.apply(replacement_session).unwrap();
        assert_eq!(mirror.publication_context().frame_epoch().get(), 1);
    }

    #[test]
    fn mixed_geometry_and_text_snapshot_round_trips_in_one_order_stream() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let delta = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        assert_eq!(delta.objects.len(), 2);
        assert_eq!(delta.objects[0].order, 0);
        assert_eq!(delta.objects[1].order, 1);
        assert!(matches!(
            delta.objects[0].content,
            TransportObjectContent::Geometry { .. }
        ));
        assert!(matches!(
            delta.objects[1].content,
            TransportObjectContent::Text { .. }
        ));

        let json = serde_json::to_string(&delta).unwrap();
        let decoded: RetainedExecutionDeltaEnvelope = serde_json::from_str(&json).unwrap();
        let mut mirror = test_mirror();
        let (outcome, changes) = mirror.apply(decoded).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());
        assert_eq!(mirror.frame().unwrap(), &frame);
    }

    #[test]
    fn inset_view_round_trips_and_invalid_reference_is_rejected_atomically() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(41);
        let mut initial = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let view = Inset2DViewState {
            camera_frame: ObjectId::new(11),
            display: ObjectId::new(12),
            camera: Camera2DState {
                center: Vec2::new(0.25, -0.5),
                height: 0.45,
            },
            display_center: Vec2::new(4.6, 2.0),
            display_size: Vec2::new(3.0, 3.0),
            display_stroke_width: 0.03,
            capture_own_display: false,
        };
        initial.inset_2d_views.push(view);
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();
        assert_eq!(mirror.inset_2d_views(), &[view]);

        let mut next_frame = frame.clone();
        next_frame.time = 0.25;
        next_frame.objects[0].transform.translation.x = 0.5;
        let mut invalid = encoder
            .encode_incremental_with_context(
                &next_frame,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        invalid.inset_2d_views.push(Inset2DViewState {
            camera_frame: ObjectId::new(999),
            ..view
        });
        let expected_sequence = mirror.next_sequence;
        assert_eq!(
            mirror.apply(invalid),
            Err(RetainedExecutionTransportError::InvalidInset2DView(
                view.display
            ))
        );
        assert_eq!(mirror.next_sequence, expected_sequence);
        assert_eq!(mirror.inset_2d_views(), &[view]);
    }

    #[test]
    fn wire_text_requires_an_installed_resource_remap() {
        let frame = mixed_frame();
        let delta = RetainedExecutionDeltaEncoder::new(4)
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let mut mirror = RetainedExecutionFrameMirror::default();
        assert!(matches!(
            mirror.apply(delta),
            Err(RetainedExecutionTransportError::UnknownTextResource(_))
        ));
        assert!(mirror.frame().is_none());
    }

    #[test]
    fn incremental_geometry_content_update_preserves_slot_identity() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(8);
        let initial = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let mut updated = frame.clone();
        updated.time = 0.5;
        updated.objects[0].content = ObjectContentRef::Geometry(GeometryRef::rectangle(2.0, 1.0));
        let delta = encoder
            .encode_incremental_with_context(
                &updated,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();

        let (_, changes) = mirror.apply(delta).unwrap();
        assert_eq!(changes.object_indices(), &[0]);
        assert_eq!(mirror.frame().unwrap(), &updated);
    }

    #[test]
    fn object_patch_updates_existing_row_and_rejects_conflicts_atomically() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(81);
        let initial = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let slot = initial.objects[0].slot;
        let object = initial.objects[0].object;
        let mut mirror = test_mirror();
        let mut invalid_snapshot = initial.clone();
        invalid_snapshot
            .object_patches
            .push(RetainedTransportObjectPatch {
                slot,
                object,
                transform: None,
                style: None,
                morph: Some(0.25),
                render_translation: None,
            });
        assert_eq!(
            mirror.apply(invalid_snapshot),
            Err(RetainedExecutionTransportError::InvalidObjectPatch(slot))
        );
        assert!(mirror.frame().is_none());
        mirror.apply(initial).unwrap();

        let mut next = frame.clone();
        next.time = 0.5;
        next.objects[0].transform.translation = Vec2::new(3.0, 4.0);
        next.objects[0].style.fill = Some(Color::WHITE);
        next.morphs[0] = 0.25;
        let mut delta = encoder
            .encode_incremental_with_context(
                &next,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        delta.objects.clear();
        delta.object_patches.push(RetainedTransportObjectPatch {
            slot,
            object,
            transform: Some(next.objects[0].transform),
            style: Some(next.objects[0].style),
            morph: Some(next.morphs[0]),
            render_translation: None,
        });
        let json = serde_json::to_string(&delta).unwrap();
        let decoded = serde_json::from_str(&json).unwrap();
        let mut conflict = delta.clone();
        conflict
            .object_patches
            .push(conflict.object_patches[0].clone());
        assert_eq!(
            mirror.apply(conflict),
            Err(RetainedExecutionTransportError::DuplicateSlot(slot))
        );
        assert_eq!(mirror.applied_sequence(), Some(0));
        assert_eq!(mirror.frame(), Some(&frame));

        let (_, changes) = mirror.apply(decoded).unwrap();
        assert_eq!(changes.object_indices(), &[0]);
        assert_eq!(mirror.frame(), Some(&next));
    }

    #[test]
    fn render_translation_patch_preserves_frame_and_rejects_invalid_updates_atomically() {
        let mut frame = mixed_frame();
        frame.render_geometries[0] = Some(Arc::new(GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::ONE),
        )));
        frame.render_transforms[0] = Some(Transform2D {
            scale: Vec2::new(2.0, 3.0),
            rotation: 0.5,
            ..Transform2D::IDENTITY
        });
        let mut encoder = RetainedExecutionDeltaEncoder::new(82);
        let initial = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let slot = initial.objects[0].slot;
        let object = initial.objects[0].object;
        let mut mirror = test_mirror();
        mirror.apply(initial.clone()).unwrap();
        let installed_geometry = mirror.frame().unwrap().render_geometries[0]
            .clone()
            .unwrap();
        let mut next = frame.clone();
        next.time = 0.5;
        next.objects[1].transform.translation = Vec2::new(3.0, 4.0);
        next.render_transforms[0].as_mut().unwrap().translation = Vec2::new(8.0, 9.0);
        let mut delta = encoder
            .encode_incremental_with_context(
                &next,
                &FrameChanges::objects(vec![0, 1]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        delta.objects.retain(|row| row.object != object);
        delta.object_patches.push(RetainedTransportObjectPatch {
            slot,
            object,
            transform: None,
            style: None,
            morph: None,
            render_translation: next.render_transforms[0].map(|transform| transform.translation),
        });
        // The valid full text row must also remain unapplied when a later patch
        // is rejected. Every component uses the same finite validation as rows.
        for component in 0..2 {
            let mut invalid = delta.clone();
            let translation = invalid.object_patches[0]
                .render_translation
                .as_mut()
                .unwrap();
            match component {
                0 => translation.x = f32::NAN,
                _ => translation.y = f32::INFINITY,
            }
            assert_eq!(
                mirror.apply(invalid),
                Err(RetainedExecutionTransportError::InvalidRenderTransform(
                    slot
                ))
            );
            assert_eq!(mirror.frame(), Some(&frame));
            assert_eq!(mirror.applied_sequence(), Some(0));
        }
        let mut inactive = delta.clone();
        inactive.objects.clear();
        inactive.object_patches[0].slot = initial.objects[1].slot;
        inactive.object_patches[0].object = initial.objects[1].object;
        assert_eq!(
            mirror.apply(inactive),
            Err(RetainedExecutionTransportError::InvalidRenderTransform(
                initial.objects[1].slot
            ))
        );
        assert_eq!(mirror.frame(), Some(&frame));
        assert_eq!(mirror.applied_sequence(), Some(0));

        let decoded = serde_json::from_slice(&serde_json::to_vec(&delta).unwrap()).unwrap();
        let (_, changes) = mirror.apply(decoded).unwrap();
        assert_eq!(changes.object_indices(), &[0, 1]);
        assert_eq!(mirror.frame(), Some(&next));
        assert!(Arc::ptr_eq(
            &installed_geometry,
            mirror.frame().unwrap().render_geometries[0]
                .as_ref()
                .unwrap()
        ));
        let (outcome, changes) = mirror.apply(delta).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::DroppedStale);
        assert!(changes.object_indices().is_empty());
        assert_eq!(mirror.frame(), Some(&next));
        assert_eq!(mirror.applied_sequence(), Some(1));
    }

    #[test]
    fn incremental_transform_keeps_text_content_identity() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(9);
        let initial = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let mut updated = frame.clone();
        updated.time = 0.5;
        updated.objects[1].transform.translation = Vec2::new(2.0, -1.0);
        let delta = encoder
            .encode_incremental_with_context(
                &updated,
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(delta.objects.len(), 1);
        assert!(matches!(
            delta.objects[0].content,
            TransportObjectContent::Text { .. }
        ));

        let (_, changes) = mirror.apply(delta).unwrap();
        assert_eq!(changes.object_indices(), &[1]);
        assert_eq!(mirror.frame().unwrap(), &updated);
    }

    #[test]
    fn compact_painter_splice_reorders_and_replaces_without_resending_live_rows() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(19);
        let initial = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let reorder = encoder
            .encode_incremental_with_painter_order_and_context(
                &frame,
                &FrameChanges::painter_order(0..2),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
                &[1, 0],
            )
            .unwrap()
            .unwrap();
        assert!(reorder.objects.is_empty());
        assert!(reorder.removed_slots.is_empty());
        assert_eq!(reorder.painter_order.as_ref().unwrap().slots.len(), 2);
        let (_, changes) = mirror.apply(reorder).unwrap();
        assert_eq!(changes.painter_order_range(), Some(0..2));
        assert_eq!(mirror.painter_order(), &[1, 0]);

        let mut replaced = frame.clone();
        replaced.objects.push(FrameObjectState {
            fill_paint: None,
            draw_enabled: true,
            effects: None,
            spatial: None,
            z_index: 0.0,
            id: ObjectId::new(13),
            content: ObjectContentRef::Geometry(GeometryRef::rectangle(3.0, 1.0)),
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            text_bounds: None,
        });
        replaced.presences.push(true);
        replaced.reveals.push(1.0);
        replaced.morphs.push(0.0);
        replaced.render_geometries.push(None);
        replaced.render_transforms.push(None);
        let structural =
            FrameChanges::with_structure(vec![0, 2], vec![2], vec![0]).with_painter_order(0..2);
        let replace = encoder
            .encode_incremental_with_painter_order_and_context(
                &replaced,
                &structural,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
                &[2, 1],
            )
            .unwrap()
            .unwrap();
        assert_eq!(replace.objects.len(), 1);
        assert_eq!(replace.removed_slots.len(), 1);
        let mut malformed = replace.clone();
        malformed.painter_order.as_mut().unwrap().slots[1] =
            malformed.painter_order.as_ref().unwrap().slots[0];
        let before_frame = mirror.frame().unwrap().clone();
        let before_order = mirror.painter_order().to_vec();
        assert!(matches!(
            mirror.apply(malformed),
            Err(RetainedExecutionTransportError::DuplicateSlot(_))
        ));
        assert_eq!(mirror.frame().unwrap(), &before_frame);
        assert_eq!(mirror.painter_order(), before_order);

        let (_, changes) = mirror.apply(replace).unwrap();
        assert_eq!(changes.added_indices(), &[2]);
        assert_eq!(changes.removed_indices(), &[0]);
        assert_eq!(changes.painter_order_range(), Some(0..2));
        assert_eq!(mirror.painter_order(), &[2, 1]);
        assert_eq!(mirror.frame().unwrap().objects[2].id, ObjectId::new(13));
    }

    #[test]
    fn removed_text_slot_is_absent_after_sparse_painter_update() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(29);
        let initial = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let mut removed = frame.clone();
        removed.time = 1.0;
        removed.presences[1] = false;
        let changes =
            FrameChanges::with_structure(vec![1], Vec::new(), vec![1]).with_painter_order(1..2);
        let delta = encoder
            .encode_incremental_with_painter_order_and_context(
                &removed,
                &changes,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
                &[0],
            )
            .unwrap()
            .unwrap();

        assert!(delta.objects.is_empty());
        let (_, applied) = mirror.apply(delta).unwrap();
        assert_eq!(applied.removed_indices(), &[1]);
        assert_eq!(mirror.painter_order(), &[0]);
        let installed = mirror.frame().unwrap();
        assert!(!installed.presences[1]);
        assert!(installed.render_geometries[1].is_none());
        assert!(installed.render_transforms[1].is_none());
    }

    #[test]
    fn coalesced_readd_replace_publishes_only_net_membership() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(23);
        let initial = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let removed = FrameChanges::with_structure(vec![0, 1], Vec::new(), vec![0, 1])
            .with_painter_order(0..2);
        let empty = encoder
            .encode_incremental_with_painter_order_and_context(
                &frame,
                &removed,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
                &[],
            )
            .unwrap()
            .unwrap();
        mirror.apply(empty).unwrap();
        assert!(mirror.painter_order().is_empty());

        let mut replaced = frame.clone();
        replaced.objects.push(FrameObjectState {
            fill_paint: None,
            draw_enabled: true,
            effects: None,
            spatial: None,
            z_index: 0.0,
            id: ObjectId::new(13),
            content: ObjectContentRef::Geometry(GeometryRef::rectangle(3.0, 1.0)),
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            text_bounds: None,
        });
        replaced.presences.push(true);
        replaced.reveals.push(1.0);
        replaced.morphs.push(0.0);
        replaced.render_geometries.push(None);
        replaced.render_transforms.push(None);
        let structural = FrameChanges::with_structure(vec![0, 1, 2], vec![0, 1, 2], vec![0, 1])
            .with_painter_order(0..2);
        let replacement = encoder
            .encode_incremental_with_painter_order_and_context(
                &replaced,
                &structural,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
                &[2],
            )
            .unwrap()
            .unwrap();
        assert!(replacement.removed_slots.is_empty());
        mirror.apply(replacement).unwrap();
        assert_eq!(mirror.painter_order(), &[2]);

        replaced.objects[2].appearance = 0.5;
        let mut later = encoder
            .encode_incremental_with_context(
                &replaced,
                &FrameChanges::objects(vec![2]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        later.objects[0].order = 2;
        mirror.apply(later).unwrap();
        assert_eq!(mirror.painter_order(), &[2]);
        assert_eq!(mirror.frame().unwrap().objects[2].appearance, 0.5);
    }

    #[test]
    fn coalesced_remove_readd_of_live_row_publishes_final_live_state() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(31);
        let initial = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let mut readded = frame.clone();
        readded.time = 0.5;
        readded.reveals[0] = 0.5;
        let structural =
            FrameChanges::with_structure(vec![0], vec![0], vec![0]).with_painter_order(0..1);
        let delta = encoder
            .encode_incremental_with_painter_order_and_context(
                &readded,
                &structural,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
                &[0, 1],
            )
            .unwrap()
            .unwrap();

        assert!(delta.removed_slots.is_empty());
        assert_eq!(delta.objects.len(), 1);
        assert_eq!(delta.objects[0].object, ObjectId::new(11));
        assert_eq!(delta.objects[0].reveal, 0.5);
        let (_, applied) = mirror.apply(delta).unwrap();
        assert!(applied.added_indices().is_empty());
        assert!(applied.removed_indices().is_empty());
        assert_eq!(mirror.painter_order(), &[0, 1]);
        assert_eq!(mirror.frame().unwrap().reveals[0], 0.5);
    }

    #[test]
    fn incremental_text_resource_change_preserves_slot_identity() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(10);
        let initial = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let mut changed = frame.clone();
        changed.objects[1].content = ObjectContentRef::Text(TextResourceHandle {
            arena: 0,
            id: TextResourceId::new(8),
            version: 1,
        });
        let delta = encoder
            .encode_incremental_with_context(
                &changed,
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        let (_, changes) = mirror.apply(delta).unwrap();
        assert_eq!(changes.object_indices(), &[1]);
        assert_eq!(mirror.frame().unwrap(), &changed);
    }

    #[test]
    fn incremental_content_swap_preserves_slot_and_unrelated_rows() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(3);
        let initial = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let mut mirror = test_mirror();
        mirror.apply(initial).unwrap();

        let mut changed = frame.clone();
        changed.objects[1].content = ObjectContentRef::Geometry(GeometryRef::circle(0.5));
        let delta = encoder
            .encode_incremental_with_context(
                &changed,
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        let (outcome, changes) = mirror.apply(delta).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert_eq!(changes.object_indices(), &[1]);
        assert!(changes.added_indices().is_empty());
        assert!(changes.removed_indices().is_empty());
        assert_eq!(mirror.frame().unwrap(), &changed);
    }

    #[test]
    fn text_never_accepts_transient_geometry() {
        let mut frame = mixed_frame();
        frame.render_geometries[1] = Some(Arc::new(GeometryRef::circle(0.25)));
        let mut encoder = RetainedExecutionDeltaEncoder::new(1);
        assert!(matches!(
            encoder.encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default()
            ),
            Err(RetainedExecutionTransportError::TextRenderGeometry(_))
        ));
    }

    #[test]
    fn continuation_morph_geometry_is_inlined_once_then_reused_by_slot() {
        let mut frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(37);
        let mut mirror = test_mirror();
        let initial = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        mirror.apply(initial).unwrap();

        let path = Arc::new(GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1.0, 0.0)),
        ));
        frame.time = 0.25;
        frame.morphs[0] = 0.25;
        frame.render_geometries[0] = Some(path.clone());
        frame.render_transforms[0] = Some(Transform2D::IDENTITY);
        let first = encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        assert!(first.objects[0].render_geometry.is_some());
        assert!(serde_json::to_string(&first).unwrap().contains("line_to"));
        mirror.apply(first).unwrap();
        assert_eq!(mirror.frame(), Some(&frame));

        frame.time = 0.5;
        frame.morphs[0] = 0.5;
        let reused = encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        assert!(reused.objects[0].render_geometry.is_none());
        assert!(reused.objects[0].render_geometry_resource.is_none());
        assert!(reused.objects[0].render_transform.is_some());
        assert!(!serde_json::to_string(&reused).unwrap().contains("line_to"));
        mirror.apply(reused).unwrap();
        assert_eq!(mirror.frame(), Some(&frame));

        frame.time = 0.75;
        frame.morphs[0] = 0.75;
        frame.render_geometries[0] = Some(Arc::new(GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(2.0, 0.0)),
        )));
        let changed = encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        assert!(changed.objects[0].render_geometry.is_some());
        mirror.apply(changed).unwrap();
        assert_eq!(mirror.frame(), Some(&frame));

        frame.time = 1.0;
        frame.morphs[0] = 1.0;
        frame.render_geometries[0] = None;
        frame.render_transforms[0] = None;
        let cleared = encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        assert!(cleared.objects[0].render_geometry.is_none());
        assert!(cleared.objects[0].render_transform.is_none());
        mirror.apply(cleared).unwrap();
        assert_eq!(mirror.frame(), Some(&frame));

        frame.time = 1.25;
        frame.morphs[0] = 0.25;
        frame.render_geometries[0] = Some(path);
        frame.render_transforms[0] = Some(Transform2D::IDENTITY);
        let reentered = encoder
            .encode_incremental_with_context(
                &frame,
                &FrameChanges::objects(vec![0]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        assert!(reentered.objects[0].render_geometry.is_some());
        mirror.apply(reentered).unwrap();
        assert_eq!(mirror.frame(), Some(&frame));
    }

    #[test]
    fn immutable_morph_resources_round_trip_without_inline_paths_and_survive_seek() {
        let path = Arc::new(GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1.0, 0.0)),
        ));
        let resources: Arc<[Arc<GeometryRef>]> = vec![path.clone()].into();
        let mut frame = mixed_frame();
        frame.render_geometries[0] = Some(path);
        frame.render_transforms[0] = Some(Transform2D::IDENTITY);
        let mut encoder =
            RetainedExecutionDeltaEncoder::with_render_geometries(4, resources.clone());
        let mut mirror = test_mirror_with_render_geometries(4, resources);
        for (step, time) in [0.0, 0.25, 0.75, 0.0].into_iter().enumerate() {
            frame.time = time;
            frame.morphs[0] = time as f32;
            let delta = if step == 0 || step == 3 {
                encoder
                    .encode_snapshot_with_context(
                        &frame,
                        Camera2DState::default(),
                        noon_core::PublicationContext::default(),
                    )
                    .unwrap()
            } else {
                encoder
                    .encode_incremental_with_context(
                        &frame,
                        &FrameChanges::objects(vec![0]),
                        Camera2DState::default(),
                        noon_core::PublicationContext::default(),
                    )
                    .unwrap()
                    .unwrap()
            };
            assert_eq!(delta.objects[0].render_geometry_resource, Some(0));
            assert!(delta.objects[0].render_geometry.is_none());
            let json = serde_json::to_string(&delta).unwrap();
            assert!(!json.contains("line_to"));
            mirror.apply(serde_json::from_str(&json).unwrap()).unwrap();
            assert_eq!(mirror.frame(), Some(&frame));
        }
    }

    #[test]
    fn unregistered_compiled_path_cannot_silently_fall_back_to_inline_transport() {
        let path = noon_core::VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, 0.0));
        let mut encoder = RetainedExecutionDeltaEncoder::with_render_geometries(
            4,
            vec![Arc::new(GeometryRef::path(path.clone()))].into(),
        );
        let mut frame = mixed_frame();
        frame.render_geometries[0] = Some(Arc::new(GeometryRef::path(path)));
        frame.render_transforms[0] = Some(Transform2D::IDENTITY);
        assert!(matches!(
            encoder.encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default()
            ),
            Err(RetainedExecutionTransportError::MissingCompiledRenderResource(_))
        ));
    }

    #[test]
    fn invalid_later_resource_row_does_not_publish_earlier_rows_or_consume_sequence() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let mut mirror = test_mirror();
        mirror
            .apply(
                encoder
                    .encode_snapshot_with_context(
                        &frame,
                        Camera2DState::default(),
                        noon_core::PublicationContext::default(),
                    )
                    .unwrap(),
            )
            .unwrap();
        let mut changed = frame.clone();
        changed.time = 0.5;
        changed.objects[1].transform.translation.x = 2.0;
        let mut valid = encoder
            .encode_incremental_with_context(
                &changed,
                &FrameChanges::objects(vec![1, 0]),
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap()
            .unwrap();
        valid.objects.swap(0, 1);
        let mut invalid = valid.clone();
        invalid.objects[1].render_geometry_resource = Some(99);
        assert!(matches!(
            mirror.apply(invalid),
            Err(RetainedExecutionTransportError::InvalidRenderGeometryResource(99))
        ));
        assert_eq!(mirror.frame(), Some(&frame));
        mirror.apply(valid).unwrap();
        assert_eq!(mirror.frame(), Some(&changed));
    }

    #[test]
    fn geometry_resource_indices_are_scoped_to_the_installed_session() {
        let path = Arc::new(GeometryRef::path(noon_core::VectorPath::new()));
        let resources: Arc<[Arc<GeometryRef>]> = vec![path.clone()].into();
        let mut frame = mixed_frame();
        frame.render_geometries[0] = Some(path);
        let mut encoder =
            RetainedExecutionDeltaEncoder::with_render_geometries(5, resources.clone());
        let delta = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let mut mirror = test_mirror_with_render_geometries(4, resources);
        assert!(matches!(
            mirror.apply(delta),
            Err(RetainedExecutionTransportError::InvalidRenderGeometryResource(0))
        ));
        assert!(mirror.frame().is_none());
    }

    #[test]
    fn high_generation_render_resource_survives_json_metadata_roundtrip() {
        let frame = mixed_frame();
        let mut encoder = RetainedExecutionDeltaEncoder::new(4);
        let mut delta = encoder
            .encode_snapshot_with_context(
                &frame,
                Camera2DState::default(),
                noon_core::PublicationContext::default(),
            )
            .unwrap();
        let id = crate::retained_resource_transport::render_geometry_id(7, 2_200_000);
        assert!(id > (1_u64 << 53));
        delta.objects[0].render_geometry_resource = Some(id);
        let json = serde_json::to_string(&delta).unwrap();
        assert!(json.contains(&format!("\"render_geometry_resource\":\"{id}\"")));
        let reparsed: RetainedExecutionDeltaEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(reparsed.objects[0].render_geometry_resource, Some(id));
    }
}

#[cfg(test)]
mod z_index_tests;
