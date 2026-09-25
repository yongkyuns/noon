//! Table language adapters use the shared live-session publication boundary.
use super::SemanticExecutionPlayer;
use crate::authoring_error::AuthoringFailure;

impl SemanticExecutionPlayer {
    pub(crate) fn live_table_cell(
        &mut self,
        table: &noon::Table,
        row: usize,
        column: usize,
    ) -> Result<noon::Mobject, AuthoringFailure> {
        self.with_live_session(|live| Ok(table.get_cell_in_live_session(live, row, column)))?
            .map_err(AuthoringFailure::from)
    }
    pub(crate) fn live_highlight_table_cell(
        &mut self,
        table: &noon::Table,
        row: usize,
        column: usize,
        color: noon::Color,
        opacity: f64,
    ) -> Result<noon::Mobject, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(table.highlight_cell_in_live_session(live, row, column, color, opacity))
        })?
        .map_err(AuthoringFailure::from)
    }
    pub(crate) fn live_create_table(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        rows: Vec<Vec<String>>,
        options: noon::TableOptions,
    ) -> Result<noon::Table, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(noon::Table::from_rows_in_live_session(
                live, backend, rows, options,
            ))
        })?
        .map_err(AuthoringFailure::from)
    }
    pub(crate) fn live_create_math_table(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        rows: Vec<Vec<String>>,
        options: noon::TableOptions,
    ) -> Result<noon::Table, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(noon::Table::from_rows_in_live_session(
                live, backend, rows, options,
            ))
        })?
        .map_err(AuthoringFailure::from)
    }
    pub(crate) fn live_create_integer_table(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        rows: Vec<Vec<f64>>,
        options: noon::TableOptions,
    ) -> Result<noon::IntegerTable, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(noon::IntegerTable::from_rows_in_live_session(
                live, backend, rows, options,
            ))
        })?
        .map_err(AuthoringFailure::from)
    }
    pub(crate) fn live_create_decimal_table(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        rows: Vec<Vec<f64>>,
        options: noon::TableOptions,
    ) -> Result<noon::DecimalTable, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(noon::DecimalTable::from_rows_in_live_session(
                live,
                backend,
                rows,
                noon::DecimalFormat {
                    decimal_places: 1,
                    ..Default::default()
                },
                options,
            ))
        })?
        .map_err(AuthoringFailure::from)
    }
    pub(crate) fn live_create_mobject_table<'a>(
        &mut self,
        rows: Vec<Vec<noon::MobjectTarget<'a>>>,
        options: noon::TableOptions,
    ) -> Result<noon::MobjectTable, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(noon::MobjectTable::from_target_rows_in_live_session(
                live, rows, options,
            ))
        })?
        .map_err(AuthoringFailure::from)
    }
}
