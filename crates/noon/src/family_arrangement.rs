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
use crate::{family_grid::GridPlan, AuthoringError, FamilyGridOptions};
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
    pub fn arrange(&self, x: f64, y: f64, buff: f64, center: bool) -> Result<(), AuthoringError> {
        self.arrange_with_options(&FamilyArrangeOptions::new(x, y, buff, center))
    }

    /// Resolve all selections and stage successive placements before one commit.
    pub fn arrange_with_options(
        &self,
        options: &FamilyArrangeOptions,
    ) -> Result<(), AuthoringError> {
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
    ) -> Result<(), AuthoringError> {
        self.arrange_in_grid_with_options(&FamilyGridOptions {
            rows,
            columns,
            gap: (gap_x, gap_y),
            ..Default::default()
        })
    }

    /// Arrange with shared alignment, sizing and fill-order policy in one transaction.
    pub fn arrange_in_grid_with_options(
        &self,
        options: &FamilyGridOptions,
    ) -> Result<(), AuthoringError> {
        self.commit_arrangement(FamilyArrangePlan::grid(self, options)?)
    }

    fn commit_arrangement(&self, plan: FamilyArrangePlan) -> Result<(), AuthoringError> {
        plan.authored_transaction(self.integration_store())?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }
}

struct PlacementStep<K> {
    cell: Option<(usize, usize)>,
    moved: Vec<K>,
    source: Vec<K>,
    target: Vec<K>,
}

/// Measurements derived from one immutable leaf, with no independent identity.
#[derive(Clone, Copy, Default)]
pub(crate) struct ArrangementBounds {
    pub dimensions: Option<Bounds2D64>,
    pub anchors: Option<Bounds2D64>,
}

/// Request-local observations and proposed deltas, never another scene or runtime.
pub(crate) struct FamilyArrangePlan<K = SemanticNodeId> {
    roots: Vec<K>,
    steps: Vec<PlacementStep<K>>,
    bounds: BTreeMap<K, ArrangementBounds>,
    observed: bool,
    placement: ManimNextToArgs,
    center: bool,
    grid: Option<GridPlan>,
}

impl FamilyArrangePlan {
    /// Prepare from authored observations without publishing. Scene-owned cold
    /// placement and explicit raw-store authoring share this exact preparation.
    pub(crate) fn authored_transaction(
        mut self,
        store: &Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.observe_leaf_bounds(|leaf| {
            let object = Mobject::from_node(Rc::clone(store), leaf)?;
            Ok::<_, AuthoringError>(ArrangementBounds {
                dimensions: object.content_layout_bounds()?,
                anchors: object.content_boundary_bounds()?,
            })
        })?;
        self.transaction(|leaf| {
            Ok(Mobject::from_node(Rc::clone(store), leaf)?
                .state()?
                .transform
                .translation)
        })
    }

    pub(crate) fn begin(
        family: &MobjectFamily,
        options: &FamilyArrangeOptions,
    ) -> Result<Self, AuthoringError> {
        family.validate()?;
        // Normalize/check even an empty request without publishing anything.
        RelativePlacement::Next(options.placement)
            .delta::<AuthoringError>(None, |_, _| Ok((0.0, 0.0)))?;
        let ids = family
            .integration_store()
            .borrow()
            .node(family.node_id())
            .unwrap()
            .members()
            .to_vec();
        let leaves = |anchor: LayoutAnchor| -> Result<Vec<SemanticNodeId>, AuthoringError> {
            if !Rc::ptr_eq(family.integration_store(), anchor.integration_store()) {
                return Err(AuthoringError::ForeignStore);
            }
            let node = anchor.resolve()?;
            family
                .integration_store()
                .borrow()
                .ordered_leaf_nodes(node)
                .map_err(AuthoringError::from)
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
            bounds: required
                .into_iter()
                .map(|id| (id, ArrangementBounds::default()))
                .collect(),
            observed: false,
            placement: options.placement,
            center: options.center,
            grid: None,
        })
    }

    pub(crate) fn grid(
        family: &MobjectFamily,
        options: &FamilyGridOptions,
    ) -> Result<Self, AuthoringError> {
        family.validate()?;
        let store = family.integration_store().borrow();
        let ids = store.node(family.node_id()).unwrap().members();
        let roots = store
            .ordered_leaf_nodes(family.node_id())
            .map_err(AuthoringError::from)?;
        let members = ids
            .iter()
            .map(|&id| store.ordered_leaf_nodes(id).map_err(AuthoringError::from))
            .collect::<Result<Vec<_>, _>>()?;
        Self::grid_entries(roots, members, options)
    }

    pub(crate) fn transaction<E: From<AuthoringError>>(
        self,
        authored_translation: impl FnMut(SemanticNodeId) -> Result<SemanticVec3, E>,
    ) -> Result<SemanticMutationTransaction, E> {
        let placement = self.resolve().map_err(E::from)?;
        translation_transaction(
            placement.deltas.into_iter().filter_map(|(id, (x, y))| {
                (x != 0.0 || y != 0.0).then_some((id, SemanticVec3::new(x, y, 0.0)))
            }),
            authored_translation,
        )
    }
}

