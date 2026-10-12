//! Persistent parameter interval on an ordinary plotted path.
//!
//! This is semantic query metadata only.  Renderers still receive the same
//! ordinary path resource as every other curve.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticFunctionPlotRole {
    range_bits: [u64; 2],
    axes_mapped: bool,
}

impl SemanticFunctionPlotRole {
    /// The parameter interval of a scalar plot sampled through an Axes frame.
    pub fn new(range: [f64; 2]) -> Self {
        Self {
            range_bits: range.map(|value| if value == 0.0 { 0 } else { value.to_bits() }),
            axes_mapped: true,
        }
    }

    /// A scalar plot in scene coordinates has a query interval, but is not
    /// admitted to the bounded Axes area/partition preparation profile.
    pub fn scene_coordinates(range: [f64; 2]) -> Self {
        Self {
            axes_mapped: false,
            ..Self::new(range)
        }
    }

    pub fn is_axes_mapped(self) -> bool {
        self.axes_mapped
    }

    pub fn range(self) -> [f64; 2] {
        self.range_bits.map(f64::from_bits)
    }

    pub fn is_valid(self) -> bool {
        let [start, end] = self.range();
        start.is_finite() && end.is_finite() && start < end
    }
}
