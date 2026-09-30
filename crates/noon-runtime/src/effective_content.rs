//! Runtime-owned content leases. The compiled object's authored content remains
//! the release baseline; only the effective frame row changes at publication.

use noon_compile::{CompilePatchError, ExecutionMutationTransaction, ExecutionPatch};
use noon_core::{GeometryRef, ObjectContentRef, ObjectId, Property, PublicationContext, Rect};

use crate::{RuntimeIdentity, SceneInstance};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectiveContentLease {
    pub(crate) runtime: RuntimeIdentity,
    object: ObjectId,
    sequence: u64,
}

impl EffectiveContentLease {
    pub const fn object(self) -> ObjectId {
        self.object
    }
}

#[derive(Clone, Debug)]
pub struct PreparedEffectiveContentReplacement {
    lease: EffectiveContentLease,
    expected: PublicationContext,
    expected_version: Option<u64>,
    object_index: usize,
    content: ObjectContentRef,
    text_bounds: Option<Rect>,
}

#[derive(Clone, Debug)]
pub(crate) struct EffectiveContentDriver {
    pub(crate) lease: EffectiveContentLease,
    version: u64,
    pub(crate) content: ObjectContentRef,
    pub(crate) text_bounds: Option<Rect>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EffectiveContentError {
    Invalid(CompilePatchError),
    UnknownObject(ObjectId),
    DriverConflict(ObjectId),
    UnsupportedExternalGeometry(ObjectId),
    ActiveRenderOverride(ObjectId),
    GeometryDriverConflict(ObjectId),
    ReplaySealed,
    ForeignRuntime,
    StalePublication {
        expected: PublicationContext,
        actual: PublicationContext,
    },
    StaleLease(ObjectId),
    SequenceExhausted,
    FrameEpochExhausted,
}

impl std::fmt::Display for EffectiveContentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(error) => error.fmt(formatter),
            Self::UnknownObject(object) => write!(
                formatter,
                "unknown effective content object {}",
                object.get()
            ),
            Self::DriverConflict(object) => write!(
                formatter,
                "object {} has another effective content driver",
                object.get()
            ),
            Self::UnsupportedExternalGeometry(object) => write!(
                formatter,
                "object {} requires an owned inline or versioned content resource",
                object.get()
            ),
            Self::ActiveRenderOverride(object) => write!(
                formatter,
                "object {} has an active render geometry override",
                object.get()
            ),
            Self::GeometryDriverConflict(object) => write!(
                formatter,
                "object {} has a geometry or family animation driver",
                object.get()
            ),
            Self::ReplaySealed => {
                formatter.write_str("sealed replay cannot acquire effective content")
            }
            Self::ForeignRuntime => {
                formatter.write_str("effective content lease belongs to another runtime")
            }
            Self::StalePublication { expected, actual } => write!(
                formatter,
                "effective content expected {expected:?}, found {actual:?}"
            ),
            Self::StaleLease(object) => write!(
                formatter,
                "effective content lease for object {} is stale",
                object.get()
            ),
            Self::SequenceExhausted => {
                formatter.write_str("effective content lease sequence exhausted")
            }
            Self::FrameEpochExhausted => {
                formatter.write_str("effective content frame epoch exhausted")
            }
        }
    }
}

impl std::error::Error for EffectiveContentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Invalid(error) => Some(error),
            _ => None,
        }
    }
}

impl SceneInstance {
    /// Observe the lease bound to this runtime incarnation. A cloned runtime
    /// has fresh identity, so a caller deliberately adopting that clone must
    /// obtain its lease rather than reuse a token from the source runtime.
    pub fn effective_content_lease(&self, object: ObjectId) -> Option<EffectiveContentLease> {
        let index = self.compiled.object_index(object)? as usize;
        self.effective_content_drivers
            .get(&index)
            .map(|driver| driver.lease)
    }

