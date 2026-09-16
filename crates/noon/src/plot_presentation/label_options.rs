use super::PlotPresentationError;

/// Presentation of ordinary native Text labels on a coordinate shaft.
/// Positions are prepared in world coordinates; subsequent family transforms
/// act on labels and shafts together without reformatting or reshaping text.
#[derive(Clone, Debug)]
pub struct NumberLabelOptions {
    pub font: String,
    pub font_size: f32,
    pub decimal_places: u32,
    pub exclude_zero: bool,
    pub direction: [f64; 2],
    pub buff: f64,
    pub color: crate::Color,
}

impl Default for NumberLabelOptions {
    fn default() -> Self {
        Self {
            font: "DejaVu Sans Mono".into(),
            font_size: 18.0,
            decimal_places: 0,
            exclude_zero: true,
            direction: [0.0, -1.0],
            buff: 0.12,
            color: crate::WHITE,
        }
    }
}

#[derive(Debug)]
pub enum NumberLabelAuthoringError {
    Preparation(PlotPresentationError),
    Coordinate(crate::CoordinateAuthoringError),
    Text(crate::TextAuthoringError),
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
label_error_from!(crate::AuthoringError, Authoring);
label_error_from!(noon_core::SemanticTextImportError, Import);
label_error_from!(std::collections::TryReserveError, Allocation);
impl From<noon_core::SemanticMutationTransactionError> for NumberLabelAuthoringError {
    fn from(value: noon_core::SemanticMutationTransactionError) -> Self {
        Self::Authoring(value.into())
    }
}
