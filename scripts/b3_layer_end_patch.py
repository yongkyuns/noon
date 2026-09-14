from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def rep(path: str, old: str, new: str, count: int = 1) -> None:
    p = ROOT / path
    text = p.read_text()
    actual = text.count(old)
    if actual < count:
        raise RuntimeError(f"{path}: expected >= {count} occurrences, found {actual}: {old[:120]!r}")
    p.write_text(text.replace(old, new, count))


# Runtime evaluation: preserve placement on the identity-free output instead of
# rejecting LayerEnd before publication has a chance to order it.
runtime_eval = "crates/noon-runtime/src/derived_display_evaluation.rs"
rep(
    runtime_eval,
    '''            let state = derived_from_row(occurrence.base.text_bounds, base_content, row);
            let object = match occurrence.painter_placement {
                TransientPresentationPainterPlacement::AfterStable {
                    anchor_object_index,
                } => DerivedDisplayObject::new(
                    anchor_object_index,
                    occurrence.occurrence_index,
                    state,
                ),
                TransientPresentationPainterPlacement::LayerEnd => {
                    return Err(DerivedDisplayEvaluationError::UnsupportedPainterPlacement(
                        occurrence.occurrence_index,
                    ));
                }
            };
            objects.push(object);
''',
    '''            let state = derived_from_row(occurrence.base.text_bounds, base_content, row);
            objects.push(DerivedDisplayObject::with_painter_placement(
                occurrence.painter_placement,
                occurrence.occurrence_index,
                state,
            ));
''',
)
rep(
    runtime_eval,
    '''    fn layer_end_occurrence_fails_closed_until_publication_supports_it() {
        let occurrence = DerivedDisplayAnimationOccurrence {
            painter_placement: TransientPresentationPainterPlacement::LayerEnd,
            occurrence_index: 11,
            base: base(GeometryRef::circle(1.0)),
            tracks: vec![track(
                Property::Appearance,
                TrackValues::Scalar { from: 0.0, to: 1.0 },
            )],
        };
        let plan = DerivedDisplayAnimationPlan::new(vec![occurrence]).unwrap();
        assert_eq!(
            plan.evaluate(1.0),
            Err(DerivedDisplayEvaluationError::UnsupportedPainterPlacement(
                11
            ))
        );
    }
''',
    '''    fn layer_end_occurrence_preserves_explicit_painter_placement() {
        let occurrence = DerivedDisplayAnimationOccurrence {
            painter_placement: TransientPresentationPainterPlacement::LayerEnd,
            occurrence_index: 11,
            base: base(GeometryRef::circle(1.0)),
            tracks: vec![track(
                Property::Appearance,
                TrackValues::Scalar { from: 0.0, to: 1.0 },
            )],
        };
        let plan = DerivedDisplayAnimationPlan::new(vec![occurrence]).unwrap();
        let evaluated = plan.evaluate(1.0).unwrap();
        assert_eq!(evaluated.len(), 1);
        assert_eq!(
            evaluated[0].painter_placement(),
            TransientPresentationPainterPlacement::LayerEnd
        );
        assert_eq!(evaluated[0].occurrence_index(), 11);
    }
''',
)

