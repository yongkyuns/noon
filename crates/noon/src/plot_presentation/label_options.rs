use super::PlotPresentationError;

/// Presentation of native Text or DecimalNumber labels on a coordinate shaft.
/// Positions are prepared in world coordinates; subsequent family transforms
/// act on labels and shafts together without reformatting or reshaping text.
/// `font` selects the native Text face and is unused by real-TeX decimal labels.
#[derive(Clone, Debug)]
pub struct NumberLabelOptions {
    pub font: String,
    pub font_size: f32,
    pub decimal_places: u32,
    pub include_sign: bool,
    pub group_with_commas: bool,
    pub show_ellipsis: bool,
    pub unit: Option<String>,
    pub exclude_zero: bool,
    /// Exact numeric exclusions, checked before allocating text resources.
    pub excluded_numbers: Vec<f64>,
    pub include_tip: bool,
    pub exclude_origin_tick: bool,
    pub direction: [f64; 2],
    pub buff: f64,
    pub color: crate::Color,
}

impl NumberLabelOptions {
    pub(crate) fn decimal_format(&self) -> noon_core::DecimalFormat {
        noon_core::DecimalFormat {
            decimal_places: self.decimal_places,
            include_sign: self.include_sign,
            group_with_commas: self.group_with_commas,
            show_ellipsis: self.show_ellipsis,
            unit: self.unit.clone(),
        }
    }
}

impl Default for NumberLabelOptions {
    fn default() -> Self {
        Self {
            font: "DejaVu Sans Mono".into(),
            font_size: 18.0,
            decimal_places: 0,
            include_sign: false,
            group_with_commas: true,
            show_ellipsis: false,
            unit: None,
            exclude_zero: true,
            excluded_numbers: Vec::new(),
            include_tip: false,
            exclude_origin_tick: false,
            direction: [0.0, -1.0],
            buff: 0.12,
            color: crate::WHITE,
        }
    }
}

impl NumberLabelOptions {
    pub(crate) fn prepare_values(
        &self,
        frame: noon_geometry::NumberLineFrame,
        numbers: Option<&[f64]>,
    ) -> Result<Vec<super::NumberLabel>, PlotPresentationError> {
        let automatic = numbers
            .is_none()
            .then(|| {
                frame.tick_values(
                    self.include_tip,
                    self.exclude_origin_tick,
                    super::MAX_PLOT_LABELS,
                )
            })
            .transpose()?;
        super::number_labels_excluding(
            frame,
            numbers.or(automatic.as_deref()),
            self.decimal_places,
            self.exclude_zero,
            &self.excluded_numbers,
        )
    }

    pub fn set_excluded_numbers(&mut self, values: &[f64]) -> Result<(), PlotPresentationError> {
        super::preparation::validate_label_exclusions(values)?;
        let mut excluded = Vec::new();
        excluded
            .try_reserve_exact(values.len())
            .map_err(|_| PlotPresentationError::AllocationFailed)?;
        excluded.extend_from_slice(values);
        self.excluded_numbers = excluded;
        Ok(())
    }
}

#[derive(Debug)]
pub enum NumberLabelAuthoringError {
    Preparation(PlotPresentationError),
    Coordinate(crate::CoordinateAuthoringError),
    Text(crate::TextAuthoringError),
    #[cfg(feature = "latex")]
    Numeric(crate::NumericAuthoringError),
    Authoring(crate::AuthoringError),
    Import(noon_core::SemanticTextImportError),
    Allocation(std::collections::TryReserveError),
}

impl std::fmt::Display for NumberLabelAuthoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preparation(e) => e.fmt(f),
            Self::Coordinate(e) => e.fmt(f),
            Self::Text(e) => e.fmt(f),
            #[cfg(feature = "latex")]
            Self::Numeric(e) => e.fmt(f),
            Self::Authoring(e) => e.fmt(f),
            Self::Import(e) => e.fmt(f),
            Self::Allocation(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for NumberLabelAuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Preparation(e) => Some(e),
            Self::Coordinate(e) => Some(e),
            Self::Text(e) => Some(e),
            #[cfg(feature = "latex")]
            Self::Numeric(e) => Some(e),
            Self::Authoring(e) => Some(e),
            Self::Import(e) => Some(e),
            Self::Allocation(e) => Some(e),
        }
    }
}
macro_rules! label_error_from {
    ($source:ty, $variant:ident) => {
        impl From<$source> for NumberLabelAuthoringError {
            fn from(value: $source) -> Self {
                Self::$variant(value)
            }
        }
    };
}
label_error_from!(PlotPresentationError, Preparation);
label_error_from!(crate::CoordinateAuthoringError, Coordinate);
label_error_from!(crate::TextAuthoringError, Text);
#[cfg(feature = "latex")]
label_error_from!(crate::NumericAuthoringError, Numeric);
label_error_from!(crate::AuthoringError, Authoring);
label_error_from!(noon_core::SemanticTextImportError, Import);
label_error_from!(std::collections::TryReserveError, Allocation);
impl From<noon_core::SemanticMutationTransactionError> for NumberLabelAuthoringError {
    fn from(value: noon_core::SemanticMutationTransactionError) -> Self {
        Self::Authoring(value.into())
    }
}
