//! Cell selection snapshots membership, then observes current effective bounds.
use super::*;
use crate::{integer_index::normalize_index, IntegerIndex};
use noon_core::{SemanticNodeId, SemanticTableStructure};
use std::sync::Arc;

/// Request-local rows captured before a host evaluates its row argument.
#[derive(Clone, Debug)]
pub struct TableCellRows {
    table: Table,
    grid: Arc<SemanticTableStructure>,
}
/// The selected row and columns captured before a host evaluates its column.
///
/// These are immutable identity observations. Geometry is read through the
/// normal authored/live publisher only when the cell is requested.
#[derive(Clone, Debug)]
pub struct TableCellColumns {
    table: Table,
    row: Vec<SemanticNodeId>,
    grid: Arc<SemanticTableStructure>,
}

impl Table {
    pub fn cell_rows(&self) -> Result<TableCellRows, TableAuthoringError> {
        Ok(TableCellRows {
            table: self.clone(),
            grid: self.query_grid()?,
        })
    }
    /// Zero-based signed row and column indices, with independent negative wrap.
    pub fn get_cell(
        &self,
        row: impl Into<IntegerIndex>,
        column: impl Into<IntegerIndex>,
    ) -> Result<Mobject, TableAuthoringError> {
        self.cell_rows()?.select_row(row)?.get_cell(column)
    }
    pub fn get_cell_in_live_session(
        &self,
        live: &mut crate::LiveSession<'_>,
        row: impl Into<IntegerIndex>,
        column: impl Into<IntegerIndex>,
    ) -> Result<Mobject, TableAuthoringError> {
        self.cell_rows()?
            .select_row(row)?
            .get_cell_in_live_session(live, column)
    }
    pub fn highlight_cell(
        &self,
        row: impl Into<IntegerIndex>,
        column: impl Into<IntegerIndex>,
        color: Color,
        opacity: f64,
    ) -> Result<Mobject, TableAuthoringError> {
        self.cell_rows()?
            .select_row(row)?
            .highlight_cell(column, color, opacity)
    }
    /// Return a detached colored cell rectangle.
    pub fn get_highlighted_cell(
        &self,
        row: impl Into<IntegerIndex>,
        column: impl Into<IntegerIndex>,
        color: Color,
        opacity: f64,
    ) -> Result<Mobject, TableAuthoringError> {
        self.cell_rows()?
            .select_row(row)?
            .get_highlighted_cell(column, color, opacity)
    }
    pub fn get_highlighted_cell_in_live_session(
        &self,
        live: &mut crate::LiveSession<'_>,
        row: impl Into<IntegerIndex>,
        column: impl Into<IntegerIndex>,
        color: Color,
        opacity: f64,
    ) -> Result<Mobject, TableAuthoringError> {
        self.cell_rows()?
            .select_row(row)?
            .get_highlighted_cell_in_live_session(live, column, color, opacity)
    }
    pub fn highlight_cell_in_live_session(
        &self,
        live: &mut crate::LiveSession<'_>,
        row: impl Into<IntegerIndex>,
        column: impl Into<IntegerIndex>,
        color: Color,
        opacity: f64,
    ) -> Result<Mobject, TableAuthoringError> {
        self.cell_rows()?
            .select_row(row)?
            .highlight_cell_in_live_session(live, column, color, opacity)
    }
}
impl TableCellRows {
    pub fn len(&self) -> usize {
        super::queries::displayed_shape(&self.grid).0
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn select_row(
        &self,
        row: impl Into<IntegerIndex>,
    ) -> Result<TableCellColumns, TableAuthoringError> {
        self.table.family.validate()?;
        let row = normalize_index(row.into(), self.len())
            .ok_or(TableAuthoringError::InvalidEntryIndex)?;
        Ok(TableCellColumns {
            row: super::queries::displayed_row(&self.grid, row)?,
            grid: self.table.query_grid()?,
            table: self.table.clone(),
        })
    }
}
impl TableCellColumns {
    pub fn len(&self) -> usize {
        super::queries::displayed_shape(&self.grid).1
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn column(&self, column: IntegerIndex) -> Result<Vec<SemanticNodeId>, TableAuthoringError> {
        self.table.family.validate()?;
        let column =
            normalize_index(column, self.len()).ok_or(TableAuthoringError::InvalidEntryIndex)?;
        super::queries::displayed_column(&self.grid, column)
    }
    pub fn get_cell(
        &self,
        column: impl Into<IntegerIndex>,
    ) -> Result<Mobject, TableAuthoringError> {
        let column = self.column(column.into())?;
        admission::cell(
            &self.table.family,
            self.table.options()?,
            &self.row,
            &column,
        )
    }
    pub fn get_cell_in_live_session(
        &self,
        live: &mut crate::LiveSession<'_>,
        column: impl Into<IntegerIndex>,
    ) -> Result<Mobject, TableAuthoringError> {
        let column = self.column(column.into())?;
        admission::cell_in_publisher(
            admission::TablePublisher::Live(live),
            &self.table.family,
            self.table.options()?,
            &self.row,
            &column,
        )
    }
    pub fn get_highlighted_cell(
        &self,
        column: impl Into<IntegerIndex>,
        color: Color,
        opacity: f64,
    ) -> Result<Mobject, TableAuthoringError> {
        let column = self.column(column.into())?;
        admission::highlighted_cell(
            &self.table.family,
            self.table.options()?,
            &self.row,
            &column,
            color,
            opacity,
        )
    }
    pub fn highlight_cell(
        &self,
        column: impl Into<IntegerIndex>,
        color: Color,
        opacity: f64,
    ) -> Result<Mobject, TableAuthoringError> {
        let column = self.column(column.into())?;
        admission::highlight(
            &self.table.family,
            &self.table.highlight_family,
            self.table.options()?,
            &self.row,
            &column,
            color,
            opacity,
        )
    }
    pub fn get_highlighted_cell_in_live_session(
        &self,
        live: &mut crate::LiveSession<'_>,
        column: impl Into<IntegerIndex>,
        color: Color,
        opacity: f64,
    ) -> Result<Mobject, TableAuthoringError> {
        let column = self.column(column.into())?;
        admission::highlighted_cell_in_publisher(
            admission::TablePublisher::Live(live),
            &self.table.family,
            self.table.options()?,
            &self.row,
            &column,
            color,
            opacity,
        )
    }
    pub fn highlight_cell_in_live_session(
        &self,
        live: &mut crate::LiveSession<'_>,
        column: impl Into<IntegerIndex>,
        color: Color,
        opacity: f64,
    ) -> Result<Mobject, TableAuthoringError> {
        let column = self.column(column.into())?;
        admission::highlight_in_publisher(
            admission::TablePublisher::Live(live),
            &self.table.family,
            &self.table.highlight_family,
            self.table.options()?,
            &self.row,
            &column,
            color,
            opacity,
        )
    }
}
