//! Logical table observations share the retained semantic declaration.
use super::*;
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
        let grid = self.query_grid()?;
        let placeholder = grid.corner_placeholder();
        self.entry_handles(unique_last(
            self.displayed_nodes()?
                .into_iter()
                .flatten()
                .filter(|node| Some(*node) != placeholder)
                .collect(),
        ))
    }
    pub fn entries(&self) -> Result<Vec<TableEntry>, TableAuthoringError> {
        self.get_entries()
    }
    pub fn get_entries_without_labels(&self) -> Result<Vec<TableEntry>, TableAuthoringError> {
        let grid = self.query_grid()?;
        self.entry_handles(unique_last(
            grid.body_rows().iter().flatten().copied().collect(),
        ))
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
    /// Zero-based positions use Manim's flat-index selection, including negatives.
    pub fn entry_at_position(
        &self,
        row: i128,
        column: i128,
    ) -> Result<TableEntry, TableAuthoringError> {
        let grid = self.query_grid()?;
        let width = grid.body_rows()[0].len() + usize::from(grid.row_labels().is_some());
        let missing = i128::from(grid.corner_placeholder().is_some());
        select_position(self.get_entries()?, row, column, width, missing)
    }
    pub fn body_entry_at_position(
        &self,
        row: i128,
        column: i128,
    ) -> Result<TableEntry, TableAuthoringError> {
        let width = self.query_grid()?.body_rows()[0].len();
        select_position(self.get_entries_without_labels()?, row, column, width, 0)
    }
    pub fn get_entry(&self, row: usize, column: usize) -> Result<TableEntry, TableAuthoringError> {
        self.entry_at_position(row as i128, column as i128)
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

fn unique_last(nodes: Vec<SemanticNodeId>) -> Vec<SemanticNodeId> {
    let mut seen = HashSet::new();
    let mut result: Vec<_> = nodes
        .into_iter()
        .rev()
        .filter(|node| seen.insert(*node))
        .collect();
    result.reverse();
    result
}
fn select_position(
    entries: Vec<TableEntry>,
    row: i128,
    column: i128,
    width: usize,
    missing: i128,
) -> Result<TableEntry, TableAuthoringError> {
    let index = row
        .checked_mul(width as i128)
        .and_then(|index| index.checked_add(column))
        .and_then(|index| index.checked_sub(missing))
        .ok_or(TableAuthoringError::InvalidEntryIndex)?;
    let length = entries.len() as i128;
    let index = if index < 0 {
        index
            .checked_add(length)
            .ok_or(TableAuthoringError::InvalidEntryIndex)?
    } else {
        index
    };
    if !(0..length).contains(&index) {
        return Err(TableAuthoringError::InvalidEntryIndex);
    }
    Ok(entries[index as usize].clone())
}

/// Read one logical row or column without traversing the full table family.
pub(super) fn displayed_shape(grid: &SemanticTableStructure) -> (usize, usize) {
    (
        grid.body_rows().len() + usize::from(grid.column_labels().is_some()),
        grid.body_rows()[0].len() + usize::from(grid.row_labels().is_some()),
    )
}

pub(super) fn displayed_row(
    grid: &SemanticTableStructure,
    row: usize,
) -> Result<Vec<SemanticNodeId>, TableAuthoringError> {
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

pub(super) fn displayed_column(
    grid: &SemanticTableStructure,
    column: usize,
) -> Result<Vec<SemanticNodeId>, TableAuthoringError> {
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
