//! Atomic family arrangement using local staged bounds and one semantic traversal.
//!
//! Manim v0.21 sequences next_to against current bounds, then centers once:
//! <https://github.com/ManimCommunity/manim/blob/v0.21.0/manim/mobject/mobject.py>.
use crate::{
    family_authoring::translation_transaction,
    family_layout::{bounds_critical_point, RelativePlacement},
    semantic_mobject::ManimNextToArgs,
    Bounds2D64, LayoutAnchor, Mobject, MobjectFamily, SemanticNodeId, SemanticVec3,
};
use noon_core::SemanticMutationTransaction;
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
        self.commit_arrangement(FamilyArrangePlan::begin(self, options)?)
    }

    /// Arrange direct members in a row-major grid, sizing rows/columns independently
    /// and preserving the family center. Gaps are in scene units.
    pub fn arrange_in_grid(
        &self,
        rows: Option<usize>,
        columns: Option<usize>,
        gap_x: f64,
        gap_y: f64,
    ) -> Result<(), String> {
        self.commit_arrangement(FamilyArrangePlan::grid(self, rows, columns, gap_x, gap_y)?)
    }

    fn commit_arrangement(&self, mut plan: FamilyArrangePlan) -> Result<(), String> {
        plan.observe_leaf_bounds(|leaf| {
            Mobject::from_node(Rc::clone(self.integration_store()), leaf)?.layout_bounds()
        })?;
        let transaction = plan.transaction(|leaf| {
            Ok(
                Mobject::from_node(Rc::clone(self.integration_store()), leaf)?
                    .state()?
                    .transform
                    .translation,
            )
        })?;
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

struct PlacementStep {
    cell: Option<usize>,
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
    grid: Option<(usize, f64, f64)>,
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
            .integration_store()
            .borrow()
            .node(family.node_id())
            .unwrap()
            .members()
            .to_vec();
        let leaves = |anchor: LayoutAnchor| -> Result<Vec<SemanticNodeId>, String> {
            if !Rc::ptr_eq(family.integration_store(), anchor.integration_store()) {
                return Err("arrangement anchors belong to different authoring stores".into());
            }
            let node = anchor.resolve()?;
            family
                .integration_store()
                .borrow()
                .ordered_leaf_nodes(node)
                .map_err(|e| e.to_string())
        };
        let selected = |node| {
            let anchor = LayoutAnchor::from_node(Rc::clone(family.integration_store()), node);
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
                cell: None,
                moved: leaves(LayoutAnchor::from_node(
                    Rc::clone(family.integration_store()),
                    pair[1],
                ))?,
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
            grid: None,
        })
    }

    pub(crate) fn grid(
        family: &MobjectFamily,
        rows: Option<usize>,
        columns: Option<usize>,
        gap_x: f64,
        gap_y: f64,
    ) -> Result<Self, String> {
        family.validate().map_err(|error| error.to_string())?;
        crate::semantic_mobject::authoring_render_f64("grid horizontal gap", gap_x)?;
        crate::semantic_mobject::authoring_render_f64("grid vertical gap", gap_y)?;
        if rows == Some(0) || columns == Some(0) {
            return Err("grid rows and columns must be positive".into());
        }
        let store = family.integration_store().borrow();
        let ids = store.node(family.node_id()).unwrap().members();
        let count = ids.len();
        let columns = columns.unwrap_or_else(|| match rows {
            Some(rows) => count.div_ceil(rows).max(1),
            None => (count as f64).sqrt().ceil().max(1.0) as usize,
        });
        let used_rows = count.div_ceil(columns);
        if rows.is_some_and(|rows| rows < used_rows) {
            return Err("too few grid rows and columns to fit all members".into());
        }
        let roots = store
            .ordered_leaf_nodes(family.node_id())
            .map_err(|e| e.to_string())?;
        let mut steps = Vec::with_capacity(count);
        // Manim places bottom rows first; preserve that ordering for shared aliases.
        for row in (0..used_rows).rev() {
            let start = row * columns;
            for (column, &id) in ids[start..start + columns.min(count - start)]
                .iter()
                .enumerate()
            {
                let moved = store.ordered_leaf_nodes(id).map_err(|e| e.to_string())?;
                steps.push(PlacementStep {
                    cell: Some(start + column),
                    source: moved.clone(),
                    moved,
                    target: Vec::new(),
                });
            }
        }
        Ok(Self {
            bounds: roots.iter().map(|&id| (id, None)).collect(),
            roots,
            steps,
            observed: false,
            placement: ManimNextToArgs {
                direction: (1.0, 0.0),
                buff: 0.0,
                aligned_edge: (0.0, 0.0),
                mask: (1.0, 1.0),
            },
            center: true,
            grid: Some((columns, gap_x, gap_y)),
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
        let original_center = if self.grid.is_some() {
            bounds_critical_point(aggregate(&self.roots, &deltas), 0.0, 0.0)
        } else {
            (0.0, 0.0)
        };
        let grid_points = self.grid.map(|(columns, gap_x, gap_y)| {
            let count = self.steps.len();
            // Only occupied rows/columns consume memory, even for huge explicit capacities.
            let mut widths = vec![0.0_f64; columns.min(count)];
            let mut heights = vec![0.0_f64; count.div_ceil(columns)];
            for step in &self.steps {
                let cell = step.cell.unwrap();
                if let Some(bounds) = aggregate(&step.source, &deltas) {
                    widths[cell % columns] = widths[cell % columns].max(bounds.width());
                    heights[cell / columns] = heights[cell / columns].max(bounds.height());
                }
            }
            let centers = |sizes: Vec<f64>, gap: f64| {
                let mut offset = 0.0;
                sizes
                    .into_iter()
                    .map(|size| {
                        let center = offset + size / 2.0;
                        offset += size + gap;
                        center
                    })
                    .collect::<Vec<_>>()
            };
            (centers(widths, gap_x), centers(heights, gap_y), columns)
        });
        for step in &self.steps {
            let source = aggregate(&step.source, &deltas);
            let delta = if let Some((xs, ys, columns)) = &grid_points {
                let cell = step.cell.unwrap();
                let center = bounds_critical_point(source, 0.0, 0.0);
                (
                    xs[cell % columns] - center.0,
                    -ys[cell / columns] - center.1,
                )
            } else {
                let target = aggregate(&step.target, &deltas);
                RelativePlacement::Next(self.placement)
                    .delta(source, |x, y| Ok(bounds_critical_point(target, x, y)))?
            };
            for leaf in &step.moved {
                let total = deltas.get_mut(leaf).unwrap();
                total.0 += delta.0;
                total.1 += delta.1;
            }
        }
        if self.center {
            let center = bounds_critical_point(aggregate(&self.roots, &deltas), 0.0, 0.0);
            for delta in deltas.values_mut() {
                delta.0 -= center.0 - original_center.0;
                delta.1 -= center.1 - original_center.1;
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
