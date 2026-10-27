use noon_compile::{CompiledResources, ExecutionMutationTransaction, ExecutionPatch};
use noon_core::{
    ExecutionRevision, FrameEpoch, Property, PublicationContext, SceneRevision, Style, Transform2D,
};

use super::runtime_transaction::{final_value_writes, AuthoredPublicationError};
use crate::{FrameRowState, FrameState, RuntimeIdentity, SceneInstance};

#[derive(Clone, Debug)]
enum PreparedAuthoredValueWrite {
    Transform {
        object_index: usize,
        transform: Transform2D,
        changes_execution: bool,
    },
    SemanticTransform {
        object_index: usize,
        base_transform: Transform2D,
        spatial: Option<Box<noon_compile::CompiledSpatialState>>,
        compiled_spatial: Option<Box<noon_compile::CompiledSpatialState>>,
        changes_execution: bool,
    },
    Effect {
        object_index: usize,
        effects: std::sync::Arc<noon_compile::CompiledEffectStack>,
        effective: std::sync::Arc<noon_compile::CompiledEffectStack>,
        changes_execution: bool,
    },
    Style {
        object_index: usize,
        style: Style,
        fill_paint: Option<std::sync::Arc<noon_core::SemanticPaint>>,
        changes_execution: bool,
    },
}

impl PreparedAuthoredValueWrite {
    const fn object_index(&self) -> usize {
        match self {
            Self::Transform { object_index, .. }
            | Self::SemanticTransform { object_index, .. }
            | Self::Style { object_index, .. }
            | Self::Effect { object_index, .. } => *object_index,
        }
    }

    const fn changes_execution(&self) -> bool {
        match self {
            Self::Transform {
                changes_execution, ..
            }
            | Self::Style {
                changes_execution, ..
            }
            | Self::SemanticTransform {
                changes_execution, ..
            }
            | Self::Effect {
                changes_execution, ..
            } => *changes_execution,
        }
    }
}

/// Runtime proof for one already-semantic-prepared batch containing only local
/// transform, spatial-routing, style, or existing glow parameter writes.
///
/// Construction validates the exact Runtime identity/context, complete compiled
/// transaction, object slots, and revision capacity. The execution-session owner
/// then holds this proof under exclusive control until semantic commit, so commit
/// itself contains no validation or fallible Runtime operation.
#[derive(Clone, Debug)]
pub struct PreparedAuthoredValuePublication {
    runtime: RuntimeIdentity,
    expected: PublicationContext,
    scene_revision: SceneRevision,
    execution_revision: ExecutionRevision,
    frame_epoch: FrameEpoch,
    resource_additions: CompiledResources,
    writes: Vec<PreparedAuthoredValueWrite>,
}

