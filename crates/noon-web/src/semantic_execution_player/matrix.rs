//! Matrix authoring through the current Scene.
use super::SemanticExecutionPlayer;
use crate::authoring_error::AuthoringFailure;

impl SemanticExecutionPlayer {
    pub(crate) fn live_create_matrix(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        rows: Vec<Vec<String>>,
        options: noon::MatrixOptions,
    ) -> Result<noon::Matrix, AuthoringFailure> {
        noon::Matrix::from_rows_with_options(self.source.scene_mut(), backend, rows, options)
            .map_err(AuthoringFailure::from)
    }
    pub(crate) fn live_create_integer_matrix(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        rows: Vec<Vec<f64>>,
        options: noon::MatrixOptions,
    ) -> Result<noon::IntegerMatrix, AuthoringFailure> {
        noon::IntegerMatrix::from_rows_with_options(self.source.scene_mut(), backend, rows, options)
            .map_err(AuthoringFailure::from)
    }
    pub(crate) fn live_create_decimal_matrix(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        rows: Vec<Vec<f64>>,
        options: noon::MatrixOptions,
    ) -> Result<noon::DecimalMatrix, AuthoringFailure> {
        noon::DecimalMatrix::from_rows_with_format_and_options(
            self.source.scene_mut(),
            backend,
            rows,
            noon::DecimalFormat {
                decimal_places: 1,
                ..Default::default()
            },
            options,
        )
        .map_err(AuthoringFailure::from)
    }
    pub(crate) fn live_create_mobject_matrix<'a>(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        rows: Vec<Vec<noon::MobjectTarget<'a>>>,
        options: noon::MatrixOptions,
    ) -> Result<noon::MobjectMatrix, AuthoringFailure> {
        noon::MobjectMatrix::from_target_rows_with_options(
            self.source.scene_mut(),
            backend,
            rows,
            options,
        )
        .map_err(AuthoringFailure::from)
    }
}