    /// Prepare one content version. Initial acquisition pins the exact effective
    /// publication; a replacement of an existing lease names that lease and
    /// may commit after unrelated frame advances. This only versions the
    /// supplied content value: a producer whose value depends on authored time
    /// or other effective inputs must separately reject obsolete source samples.
    /// A new producer passes `None` and acquires ownership only at commit.
    pub fn prepare_effective_content_replacement(
        &self,
        object: ObjectId,
        content: ObjectContentRef,
        text_bounds: Option<Rect>,
        lease: Option<EffectiveContentLease>,
    ) -> Result<PreparedEffectiveContentReplacement, EffectiveContentError> {
        if self.replay_is_sealed() {
            return Err(EffectiveContentError::ReplaySealed);
        }
        if lease.is_some_and(|lease| lease.runtime != self.identity) {
            return Err(EffectiveContentError::ForeignRuntime);
        }
        let index = self
            .compiled
            .object_index(object)
            .map(|index| index as usize)
            .filter(|&index| self.object_slot_is_live(index))
            .ok_or(EffectiveContentError::UnknownObject(object))?;
        let existing = self.effective_content_drivers.get(&index);
        let (lease, expected_version) = match (existing, lease) {
            (None, None) => {
                let sequence = self.next_effective_content_sequence;
                sequence
                    .checked_add(1)
                    .ok_or(EffectiveContentError::SequenceExhausted)?;
                (
                    EffectiveContentLease {
                        runtime: self.identity,
                        object,
                        sequence,
                    },
                    None,
                )
            }
            (Some(driver), Some(lease)) if driver.lease == lease => {
                driver
                    .version
                    .checked_add(1)
                    .ok_or(EffectiveContentError::SequenceExhausted)?;
                (lease, Some(driver.version))
            }
            (Some(_), None) => return Err(EffectiveContentError::DriverConflict(object)),
            _ => return Err(EffectiveContentError::StaleLease(object)),
        };
        if expected_version.is_none() {
            if self.numeric_text.has_object_driver(index) {
                return Err(EffectiveContentError::DriverConflict(object));
            }
            // A resident lease can coexist with ordinary affine/style channels.
            // Geometry/family/Graph owners are excluded at acquisition, while
            // later authored ownership supersedes the lease at its patch.
            if [Property::Morph, Property::Transform]
                .into_iter()
                .any(|property| {
                    !self
                        .compiled
                        .channel_tracks(noon_compile::CompiledChannelKey::new(
                            index as u32,
                            property,
                        ))
                        .is_empty()
                })
                || self
                    .compiled
                    .family_animations()
                    .iter()
                    .any(|animation| animation.object_index as usize == index)
                || !self
                    .compiled
                    .graph_dependencies_for_changed_row(index as u32)
                    .is_empty()
            {
                return Err(EffectiveContentError::GeometryDriverConflict(object));
            }
        }
        if matches!(
            content,
            ObjectContentRef::Geometry(GeometryRef::External(_))
        ) {
            return Err(EffectiveContentError::UnsupportedExternalGeometry(object));
        }
        if self.frame.render_geometries[index].is_some()
            || self.frame.render_transforms[index].is_some()
        {
            return Err(EffectiveContentError::ActiveRenderOverride(object));
        }
        let patch = ExecutionPatch::SetContent {
            object,
            content: content.clone(),
            text_bounds,
        };
        self.compiled
            .preflight_execution_transaction(&ExecutionMutationTransaction::from_mutations([patch]))
            .map_err(EffectiveContentError::Invalid)?;

        Ok(PreparedEffectiveContentReplacement {
            lease,
            expected: self.publication,
            expected_version,
            object_index: index,
            content,
            text_bounds,
        })
    }