impl SceneInstance {
    /// Prepare the narrow authored-value publication contract for local rows.
    ///
    /// `Ok(None)` means the transaction contains more than supported local base
    /// writes and must stay on the existing general publication path.
    pub fn prepare_authored_value_publication(
        &self,
        transaction: &ExecutionMutationTransaction,
        resource_additions: CompiledResources,
        expected: PublicationContext,
        scene_revision: SceneRevision,
    ) -> Result<Option<PreparedAuthoredValuePublication>, AuthoredPublicationError> {
        self.require_replay_writable()?;
        if self.replay_scope_active() {
            return Ok(None);
        }
        if transaction.mutations().iter().any(|patch| {
            !matches!(
                patch,
                ExecutionPatch::SetTransform { .. }
                    | ExecutionPatch::SetSemanticTransform { .. }
                    | ExecutionPatch::SetSpatialState { .. }
                    | ExecutionPatch::SetStyle { .. }
                    | ExecutionPatch::SetEffects { .. }
            )
        }) {
            return Ok(None);
        }

        let current = self.publication_context();
        if expected != current {
            return Err(AuthoredPublicationError::StalePublication {
                expected,
                actual: current,
            });
        }
        let scene_changed = scene_revision != current.scene_revision();
        if scene_changed && current.scene_revision().checked_next() != Some(scene_revision) {
            return Err(AuthoredPublicationError::InvalidSceneRevision {
                current: current.scene_revision(),
                proposed: scene_revision,
            });
        }

        // Validate every submitted write, including one superseded by a later value,
        // before retaining only the final value in each object/property lane.
        self.compiled
            .preflight_execution_transaction_with_resources(transaction, &resource_additions)?;
        let mut writes = Vec::new();
        let mut execution_changed = !self.compiled.resources().contains_all(&resource_additions);
        for patch in final_value_writes(transaction) {
            let (object, prepared) = match patch {
                ExecutionPatch::SetTransform { object, .. } => (*object, 0_u8),
                ExecutionPatch::SetSemanticTransform { object, .. } => (*object, 2_u8),
                ExecutionPatch::SetSpatialState { object, .. } => (*object, 2_u8),
                ExecutionPatch::SetStyle { object, .. } => (*object, 1_u8),
                ExecutionPatch::SetEffects { object, .. } => (*object, 3_u8),
                _ => return Ok(None),
            };
            let object_index = self
                .compiled
                .object_index(object)
                .ok_or(noon_compile::CompilePatchError::UnknownObject(object))?
                as usize;
            let changes_execution = self.compiled.patch_changes_execution(patch);
            execution_changed |= changes_execution;
            writes.push(match (prepared, patch) {
                (0, ExecutionPatch::SetTransform { transform, .. }) => {
                    PreparedAuthoredValueWrite::Transform {
                        object_index,
                        transform: *transform,
                        changes_execution,
                    }
                }
                (
                    1,
                    ExecutionPatch::SetStyle {
                        style, fill_paint, ..
                    },
                ) => PreparedAuthoredValueWrite::Style {
                    object_index,
                    style: *style,
                    fill_paint: fill_paint.clone(),
                    changes_execution,
                },
                (2, ExecutionPatch::SetSemanticTransform { transform, .. }) => {
                    let (base_transform, spatial) = self
                        .compiled
                        .prepare_semantic_transform_value(object_index as u32, *transform)
                        .ok_or(noon_compile::CompilePatchError::InvalidObjectState {
                            object,
                            field: noon_core::ObjectStateField::Transform,
                        })?;
                    let spatial = spatial.map(Box::new);
                    PreparedAuthoredValueWrite::SemanticTransform {
                        compiled_spatial: spatial.clone(),
                        object_index,
                        base_transform,
                        spatial,
                        changes_execution,
                    }
                }
                (
                    2,
                    ExecutionPatch::SetSpatialState {
                        base_transform,
                        spatial,
                        ..
                    },
                ) => PreparedAuthoredValueWrite::SemanticTransform {
                    object_index,
                    base_transform: *base_transform,
                    compiled_spatial: spatial.clone().map(Box::new),
                    spatial: spatial.clone().map(Box::new),
                    changes_execution,
                },
                (3, ExecutionPatch::SetEffects { effects, .. }) => {
                    PreparedAuthoredValueWrite::Effect {
                        object_index,
                        effects: std::sync::Arc::clone(effects),
                        effective: self.prepare_effective_effect_values(object_index, effects),
                        changes_execution,
                    }
                }
                _ => unreachable!("ordinary value publication classified above"),
            });
        }

        let execution_revision = if execution_changed {
            current.execution_revision().checked_next().ok_or(
                AuthoredPublicationError::ExecutionRevisionExhausted(current.execution_revision()),
            )?
        } else {
            current.execution_revision()
        };
        let frame_epoch = if scene_changed || execution_changed {
            current.frame_epoch().checked_next().ok_or(
                AuthoredPublicationError::FrameEpochExhausted(current.frame_epoch()),
            )?
        } else {
            current.frame_epoch()
        };

        Ok(Some(PreparedAuthoredValuePublication {
            runtime: self.runtime_identity(),
            expected,
            scene_revision,
            execution_revision,
            frame_epoch,
            resource_additions,
            writes,
        }))
    }

