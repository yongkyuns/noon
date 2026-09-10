use noon_core::{
    FamilyAnimationMode, FamilyAnimationSpec, ObjectContentRef, ObjectId,
    RetainedAnimationMemberError, RetainedAnimationMembers, RetainedFamilyAnimationMemberPlanError,
    RetainedFamilyAnimationPlan, SemanticNodeId, SemanticStore, SemanticTransactionNodeRef,
};

use super::super::{
    PreparedSemanticAnimationScheduleProjection, PreparedSemanticScheduledAnimationPayload,
    SemanticAnimationScheduleProjection, SemanticScheduledAnimationPayload,
};
use crate::semantic_lowering::compiled_scene::lower_semantic_geometry_value;

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
pub enum TextGlyphLoweringError {
    MissingSemanticTarget(SemanticTransactionNodeRef),
    InvalidMembers(RetainedAnimationMemberError),
    InvalidPlan(RetainedFamilyAnimationMemberPlanError),
    InvalidSpec(noon_core::FamilyAnimationError),
    InvalidTimeMap(noon_core::CompositionTimeMapError),
    InvalidFamilyMember(noon_core::SemanticFamilyAnimationMember),
    ConflictingObjectDrivers {
        target: ObjectId,
        first: SemanticTransactionNodeRef,
        second: SemanticTransactionNodeRef,
    },
}

impl std::fmt::Display for TextGlyphLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Text glyph lowering failed: {self:?}")
    }
}

impl std::error::Error for TextGlyphLoweringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidMembers(error) => Some(error),
            Self::InvalidPlan(error) => Some(error),
            Self::InvalidSpec(error) => Some(error),
            Self::InvalidTimeMap(error) => Some(error),
            _ => None,
        }
    }
}

fn plan(
    store: &SemanticStore,
    semantic_target: SemanticNodeId,
    target: ObjectId,
    family_member: Option<noon_core::SemanticFamilyAnimationMember>,
    family_spans: &std::collections::HashMap<
        noon_core::SemanticFamilyAnimationMember,
        (SemanticNodeId, u32, u32),
    >,
    spec: FamilyAnimationSpec,
    time_map: noon_core::CompositionTimeMap,
) -> Result<CompiledFamilyAnimation, TextGlyphLoweringError> {
    let state = store
        .semantic_object_state_checked(semantic_target)
        .map_err(|_| TextGlyphLoweringError::MissingSemanticTarget(semantic_target.into()))?;
    let content = match (spec.mode, state.content) {
        (_, noon_core::SemanticObjectContent::Geometry(content)) if family_member.is_some() => {
            ObjectContentRef::Geometry(
                lower_semantic_geometry_value(content, Some(store)).map_err(|_| {
                    TextGlyphLoweringError::MissingSemanticTarget(semantic_target.into())
                })?,
            )
        }
        (_, noon_core::SemanticObjectContent::Text(handle)) => ObjectContentRef::Text(handle),
        _ => {
            return Err(TextGlyphLoweringError::MissingSemanticTarget(
                semantic_target.into(),
            ));
        }
    };
    let members = RetainedAnimationMembers::resolve(&content, store.text_resources())
        .map_err(TextGlyphLoweringError::InvalidMembers)?;
    let (first_member, total_member_count) = match family_member {
        Some(member) => {
            let (leaf, first, total) = family_spans
                .get(&member)
                .copied()
                .ok_or(TextGlyphLoweringError::InvalidFamilyMember(member))?;
            if leaf != semantic_target {
                return Err(TextGlyphLoweringError::InvalidFamilyMember(member));
            }
            (first, total)
        }
        None => (0, members.member_count()),
    };
    let plan = RetainedFamilyAnimationPlan::single_leaf_span(
        semantic_target,
        target,
        members,
        first_member,
        total_member_count,
    )
    .map_err(TextGlyphLoweringError::InvalidPlan)?;
    spec.validate()
        .map_err(TextGlyphLoweringError::InvalidSpec)?;
    Ok(CompiledFamilyAnimation {
        target,
        plan,
        spec,
        time_map,
    })
}

fn resolve_family_spans(
    store: &SemanticStore,
    members: impl Iterator<
        Item = (
            noon_core::SemanticFamilyAnimationMember,
            FamilyAnimationMode,
        ),
    >,
) -> Result<
    std::collections::HashMap<noon_core::SemanticFamilyAnimationMember, (SemanticNodeId, u32, u32)>,
    TextGlyphLoweringError,