# Runtime publication: occurrence identity remains local, while painter placement
# can either name a real stable anchor or explicitly name the end of a z layer.
publication = "crates/noon-runtime/src/renderer_publication.rs"
rep(
    publication,
    '''use crate::{FrameChanges, FrameState, RetainedPlannedFamilyFrame};
''',
    '''use crate::{
    FrameChanges, FrameState, RetainedPlannedFamilyFrame, TransientPresentationPainterPlacement,
};
''',
)
rep(
    publication,
    '''/// One transient visual occurrence and its placement provenance.
///
/// `anchor_object_index` identifies an existing stable execution slot only for
/// painter placement and source-local invalidation. It is not the identity of this
/// occurrence. `occurrence_index` preserves deterministic order when multiple
/// transient occurrences share one source anchor.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedDisplayObject {
    anchor_object_index: u32,
    occurrence_index: u32,
    state: DerivedDisplayObjectState,
}

impl DerivedDisplayObject {
    pub const fn new(
        anchor_object_index: u32,
        occurrence_index: u32,
        state: DerivedDisplayObjectState,
    ) -> Self {
        Self {
            anchor_object_index,
            occurrence_index,
            state,
        }
    }

    pub const fn anchor_object_index(&self) -> u32 {
        self.anchor_object_index
    }

    pub const fn occurrence_index(&self) -> u32 {
        self.occurrence_index
    }

    pub const fn state(&self) -> &DerivedDisplayObjectState {
        &self.state
    }
}
''',
    '''/// One transient visual occurrence and its placement provenance.
///
/// Painter placement is deliberately distinct from stable execution identity.
/// `AfterStable` names a real stable row solely as a painter anchor; `LayerEnd`
/// carries no anchor at all. `occurrence_index` preserves deterministic local order.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedDisplayObject {
    painter_placement: TransientPresentationPainterPlacement,
    occurrence_index: u32,
    state: DerivedDisplayObjectState,
}

impl DerivedDisplayObject {
    /// Compatibility constructor for existing source-derived transient copies.
    pub const fn new(
        anchor_object_index: u32,
        occurrence_index: u32,
        state: DerivedDisplayObjectState,
    ) -> Self {
        Self::with_painter_placement(
            TransientPresentationPainterPlacement::AfterStable {
                anchor_object_index,
            },
            occurrence_index,
            state,
        )
    }

    pub const fn with_painter_placement(
        painter_placement: TransientPresentationPainterPlacement,
        occurrence_index: u32,
        state: DerivedDisplayObjectState,
    ) -> Self {
        Self {
            painter_placement,
            occurrence_index,
            state,
        }
    }

    pub const fn layer_end(
        occurrence_index: u32,
        state: DerivedDisplayObjectState,
    ) -> Self {
        Self::with_painter_placement(
            TransientPresentationPainterPlacement::LayerEnd,
            occurrence_index,
            state,
        )
    }

    pub const fn painter_placement(&self) -> TransientPresentationPainterPlacement {
        self.painter_placement
    }

    pub const fn stable_anchor_object_index(&self) -> Option<u32> {
        match self.painter_placement {
            TransientPresentationPainterPlacement::AfterStable {
                anchor_object_index,
            } => Some(anchor_object_index),
            TransientPresentationPainterPlacement::LayerEnd => None,
        }
    }

    /// Existing compatibility accessor for source-derived occurrences.
    /// Layer-end presentations intentionally have no stable anchor.
    pub fn anchor_object_index(&self) -> u32 {
        self.stable_anchor_object_index()
            .expect("LayerEnd transient presentation has no stable anchor")
    }

    pub const fn occurrence_index(&self) -> u32 {
        self.occurrence_index
    }

    pub const fn state(&self) -> &DerivedDisplayObjectState {
        &self.state
    }
}
''',
)
rep(
    publication,
    '''    for object in objects {
        let anchor = object.anchor_object_index as usize;
        if anchor >= frame.objects.len() {
            return Err(DerivedDisplayPublicationError::AnchorOutOfRange {
                anchor_object_index: object.anchor_object_index,
                object_count: frame.objects.len(),
            });
        }
        if !frame.is_present(anchor) {
            return Err(DerivedDisplayPublicationError::AnchorNotPresent(
                object.anchor_object_index,
            ));
        }
        if !occurrences.insert(object.occurrence_index) {
''',
    '''    for object in objects {
        if let Some(anchor_object_index) = object.stable_anchor_object_index() {
            let anchor = anchor_object_index as usize;
            if anchor >= frame.objects.len() {
                return Err(DerivedDisplayPublicationError::AnchorOutOfRange {
                    anchor_object_index,
                    object_count: frame.objects.len(),
                });
            }
            if !frame.is_present(anchor) {
                return Err(DerivedDisplayPublicationError::AnchorNotPresent(
                    anchor_object_index,
                ));
            }
        }
        if !occurrences.insert(object.occurrence_index) {
''',
)
rep(
    publication,
    '''    fn derived_display_occurrence_carries_only_existing_anchor_and_local_order() {
        let occurrence = DerivedDisplayObject::new(7, 2, derived_state());
        assert_eq!(occurrence.anchor_object_index(), 7);
        assert_eq!(occurrence.occurrence_index(), 2);
        assert_eq!(occurrence.state().appearance, 0.5);
    }
''',
    '''    fn derived_display_occurrence_carries_explicit_placement_and_local_order() {
        let occurrence = DerivedDisplayObject::new(7, 2, derived_state());
        assert_eq!(occurrence.anchor_object_index(), 7);
        assert_eq!(
            occurrence.painter_placement(),
            TransientPresentationPainterPlacement::AfterStable {
                anchor_object_index: 7
            }
        );
        assert_eq!(occurrence.occurrence_index(), 2);
        assert_eq!(occurrence.state().appearance, 0.5);

        let layer_end = DerivedDisplayObject::layer_end(3, derived_state());
        assert_eq!(layer_end.stable_anchor_object_index(), None);
        assert_eq!(
            layer_end.painter_placement(),
            TransientPresentationPainterPlacement::LayerEnd
        );
    }
''',
)
rep(
    publication,
    '''        let invalid_anchor = DerivedDisplayObject::new(1, 1, derived_state());
        assert!(matches!(
            validate_derived_display_objects(&frame, &[invalid_anchor]),
            Err(DerivedDisplayPublicationError::AnchorOutOfRange { .. })
        ));

        let mut invalid_state = derived_state();
''',
    '''        let invalid_anchor = DerivedDisplayObject::new(1, 1, derived_state());
        assert!(matches!(
            validate_derived_display_objects(&frame, &[invalid_anchor]),
            Err(DerivedDisplayPublicationError::AnchorOutOfRange { .. })
        ));

        // LayerEnd intentionally carries no stable anchor and therefore validates
        // independently of stable frame cardinality/presence.
        let layer_end = DerivedDisplayObject::layer_end(4, derived_state());
        assert_eq!(validate_derived_display_objects(&frame, &[layer_end]), Ok(()));

        let mut invalid_state = derived_state();
''',
)

