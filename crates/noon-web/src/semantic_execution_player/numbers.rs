//! Numeric resource edits use the existing live publication boundary.
use super::*;

impl SemanticExecutionPlayer {
    pub(crate) fn live_create_variable(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        label: String,
        value: f64,
        format: noon::DecimalFormat,
        font_size: f32,
    ) -> Result<noon::Variable, AuthoringFailure> {
        self.with_live_session(move |live| {
            live.create_variable(backend, label, value, format, font_size)
        })
    }

    pub(crate) fn live_create_decimal_number(
        &mut self,
        backend: &mut impl noon::LatexBackend,
        value: f64,
        format: noon::DecimalFormat,
        font_size: f32,
    ) -> Result<noon::DecimalNumber, AuthoringFailure> {
        self.with_live_session(|live| live.create_decimal_number(backend, value, format, font_size))
    }

    pub(crate) fn live_set_decimal_value(
        &mut self,
        number: &noon::DecimalNumber,
        backend: &mut impl noon::LatexBackend,
        value: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.set_decimal_value(number, backend, value))
    }

    pub(crate) fn live_increment_decimal_value(
        &mut self,
        number: &noon::DecimalNumber,
        backend: &mut impl noon::LatexBackend,
        delta: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.increment_decimal_value(number, backend, delta))
    }

    pub(crate) fn live_decimal_value(
        &mut self,
        number: &noon::DecimalNumber,
    ) -> Result<f64, AuthoringFailure> {
        self.with_live_session(|live| live.decimal_value(number))
    }

    pub(crate) fn live_decimal_font_size(
        &mut self,
        number: &noon::DecimalNumber,
    ) -> Result<f64, AuthoringFailure> {
        self.with_live_session(|live| live.decimal_font_size(number))
    }
}
