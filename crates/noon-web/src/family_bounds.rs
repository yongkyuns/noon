use noon_core::{Bounds2D64, Rect, SemanticNodeId, SemanticNodeKind, SemanticStore, Vec2};

/// Aggregate layout bounds for a semantic family using authoritative shared leaf order.
///
/// Frontends may retain wrapper trees for language-level identity, but they must not
/// recompute family geometry. This plan snapshots the semantic family's leaf order,
/// validates every supplied leaf against that order, and performs the aggregate bounds
/// union in Rust. Leaf bounds themselves remain owned by the existing shared mobject
/// handle implementation.
#[derive(Clone, Debug)]
pub struct FrontendFamilyBoundsPlan {
    leaves: Vec<SemanticNodeId>,
    next_leaf: usize,
    bounds: Option<Bounds2D64>,
}

impl FrontendFamilyBoundsPlan {
    pub fn begin(store: &SemanticStore, family: SemanticNodeId) -> Result<Self, String> {
        Ok(Self {
            leaves: semantic_family_leaf_ids(store, family)?,
            next_leaf: 0,
            bounds: None,
        })
    }

    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    pub fn accept_leaf_bounds(
        &mut self,
        leaf: SemanticNodeId,
        bounds: Option<Bounds2D64>,
    ) -> Result<(), String> {
        let expected = self
            .leaves
            .get(self.next_leaf)
            .copied()
            .ok_or_else(|| "family bounds received too many leaves".to_owned())?;
        if leaf != expected {
            return Err(format!(
                "family bounds leaf mismatch at index {}: expected {expected:?}, got {leaf:?}",
                self.next_leaf
            ));
        }

        if let Some(bounds) = bounds {
            include_bounds(&mut self.bounds, bounds);
        }
        self.next_leaf += 1;
        Ok(())
    }

    pub fn finish(&self) -> Result<Option<Bounds2D64>, String> {
        if self.next_leaf != self.leaves.len() {
            return Err(format!(
                "family bounds is incomplete: accepted {} of {} leaves",
                self.next_leaf,
                self.leaves.len()
            ));
        }
        Ok(self.bounds)
    }

    /// Finish aggregation and explicitly lower the authoritative semantic bounds
    /// to the compact world-bounds representation consumed by retained matcher
    /// geometry. This keeps f64 -> f32 conversion in shared Rust rather than in a
    /// Python/JS adapter and avoids fabricating a temporary aggregate scene object.
    pub fn finish_world_bounds(&self) -> Result<Option<Rect>, String> {
        self.finish()?
            .map(|bounds| {
                Ok(Rect::new(
                    Vec2::new(
                        lower_f32("family bounds min.x", bounds.min_x)?,
                        lower_f32("family bounds min.y", bounds.min_y)?,
                    ),
                    Vec2::new(
                        lower_f32("family bounds max.x", bounds.max_x)?,
                        lower_f32("family bounds max.y", bounds.max_y)?,
                    ),
                ))
            })
            .transpose()
    }
}

fn lower_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

fn include_bounds(aggregate: &mut Option<Bounds2D64>, bounds: Bounds2D64) {
    if let Some(aggregate) = aggregate {
        aggregate.include(bounds.min_x, bounds.min_y);
        aggregate.include(bounds.max_x, bounds.max_y);
    } else {
        *aggregate = Some(bounds);
    }
}

