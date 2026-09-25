//! Matrix language adapters use the common player live-session boundary.
use super::SemanticExecutionPlayer;
use crate::authoring_error::AuthoringFailure;

impl SemanticExecutionPlayer {
    pub(crate) fn live_create_matrix(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        rows: Vec<Vec<String>>,
        options: noon::MatrixOptions,
    ) -> Result<noon::Matrix, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(noon::Matrix::from_rows_in_live_session(
                live, backend, rows, options,
            ))
        })?
        .map_err(AuthoringFailure::from)
    }
    pub(crate) fn live_create_integer_matrix(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        rows: Vec<Vec<f64>>,
        options: noon::MatrixOptions,
    ) -> Result<noon::IntegerMatrix, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(noon::IntegerMatrix::from_rows_in_live_session(
                live, backend, rows, options,
            ))
        })?
        .map_err(AuthoringFailure::from)
    }
    pub(crate) fn live_create_decimal_matrix(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        rows: Vec<Vec<f64>>,
        options: noon::MatrixOptions,
    ) -> Result<noon::DecimalMatrix, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(noon::DecimalMatrix::from_rows_in_live_session(
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
    pub(crate) fn live_create_mobject_matrix(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        rows: Vec<Vec<noon::Mobject>>,
        options: noon::MatrixOptions,
    ) -> Result<noon::MobjectMatrix, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(noon::MobjectMatrix::from_rows_in_live_session(
                live, backend, rows, options,
            ))
        })?
        .map_err(AuthoringFailure::from)
    }
}
