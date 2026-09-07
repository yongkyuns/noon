use noon_core::{
    FamilyAnimationMode, FamilyAnimationSpec, ObjectContentRef, ObjectId,
    RetainedAnimationMemberError, RetainedAnimationMembers, RetainedFamilyAnimationMemberPlanError,
    RetainedFamilyAnimationPlan, SemanticNodeId, SemanticStore, SemanticTransactionNodeRef,
};

use super::super::{
    PreparedSemanticAnimationScheduleProjection, PreparedSemanticScheduledAnimationPayload,
    SemanticAnimationScheduleProjection, SemanticScheduledAnimationPayload,
};

/// One immutable glyph plan and its shared composition timing.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledFamilyAnimation {
    pub target: ObjectId,
    pub plan: RetainedFamilyAnimationPlan,
    pub spec: FamilyAnimationSpec,
    pub time_map: noon_core::CompositionTimeMap,
}

/// One installed family-animation driver. Its immutable glyph plan lives in the
/// compiled scene's append-only plan resource table.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledFamilyAnimationChannel {
    pub target: ObjectId,
    pub object_index: u32,
    pub plan_index: u32,
    pub spec: FamilyAnimationSpec,
    pub time_map: noon_core::CompositionTimeMap,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TextWriteLoweringError {
    MissingSemanticTarget(SemanticTransactionNodeRef),
    InvalidMembers(RetainedAnimationMemberError),
    InvalidPlan(RetainedFamilyAnimationMemberPlanError),
    InvalidSpec(noon_core::FamilyAnimationError),
    InvalidTimeMap(noon_core::CompositionTimeMapError),
    ConflictingObjectDrivers {
        target: ObjectId,
        first: SemanticTransactionNodeRef,
        second: SemanticTransactionNodeRef,
    },
}

impl std::fmt::Display for TextWriteLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "TextWrite lowering failed: {self:?}")
    }
}

impl std::error::Error for TextWriteLoweringError {}

fn plan(
    store: &SemanticStore,
    semantic_target: SemanticNodeId,
    target: ObjectId,
    spec: FamilyAnimationSpec,
    time_map: noon_core::CompositionTimeMap,
) -> Result<CompiledFamilyAnimation, TextWriteLoweringError> {
    let state = store
        .semantic_object_state_checked(semantic_target)
        .map_err(|_| TextWriteLoweringError::MissingSemanticTarget(semantic_target.into()))?;
    let noon_core::SemanticObjectContent::Text(handle) = state.content else {
        return Err(TextWriteLoweringError::MissingSemanticTarget(
            semantic_target.into(),
        ));
    };
    let members =
        RetainedAnimationMembers::resolve(&ObjectContentRef::Text(handle), store.text_resources())
            .map_err(TextWriteLoweringError::InvalidMembers)?;
    let plan = RetainedFamilyAnimationPlan::single_leaf(semantic_target, target, members)
        .map_err(TextWriteLoweringError::InvalidPlan)?;
    spec.validate()
        .map_err(TextWriteLoweringError::InvalidSpec)?;
    Ok(CompiledFamilyAnimation {
        target,
        plan,
        spec,
        time_map,
    })
}