impl<K: Copy + Ord> FamilyArrangePlan<K> {
    /// Resolve an already captured local member projection without publishing
    /// a temporary family. Keys are local observations, never new scene IDs.
    pub(crate) fn grid_entries(
        roots: Vec<K>,
        members: Vec<Vec<K>>,
        options: &FamilyGridOptions,
    ) -> Result<Self, AuthoringError> {
        let grid = GridPlan::new(options, members.len())?;
        let mut steps: Vec<_> = members
            .into_iter()
            .enumerate()
            .map(|(index, moved)| PlacementStep {
                cell: Some(grid.cell(index)),
                source: moved.clone(),
                moved,
                target: Vec::new(),
            })
            .collect();
        steps.sort_by_key(|step| step.cell);
        Ok(Self {
            bounds: roots
                .iter()
                .map(|&id| (id, ArrangementBounds::default()))
                .collect(),
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
            grid: Some(grid),
        })
    }

    pub(crate) fn observe_leaf_bounds<E>(
        &mut self,
        mut observe: impl FnMut(K) -> Result<ArrangementBounds, E>,
    ) -> Result<(), E> {
        for (&id, bounds) in &mut self.bounds {
            *bounds = observe(id)?;
        }
        self.observed = true;
        Ok(())
    }

    pub(crate) fn resolve(self) -> Result<ArrangementPlacement<K>, AuthoringError> {
        if !self.observed {
            return Err(AuthoringError::IncompleteArrangement);
        }
        let mut deltas: BTreeMap<_, _> = self.roots.iter().map(|&id| (id, (0.0, 0.0))).collect();
        let aggregate = |ids: &[K], deltas: &BTreeMap<K, (f64, f64)>, dimensions: bool| {
            aggregate_bounds(&self.bounds, deltas, ids, dimensions)
        };
        let original_center = if self.grid.is_some() {
            bounds_critical_point(aggregate(&self.roots, &deltas, false), 0.0, 0.0)
        } else {
            (0.0, 0.0)
        };
        let grid_points = self.grid.as_ref().map(|grid| {
            grid.targets(
                self.steps
                    .iter()
                    .map(|step| (step.cell.unwrap(), aggregate(&step.source, &deltas, true))),
            )
        });
        for step in &self.steps {
            let source = aggregate(&step.source, &deltas, false);
            let delta = if let Some(points) = &grid_points {
                let (target, alignment) = points[&step.cell.unwrap()];
                let from = bounds_critical_point(source, alignment.0, alignment.1);
                let to = bounds_critical_point(Some(target), alignment.0, alignment.1);
                (to.0 - from.0, to.1 - from.1)
            } else {
                let target = aggregate(&step.target, &deltas, false);
                RelativePlacement::Next(self.placement)
                    .delta::<AuthoringError>(source, |x, y| {
                        Ok(bounds_critical_point(target, x, y))
                    })?
            };
            for leaf in &step.moved {
                let total = deltas.get_mut(leaf).unwrap();
                total.0 += delta.0;
                total.1 += delta.1;
            }
        }
        if self.center {
            let center = bounds_critical_point(aggregate(&self.roots, &deltas, false), 0.0, 0.0);
            for delta in deltas.values_mut() {
                delta.0 -= center.0 - original_center.0;
                delta.1 -= center.1 - original_center.1;
            }
        }
        Ok(ArrangementPlacement {
            #[cfg(any(feature = "native-text", feature = "latex"))]
            bounds: self.bounds,
            deltas,
        })
    }
}

/// Final request-local placement, shared by family operations and composites.
/// It can be appended to the caller's existing publication transaction.
pub(crate) struct ArrangementPlacement<K> {
    #[cfg(any(feature = "native-text", feature = "latex"))]
    bounds: BTreeMap<K, ArrangementBounds>,
    pub(crate) deltas: BTreeMap<K, (f64, f64)>,
}
#[cfg(any(feature = "native-text", feature = "latex"))]
impl<K: Copy + Ord> ArrangementPlacement<K> {
    pub(crate) fn bounds(&self, ids: &[K], dimensions: bool) -> Option<Bounds2D64> {
        aggregate_bounds(&self.bounds, &self.deltas, ids, dimensions)
    }
    pub(crate) fn center_at(&mut self, x: f64, y: f64) {
        let keys: Vec<_> = self.deltas.keys().copied().collect();
        let center = bounds_critical_point(self.bounds(&keys, false), 0.0, 0.0);
        for delta in self.deltas.values_mut() {
            delta.0 += x - center.0;
            delta.1 += y - center.1;
        }
    }
}

fn aggregate_bounds<K: Copy + Ord>(
    observed: &BTreeMap<K, ArrangementBounds>,
    deltas: &BTreeMap<K, (f64, f64)>,
    ids: &[K],
    dimensions: bool,
) -> Option<Bounds2D64> {
    let mut total: Option<Bounds2D64> = None;
    for id in ids {
        let measured = observed[id];
        let Some(bounds) = (if dimensions {
            measured.dimensions
        } else {
            measured.anchors
        }) else {
            continue;
        };
        let (x, y) = deltas.get(id).copied().unwrap_or((0.0, 0.0));
        let next = Bounds2D64 {
            min_x: bounds.min_x + x,
            max_x: bounds.max_x + x,
            min_y: bounds.min_y + y,
            max_y: bounds.max_y + y,
        };
        if let Some(total) = &mut total {
            total.include(next.min_x, next.min_y);
            total.include(next.max_x, next.max_y);
        } else {
            total = Some(next);
        }
    }
    total
}