    /// Commit an authored local-value publication after its matching Semantic Scene
    /// transaction has crossed the point of no return.
    ///
    /// The execution-session `PreparedPublication` owns exclusive access between
    /// preparation and this call. The complete immutable resource set is already
    /// owned and validated; no fallible compilation or preparation remains here.
    pub fn commit_prepared_authored_value_publication(
        &mut self,
        prepared: PreparedAuthoredValuePublication,
    ) -> &FrameState {
        debug_assert_eq!(prepared.runtime, self.runtime_identity());
        debug_assert_eq!(prepared.expected, self.publication_context());
        self.last_patch_stats = crate::RuntimePatchStats::default();
        self.compiled
            .merge_prepared_resources(prepared.resource_additions);

        for write in prepared.writes {
            if !write.changes_execution() {
                continue;
            }
            let object_index = write.object_index();
            // Effect has no geometry/layout dependencies. Its active channel values
            // and any Arc copy-on-write were already prepared before semantic commit.
            if let PreparedAuthoredValueWrite::Effect {
                effects, effective, ..
            } = write
            {
                self.compiled
                    .commit_prepared_effect_values(object_index as u32, effects);
                if self.frame.objects[object_index].effects.as_ref() != Some(&effective) {
                    self.mark_nonspatial_changed(object_index);
                }
                self.frame.objects[object_index].effects = Some(effective);
                continue;
            }
            let before = FrameRowState::from_frame(&self.frame, object_index);
            if let Some(anchor_family) = self.frame.objects[object_index]
                .spatial
                .as_deref()
                .and_then(|spatial| spatial.spatial_anchor_family)
            {
                if let Some(group) = self.compiled.spatial_anchor_group_for_anchor(anchor_family) {
                    self.pending_spatial_anchor_groups.insert(group);
                }
            }
            match write {
                PreparedAuthoredValueWrite::Effect { .. } => {
                    unreachable!("glow values were committed without spatial invalidation above")
                }
                PreparedAuthoredValueWrite::Transform {
                    transform,
                    object_index,
                    ..
                } => {
                    self.compiled
                        .commit_prepared_transform_value(object_index as u32, transform);
                    self.frame.release_render_transform(object_index);
                    self.frame.objects[object_index].transform = transform;
                    self.reapply_properties(
                        object_index,
                        &[
                            Property::Transform,
                            Property::Position,
                            Property::Rotation,
                            Property::Scale,
                        ],
                    );
                }
                PreparedAuthoredValueWrite::SemanticTransform {
                    base_transform,
                    spatial,
                    compiled_spatial,
                    object_index,
                    ..
                } => {
                    self.compiled.commit_prepared_semantic_transform_value(
                        object_index as u32,
                        base_transform,
                        compiled_spatial,
                    );
                    self.frame.release_render_transform(object_index);
                    self.frame.objects[object_index].transform = base_transform;
                    match spatial {
                        Some(next) => {
                            if let Some(current) =
                                self.frame.objects[object_index].spatial.as_deref_mut()
                            {
                                *current = *next;
                            } else {
                                self.frame.objects[object_index].spatial = Some(next);
                            }
                        }
                        None => self.frame.objects[object_index].spatial = None,
                    }
                    self.reapply_properties(
                        object_index,
                        &[
                            Property::Transform,
                            Property::Position,
                            Property::Rotation,
                            Property::Scale,
                            Property::WorldTransform,
                            Property::CameraProfile,
                        ],
                    );
                }
                PreparedAuthoredValueWrite::Style {
                    style,
                    fill_paint,
                    object_index,
                    ..
                } => {
                    self.compiled.commit_prepared_style_value(
                        object_index as u32,
                        style,
                        fill_paint.clone(),
                    );
                    self.frame.objects[object_index].style = style;
                    self.frame.objects[object_index].fill_paint = fill_paint;
                    self.reapply_properties(
                        object_index,
                        &[
                            Property::Transform,
                            Property::Fill,
                            Property::Stroke,
                            Property::StrokeWidth,
                            Property::Opacity,
                        ],
                    );
                }
            }
            self.reapply_reactive_for_object(object_index);
            self.reapply_numeric_text_for_object(object_index);
            if before.differs_from_frame(&self.frame, object_index) {
                if before.spatially_differs_from_frame(&self.frame, object_index) {
                    self.mark_changed(object_index);
                } else {
                    self.mark_nonspatial_changed(object_index);
                }
            }
        }

        self.flush_spatial_anchor_changes();

        self.publication = PublicationContext::new(
            prepared.scene_revision,
            prepared.execution_revision,
            prepared.frame_epoch,
        );
        &self.frame
    }
}

impl SceneInstance {
    pub(crate) fn prepare_effective_effect_values(
        &self,
        object_index: usize,
        base: &std::sync::Arc<noon_compile::CompiledEffectStack>,
    ) -> std::sync::Arc<noon_compile::CompiledEffectStack> {
        let mut effective = std::sync::Arc::clone(base);
        for binding in base.iter() {
            let value = self.prepare_effective_attachment_value(object_index, binding);
            if value != binding {
                std::sync::Arc::make_mut(&mut effective).replace(value);
            }
        }
        effective
    }

    fn prepare_effective_attachment_value(
        &self,
        object_index: usize,
        base: noon_compile::CompiledEffect,
    ) -> noon_compile::CompiledEffect {
        let previous = self.compiled.objects()[object_index]
            .effects
            .as_ref()
            .and_then(|stack| stack.get(base.attachment));
        let mut effective = self.frame.objects[object_index]
            .effects
            .as_ref()
            .and_then(|stack| stack.get(base.attachment))
            .unwrap_or(base);
        // Persistent edits own only the authored parameters they change. Preserve
        // another parameter's effective driver, including an input-owned pulse.
        for property in noon_compile::CompiledEffect::PARAMETER_PROPERTIES {
            let next = noon_core::EffectTrackValue::from_definition(property, base.definition);
            let old = previous.and_then(|value| {
                noon_core::EffectTrackValue::from_definition(property, value.definition)
            });
            if next != old {
                if let Some(next) = next {
                    effective.definition = next
                        .update()
                        .apply_to(effective.definition)
                        .expect("validated surviving schema");
                }
            }
        }
        for property in [
            Property::EffectColor,
            Property::EffectRadius,
            Property::EffectIntensity,
            Property::EffectOffset,
        ] {
            let channel = noon_compile::CompiledChannelKey::effect(
                object_index as u32,
                property,
                base.attachment,
            );
            let Some(mut group) = self.groups.get(&channel).cloned() else {
                continue;
            };
            let tracks = self.compiled.channel_tracks(channel);
            group.cursor = crate::upper_bound_start(tracks, self.frame.time, &mut 0);
            crate::apply_effect_channel(
                Some(&base),
                &mut effective,
                tracks,
                &group,
                self.frame.time,
            );
        }
        effective
    }
}
