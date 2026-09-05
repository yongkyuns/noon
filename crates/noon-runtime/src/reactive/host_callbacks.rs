use std::collections::BTreeMap;

use noon_compile::CompilePatchError;
use noon_core::{
    HostCallbackId, HostCallbackRegistry, MutationImpact, MutationTransaction, ObjectId,
    ReactiveValue, SignalId, Style, Transform2D,
};

use crate::{FrameState, SceneInstance, TimedSceneInstance, TimedSceneRuntimeError};

const CALLBACK_TIME_EPSILON: f64 = 1e-12;

fn callback_is_active(time: f64, active_after: Option<f64>, active_through: Option<f64>) -> bool {
    active_after.is_none_or(|start| time + CALLBACK_TIME_EPSILON >= start)
        && active_through.is_none_or(|end| time + CALLBACK_TIME_EPSILON < end)
}

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
    scene: TimedSceneInstance,
    watched_dense_indices: Vec<usize>,
    scheduled_invocations: Vec<(HostCallbackInvocation, Option<f64>, Option<f64>)>,
    last_callback_time: f64,
    last_active_callbacks: Vec<HostCallbackId>,
    last_commit_stats: HostCommitStats,
}

impl HostDrivenScene {
    pub fn new(
        scene: SceneInstance,
        registry: &HostCallbackRegistry,
    ) -> Result<Self, HostCallbackAttachError> {
        Self::from_timed(TimedSceneInstance::from_scene_instance(scene), registry)
    }