    /// Commit exactly one prepared content version. Only the addressed row is
    /// invalidated; Graph-owned targets are excluded at acquisition.
    pub fn commit_effective_content_replacement(
        &mut self,
        prepared: PreparedEffectiveContentReplacement,
    ) -> Result<EffectiveContentLease, EffectiveContentError> {
        if self.replay_is_sealed() {
            return Err(EffectiveContentError::ReplaySealed);
        }
        self.validate_content_prepared(&prepared)?;
        let index = prepared.object_index;
        let changed = self.frame.objects[index].content != prepared.content
            || self.frame.objects[index].text_bounds != prepared.text_bounds
            || self.frame.render_geometries[index].is_some()
            || self.frame.render_transforms[index].is_some();
        let next_epoch = if changed {
            Some(
                self.publication
                    .frame_epoch()
                    .checked_next()
                    .ok_or(EffectiveContentError::FrameEpochExhausted)?,
            )
        } else {
            None
        };
        let version = prepared.expected_version.map_or(0, |version| version + 1);
        if prepared.expected_version.is_none() {
            self.next_effective_content_sequence += 1;
        }
        self.effective_content_drivers.insert(
            index,
            EffectiveContentDriver {
                lease: prepared.lease,
                version,
                content: prepared.content,
                text_bounds: prepared.text_bounds,
            },
        );
        self.invalidate_replay_domain();
        if let Some(next_epoch) = next_epoch {
            self.reapply_effective_content_driver(index);
            self.mark_changed(index);
            self.publication = self.publication.with_frame_epoch(next_epoch);
        }
        Ok(prepared.lease)
    }

    /// Release the exact producer and reveal the current authored/compiled
    /// content. A later authored edit made under the lease is therefore visible.
    pub fn release_effective_content(
        &mut self,
        lease: EffectiveContentLease,
    ) -> Result<(), EffectiveContentError> {
        if self.replay_is_sealed() {
            return Err(EffectiveContentError::ReplaySealed);
        }
        if lease.runtime != self.identity {
            return Err(EffectiveContentError::ForeignRuntime);
        }
        let index = self
            .compiled
            .object_index(lease.object)
            .map(|index| index as usize)
            .filter(|&index| self.object_slot_is_live(index))
            .ok_or(EffectiveContentError::UnknownObject(lease.object))?;
        if self
            .effective_content_drivers
            .get(&index)
            .is_none_or(|driver| driver.lease != lease)
        {
            return Err(EffectiveContentError::StaleLease(lease.object));
        }
        let authored = &self.compiled.objects()[index];
        let changed = self.frame.objects[index].content != authored.content
            || self.frame.objects[index].text_bounds != authored.text_bounds
            || self.frame.render_geometries[index].is_some()
            || self.frame.render_transforms[index].is_some();
        let next_epoch = if changed {
            Some(
                self.publication
                    .frame_epoch()
                    .checked_next()
                    .ok_or(EffectiveContentError::FrameEpochExhausted)?,
            )
        } else {
            None
        };
        let authored_content = changed.then(|| authored.content.clone());
        let authored_text_bounds = authored.text_bounds;
        self.effective_content_drivers.remove(&index);
        self.invalidate_replay_domain();
        if let Some(next_epoch) = next_epoch {
            self.frame.objects[index].content =
                authored_content.expect("changed content was copied");
            self.frame.objects[index].text_bounds = authored_text_bounds;
            self.frame.render_geometries[index] = None;
            self.frame.render_transforms[index] = None;
            self.mark_changed(index);
            self.publication = self.publication.with_frame_epoch(next_epoch);
        }
        Ok(())
    }

    fn validate_content_prepared(
        &self,
        prepared: &PreparedEffectiveContentReplacement,
    ) -> Result<(), EffectiveContentError> {
        if prepared.lease.runtime != self.identity {
            return Err(EffectiveContentError::ForeignRuntime);
        }
        // A producer that already owns this object may finish preparation after
        // unrelated effective frames advance. Its lease version still orders
        // content results, while the current row supplies the transform/style
        // used for bounds and rendering. Initial acquisition remains pinned to
        // the exact frame because its sequence is reserved only at commit.
        let compatible = if prepared.expected_version.is_some() {
            prepared.expected.scene_revision() == self.publication.scene_revision()
                && prepared.expected.execution_revision() == self.publication.execution_revision()
        } else {
            prepared.expected == self.publication
        };
        if !compatible {
            return Err(EffectiveContentError::StalePublication {
                expected: prepared.expected,
                actual: self.publication,
            });
        }
        let index = prepared.object_index;
        if !self.object_slot_is_live(index) || self.frame.objects[index].id != prepared.lease.object
        {
            return Err(EffectiveContentError::UnknownObject(prepared.lease.object));
        }
        if self.frame.render_geometries[index].is_some()
            || self.frame.render_transforms[index].is_some()
        {
            return Err(EffectiveContentError::ActiveRenderOverride(
                prepared.lease.object,
            ));
        }
        match (
            self.effective_content_drivers.get(&index),
            prepared.expected_version,
        ) {
            (None, None) if prepared.lease.sequence == self.next_effective_content_sequence => {
                Ok(())
            }
            (Some(driver), Some(version))
                if driver.lease == prepared.lease && driver.version == version =>
            {
                Ok(())
            }
            _ => Err(EffectiveContentError::StaleLease(prepared.lease.object)),
        }
    }