> {
    let mut families = std::collections::HashMap::new();
    for (member, mode) in members {
        families.entry(member.family).or_insert(mode);
    }
    let mut spans = std::collections::HashMap::new();
    for (family, mode) in families {
        let leaves = store
            .ordered_leaf_nodes(family)
            .map_err(|_| TextGlyphLoweringError::MissingSemanticTarget(family.into()))?;
        let mut counts = Vec::with_capacity(leaves.len());
        let mut total = 0_u32;
        for leaf in &leaves {
            let state = store
                .semantic_object_state_checked(*leaf)
                .map_err(|_| TextGlyphLoweringError::MissingSemanticTarget((*leaf).into()))?;
            let content = match (mode, state.content) {
                (_, noon_core::SemanticObjectContent::Geometry(_)) => None,
                (_, noon_core::SemanticObjectContent::Text(handle)) => Some(handle),
            };
            let count = match content {
                Some(handle) => RetainedAnimationMembers::resolve(
                    &ObjectContentRef::Text(handle),
                    store.text_resources(),
                )
                .map_err(TextGlyphLoweringError::InvalidMembers)?
                .member_count(),
                None => 1,
            };
            counts.push((total, count));
            total = total
                .checked_add(count)
                .ok_or(TextGlyphLoweringError::InvalidFamilyMember(
                    noon_core::SemanticFamilyAnimationMember {
                        family,
                        leaf_index: counts.len() - 1,
                    },
                ))?;
        }
        for (leaf_index, (first, _)) in counts.into_iter().enumerate() {
            spans.insert(
                noon_core::SemanticFamilyAnimationMember { family, leaf_index },
                (leaves[leaf_index], first, total),
            );
        }
    }
    Ok(spans)
}

