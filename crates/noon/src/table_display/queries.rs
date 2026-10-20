//! Logical table observations share the retained semantic declaration.
use super::*;
use crate::{integer_index::normalize_index, IntegerIndex};
use noon_core::{SemanticNodeId, SemanticTableLayout, SemanticTableStructure};
use std::collections::HashSet;

impl Table {
    fn query_layout(&self) -> Result<SemanticTableLayout, TableAuthoringError> {
        self.family.validate()?;
        self.family
            .integration_store()
            .borrow()
            .semantic_table_layout(self.family.node_id())
            .map_err(AuthoringError::from)?
            .ok_or(TableAuthoringError::InvalidStructure)
    }
    pub(super) fn query_grid(
        &self,
    ) -> Result<std::sync::Arc<SemanticTableStructure>, TableAuthoringError> {
        self.query_layout()?
            .shared_structure()
            .ok_or(TableAuthoringError::InvalidStructure)
    }
    fn entry_handles(
        &self,
        nodes: impl IntoIterator<Item = SemanticNodeId>,
    ) -> Result<Vec<TableEntry>, TableAuthoringError> {
        let store = self.family.integration_store();
        nodes
            .into_iter()
            .map(|node| CompositeEntryHandle::from_node(Rc::clone(store), node).map_err(Into::into))
            .collect()
    }
    fn displayed_nodes(&self) -> Result<Vec<Vec<SemanticNodeId>>, TableAuthoringError> {
        let grid = self.query_grid()?;
        let rows = (0..displayed_shape(&grid).0)
            .map(|row| displayed_row(&grid, row))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
    /// The displayed grid includes labels and its retained missing-corner object.
    pub fn get_rows(&self) -> Result<Vec<Vec<TableEntry>>, TableAuthoringError> {
        self.displayed_nodes()?
            .into_iter()
            .map(|row| self.entry_handles(unique_last(row)))
            .collect()
    }
    pub fn rows(&self) -> Result<Vec<Vec<TableEntry>>, TableAuthoringError> {
        self.get_rows()
    }
    pub fn get_columns(&self) -> Result<Vec<Vec<TableEntry>>, TableAuthoringError> {
        let rows = self.displayed_nodes()?;
        (0..rows[0].len())
            .map(|column| {
                self.entry_handles(unique_last(rows.iter().map(|row| row[column]).collect()))
            })
            .collect()
    }
    pub fn columns(&self) -> Result<Vec<Vec<TableEntry>>, TableAuthoringError> {
        self.get_columns()
    }
    /// Flat entries omit the unsupplied corner and preserve last-occurrence order.
    pub fn get_entries(&self) -> Result<Vec<TableEntry>, TableAuthoringError> {
        self.family_entries(&self.painted_entries)
    }
    pub fn entries(&self) -> Result<Vec<TableEntry>, TableAuthoringError> {
        self.get_entries()
    }
    pub fn get_entries_without_labels(&self) -> Result<Vec<TableEntry>, TableAuthoringError> {
        self.family_entries(&self.body_entries)
    }
    fn family_entries(
        &self,
        family: &MobjectFamily,
    ) -> Result<Vec<TableEntry>, TableAuthoringError> {
        family.validate()?;
        let nodes = family
            .integration_store()
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .map_err(AuthoringError::from)?;
        self.entry_handles(nodes)
    }
    pub fn get_row_labels(&self) -> Result<Vec<TableEntry>, TableAuthoringError> {
        self.entry_handles(unique_last(
            self.query_grid()?.row_labels().unwrap_or_default().to_vec(),
        ))
    }
    pub fn get_col_labels(&self) -> Result<Vec<TableEntry>, TableAuthoringError> {
        self.entry_handles(unique_last(
            self.query_grid()?
                .column_labels()
                .unwrap_or_default()
                .to_vec(),
        ))
    }
    pub fn get_labels(&self) -> Result<Vec<TableEntry>, TableAuthoringError> {
        let grid = self.query_grid()?;
        self.entry_handles(unique_last(
            grid.top_left_entry()
                .into_iter()
                .chain(grid.column_labels().unwrap_or_default().iter().copied())
                .chain(grid.row_labels().unwrap_or_default().iter().copied())
                .collect(),
        ))
    }
    /// Capture the displayed entry-indexing rule without enumerating its members.
    pub fn entries_indexing(&self) -> Result<TableEntrySelection, TableAuthoringError> {
        let grid = self.query_grid()?;
        Ok(TableEntrySelection {
            owner: self.family.clone(),
            family: self.painted_entries.clone(),
            stride: displayed_shape(&grid).1,
            missing: usize::from(grid.corner_placeholder().is_some()),
        })
    }
    pub fn body_entries_indexing(&self) -> Result<TableEntrySelection, TableAuthoringError> {
        Ok(TableEntrySelection {
            owner: self.family.clone(),
            family: self.body_entries.clone(),
            stride: self.query_grid()?.body_rows()[0].len(),
            missing: 0,
        })
    }
    /// Zero-based positions use Manim's exact flat-index arithmetic.
    pub fn entry_at_position(
        &self,
        row: impl Into<IntegerIndex>,
        column: impl Into<IntegerIndex>,
    ) -> Result<TableEntry, TableAuthoringError> {
        self.entries_indexing()?.at_position(row, column)
    }
    pub fn body_entry_at_position(
        &self,
        row: impl Into<IntegerIndex>,
        column: impl Into<IntegerIndex>,
    ) -> Result<TableEntry, TableAuthoringError> {
        self.body_entries_indexing()?.at_position(row, column)
    }
    pub fn get_entry(&self, row: usize, column: usize) -> Result<TableEntry, TableAuthoringError> {
        self.entry_at_position(row, column)
    }
    pub fn entry(&self, row: usize, column: usize) -> Result<TableEntry, TableAuthoringError> {
        self.get_entry(row, column)
    }
    pub fn row_families(&self) -> Result<Vec<MobjectFamily>, TableAuthoringError> {
        self.get_rows()?
            .iter()
            .map(|entries| admission::alias(self.family.integration_store(), entries))
            .collect()
    }
    pub fn column_families(&self) -> Result<Vec<MobjectFamily>, TableAuthoringError> {
        self.get_columns()?
            .iter()
            .map(|entries| admission::alias(self.family.integration_store(), entries))
            .collect()
    }
}

/// A request-local query rule over the existing canonical entry family.
///
/// Capturing the rule before evaluating a host argument preserves sequential
/// Python operator callbacks. Selection still reads one current Rust member;
/// this value owns neither a second membership list nor scene state.
#[derive(Clone, Debug)]
pub struct TableEntrySelection {
    owner: MobjectFamily,
    family: MobjectFamily,
    stride: usize,
    missing: usize,
}
impl TableEntrySelection {
    pub fn row_stride(&self) -> usize {
        self.stride
    }
    /// Subtraction applied to a one-based column in the host expression.
    pub fn column_offset(&self) -> usize {
        self.missing + 1
    }
    pub fn len(&self) -> Result<usize, TableAuthoringError> {
        self.owner.validate()?;
        self.family.validate()?;
        Ok(self
            .family
            .integration_store()
            .borrow()
            .node(self.family.node_id())
            .ok_or(TableAuthoringError::InvalidStructure)?
            .members()
            .len())
    }
    pub fn is_empty(&self) -> Result<bool, TableAuthoringError> {
        Ok(self.len()? == 0)
    }
    pub fn at_position(
        &self,
        row: impl Into<IntegerIndex>,
        column: impl Into<IntegerIndex>,
    ) -> Result<TableEntry, TableAuthoringError> {
        self.select_index(row.into() * self.stride + column.into() - self.missing)
    }
    pub fn select_index(
        &self,
        index: impl Into<IntegerIndex>,
    ) -> Result<TableEntry, TableAuthoringError> {
        let index = normalize_index(index.into(), self.len()?)
            .ok_or(TableAuthoringError::InvalidEntryIndex)?;
        let node = self
            .family
            .integration_store()
            .borrow()
            .semantic_family_member_at_checked(self.family.node_id(), index)
            .map_err(AuthoringError::from)?
            .ok_or(TableAuthoringError::InvalidStructure)?;
        Ok(CompositeEntryHandle::from_node(
            Rc::clone(self.family.integration_store()),
            node,
        )?)
    }
}

pub(super) fn unique_last<N: Copy + std::hash::Hash + Eq>(nodes: Vec<N>) -> Vec<N> {
    let mut seen = HashSet::new();
    let mut result: Vec<_> = nodes
        .into_iter()
        .rev()
        .filter(|node| seen.insert(*node))
        .collect();
    result.reverse();
    result
}
/// Read one logical row or column without traversing the full table family.
pub(super) fn displayed_shape<N: Copy>(grid: &SemanticTableStructure<N>) -> (usize, usize) {
    (
        grid.body_rows().len() + usize::from(grid.column_labels().is_some()),
        grid.body_rows()[0].len() + usize::from(grid.row_labels().is_some()),
    )
}

pub(super) fn displayed_row<N: Copy>(
    grid: &SemanticTableStructure<N>,
    row: usize,
) -> Result<Vec<N>, TableAuthoringError> {
    if row >= displayed_shape(grid).0 {
        return Err(TableAuthoringError::InvalidEntryIndex);
    }
    if let Some(labels) = grid.column_labels() {
        if row == 0 {
            let mut header = labels.to_vec();
            if grid.row_labels().is_some() {
                header.insert(
                    0,
                    grid.top_left_entry()
                        .or(grid.corner_placeholder())
                        .ok_or(TableAuthoringError::InvalidStructure)?,
                );
            }
            return Ok(header);
        }
    }
    let body_row = row - usize::from(grid.column_labels().is_some());
    let mut nodes = grid.body_rows()[body_row].clone();
    if let Some(labels) = grid.row_labels() {
        nodes.insert(0, labels[body_row]);
    }
    Ok(nodes)
}

pub(super) fn displayed_column<N: Copy>(
    grid: &SemanticTableStructure<N>,
    column: usize,
) -> Result<Vec<N>, TableAuthoringError> {
    if column >= displayed_shape(grid).1 {
        return Err(TableAuthoringError::InvalidEntryIndex);
    }
    let mut nodes = Vec::with_capacity(displayed_shape(grid).0);
    if let Some(labels) = grid.column_labels() {
        nodes.push(if column == 0 && grid.row_labels().is_some() {
            grid.top_left_entry()
                .or(grid.corner_placeholder())
                .ok_or(TableAuthoringError::InvalidStructure)?
        } else {
            labels[column - usize::from(grid.row_labels().is_some())]
        });
    }
    if column == 0 && grid.row_labels().is_some() {
        nodes.extend_from_slice(grid.row_labels().unwrap());
    } else {
        let body_column = column - usize::from(grid.row_labels().is_some());
        nodes.extend(grid.body_rows().iter().map(|row| row[body_column]));
    }
    Ok(nodes)
}
