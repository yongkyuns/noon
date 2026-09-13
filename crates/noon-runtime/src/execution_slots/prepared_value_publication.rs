use noon_compile::{ExecutionMutationTransaction, ExecutionPatch};
use noon_core::{
    ExecutionRevision, FrameEpoch, Property, PublicationContext, SceneRevision, Style, Transform2D,
};

use super::runtime_transaction::{final_value_writes, AuthoredPublicationError};
use crate::{FrameRowState, FrameState, RuntimeIdentity, SceneInstance};

#[derive(Clone, Copy, Debug)]
enum PreparedAuthoredValueWrite {
    Transform {
        object_index: usize,
        transform: Transform2D,
        changes_execution: bool,
    },
    Style {
        object_index: usize,
        style: Style,
        changes_execution: bool,
    },
}

impl PreparedAuthoredValueWrite {
    const fn object_index(self) -> usize {
        match self {
            Self::Transform { object_index, .. } | Self::Style { object_index, .. } => object_index,
        }
    }

    const fn changes_execution(self) -> bool {
        match self {
            Self::Transform {
                changes_execution, ..
            }
            | Self::Style {
                changes_execution, ..
            } => changes_execution,
        }
    }
}

/// Runtime proof for one already-semantic-prepared batch containing only ordinary
/// local transform/style writes.
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
    writes: Vec<PreparedAuthoredValueWrite>,
}

impl SceneInstance {
    /// Prepare the narrow P1 authored-value publication contract.
    ///
    /// `Ok(None)` means the transaction contains something other than transform/style
    /// base writes and must stay on the existing general publication path.
    pub fn prepare_authored_value_publication(
        &self,
        transaction: &ExecutionMutationTransaction,
        expected: PublicationContext,
        scene_revision: SceneRevision,
    ) -> Result<Option<PreparedAuthoredValuePublication>, AuthoredPublicationError> {
        if transaction.mutations().iter().any(|patch| {
            !matches!(
                patch,
                ExecutionPatch::SetTransform { .. } | ExecutionPatch::SetStyle { .. }
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
        self.compiled.preflight_execution_transaction(transaction)?;
        let mut writes = Vec::new();
        let mut execution_changed = false;
        for patch in final_value_writes(transaction) {
            let (object, prepared) = match patch {
                ExecutionPatch::SetTransform { object, .. } => (*object, 0_u8),
                ExecutionPatch::SetStyle { object, .. } => (*object, 1_u8),
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
                (1, ExecutionPatch::SetStyle { style, .. }) => PreparedAuthoredValueWrite::Style {
                    object_index,
                    style: *style,
                    changes_execution,
                },
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
            writes,
        }))
    }

    /// Commit an authored local-value publication after its matching Semantic Scene
    /// transaction has crossed the point of no return.
    ///
    /// The execution-session `PreparedPublication` owns exclusive access between
    /// preparation and this call. No check, lookup, allocation, or fallible compile
    /// operation remains here.
    pub fn commit_prepared_authored_value_publication(
        &mut self,
        prepared: PreparedAuthoredValuePublication,
    ) -> &FrameState {
        debug_assert_eq!(prepared.runtime, self.runtime_identity());
        debug_assert_eq!(prepared.expected, self.publication_context());
        self.last_patch_stats = crate::RuntimePatchStats::default();

        for write in prepared.writes {
            if !write.changes_execution() {
                continue;
            }
            let object_index = write.object_index();
            let before = FrameRowState::from_frame(&self.frame, object_index);
            match write {
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
                PreparedAuthoredValueWrite::Style {
                    style,
                    object_index,
                    ..
                } => {
                    self.compiled
                        .commit_prepared_style_value(object_index as u32, style);
                    self.frame.objects[object_index].style = style;
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
            if before.differs_from_frame(&self.frame, object_index) {
                self.mark_changed(object_index);
            }
        }

        self.publication = PublicationContext::new(
            prepared.scene_revision,
            prepared.execution_revision,
            prepared.frame_epoch,
        );
        &self.frame
    }
}
