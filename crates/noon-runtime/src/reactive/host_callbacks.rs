use std::collections::BTreeMap;

use noon_compile::CompilePatchError;
use noon_core::{
    HostCallbackId, HostCallbackRegistry, MutationImpact, MutationTransaction, ObjectId, ScenePatch,
    Style, Transform2D,
};

use crate::{EvaluationError, FrameState, SceneInstance};

/// Dynamic object state captured once for one host callback phase.
///
/// Geometry is intentionally excluded from this frame-critical snapshot. Stable
/// geometry metadata can be exposed separately; callback phases should not clone
/// large path payloads merely to read transforms/styles or lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostObjectFrameState {
    pub object: ObjectId,
    pub transform: Transform2D,
    pub style: Style,
    pub presence: bool,
    pub appearance: f32,
    pub reveal: f32,
    pub morph: f32,
}

/// One callback invocation referencing the phase-wide object snapshot table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCallbackInvocation {
    pub callback: HostCallbackId,
    pub object_indices: Vec<u32>,
}

/// Coherent read-side payload for one host callback phase.
///
/// Objects watched by multiple callbacks appear once in `objects`; each callback
/// references that shared table by compact indices. This is the payload intended
/// to cross a Python/JS/native host boundary once per callback phase.
#[derive(Clone, Debug, PartialEq)]
pub struct HostCallbackFrame {
    pub time: f64,
    pub delta_time: f64,
    pub objects: Vec<HostObjectFrameState>,
    pub invocations: Vec<HostCallbackInvocation>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostCommitStats {
    pub mutations: usize,
    pub impact: Option<MutationImpact>,
    pub staged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostCallbackAttachError {
    UnknownObject {
        callback: HostCallbackId,
        object: ObjectId,
    },
    TooManyWatchedObjects(usize),
}

impl std::fmt::Display for HostCallbackAttachError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownObject { callback, object } => write!(
                formatter,
                "host callback {} references unknown object {}",
                callback.get(),
                object.get()
            ),
            Self::TooManyWatchedObjects(count) => {
                write!(formatter, "too many host callback watched objects: {count}")
            }
        }
    }
}

impl std::error::Error for HostCallbackAttachError {}

#[derive(Clone, Debug, PartialEq)]
pub enum HostCommitError {
    Patch(CompilePatchError),
    ReactiveReloweringRequired(MutationImpact),
}

impl std::fmt::Display for HostCommitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Patch(error) => error.fmt(formatter),
            Self::ReactiveReloweringRequired(impact) => write!(
                formatter,
                "host callback {impact:?} mutation requires reactive semantic re-lowering"
            ),
        }
    }
}

impl std::error::Error for HostCommitError {}

impl From<CompilePatchError> for HostCommitError {
    fn from(value: CompilePatchError) -> Self {
        Self::Patch(value)
    }
}

#[derive(Clone, Debug)]
pub struct HostDrivenScene {
    scene: SceneInstance,
    watched_dense_indices: Vec<usize>,
    invocations: Vec<HostCallbackInvocation>,
    last_callback_time: f64,
    last_commit_stats: HostCommitStats,
}

impl HostDrivenScene {
    pub fn new(
        scene: SceneInstance,
        registry: &HostCallbackRegistry,
    ) -> Result<Self, HostCallbackAttachError> {
        let mut watched_dense_indices = Vec::<usize>::new();
        let mut snapshot_index_by_object = BTreeMap::<ObjectId, u32>::new();
        let mut invocations = Vec::with_capacity(registry.slots().len());

        for slot in registry.slots() {
            let mut object_indices = Vec::with_capacity(slot.objects.len());
            for object in &slot.objects {
                let dense_index = scene
                    .frame()
                    .objects
                    .iter()
                    .position(|candidate| candidate.id == *object)
                    .ok_or(HostCallbackAttachError::UnknownObject {
                        callback: slot.id,
                        object: *object,
                    })?;
                let snapshot_index = if let Some(index) = snapshot_index_by_object.get(object) {
                    *index
                } else {
                    let index = u32::try_from(watched_dense_indices.len()).map_err(|_| {
                        HostCallbackAttachError::TooManyWatchedObjects(watched_dense_indices.len())
                    })?;
                    watched_dense_indices.push(dense_index);
                    snapshot_index_by_object.insert(*object, index);
                    index
                };
                object_indices.push(snapshot_index);
            }
            invocations.push(HostCallbackInvocation {
                callback: slot.id,
                object_indices,
            });
        }

        let last_callback_time = scene.frame().time;
        Ok(Self {
            scene,
            watched_dense_indices,
            invocations,
            last_callback_time,
            last_commit_stats: HostCommitStats::default(),
        })
    }

