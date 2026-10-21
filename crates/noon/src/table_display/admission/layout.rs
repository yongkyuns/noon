//! One request-local placement path for retained, text and numeric table entries.
use super::*;
use crate::family_arrangement::{ArrangementBounds, FamilyArrangePlan};
use noon_core::{SemanticNodeId, SemanticStore, SemanticTransactionNodeRef};
use std::collections::{BTreeSet, HashMap};

pub(super) struct Entry {
    pub root: SemanticTransactionNodeRef,
    leaves: Vec<(SemanticTransactionNodeRef, SemanticObjectState)>,
}
impl Entry {
    pub fn retained(entry: &crate::composite_entry::CompositeEntry) -> Self {
        Self {
            root: entry.root().into(),
            leaves: entry
                .leaves()
                .iter()
                .map(|(leaf, state)| (leaf.node_id().into(), state.clone()))
                .collect(),
        }
    }
    pub fn pending(root: SemanticTransactionNodeRef, state: SemanticObjectState) -> Self {
        Self {
            root,
            leaves: vec![(root, state)],
        }
    }
}
#[derive(Default)]
pub(super) struct Labels {
    pub rows: Option<Vec<Entry>>,
    pub columns: Option<Vec<Entry>>,
    pub corner: Option<Entry>,
}
impl Labels {
    fn roots(entries: Option<&[Entry]>) -> Option<Vec<SemanticNodeId>> {
        entries.map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    entry
                        .root
                        .existing()
                        .expect("retained labels have existing identities")
                })
                .collect()
        })
    }
}