# Renderer: carry explicit placement into transient slots and merge LayerEnd rows
# at the end of their z layer. Anchor-based visibility filtering remains specific
# to AfterStable; anchorless rows are conservatively retained.
render = "crates/noon-render-wgpu/src/render_order.rs"
rep(
    render,
    '''use noon_runtime::{FrameChanges, FrameState};
''',
    '''use noon_runtime::{
    FrameChanges, FrameState, TransientPresentationPainterPlacement,
};
''',
)
rep(
    render,
    '''pub struct PreparedDerivedDisplaySlot {
    pub anchor_object_index: u32,
    pub occurrence_index: u32,
    pub primitive: DerivedDisplayPrimitive,
    pub instance_index: usize,
}
''',
    '''pub struct PreparedDerivedDisplaySlot {
    pub painter_placement: TransientPresentationPainterPlacement,
    pub occurrence_index: u32,
    pub primitive: DerivedDisplayPrimitive,
    pub instance_index: usize,
}
''',
)
old_inner = '''fn prepare_derived_display_inner(
    frame: &FrameState,
    painter_order: &[u32],
    transient_presentations: &[noon_runtime::TransientPresentationOccurrence],
    visible: Option<&std::collections::HashSet<usize>>,
    mut path_preparer: Option<&mut crate::FramePreparer>,
) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {
    let mut by_anchor =
        std::collections::BTreeMap::<u32, Vec<&noon_runtime::DerivedDisplayObject>>::new();
    for object in transient_presentations {
        if visible
            .is_some_and(|visible| !visible.contains(&(object.anchor_object_index() as usize)))
        {
            continue;
        }
        let anchor = object.anchor_object_index();
        let state = object.state();
        if state.z_index != frame.objects[anchor as usize].z_index {
            return Err(DerivedDisplayRenderError::ZIndexDiffersFromAnchor(
                object.occurrence_index(),
            ));
        }
        by_anchor.entry(anchor).or_default().push(object);
    }
    for objects in by_anchor.values_mut() {
        objects.sort_unstable_by_key(|object| object.occurrence_index());
    }

    let mut prepared = PreparedDerivedDisplay::default();
    let mut seen_anchors = HashSet::with_capacity(by_anchor.len());
    for &object_index in painter_order {
        if visible.is_some_and(|visible| !visible.contains(&(object_index as usize))) {
            continue;
        }
        prepared
            .painter_items
            .push(DisplayPainterItem::Stable { object_index });
        let Some(objects) = by_anchor.get(&object_index) else {
            continue;
        };
        seen_anchors.insert(object_index);
        for &object in objects {
            pack_derived_display_object(object, &mut prepared, path_preparer.as_deref_mut())?;
            prepared.painter_items.push(DisplayPainterItem::Derived {
                occurrence_index: object.occurrence_index(),
            });
        }
    }
    if let Some(&anchor) = by_anchor
        .keys()
        .find(|anchor| !seen_anchors.contains(anchor))
    {
        return Err(DerivedDisplayRenderError::MissingAnchorInPainterOrder(
            anchor,
        ));
    }
    Ok(prepared)
}
'''
new_inner = '''fn prepare_derived_display_inner(
    frame: &FrameState,
    painter_order: &[u32],
    transient_presentations: &[noon_runtime::TransientPresentationOccurrence],
    visible: Option<&std::collections::HashSet<usize>>,
    mut path_preparer: Option<&mut crate::FramePreparer>,
) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {
    let mut by_anchor =
        std::collections::BTreeMap::<u32, Vec<&noon_runtime::DerivedDisplayObject>>::new();
    let mut layer_end = Vec::<&noon_runtime::DerivedDisplayObject>::new();
    for object in transient_presentations {
        match object.painter_placement() {
            TransientPresentationPainterPlacement::AfterStable {
                anchor_object_index,
            } => {
                if visible.is_some_and(|visible| {
                    !visible.contains(&(anchor_object_index as usize))
                }) {
                    continue;
                }
                let anchor = anchor_object_index as usize;
                if anchor >= frame.objects.len() {
                    return Err(DerivedDisplayRenderError::MissingAnchorInPainterOrder(
                        anchor_object_index,
                    ));
                }
                if object.state().z_index != frame.objects[anchor].z_index {
                    return Err(DerivedDisplayRenderError::ZIndexDiffersFromAnchor(
                        object.occurrence_index(),
                    ));
                }
                by_anchor.entry(anchor_object_index).or_default().push(object);
            }
            TransientPresentationPainterPlacement::LayerEnd => layer_end.push(object),
        }
    }
    for objects in by_anchor.values_mut() {
        objects.sort_unstable_by_key(|object| object.occurrence_index());
    }
    layer_end.sort_by(|left, right| {
        left.state()
            .z_index
            .total_cmp(&right.state().z_index)
            .then_with(|| left.occurrence_index().cmp(&right.occurrence_index()))
    });

    let visible_order = painter_order
        .iter()
        .copied()
        .filter(|object_index| {
            !visible.is_some_and(|visible| visible.contains(&(*object_index as usize)).not())
        })
        .collect::<Vec<_>>();
    debug_assert!(visible_order.windows(2).all(|pair| {
        frame.objects[pair[0] as usize].z_index <= frame.objects[pair[1] as usize].z_index
    }));

    let mut prepared = PreparedDerivedDisplay::default();
    let mut seen_anchors = HashSet::with_capacity(by_anchor.len());
    let mut layer_end_index = 0usize;
    for (position, &object_index) in visible_order.iter().enumerate() {
        let current_z = frame.objects[object_index as usize].z_index;
        while layer_end_index < layer_end.len()
            && layer_end[layer_end_index].state().z_index < current_z
        {
            append_derived_display_object(
                layer_end[layer_end_index],
                &mut prepared,
                path_preparer.as_deref_mut(),
            )?;
            layer_end_index += 1;
        }

        prepared
            .painter_items
            .push(DisplayPainterItem::Stable { object_index });
        if let Some(objects) = by_anchor.get(&object_index) {
            seen_anchors.insert(object_index);
            for &object in objects {
                append_derived_display_object(
                    object,
                    &mut prepared,
                    path_preparer.as_deref_mut(),
                )?;
            }
        }

        let next_z = visible_order
            .get(position + 1)
            .map(|next| frame.objects[*next as usize].z_index);
        if next_z.is_none_or(|next_z| next_z > current_z) {
            while layer_end_index < layer_end.len()
                && layer_end[layer_end_index].state().z_index <= current_z
            {
                append_derived_display_object(
                    layer_end[layer_end_index],
                    &mut prepared,
                    path_preparer.as_deref_mut(),
                )?;
                layer_end_index += 1;
            }
        }
    }
    while layer_end_index < layer_end.len() {
        append_derived_display_object(
            layer_end[layer_end_index],
            &mut prepared,
            path_preparer.as_deref_mut(),
        )?;
        layer_end_index += 1;
    }

    if let Some(&anchor) = by_anchor
        .keys()
        .find(|anchor| !seen_anchors.contains(anchor))
    {
        return Err(DerivedDisplayRenderError::MissingAnchorInPainterOrder(
            anchor,
        ));
    }
    Ok(prepared)
}

fn append_derived_display_object(
    object: &noon_runtime::DerivedDisplayObject,
    prepared: &mut PreparedDerivedDisplay,
    path_preparer: Option<&mut crate::FramePreparer>,
) -> Result<(), DerivedDisplayRenderError> {
    pack_derived_display_object(object, prepared, path_preparer)?;
    prepared.painter_items.push(DisplayPainterItem::Derived {
        occurrence_index: object.occurrence_index(),
    });
    Ok(())
}
'''
# Avoid Bool::not trait requirement by using a straightforward filter after insertion.
new_inner = new_inner.replace(
    '''        .filter(|object_index| {
            !visible.is_some_and(|visible| visible.contains(&(*object_index as usize)).not())
        })''',
    '''        .filter(|object_index| {
            visible.is_none_or(|visible| visible.contains(&(**object_index as usize)))
        })'''
)
rep(render, old_inner, new_inner)
rep(
    render,
    '''    prepared.slots.push(PreparedDerivedDisplaySlot {
        anchor_object_index: object.anchor_object_index(),
        occurrence_index: occurrence,
        primitive,
        instance_index,
    });
''',
    '''    prepared.slots.push(PreparedDerivedDisplaySlot {
        painter_placement: object.painter_placement(),
        occurrence_index: occurrence,
        primitive,
        instance_index,
    });
''',
)
# Existing slot assertions should describe explicit placement instead of an anchor field.
text_path = ROOT / render
text = text_path.read_text()
text = text.replace(
    '''PreparedDerivedDisplaySlot {
                anchor_object_index: 0,''',
    '''PreparedDerivedDisplaySlot {
                painter_placement: TransientPresentationPainterPlacement::AfterStable {
                    anchor_object_index: 0,
                },'''
)
text_path.write_text(text)