    pub fn scene(&self) -> &SceneInstance {
        &self.scene
    }

    pub fn scene_mut(&mut self) -> &mut SceneInstance {
        &mut self.scene
    }

    pub const fn last_commit_stats(&self) -> HostCommitStats {
        self.last_commit_stats
    }

    pub fn evaluate(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.scene.evaluate(time)
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.scene.seek(time)
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        self.scene.advance_to(time)
    }

    /// Capture one coherent host callback phase.
    ///
    /// `delta_time` is signed. Hosts that need Manim-style forward updater `dt`
    /// can use normal forward playback; signed deltas keep seeks/reverse control
    /// deterministic rather than silently fabricating elapsed time.
    pub fn callback_frame(&mut self) -> HostCallbackFrame {
        let frame = self.scene.frame();
        let delta_time = frame.time - self.last_callback_time;
        self.last_callback_time = frame.time;
        let objects = self
            .watched_dense_indices
            .iter()
            .map(|index| {
                let object = &frame.objects[*index];
                HostObjectFrameState {
                    object: object.id,
                    transform: object.transform,
                    style: object.style,
                    presence: frame.presences[*index],
                    appearance: object.appearance,
                    reveal: frame.reveals[*index],
                    morph: frame.morphs[*index],
                }
            })
            .collect();
        HostCallbackFrame {
            time: frame.time,
            delta_time,
            objects,
            invocations: self.invocations.clone(),
        }
    }