pub fn lower_semantic_text_write_animations(
    store: &SemanticStore,
    schedule: &SemanticAnimationScheduleProjection,
) -> Result<Vec<CompiledFamilyAnimation>, TextWriteLoweringError> {
    let drivers = schedule
        .leaves()
        .iter()
        .map(|leaf| {
            let interval = noon_core::continuous_time_map_interval(leaf.timing, &leaf.time_map)
                .map_err(TextWriteLoweringError::InvalidTimeMap)?;
            Ok((
                leaf.animation.into(),
                leaf.execution_object_id,
                interval,
                matches!(
                    leaf.payload,
                    SemanticScheduledAnimationPayload::TextWrite { .. }
                ),
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    reject_conflicts(&drivers)?;
    schedule
        .leaves()
        .iter()
        .filter_map(|leaf| {
            let SemanticScheduledAnimationPayload::TextWrite {
                reverse_member_order,
            } = leaf.payload
            else {
                return None;
            };
            Some(
                FamilyAnimationSpec::new(
                    FamilyAnimationMode::DrawBorderThenFill,
                    leaf.timing.start_time,
                    leaf.timing.duration,
                    leaf.options.lag_ratio,
                    leaf.options.rate_func,
                    leaf.options.reverse_rate_function,
                    reverse_member_order,
                )
                .map_err(TextWriteLoweringError::InvalidSpec)
                .and_then(|spec| {
                    plan(
                        store,
                        leaf.target,
                        leaf.execution_object_id,
                        spec,
                        leaf.time_map.clone(),
                    )
                }),
            )
        })
        .collect()
}

pub fn lower_prepared_text_write_animations(
    store: &SemanticStore,
    schedule: &PreparedSemanticAnimationScheduleProjection,
) -> Result<Vec<CompiledFamilyAnimation>, TextWriteLoweringError> {
    let drivers = schedule
        .leaves()
        .iter()
        .map(|leaf| {
            let interval = noon_core::continuous_time_map_interval(leaf.timing, &leaf.time_map)
                .map_err(TextWriteLoweringError::InvalidTimeMap)?;
            Ok((
                leaf.animation,
                leaf.execution_object_id,
                interval,
                matches!(
                    leaf.payload,
                    PreparedSemanticScheduledAnimationPayload::TextWrite { .. }
                ),
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    reject_conflicts(&drivers)?;
    schedule
        .leaves()
        .iter()
        .filter_map(|leaf| {
            let PreparedSemanticScheduledAnimationPayload::TextWrite {
                reverse_member_order,
            } = leaf.payload
            else {
                return None;
            };
            let Some(target) = leaf.target.existing() else {
                return Some(Err(TextWriteLoweringError::MissingSemanticTarget(
                    leaf.target,
                )));
            };
            Some(
                FamilyAnimationSpec::new(
                    FamilyAnimationMode::DrawBorderThenFill,
                    leaf.timing.start_time,
                    leaf.timing.duration,
                    leaf.options.lag_ratio,
                    leaf.options.rate_func,
                    leaf.options.reverse_rate_function,
                    reverse_member_order,
                )
                .map_err(TextWriteLoweringError::InvalidSpec)
                .and_then(|spec| {
                    plan(
                        store,
                        target,
                        leaf.execution_object_id,
                        spec,
                        leaf.time_map.clone(),
                    )
                }),
            )
        })
        .collect()
}

fn reject_conflicts(
    drivers: &[(SemanticTransactionNodeRef, ObjectId, (f64, f64), bool)],
) -> Result<(), TextWriteLoweringError> {
    if !drivers.iter().any(|driver| driver.3) {
        return Ok(());
    }
    let mut by_target = std::collections::BTreeMap::<ObjectId, Vec<_>>::new();
    for &driver in drivers {
        by_target.entry(driver.1).or_default().push(driver);
    }
    for (target, target_drivers) in &mut by_target {
        target_drivers.sort_by(|left, right| left.2 .0.total_cmp(&right.2 .0));
        let mut latest_any: Option<(f64, SemanticTransactionNodeRef)> = None;
        let mut latest_text: Option<(f64, SemanticTransactionNodeRef)> = None;
        for &(animation, _, (start, end), is_text_write) in target_drivers.iter() {
            let conflicting = if is_text_write {
                latest_any.filter(|(latest_end, _)| start < *latest_end)
            } else {
                latest_text.filter(|(latest_end, _)| start < *latest_end)
            };
            if let Some((_, first)) = conflicting {
                return Err(TextWriteLoweringError::ConflictingObjectDrivers {
                    target: *target,
                    first,
                    second: animation,
                });
            }
            if latest_any.is_none_or(|(latest_end, _)| end > latest_end) {
                latest_any = Some((end, animation));
            }
            if is_text_write && latest_text.is_none_or(|(latest_end, _)| end > latest_end) {
                latest_text = Some((end, animation));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_core::{
        AnimationOptions, FontFaceIdentity, FontResourceArena, GeometryResourceArena, GlyphRun,
        PositionedGlyph, Rect, SemanticAnimationCompositionKind, SemanticMutationTransaction,
        SemanticObjectState, TextAffineTransform, TextClusterIdentity, TextDirection,
        TextRenderItem, TextResource, TextSourceKind, TextSourceSpan, Vec2,
    };

    use super::*;
    use crate::{
        lower_prepared_semantic_animation_composition, lower_semantic_animation_schedule,
        EffectiveAnimationProperties, PreparedSemanticAnimationLoweringError,
        SemanticExecutionIndex,
    };

    fn plain_text(store: &mut SemanticStore) -> SemanticNodeId {
        let source = "ABCDEFGHIJKLMN ";
        let face = FontFaceIdentity {
            family: Arc::from("Test"),
            face_key: Arc::from("test-face"),
            face_index: 0,
            variation_key: Arc::from(""),
        };
        let mut fonts = FontResourceArena::new();
        fonts.intern_face(&face, Arc::<[u8]>::from([1_u8])).unwrap();
        let glyphs = source
            .char_indices()
            .enumerate()
            .map(|(index, (start, character))| PositionedGlyph {
                glyph_id: u32::try_from(index + 1).unwrap(),
                cluster: TextClusterIdentity {
                    source_span: TextSourceSpan::new(
                        u32::try_from(start).unwrap(),
                        u32::try_from(start + character.len_utf8()).unwrap(),
                    ),
                    cluster_ordinal: u32::try_from(index).unwrap(),
                    semantic_key: None,
                },
                origin: Vec2::new(index as f32, 0.0),
                advance: Vec2::ONE,
                bounds: Rect::new(
                    Vec2::new(index as f32, 0.0),
                    Vec2::new(index as f32 + 1.0, 1.0),
                ),
            })
            .collect::<Vec<_>>();
        let handle = store
            .import_text_resource(
                TextResource {
                    source: Arc::from(source),
                    kind: TextSourceKind::Plain,
                    runs: Arc::from([GlyphRun {
                        font: face,
                        variations: Arc::from([]),
                        font_size: 24.0,
                        direction: TextDirection::LeftToRight,
                        fill: None,
                        stroke: None,
                        transform: TextAffineTransform::IDENTITY,
                        glyphs: glyphs.into(),
                    }]),
                    vector_items: Arc::from([]),
                    render_items: Arc::from([TextRenderItem::GlyphRun(0)]),
                    parts: Arc::from([]),
                    bounds: Rect::new(Vec2::ZERO, Vec2::new(15.0, 1.0)),
                    baseline: 0.0,
                    layout_artifact: None,
                },
                &fonts,
                &GeometryResourceArena::new(),
            )
            .unwrap();
        let target = store.insert_semantic_object(SemanticObjectState::new(handle));
        store.attach_to_scene(target).unwrap();
        target
    }

    fn index(store: &SemanticStore) -> SemanticExecutionIndex {
        let mut index = SemanticExecutionIndex::new();
        index.lower_scene(store).unwrap();
        index
    }

    #[test]
    fn prepared_and_published_text_write_share_nested_map_and_visible_glyph_defaults() {
        let mut store = SemanticStore::new();
        let target = plain_text(&mut store);
        let index = index(&store);
        let mut transaction = SemanticMutationTransaction::new();
        let write = transaction.create_text_write_animation(target, false, AnimationOptions::new());
        let nested = transaction.create_animation_composition(
            SemanticAnimationCompositionKind::Sequence,
            [write],
            AnimationOptions::new(),
        );
        let root = transaction.create_animation_composition(
            SemanticAnimationCompositionKind::Sequence,
            [nested],
            AnimationOptions::new().run_time(3.0),
        );
        let prepared = transaction.prepare(&mut store).unwrap();
        let lowered = lower_prepared_semantic_animation_composition(
            &prepared,
            &index,
            root,
            4.0,
            AnimationOptions::new(),
            |_| Option::<EffectiveAnimationProperties>::None,
        )
        .unwrap();
        assert_eq!(lowered.family_animations().len(), 1);
        let prepared_animation = lowered.family_animations()[0].clone();
        assert_eq!(
            prepared_animation.plan.member_plan().total_member_count(),
            14
        );
        assert_eq!(prepared_animation.spec.duration, 3.0);
        assert_eq!(prepared_animation.spec.lag_ratio, 0.2);
        assert!(!prepared_animation.time_map.is_identity());

        let committed = prepared.commit();
        let published_root = committed.resolve(root).unwrap();
        let schedule = lower_semantic_animation_schedule(
            &store,
            &index,
            published_root,
            4.0,
            AnimationOptions::new(),
        )
        .unwrap();
        assert_eq!(
            lower_semantic_text_write_animations(&store, &schedule).unwrap(),
            vec![prepared_animation]
        );
    }

    #[test]
    fn same_target_text_write_rejects_overlap_and_accepts_sequence() {
        let mut store = SemanticStore::new();
        let target = plain_text(&mut store);
        let index = index(&store);

        let declare = |kind| {
            let mut transaction = SemanticMutationTransaction::new();
            let first = transaction.create_text_write_animation(
                target,
                false,
                AnimationOptions::new().run_time(1.0),
            );
            let second = transaction.create_text_write_animation(
                target,
                true,
                AnimationOptions::new().run_time(1.0),
            );
            let root = transaction.create_animation_composition(
                kind,
                [first, second],
                AnimationOptions::new(),
            );
            (transaction, root)
        };

        let (transaction, root) = declare(SemanticAnimationCompositionKind::Parallel);
        let parallel = transaction.prepare(&mut store).unwrap();
        assert!(matches!(
            lower_prepared_semantic_animation_composition(
                &parallel,
                &index,
                root,
                0.0,
                AnimationOptions::new(),
                |_| Option::<EffectiveAnimationProperties>::None,
            ),
            Err(PreparedSemanticAnimationLoweringError::TextWrite(
                TextWriteLoweringError::ConflictingObjectDrivers { .. }
            ))
        ));
        drop(parallel);

        let (transaction, root) = declare(SemanticAnimationCompositionKind::Sequence);
        let sequence = transaction.prepare(&mut store).unwrap();
        let lowered = lower_prepared_semantic_animation_composition(
            &sequence,
            &index,
            root,
            0.0,
            AnimationOptions::new(),
            |_| Option::<EffectiveAnimationProperties>::None,
        )
        .unwrap();
        assert_eq!(lowered.family_animations().len(), 2);
    }
}