fn semantic_family_leaf_ids(
    store: &SemanticStore,
    family: SemanticNodeId,
) -> Result<Vec<SemanticNodeId>, String> {
    let root = store
        .node(family)
        .ok_or_else(|| format!("unknown family semantic node {family:?}"))?;
    if !matches!(root.kind(), SemanticNodeKind::Family) {
        return Err(format!("semantic node {family:?} is not a family"));
    }

    let leaves = store
        .ordered_leaf_nodes(family)
        .map_err(|error| error.to_string())?;
    for leaf in &leaves {
        let node = store
            .node(*leaf)
            .ok_or_else(|| format!("unknown semantic family member {leaf:?}"))?;
        if !matches!(node.kind(), SemanticNodeKind::AuthoringObject) {
            return Err(format!(
                "family bounds member {leaf:?} is not an authoring object"
            ));
        }
    }
    Ok(leaves)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested_family() -> (
        SemanticStore,
        SemanticNodeId,
        SemanticNodeId,
        SemanticNodeId,
        SemanticNodeId,
    ) {
        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let third = store.insert_authoring_object();
        let nested = store.insert_family();
        let root = store.insert_family();
        store.add_member(nested, second).expect("nested second");
        store.add_member(nested, third).expect("nested third");
        store.add_member(root, first).expect("root first");
        store.add_member(root, nested).expect("root nested");
        (store, root, first, second, third)
    }

    #[test]
    fn aggregates_nested_family_bounds_in_semantic_leaf_order() {
        let (store, root, first, second, third) = nested_family();
        let mut plan = FrontendFamilyBoundsPlan::begin(&store, root).expect("family bounds plan");
        assert_eq!(plan.leaf_count(), 3);

        plan.accept_leaf_bounds(
            first,
            Some(Bounds2D64 {
                min_x: -2.0,
                min_y: -1.0,
                max_x: 0.0,
                max_y: 1.0,
            }),
        )
        .expect("first bounds");
        plan.accept_leaf_bounds(
            second,
            Some(Bounds2D64 {
                min_x: 3.0,
                min_y: -4.0,
                max_x: 5.0,
                max_y: -2.0,
            }),
        )
        .expect("second bounds");
        plan.accept_leaf_bounds(third, None)
            .expect("bounds-free leaf is still consumed");

        assert_eq!(
            plan.finish().expect("complete plan"),
            Some(Bounds2D64 {
                min_x: -2.0,
                min_y: -4.0,
                max_x: 5.0,
                max_y: 1.0,
            })
        );
        assert_eq!(
            plan.finish_world_bounds().expect("lowered bounds"),
            Some(Rect::new(Vec2::new(-2.0, -4.0), Vec2::new(5.0, 1.0)))
        );
    }

    #[test]
    fn shared_alias_is_consumed_once_in_first_semantic_occurrence() {
        let mut store = SemanticStore::new();
        let shared = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let nested = store.insert_family();
        let root = store.insert_family();
        store.add_member(nested, second).unwrap();
        store.add_member(nested, shared).unwrap();
        store.add_member(root, shared).unwrap();
        store.add_member(root, nested).unwrap();

        let mut plan = FrontendFamilyBoundsPlan::begin(&store, root).unwrap();
        assert_eq!(plan.leaf_count(), 2);
        plan.accept_leaf_bounds(shared, None).unwrap();
        plan.accept_leaf_bounds(second, None).unwrap();
        assert_eq!(plan.finish().unwrap(), None);
    }

    #[test]
    fn rejects_frontend_leaf_reordering_before_aggregation() {
        let (store, root, first, second, _) = nested_family();
        let mut plan = FrontendFamilyBoundsPlan::begin(&store, root).expect("family bounds plan");
        let error = plan
            .accept_leaf_bounds(second, Some(Bounds2D64::point(1.0, 2.0)))
            .expect_err("out-of-order wrapper must fail");
        assert!(error.contains("leaf mismatch"));

        plan.accept_leaf_bounds(first, Some(Bounds2D64::point(0.0, 0.0)))
            .expect("failed attempt must not advance plan");
    }

    #[test]
    fn rejects_incomplete_family_traversal() {
        let (store, root, first, _, _) = nested_family();
        let mut plan = FrontendFamilyBoundsPlan::begin(&store, root).expect("family bounds plan");
        plan.accept_leaf_bounds(first, Some(Bounds2D64::point(0.0, 0.0)))
            .expect("first bounds");
        let error = plan.finish().expect_err("partial family must fail closed");
        assert!(error.contains("incomplete"));
    }

    #[test]
    fn empty_family_has_no_aggregate_bounds() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let plan = FrontendFamilyBoundsPlan::begin(&store, family).expect("empty family plan");
        assert_eq!(plan.leaf_count(), 0);
        assert_eq!(plan.finish().expect("empty family is complete"), None);
        assert_eq!(
            plan.finish_world_bounds().expect("empty lowered bounds"),
            None
        );
    }

    #[test]
    fn rejects_non_family_roots() {
        let mut store = SemanticStore::new();
        let leaf = store.insert_authoring_object();
        let error = FrontendFamilyBoundsPlan::begin(&store, leaf)
            .expect_err("authoring object is not a family");
        assert!(error.contains("is not a family"));
    }

    #[test]
    fn world_bounds_lowering_rejects_values_outside_runtime_precision() {
        let (store, root, first, second, third) = nested_family();
        let mut plan = FrontendFamilyBoundsPlan::begin(&store, root).expect("family bounds plan");
        plan.accept_leaf_bounds(
            first,
            Some(Bounds2D64 {
                min_x: 0.0,
                min_y: 0.0,
                max_x: f64::from(f32::MAX) * 2.0,
                max_y: 1.0,
            }),
        )
        .expect("semantic f64 bounds remain valid");
        plan.accept_leaf_bounds(second, None).expect("second leaf");
        plan.accept_leaf_bounds(third, None).expect("third leaf");

        let error = plan
            .finish_world_bounds()
            .expect_err("runtime lowering must be explicit and checked");
        assert!(error.contains("f32-compatible"));
    }
}