# Add focused ordering and visibility regressions next to existing painter-order test.
rep(
    render,
    '''    #[test]
    fn transient_renderer_packs_path_without_synthetic_id() {
''',
    '''    #[test]
    fn layer_end_transient_is_merged_after_its_complete_z_layer() {
        let frame = frame(vec![
            object(0, GeometryRef::circle(1.0)),
            object(1, GeometryRef::rectangle(1.0, 1.0)),
        ]);
        let after_stable = DerivedDisplayObject::new(0, 2, state(GeometryRef::circle(0.4)));
        let layer_end = DerivedDisplayObject::layer_end(3, state(GeometryRef::circle(0.3)));
        let prepared = prepare_transient_presentation_rows(
            &frame,
            &[0, 1],
            &[after_stable, layer_end],
        )
        .unwrap();
        assert_eq!(
            prepared.painter_items,
            vec![
                DisplayPainterItem::Stable { object_index: 0 },
                DisplayPainterItem::Derived {
                    occurrence_index: 2
                },
                DisplayPainterItem::Stable { object_index: 1 },
                DisplayPainterItem::Derived {
                    occurrence_index: 3
                },
            ]
        );
        assert_eq!(
            prepared.slot_for_occurrence(3).unwrap().painter_placement,
            TransientPresentationPainterPlacement::LayerEnd
        );
    }

    #[test]
    fn layer_end_transient_is_not_dropped_by_anchor_visibility_filtering() {
        let frame = frame(vec![object(0, GeometryRef::circle(1.0))]);
        let layer_end = DerivedDisplayObject::layer_end(5, state(GeometryRef::circle(0.5)));
        let visible = std::collections::HashSet::new();
        let prepared = prepare_derived_display_inner(
            &frame,
            &[0],
            &[layer_end],
            Some(&visible),
            None,
        )
        .unwrap();
        assert_eq!(
            prepared.painter_items,
            vec![DisplayPainterItem::Derived {
                occurrence_index: 5
            }]
        );
    }

    #[test]
    fn layer_end_transient_sorts_between_distinct_z_layers() {
        let mut frame = frame(vec![
            object(0, GeometryRef::circle(1.0)),
            object(1, GeometryRef::rectangle(1.0, 1.0)),
        ]);
        frame.objects[0].z_index = 0.0;
        frame.objects[1].z_index = 2.0;
        let mut layer_state = state(GeometryRef::circle(0.5));
        layer_state.z_index = 1.0;
        let layer_end = DerivedDisplayObject::layer_end(6, layer_state);
        let prepared = prepare_transient_presentation_rows(&frame, &[0, 1], &[layer_end]).unwrap();
        assert_eq!(
            prepared.painter_items,
            vec![
                DisplayPainterItem::Stable { object_index: 0 },
                DisplayPainterItem::Derived {
                    occurrence_index: 6
                },
                DisplayPainterItem::Stable { object_index: 1 },
            ]
        );
    }

    #[test]
    fn transient_renderer_packs_path_without_synthetic_id() {
''',
)

print("LayerEnd transient publication patch applied")
