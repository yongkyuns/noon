//! Persistent parameter interval on an ordinary plotted path.
//!
//! This is semantic query metadata only.  Renderers still receive the same
//! ordinary path resource as every other curve.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticFunctionPlotRole {
    range_bits: [u64; 2],
}

impl SemanticFunctionPlotRole {
    pub fn new(range: [f64; 2]) -> Self {
        Self {
            range_bits: range.map(|value| if value == 0.0 { 0 } else { value.to_bits() }),
        }
    }

    pub fn range(self) -> [f64; 2] {
        self.range_bits.map(f64::from_bits)
    }

    pub fn is_valid(self) -> bool {
        let [start, end] = self.range();
        start.is_finite() && end.is_finite() && start < end
    }
}