    pub(crate) fn reapply_effective_content_driver(&mut self, index: usize) {
        if let Some(driver) = self.effective_content_drivers.get(&index) {
            self.frame.objects[index].content = driver.content.clone();
            self.frame.objects[index].text_bounds = driver.text_bounds;
            self.frame.render_geometries[index] = None;
            self.frame.render_transforms[index] = None;
        }
    }

    /// An authored geometry owner supersedes a content lease. This runs only
    /// after its patch has succeeded; the old producer token then becomes stale.
    pub(crate) fn retire_effective_content_driver(&mut self, index: usize) {
        if self.effective_content_drivers.remove(&index).is_some() {
            self.frame.objects[index].content = self.compiled.objects()[index].content.clone();
            self.frame.objects[index].text_bounds = self.compiled.objects()[index].text_bounds;
            self.mark_changed(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
    use noon_core::{
        CompositionTimeMap, GeometryRef, ObjectContentRef, ObjectId, Property, RateFunction, Style,
        TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D,
    };

    use super::{EffectiveContentError, SceneInstance};

    fn scene(count: usize, animate_last: bool) -> SceneInstance {
        let objects = (0..count)
            .map(|index| {
                CompiledObject::new(
                    ObjectId::new(index as u64),
                    GeometryRef::circle(1.0),
                    Transform2D::IDENTITY,
                    Style::default(),
                )
            })
            .collect();
        let tracks = animate_last
            .then(|| TrackDefinition {
                id: TrackId::new(1),
                object: ObjectId::new((count - 1) as u64),
                property: Property::Opacity,
                values: TrackValues::Scalar { from: 1.0, to: 0.0 },
                timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            })
            .into_iter()
            .collect::<Vec<_>>();
        SceneInstance::new(CompiledScene::compile_objects(objects, &tracks).unwrap())
    }

    #[test]
    fn replacement_is_effective_only_and_release_reveals_latest_authored_content() {
        let mut instance = scene(1, false);
        let object = ObjectId::new(0);
        let before = instance.publication_context();
        let first = instance
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            )
            .unwrap();
        let stale = first.clone();
        let lease = instance
            .commit_effective_content_replacement(first)
            .unwrap();
        let published = instance.publication_context();
        assert_eq!(published.scene_revision(), before.scene_revision());
        assert_eq!(published.execution_revision(), before.execution_revision());
        assert_ne!(published.frame_epoch(), before.frame_epoch());
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(2.0))
        );
        assert!(matches!(
            instance.commit_effective_content_replacement(stale),
            Err(EffectiveContentError::StalePublication { .. })
        ));

        instance
            .apply_execution_patch(&ExecutionPatch::SetContent {
                object,
                content: ObjectContentRef::Geometry(GeometryRef::circle(3.0)),
                text_bounds: None,
            })
            .unwrap();
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(2.0))
        );
        instance.release_effective_content(lease).unwrap();
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(3.0))
        );
        assert!(matches!(
            instance.release_effective_content(lease),
            Err(EffectiveContentError::StaleLease(_))
        ));
    }

    #[test]
    fn held_content_does_not_stage_or_dirty_unrelated_animated_rows() {
        let mut instance = scene(601, true);
        for index in 0..600 {
            let prepared = instance
                .prepare_effective_content_replacement(
                    ObjectId::new(index),
                    ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                    None,
                    None,
                )
                .unwrap();
            instance
                .commit_effective_content_replacement(prepared)
                .unwrap();
        }
        instance.take_frame_changes();
        instance.take_spatial_changes();
        let prepared = instance.prepare_advance_to(1.0).unwrap();
        assert_eq!(prepared.staged_row_count(), 1);
        assert_eq!(prepared.prior_driver_rows(), 0);
        instance.advance_to(1.0).unwrap();
        assert_eq!(instance.take_frame_changes().object_indices(), &[600]);
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(2.0))
        );
    }

    #[test]
    fn invalid_replacement_and_geometry_driver_conflict_leave_frame_unchanged() {
        let mut instance = scene(1, false);
        let before = instance.frame().clone();
        let publication = instance.publication_context();
        assert!(instance
            .prepare_effective_content_replacement(
                ObjectId::new(0),
                ObjectContentRef::Geometry(GeometryRef::circle(f32::NAN)),
                None,
                None,
            )
            .is_err());
        assert_eq!(instance.frame(), &before);
        assert_eq!(instance.publication_context(), publication);

        let prepared = instance
            .prepare_effective_content_replacement(
                ObjectId::new(0),
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            )
            .unwrap();
        let lease = instance
            .commit_effective_content_replacement(prepared)
            .unwrap();
        instance
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(2),
                object: ObjectId::new(0),
                property: Property::Morph,
                values: TrackValues::Scalar { from: 0.0, to: 1.0 },
                timing: TrackTiming::new(1.0, 1.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }))
            .unwrap();
        assert!(matches!(
            instance.release_effective_content(lease),
            Err(EffectiveContentError::StaleLease(_))
        ));
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(1.0))
        );
        assert!(matches!(
            instance.prepare_effective_content_replacement(
                ObjectId::new(0),
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            ),
            Err(EffectiveContentError::GeometryDriverConflict(_))
        ));
    }

    #[test]
    fn cloned_runtime_rebinds_its_lease_without_accepting_the_source_token() {
        let mut original = scene(1, false);
        let object = ObjectId::new(0);
        let prepared = original
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            )
            .unwrap();
        let source_lease = original
            .commit_effective_content_replacement(prepared)
            .unwrap();
        let mut cloned = original.clone();
        assert!(matches!(
            cloned.release_effective_content(source_lease),
            Err(EffectiveContentError::ForeignRuntime)
        ));
        let cloned_lease = cloned.effective_content_lease(object).unwrap();
        cloned.release_effective_content(cloned_lease).unwrap();
        assert_eq!(
            cloned.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(1.0))
        );
        assert_eq!(
            original.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(2.0))
        );
    }

    #[test]
    fn leased_result_survives_unrelated_frame_but_not_authored_plan_change() {
        let mut instance = scene(2, true);
        let object = ObjectId::new(0);
        let first = instance
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            )
            .unwrap();
        let lease = instance
            .commit_effective_content_replacement(first)
            .unwrap();
        let prepared = instance
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Geometry(GeometryRef::circle(3.0)),
                None,
                Some(lease),
            )
            .unwrap();
        let superseded = instance
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Geometry(GeometryRef::circle(5.0)),
                None,
                Some(lease),
            )
            .unwrap();
        instance.advance_to(1.0).unwrap();
        instance
            .commit_effective_content_replacement(prepared)
            .unwrap();
        assert!(matches!(
            instance.commit_effective_content_replacement(superseded),
            Err(EffectiveContentError::StaleLease(_))
        ));
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(3.0))
        );

        let stale = instance
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Geometry(GeometryRef::circle(4.0)),
                None,
                Some(lease),
            )
            .unwrap();
        instance
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: ObjectId::new(1),
                style: Style {
                    opacity: 0.6,
                    ..Style::default()
                },
            })
            .unwrap();
        assert!(matches!(
            instance.commit_effective_content_replacement(stale),
            Err(EffectiveContentError::StalePublication { .. })
        ));
        assert_eq!(
            instance.frame().objects[0].geometry(),
            Some(&GeometryRef::circle(3.0))
        );
    }
}