pub(super) fn stage_layout(
    store: &SemanticStore,
    tx: &mut SemanticMutationTransaction,
    body: Vec<Entry>,
    shape: TableShape,
    labels: Labels,
    options: TableOptions,
) -> Result<StagedTable, AuthoringError> {
    let body_roots: Vec<_> = body.iter().map(|entry| entry.root).collect();
    let row_roots = Labels::roots(labels.rows.as_deref());
    let column_roots = Labels::roots(labels.columns.as_deref());
    let top_root = labels.corner.as_ref().map(|entry| {
        entry
            .root
            .existing()
            .expect("retained corner has an existing identity")
    });
    // Ordinals address only this immutable observation batch. Publication still
    // targets the original Existing/Pending semantic references.
    let mut leaves = Vec::new();
    let mut leaf_indices = HashMap::new();
    let mut entries = HashMap::new();
    for entry in body
        .iter()
        .chain(labels.rows.iter().flatten())
        .chain(labels.columns.iter().flatten())
        .chain(labels.corner.iter())
    {
        let mut indices = Vec::new();
        for (node, state) in &entry.leaves {
            let index = *leaf_indices.entry(*node).or_insert_with(|| {
                let index = leaves.len();
                leaves.push((*node, state.clone()));
                index
            });
            indices.push(index);
        }
        entries.insert(entry.root, indices);
    }
    // A missing corner reserves a grid slot but has no points to transform.
    let declaration = noon_core::SemanticTableStructure::new(
        body_roots
            .chunks(shape.columns)
            .map(|row| row.iter().copied().map(Some).collect())
            .collect(),
        row_roots
            .as_ref()
            .map(|roots| roots.iter().copied().map(|id| Some(id.into())).collect()),
        column_roots
            .as_ref()
            .map(|roots| roots.iter().copied().map(|id| Some(id.into())).collect()),
        top_root.map(|id| Some(id.into())),
        (row_roots.is_some() && column_roots.is_some() && top_root.is_none()).then_some(None),
    );
    let grid = (0..super::super::queries::displayed_shape(&declaration).0)
        .map(|row| {
            super::super::queries::displayed_row(&declaration, row)
                .map_err(|_| AuthoringError::IncompleteArrangement)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let flat = super::super::queries::unique_last(grid.iter().flatten().copied().collect());
    let members: Vec<Vec<usize>> = flat
        .iter()
        .map(|root| root.map_or_else(Vec::new, |id| entries[&id].clone()))
        .collect();
    let roots: Vec<_> = members
        .iter()
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut plan = FamilyArrangePlan::grid_entries(
        roots,
        members,
        &crate::FamilyGridOptions {
            rows: Some(grid.len()),
            columns: Some(grid[0].len()),
            gap: (options.h_buff, options.v_buff),
            cell_alignment: (options.cell_alignment_x, options.cell_alignment_y),
            ..Default::default()
        },
    )?;
    plan.observe_leaf_bounds(|index| {
        let state = &leaves[index].1;
        let transform = state
            .transform
            .as_planar()
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        Ok::<_, AuthoringError>(ArrangementBounds {
            dimensions: crate::semantic_mobject::layout_for_content(
                store,
                state.content,
                transform,
            )?,
            anchors: crate::semantic_mobject::boundary_for_content(
                store,
                state.content,
                transform,
            )?,
        })
    })?;
    let mut placement = plan.resolve()?;
    placement.center_at(0.0, 0.0);
    let projected = |nodes: &[Option<SemanticTransactionNodeRef>]| {
        let indices: Vec<_> = nodes
            .iter()
            .flatten()
            .flat_map(|id| entries[id].iter().copied())
            .collect();
        placement.bounds(&indices, false)
    };
    let row_bounds: Vec<_> = grid.iter().map(|row| projected(row)).collect();
    let column_bounds: Vec<_> = (0..grid[0].len())
        .map(|col| projected(&grid.iter().map(|row| row[col]).collect::<Vec<_>>()))
        .collect();
    let global = projected(&flat);
    let point = |bounds: Option<Bounds2D64>| bounds.unwrap_or(Bounds2D64::point(0.0, 0.0));
    let overall = point(global);
    let left = overall.min_x - options.h_buff * 0.5;
    let right = overall.max_x + options.h_buff * 0.5;
    let bottom = overall.min_y - options.v_buff * 0.5;
    let top = overall.max_y + options.v_buff * 0.5;
    let mut lines = Vec::new();
    let mut line = |x1: f64, y1: f64, x2: f64, y2: f64| {
        let mut state = SemanticObjectState::new(StoredGeometry::Line {
            start: noon_core::Vec2::new(x1 as f32, y1 as f32),
            end: noon_core::Vec2::new(x2 as f32, y2 as f32),
        });
        state.style = table_line_style(options);
        lines.push(state);
    };
    for index in 0..=row_bounds.len() {
        let outer = index == 0 || index == row_bounds.len();
        if (outer && options.include_outer_lines) || (!outer && options.include_inner_lines) {
            let y = if index == 0 {
                point(row_bounds[0]).max_y + options.v_buff * 0.5
            } else {
                point(row_bounds[index - 1]).min_y - options.v_buff * 0.5
            };
            line(left, y, right, y);
        }
    }
    for index in 0..=column_bounds.len() {
        let outer = index == 0 || index == column_bounds.len();
        if (outer && options.include_outer_lines) || (!outer && options.include_inner_lines) {
            let x = if index == 0 {
                point(column_bounds[0]).min_x - options.h_buff * 0.5
            } else {
                point(column_bounds[index - 1]).max_x + options.h_buff * 0.5
            };
            line(x, bottom, x, top);
        }
    }
    let mut painted: Vec<_> = flat.iter().flatten().copied().collect();
    if flat.first().is_some_and(|root| root.is_some()) && projected(&flat[..1]).is_none() {
        painted.remove(0);
    }
    for (index, (x, y)) in placement.deltas {
        if x == 0.0 && y == 0.0 {
            continue;
        }
        let (node, state) = &leaves[index];
        let mut translation = state.transform.translation;
        translation.x += x;
        translation.y += y;
        translation.lower_xy_f32().map_err(AuthoringError::from)?;
        tx.set_property(*node, SemanticObjectProperty::Translation, translation);
    }
    stage(
        tx,
        body_roots,
        shape,
        lines,
        TableLabels {
            rows: row_roots.as_deref(),
            columns: column_roots.as_deref(),
            top_left_entry: top_root,
        },
        options,
        painted,
    )
}