pub fn lower_semantic_text_glyph_animations(
    store: &SemanticStore,
    schedule: &SemanticAnimationScheduleProjection,
) -> Result<Vec<CompiledFamilyAnimation>, TextGlyphLoweringError> {
    let family_spans = resolve_family_spans(
        store,
        schedule
            .leaves()
            .iter()
            .filter_map(|leaf| match leaf.payload {
                SemanticScheduledAnimationPayload::TextGlyph {
                    mode,
                    family_member: Some(member),
                    ..
                } => Some((member, mode)),
                _ => None,
            }),
    )?;
    let drivers = schedule
        .leaves()
        .iter()
        .map(|leaf| {
            let interval = noon_core::continuous_time_map_interval(leaf.timing, &leaf.time_map)
                .map_err(TextGlyphLoweringError::InvalidTimeMap)?;
            Ok((
                leaf.animation.into(),
                leaf.execution_object_id,
                interval,
                matches!(
                    leaf.payload,
                    SemanticScheduledAnimationPayload::TextGlyph { .. }
                ),
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    reject_family_driver_conflicts(&drivers)?;
    schedule
        .leaves()
        .iter()
        .filter_map(|leaf| {
            let SemanticScheduledAnimationPayload::TextGlyph {
                mode,
                reverse_member_order,
                family_member,
            } = leaf.payload
            else {
                return None;
            };
            Some(
                FamilyAnimationSpec::new(
                    mode,
                    leaf.timing.start_time,
                    leaf.timing.duration,
                    leaf.options.lag_ratio,
                    leaf.options.rate_func,
                    leaf.options.reverse_rate_function,
                    reverse_member_order,
                )
                .map_err(TextGlyphLoweringError::InvalidSpec)
                .and_then(|spec| {
                    plan(
                        store,
                        leaf.target,
                        leaf.execution_object_id,
                        family_member,
                        &family_spans,
                        spec,
                        leaf.time_map.clone(),
                    )
                }),
            )
        })
        .collect()
}

pub fn lower_prepared_text_glyph_animations(
    store: &SemanticStore,
    schedule: &PreparedSemanticAnimationScheduleProjection,
) -> Result<Vec<CompiledFamilyAnimation>, TextGlyphLoweringError> {
    let family_spans = resolve_family_spans(
        store,
        schedule
            .leaves()
            .iter()
            .filter_map(|leaf| match leaf.payload {
                PreparedSemanticScheduledAnimationPayload::TextGlyph {
                    mode,
                    family_member: Some(member),
                    ..
                } => Some((member, mode)),
                _ => None,
            }),
    )?;
    let drivers = schedule
        .leaves()
        .iter()
        .map(|leaf| {
            let interval = noon_core::continuous_time_map_interval(leaf.timing, &leaf.time_map)
                .map_err(TextGlyphLoweringError::InvalidTimeMap)?;
            Ok((
                leaf.animation,
                leaf.execution_object_id,
                interval,
                matches!(
                    leaf.payload,
                    PreparedSemanticScheduledAnimationPayload::TextGlyph { .. }
                ),
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    reject_family_driver_conflicts(&drivers)?;
    schedule
        .leaves()
        .iter()
        .filter_map(|leaf| {
            let PreparedSemanticScheduledAnimationPayload::TextGlyph {
                mode,
                reverse_member_order,
                family_member,
            } = leaf.payload
            else {
                return None;
            };
            let Some(target) = leaf.target.existing() else {
                return Some(Err(TextGlyphLoweringError::MissingSemanticTarget(
                    leaf.target,
                )));
            };
            Some(
                FamilyAnimationSpec::new(
                    mode,
                    leaf.timing.start_time,
                    leaf.timing.duration,
                    leaf.options.lag_ratio,
                    leaf.options.rate_func,
                    leaf.options.reverse_rate_function,
                    reverse_member_order,
                )
                .map_err(TextGlyphLoweringError::InvalidSpec)
                .and_then(|spec| {
                    plan(
                        store,
                        target,
                        leaf.execution_object_id,
                        family_member,
                        &family_spans,
                        spec,
                        leaf.time_map.clone(),
                    )
                }),
            )
        })
        .collect()
}

pub(crate) fn reject_family_driver_conflicts(
    drivers: &[(SemanticTransactionNodeRef, ObjectId, (f64, f64), bool)],
) -> Result<(), TextGlyphLoweringError> {
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
        let mut latest_family: Option<(f64, SemanticTransactionNodeRef)> = None;
        for &(animation, _, (start, end), is_family_animation) in target_drivers.iter() {
            let conflicting = if is_family_animation {
                latest_any.filter(|(latest_end, _)| start < *latest_end)
            } else {
                latest_family.filter(|(latest_end, _)| start < *latest_end)
            };
            if let Some((_, first)) = conflicting {
                return Err(TextGlyphLoweringError::ConflictingObjectDrivers {
                    target: *target,
                    first,
                    second: animation,
                });
            }
            if latest_any.is_none_or(|(latest_end, _)| end > latest_end) {
                latest_any = Some((end, animation));
            }
            if is_family_animation && latest_family.is_none_or(|(latest_end, _)| end > latest_end) {
                latest_family = Some((end, animation));
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

    fn plain_text_source(store: &mut SemanticStore, source: &str, attach: bool) -> SemanticNodeId {
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
        if attach {
            store.attach_to_scene(target).unwrap();
        }
        target
    }

    fn plain_text(store: &mut SemanticStore) -> SemanticNodeId {
        plain_text_source(store, "ABCDEFGHIJKLMN ", true)
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
            lower_semantic_text_glyph_animations(&store, &schedule).unwrap(),
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
            Err(PreparedSemanticAnimationLoweringError::TextGlyph(
                TextGlyphLoweringError::ConflictingObjectDrivers { .. }
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

    #[test]
    fn neutral_mixed_family_members_preserve_global_order_and_independent_reversal() {
        for mode in [
            FamilyAnimationMode::Reveal,
            FamilyAnimationMode::DrawBorderThenFill,
        ] {
            let mut store = SemanticStore::new();
            let text = plain_text_source(&mut store, "AB", false);
            let geometry = store.insert_semantic_object(SemanticObjectState::new(
                noon_core::StoredGeometry::Circle { radius: 1.0 },
            ));
            let family = store.insert_family();
            store.add_semantic_family_member(family, text).unwrap();
            store.add_semantic_family_member(family, geometry).unwrap();
            store.attach_to_scene(family).unwrap();
            let index = index(&store);

            let options = AnimationOptions::new()
                .run_time(2.0)
                .rate_func(noon_core::RateFunction::Linear)
                .lag_ratio(0.25)
                .reverse_rate_function(true)
                .introducer(false)
                .remover(false);
            let mut transaction = SemanticMutationTransaction::new();
            let text_leaf = transaction.create_family_animation_member(
                text,
                mode,
                true,
                noon_core::SemanticFamilyAnimationMember {
                    family,
                    leaf_index: 0,
                },
                options,
            );
            let geometry_leaf = transaction.create_family_animation_member(
                geometry,
                mode,
                true,
                noon_core::SemanticFamilyAnimationMember {
                    family,
                    leaf_index: 1,
                },
                options,
            );
            let root = transaction.create_animation_composition(
                SemanticAnimationCompositionKind::Parallel,
                [text_leaf, geometry_leaf],
                AnimationOptions::new().rate_func(noon_core::RateFunction::Linear),
            );
            let prepared = transaction.prepare(&mut store).unwrap();
            let lowered = lower_prepared_semantic_animation_composition(
                &prepared,
                &index,
                root,
                -0.5,
                AnimationOptions::new(),
                |_| Option::<EffectiveAnimationProperties>::None,
            )
            .unwrap();

            assert_eq!(lowered.family_animations().len(), 2);
            let text_plan = &lowered.family_animations()[0];
            let geometry_plan = &lowered.family_animations()[1];
            assert_eq!(text_plan.plan.member_plan().total_member_count(), 3);
            assert_eq!(geometry_plan.plan.member_plan().total_member_count(), 3);
            assert_eq!(text_plan.plan.member_plan().leaves()[0].first_member, 0);
            assert_eq!(text_plan.plan.member_plan().leaves()[0].member_count, 2);
            assert_eq!(geometry_plan.plan.member_plan().leaves()[0].first_member, 2);
            assert_eq!(geometry_plan.plan.member_plan().leaves()[0].member_count, 1);
            for animation in lowered.family_animations() {
                assert_eq!(animation.spec.mode, mode);
                assert_eq!(animation.spec.start_time, -0.5);
                assert_eq!(animation.spec.duration, 2.0);
                assert_eq!(animation.spec.lag_ratio, 0.25);
                assert_eq!(
                    animation.spec.rate_function,
                    noon_core::RateFunction::Linear
                );
                assert!(animation.spec.reverse_rate_function);
                assert!(animation.spec.reverse_member_order);
            }
        }
    }

    #[test]
    fn family_text_write_projects_unequal_leaves_into_one_global_glyph_order() {
        let mut store = SemanticStore::new();
        let first = plain_text_source(&mut store, "A", false);
        let second = plain_text_source(&mut store, "BCDE", false);
        let family = store.insert_family();
        store.add_semantic_family_member(family, first).unwrap();
        store.add_semantic_family_member(family, second).unwrap();
        store.attach_to_scene(family).unwrap();
        let index = index(&store);

        let mut transaction = SemanticMutationTransaction::new();
        let options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(noon_core::RateFunction::Linear)
            .lag_ratio(0.25)
            .introducer(true);
        let children = [first, second]
            .into_iter()
            .enumerate()
            .map(|(leaf_index, target)| {
                transaction.create_family_text_write_member_animation(
                    target,
                    false,
                    noon_core::SemanticFamilyAnimationMember { family, leaf_index },
                    options,
                )
            })
            .collect::<Vec<_>>();
        let root = transaction.create_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            children,
            AnimationOptions::new().rate_func(noon_core::RateFunction::Linear),
        );
        let prepared = transaction.prepare(&mut store).unwrap();
        let lowered = lower_prepared_semantic_animation_composition(
            &prepared,
            &index,
            root,
            0.0,
            AnimationOptions::new(),
            |_| Option::<EffectiveAnimationProperties>::None,
        )
        .unwrap();
        assert_eq!(lowered.family_animations().len(), 2);
        let spans = lowered
            .family_animations()
            .iter()
            .map(|animation| animation.plan.member_plan().leaves()[0])
            .collect::<Vec<_>>();
        assert_eq!(spans[0].first_member, 0);
        assert_eq!(spans[0].member_count, 1);
        assert_eq!(spans[1].first_member, 1);
        assert_eq!(spans[1].member_count, 4);
        assert!(lowered
            .family_animations()
            .iter()
            .all(|animation| { animation.plan.member_plan().total_member_count() == 5 }));

        let state = noon_core::FamilyAnimationState {
            mode: FamilyAnimationMode::DrawBorderThenFill,
            overall_progress: 0.5,
            lag_ratio: 0.25,
            rate_function: noon_core::RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order: false,
        };
        let first_progress = lowered.family_animations()[0]
            .plan
            .member_plan()
            .leaf_progress(state, first)
            .unwrap();
        let second_progress = lowered.family_animations()[1]
            .plan
            .member_plan()
            .leaf_progress(state, second)
            .unwrap();
        assert_eq!(first_progress.member_progress(0).unwrap(), 1.0);
        assert_eq!(second_progress.member_progress(0).unwrap(), 0.75);
        assert_eq!(second_progress.member_progress(3).unwrap(), 0.0);

        let reversed = noon_core::FamilyAnimationState {
            reverse_member_order: true,
            ..state
        };
        let first_reversed = lowered.family_animations()[0]
            .plan
            .member_plan()
            .leaf_progress(reversed, first)
            .unwrap();
        let second_reversed = lowered.family_animations()[1]
            .plan
            .member_plan()
            .leaf_progress(reversed, second)
            .unwrap();
        assert_eq!(first_reversed.member_progress(0).unwrap(), 0.0);
        assert_eq!(second_reversed.member_progress(0).unwrap(), 0.25);
        assert_eq!(second_reversed.member_progress(3).unwrap(), 1.0);
    }
}
