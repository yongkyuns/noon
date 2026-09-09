//! Atomic family arrangement using local staged bounds and one semantic traversal.
//!
//! Manim v0.21 sequences next_to against current bounds, then centers once:
//! <https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py>.
use crate::{
    family_authoring::translation_transaction,
    family_layout::{bounds_critical_point, RelativePlacement},
    semantic_mobject::ManimNextToArgs,
    Bounds2D64, LayoutAnchor, Mobject, MobjectFamily, SemanticMutationTransaction, SemanticNodeId,
    SemanticVec3,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

/// Shared placement policy for each consecutive pair of direct family members.
#[derive(Clone, Debug)]
pub struct FamilyArrangeOptions {
    pub placement: ManimNextToArgs,
    pub center: bool,
    pub aligner: Option<LayoutAnchor>,
    pub member_index: Option<isize>,
}

impl FamilyArrangeOptions {
    pub fn new(direction_x: f64, direction_y: f64, buff: f64, center: bool) -> Self {
        Self {
            placement: ManimNextToArgs {
                direction: (direction_x, direction_y),
                buff,
                aligned_edge: (0.0, 0.0),
                mask: (1.0, 1.0),
            },
            center,
            aligner: None,
            member_index: None,
        }
    }
}

impl MobjectFamily {
    pub fn arrange(&self, x: f64, y: f64, buff: f64, center: bool) -> Result<(), String> {
        self.arrange_with_options(&FamilyArrangeOptions::new(x, y, buff, center))
    }

    /// Resolve all selections and stage successive placements before one commit.
    pub fn arrange_with_options(&self, options: &FamilyArrangeOptions) -> Result<(), String> {
        let mut plan = FamilyArrangePlan::begin(self, options)?;
        plan.observe_leaf_bounds(|leaf| {
            Mobject::from_node(Rc::clone(self.store()), leaf)?.layout_bounds()
        })?;
        let transaction = plan.transaction(|leaf| {
            Ok(Mobject::from_node(Rc::clone(self.store()), leaf)?
                .state()?
                .transform
                .translation)
        })?;
        transaction
            .apply(&mut self.store().borrow_mut())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

struct PlacementStep {
    moved: Vec<SemanticNodeId>,
    source: Vec<SemanticNodeId>,
    target: Vec<SemanticNodeId>,
}

/// Request-local observations and proposed deltas, never another scene or runtime.
pub(crate) struct FamilyArrangePlan {
    roots: Vec<SemanticNodeId>,
    steps: Vec<PlacementStep>,
    bounds: BTreeMap<SemanticNodeId, Option<Bounds2D64>>,
    observed: bool,
    placement: ManimNextToArgs,
    center: bool,
}

impl FamilyArrangePlan {
    pub(crate) fn begin(
        family: &MobjectFamily,
        options: &FamilyArrangeOptions,
    ) -> Result<Self, String> {
        family.validate().map_err(|error| error.to_string())?;
        // Normalize/check even an empty request without publishing anything.
        RelativePlacement::Next(options.placement).delta(None, |_, _| Ok((0.0, 0.0)))?;
        let ids = family
            .store()
            .borrow()
            .node(family.node_id())
            .unwrap()
            .members()
            .to_vec();
        let leaves = |anchor: LayoutAnchor| -> Result<Vec<SemanticNodeId>, String> {
            if !Rc::ptr_eq(family.store(), anchor.store()) {
                return Err("arrangement anchors belong to different authoring stores".into());
            }
            let node = anchor.resolve()?;
            family
                .store()
                .borrow()
                .ordered_leaf_nodes(node)
                .map_err(|e| e.to_string())
        };
        let selected = |node| {
            let anchor = LayoutAnchor::from_node(Rc::clone(family.store()), node);
            match options.member_index {
                Some(index) => anchor.member(index),
                None => anchor,
            }
        };
        let roots = leaves(LayoutAnchor::from(family))?;
        let mut required: BTreeSet<_> = roots.iter().copied().collect();
        let mut steps = Vec::with_capacity(ids.len().saturating_sub(1));
        for pair in ids.windows(2) {
            let source = leaves(options.aligner.clone().unwrap_or_else(|| selected(pair[1])))?;
            let target = leaves(selected(pair[0]))?;
            required.extend(source.iter().chain(&target).copied());
            steps.push(PlacementStep {
                moved: leaves(LayoutAnchor::from_node(Rc::clone(family.store()), pair[1]))?,
                source,
                target,
            });
        }
        Ok(Self {
            roots,
            steps,
            bounds: required.into_iter().map(|id| (id, None)).collect(),
            observed: false,
            placement: options.placement,
            center: options.center,
        })
    }

    pub(crate) fn observe_leaf_bounds(
        &mut self,
        mut observe: impl FnMut(SemanticNodeId) -> Result<Option<Bounds2D64>, String>,
    ) -> Result<(), String> {
        for (&id, bounds) in &mut self.bounds {
            *bounds = observe(id)?;
        }
        self.observed = true;
        Ok(())
    }

    pub(crate) fn transaction(
        self,
        authored_translation: impl FnMut(SemanticNodeId) -> Result<SemanticVec3, String>,
    ) -> Result<SemanticMutationTransaction, String> {
        if !self.observed {
            return Err("family arrangement bounds are incomplete".into());
        }
        let mut deltas: BTreeMap<_, _> = self.roots.iter().map(|&id| (id, (0.0, 0.0))).collect();
        let aggregate = |ids: &[SemanticNodeId], deltas: &BTreeMap<SemanticNodeId, (f64, f64)>| {
            let mut total: Option<Bounds2D64> = None;
            for id in ids {
                let Some(bounds) = self.bounds[id] else {
                    continue;
                };
                let (x, y) = deltas.get(id).copied().unwrap_or((0.0, 0.0));
                let shifted = Bounds2D64 {
                    min_x: bounds.min_x + x,
                    max_x: bounds.max_x + x,
                    min_y: bounds.min_y + y,
                    max_y: bounds.max_y + y,
                };
                if let Some(total) = &mut total {
                    total.include(shifted.min_x, shifted.min_y);
                    total.include(shifted.max_x, shifted.max_y);
                } else {
                    total = Some(shifted);
                }
            }
            total
        };
        for step in &self.steps {
            let source = aggregate(&step.source, &deltas);
            let target = aggregate(&step.target, &deltas);
            let delta = RelativePlacement::Next(self.placement)
                .delta(source, |x, y| Ok(bounds_critical_point(target, x, y)))?;
            for leaf in &step.moved {
                let total = deltas.get_mut(leaf).unwrap();
                total.0 += delta.0;
                total.1 += delta.1;
            }
        }
        if self.center {
            let center = bounds_critical_point(aggregate(&self.roots, &deltas), 0.0, 0.0);
            for delta in deltas.values_mut() {
                delta.0 -= center.0;
                delta.1 -= center.1;
            }
        }
        translation_transaction(
            deltas
                .into_iter()
                .filter_map(|(id, (x, y))| (x != 0.0 || y != 0.0).then_some((id, x, y))),
            authored_translation,
        )
    }
}