    /// Atomically commit one callback-phase mutation batch.
    ///
    /// Property-only batches are preflighted and applied in place through the
    /// existing incremental runtime path. Higher-impact batches stage a cloned
    /// runtime for atomicity. A runtime with native reactive state rejects those
    /// higher-impact mutations until semantic graph revalidation/re-lowering is
    /// available; silently retaining stale reactive dense targets is forbidden.
    pub fn commit(
        &mut self,
        transaction: &MutationTransaction,
    ) -> Result<&FrameState, HostCommitError> {
        let impact = transaction.impact();
        if transaction.is_empty() {
            self.last_commit_stats = HostCommitStats::default();
            return Ok(self.scene.frame());
        }

        if impact == Some(MutationImpact::Property) {
            for patch in transaction.mutations() {
                let object = match patch {
                    ScenePatch::SetTransform { object, .. }
                    | ScenePatch::SetStyle { object, .. } => *object,
                    _ => unreachable!("property-impact transaction must contain property patches"),
                };
                if !self.scene.contains_object(object) {
                    return Err(HostCommitError::Patch(CompilePatchError::UnknownObject(
                        object,
                    )));
                }
            }
            for patch in transaction.mutations() {
                self.scene
                    .apply_patch(patch)
                    .expect("property callback transaction was preflighted");
            }
            self.last_commit_stats = HostCommitStats {
                mutations: transaction.mutations().len(),
                impact,
                staged: false,
            };
            return Ok(self.scene.frame());
        }

        if self.scene.reactive.is_some() {
            return Err(HostCommitError::ReactiveReloweringRequired(
                impact.expect("non-empty transaction has impact"),
            ));
        }

        let mut staged = self.scene.clone();
        for patch in transaction.mutations() {
            staged.apply_patch(patch)?;
        }
        self.scene = staged;
        self.last_commit_stats = HostCommitStats {
            mutations: transaction.mutations().len(),
            impact,
            staged: true,
        };
        Ok(self.scene.frame())
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::CompiledScene;
    use noon_core::{
        GeometryRef, MutationTransaction, ReactiveExpr, SceneDefinition, ScenePatch, SemanticScene,
        Style, Vec2,
    };

    use super::*;

    fn plain_scene(count: usize) -> (SceneInstance, Vec<ObjectId>) {
        let mut definition = SceneDefinition::new();
        let objects = (0..count)
            .map(|_| definition.add(GeometryRef::circle(1.0)))
            .collect::<Vec<_>>();
        let compiled = CompiledScene::compile(&definition).expect("scene must compile");
        (SceneInstance::new(compiled), objects)
    }

    #[test]
    fn phase_snapshot_deduplicates_objects_shared_by_callbacks() {
        let (scene, objects) = plain_scene(3);
        let mut registry = HostCallbackRegistry::new();
        let first = registry.register([objects[0], objects[1]]);
        let second = registry.register([objects[1], objects[2]]);
        let mut driven = HostDrivenScene::new(scene, &registry).unwrap();

        let frame = driven.callback_frame();
        assert_eq!(frame.objects.len(), 3);
        assert_eq!(frame.invocations[0].callback, first);
        assert_eq!(frame.invocations[0].object_indices, vec![0, 1]);
        assert_eq!(frame.invocations[1].callback, second);
        assert_eq!(frame.invocations[1].object_indices, vec![1, 2]);
    }

    #[test]
    fn callback_delta_time_tracks_the_runtime_playhead_coherently() {
        let (scene, objects) = plain_scene(1);
        let mut registry = HostCallbackRegistry::new();
        registry.register([objects[0]]);
        let mut driven = HostDrivenScene::new(scene, &registry).unwrap();
        assert_eq!(driven.callback_frame().delta_time, 0.0);
        driven.advance_to(0.25).unwrap();
        assert_eq!(driven.callback_frame().delta_time, 0.25);
        driven.seek(0.1).unwrap();
        assert!((driven.callback_frame().delta_time + 0.15).abs() < 1e-12);
    }

    #[test]
    fn property_transaction_is_atomic_and_marks_only_changed_dense_object() {
        let (mut scene, objects) = plain_scene(10_000);
        scene.take_frame_changes();
        let mut registry = HostCallbackRegistry::new();
        registry.register([objects[9_999]]);
        let mut driven = HostDrivenScene::new(scene, &registry).unwrap();

        let transaction = MutationTransaction::from_mutations([ScenePatch::SetTransform {
            object: objects[9_999],
            transform: Transform2D {
                translation: Vec2::new(3.0, -2.0),
                ..Transform2D::IDENTITY
            },
        }]);
        driven.commit(&transaction).unwrap();
        assert_eq!(
            driven.scene().frame().objects[9_999].transform.translation,
            Vec2::new(3.0, -2.0)
        );
        assert_eq!(
            driven.scene_mut().take_frame_changes().object_indices(),
            &[9_999]
        );
        assert_eq!(
            driven.last_commit_stats(),
            HostCommitStats {
                mutations: 1,
                impact: Some(MutationImpact::Property),
                staged: false,
            }
        );

        let before = driven.scene().frame().objects[9_999].transform;
        let invalid = MutationTransaction::from_mutations([
            ScenePatch::SetTransform {
                object: objects[9_999],
                transform: Transform2D::IDENTITY,
            },
            ScenePatch::SetStyle {
                object: ObjectId::new(999_999),
                style: Style::default(),
            },
        ]);
        assert!(matches!(
            driven.commit(&invalid),
            Err(HostCommitError::Patch(CompilePatchError::UnknownObject(_)))
        ));
        assert_eq!(driven.scene().frame().objects[9_999].transform, before);
    }

    #[test]
    fn structural_transaction_stages_plain_runtime_and_rolls_back_on_failure() {
        let (scene, objects) = plain_scene(2);
        let mut driven = HostDrivenScene::new(scene, &HostCallbackRegistry::new()).unwrap();
        let before = driven.scene().frame().clone();
        let invalid = MutationTransaction::from_mutations([
            ScenePatch::RemoveObject(objects[1]),
            ScenePatch::RemoveObject(ObjectId::new(500)),
        ]);
        assert!(driven.commit(&invalid).is_err());
        assert_eq!(driven.scene().frame(), &before);
    }

    #[test]
    fn reactive_runtime_allows_property_callback_commits_but_rejects_structural_work() {
        let mut semantic = SemanticScene::new();
        let object = semantic.add(GeometryRef::circle(1.0));
        let input = semantic.add_input(1.0_f32);
        let derived = semantic.add_derived(ReactiveExpr::scalar(0.5));
        semantic.bind(derived, object, noon_core::Property::Rotation);
        let scene = SceneInstance::from_semantic(&semantic).unwrap();
        let mut driven = HostDrivenScene::new(scene, &HostCallbackRegistry::new()).unwrap();

        driven
            .commit(&MutationTransaction::from_mutations([
                ScenePatch::SetStyle {
                    object,
                    style: Style {
                        opacity: 0.5,
                        ..Style::default()
                    },
                },
            ]))
            .unwrap();
        assert_eq!(driven.scene().frame().objects[0].style.opacity, 0.5);

        let structure = MutationTransaction::from_mutations([ScenePatch::RemoveObject(object)]);
        assert_eq!(
            driven.commit(&structure),
            Err(HostCommitError::ReactiveReloweringRequired(
                MutationImpact::Structure
            ))
        );
        assert_eq!(
            driven.scene().reactive_value(input),
            Some(&noon_core::ReactiveValue::Scalar(1.0))
        );
    }
}