    pub fn from_timed(
        scene: TimedSceneInstance,
        registry: &HostCallbackRegistry,
    ) -> Result<Self, HostCallbackAttachError> {
        let dense_index_by_object = scene
            .frame()
            .objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.id, index))
            .collect::<BTreeMap<_, _>>();
        let mut watched_dense_indices = Vec::<usize>::new();
        let mut snapshot_index_by_object = BTreeMap::<ObjectId, u32>::new();
        let mut scheduled_invocations = Vec::with_capacity(registry.slots().len());

        for slot in registry.slots() {
            let mut object_indices = Vec::with_capacity(slot.objects.len());
            for object in &slot.objects {
                let dense_index = *dense_index_by_object.get(object).ok_or(
                    HostCallbackAttachError::UnknownObject {
                        callback: slot.id,
                        object: *object,
                    },
                )?;
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
            scheduled_invocations.push((
                HostCallbackInvocation {
                    callback: slot.id,
                    object_indices,
                },
                slot.active_after,
                slot.active_through,
            ));
        }

        let last_callback_time = scene.frame().time;
        let last_active_callbacks = scheduled_invocations
            .iter()
            .filter(|(_, active_after, active_through)| {
                callback_is_active(last_callback_time, *active_after, *active_through)
            })
            .map(|(invocation, _, _)| invocation.callback)
            .collect();
        Ok(Self {
            scene,
            watched_dense_indices,
            scheduled_invocations,
            last_callback_time,
            last_active_callbacks,
            last_commit_stats: HostCommitStats::default(),
        })
    }

    pub fn scene(&self) -> &SceneInstance {
        self.scene.scene()
    }

    pub fn scene_mut(&mut self) -> &mut SceneInstance {
        self.scene.scene_mut()
    }

    pub fn reactive_value(&self, signal: SignalId) -> Option<&ReactiveValue> {
        self.scene.reactive_value(signal)
    }

    pub const fn last_commit_stats(&self) -> HostCommitStats {
        self.last_commit_stats
    }

    pub fn evaluate(&mut self, time: f64) -> Result<&FrameState, TimedSceneRuntimeError> {
        self.scene.evaluate(time)
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, TimedSceneRuntimeError> {
        self.scene.seek(time)
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, TimedSceneRuntimeError> {
        self.scene.advance_to(time)
    }

    /// Capture one coherent host callback phase.
    ///
    /// `delta_time` is signed while the same callback set remains active. A change
    /// in the active set starts a new callback phase at `dt=0`, matching host
    /// animation loops that reset their local updater clock at authored boundaries.
    /// Callbacks already active at the initial playhead retain normal elapsed time.
    /// The small endpoint epsilon prevents decimal frame-time roundoff from keeping
    /// a callback alive for one extra nominal frame.
    pub fn callback_frame(&mut self) -> HostCallbackFrame {
        let frame = self.scene.frame();
        let invocations = self
            .scheduled_invocations
            .iter()
            .filter(|(_, active_after, active_through)| {
                callback_is_active(frame.time, *active_after, *active_through)
            })
            .map(|(invocation, _, _)| invocation.clone())
            .collect::<Vec<_>>();
        let active_callbacks = invocations
            .iter()
            .map(|invocation| invocation.callback)
            .collect::<Vec<_>>();
        let delta_time = if active_callbacks == self.last_active_callbacks {
            frame.time - self.last_callback_time
        } else {
            0.0
        };
        self.last_callback_time = frame.time;
        self.last_active_callbacks = active_callbacks;
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
            invocations,
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
            self.scene.scene_mut().apply_transaction(transaction)?;
            self.last_commit_stats = HostCommitStats {
                mutations: transaction.mutations().len(),
                impact,
                staged: false,
            };
            return Ok(self.scene.frame());
        }

        if self.scene.scene().reactive.is_some() {
            return Err(HostCommitError::ReactiveReloweringRequired(
                impact.expect("non-empty transaction has impact"),
            ));
        }

        let mut staged = self.scene.scene().clone();
        staged.apply_transaction(transaction)?;
        self.scene = TimedSceneInstance::from_scene_instance(staged);
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
    fn callback_invocations_follow_activation_windows() {
        let (scene, objects) = plain_scene(1);
        let registry = HostCallbackRegistry::from_slots(vec![
            noon_core::HostCallbackSlot {
                id: HostCallbackId::new(0),
                objects: vec![objects[0]],
                active_after: Some(0.0),
                active_through: Some(1.0),
            },
            noon_core::HostCallbackSlot {
                id: HostCallbackId::new(1),
                objects: vec![objects[0]],
                active_after: Some(1.0),
                active_through: Some(2.0),
            },
        ])
        .unwrap();
        let mut driven = HostDrivenScene::new(scene, &registry).unwrap();

        let frame = driven.callback_frame();
        assert_eq!(frame.invocations[0].callback, HostCallbackId::new(0));
        assert_eq!(frame.delta_time, 0.0);
        driven.advance_to(0.5).unwrap();
        let frame = driven.callback_frame();
        assert_eq!(frame.invocations[0].callback, HostCallbackId::new(0));
        assert_eq!(frame.delta_time, 0.5);
        driven.advance_to(1.0).unwrap();
        let frame = driven.callback_frame();
        assert_eq!(frame.invocations[0].callback, HostCallbackId::new(1));
        assert_eq!(frame.delta_time, 0.0);
        driven.advance_to(1.5).unwrap();
        let frame = driven.callback_frame();
        assert_eq!(frame.invocations[0].callback, HostCallbackId::new(1));
        assert_eq!(frame.delta_time, 0.5);
        driven.advance_to(2.0 - 5e-15).unwrap();
        assert!(driven.callback_frame().invocations.is_empty());
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
    fn callback_batch_publishes_once_and_repeated_values_publish_nothing() {
        let (scene, objects) = plain_scene(2);
        let mut driven = HostDrivenScene::new(scene, &HostCallbackRegistry::new()).unwrap();
        let before = driven.scene().publication_context();
        let transaction = MutationTransaction::from_mutations(objects.iter().map(|object| {
            ScenePatch::SetTransform {
                object: *object,
                transform: Transform2D {
                    translation: Vec2::ONE,
                    ..Transform2D::IDENTITY
                },
            }
        }));
        driven.commit(&transaction).unwrap();
        let after = driven.scene().publication_context();
        assert_eq!(
            after.execution_revision(),
            before.execution_revision().checked_next().unwrap()
        );
        assert_eq!(
            after.frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );
        driven.scene_mut().take_frame_changes();
        driven.commit(&transaction).unwrap();
        assert_eq!(driven.scene().publication_context(), after);
        assert!(driven.scene_mut().take_frame_changes().is_empty());
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
    fn timed_host_scene_exposes_evaluated_signal_values() {
        let mut semantic = SemanticScene::new();
        let object = semantic.add(GeometryRef::circle(0.5));
        let tracker = semantic.add_input(0.0_f32);
        semantic.bind(tracker, object, noon_core::Property::Rotation);
        let mut timeline = noon_core::SignalTimelineDefinition::new();
        timeline
            .add_scalar_track(
                semantic.reactive(),
                tracker,
                0.0,
                4.0,
                noon_core::TrackTiming::new(0.0, 2.0, noon_core::RateFunction::Linear),
            )
            .unwrap();
        let timed = noon_core::TimedSemanticScene::from_parts(semantic, timeline).unwrap();
        let instance = TimedSceneInstance::from_timed(&timed).unwrap();
        let mut registry = HostCallbackRegistry::new();
        registry.register([object]);
        let mut driven = HostDrivenScene::from_timed(instance, &registry).unwrap();

        driven.advance_to(1.0).unwrap();
        assert_eq!(
            driven.reactive_value(tracker),
            Some(&ReactiveValue::Scalar(2.0))
        );
        assert_eq!(driven.callback_frame().objects[0].transform.rotation, 2.0);
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
