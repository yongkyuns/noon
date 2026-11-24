use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{ValueKind, Vec2};

use super::ReactiveValue;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SampleKind {
    Scalar,
    Vec2,
}

/// A retained set of scalar or planar vector keyframes, ordered by input value.
/// Values clamp to the endpoint samples outside the authored input range.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SampleTable")]
pub struct SignalSamples {
    kind: SampleKind,
    linear: bool,
    data: Arc<[f32]>,
}

#[derive(Deserialize)]
struct SampleTable {
    kind: SampleKind,
    linear: bool,
    data: Vec<f32>,
}

impl TryFrom<SampleTable> for SignalSamples {
    type Error = &'static str;

    fn try_from(table: SampleTable) -> Result<Self, Self::Error> {
        Self::new(table.kind.value_kind(), table.linear, table.data)
            .ok_or("sample rows must be finite and strictly increasing")
    }
}

impl SampleKind {
    const fn value_kind(self) -> ValueKind {
        match self {
            Self::Scalar => ValueKind::Scalar,
            Self::Vec2 => ValueKind::Vec2,
        }
    }

    const fn stride(self) -> usize {
        match self {
            Self::Scalar => 2,
            Self::Vec2 => 3,
        }
    }
}

impl SignalSamples {
    pub const MAX_KEYS: usize = 32_768;

    /// Scalar rows are [input, value], and vector rows are [input, x, y].
    pub fn new(kind: ValueKind, linear: bool, data: Vec<f32>) -> Option<Self> {
        let kind = match kind {
            ValueKind::Scalar => SampleKind::Scalar,
            ValueKind::Vec2 => SampleKind::Vec2,
            _ => return None,
        };
        let stride = kind.stride();
        let keys = data.len() / stride;
        if keys == 0 || keys > Self::MAX_KEYS || !data.len().is_multiple_of(stride) {
            return None;
        }
        let mut previous = f32::NEG_INFINITY;
        for key in data.chunks_exact(stride) {
            if key[0] <= previous || key.iter().any(|value| !value.is_finite()) {
                return None;
            }
            previous = key[0];
        }
        Some(Self {
            kind,
            linear,
            data: data.into(),
        })
    }

    pub const fn value_kind(&self) -> ValueKind {
        self.kind.value_kind()
    }

    /// Evaluate the retained table at a finite input value.
    pub fn sample(&self, input: f32) -> ReactiveValue {
        let stride = self.kind.stride();
        let key_count = self.data.len() / stride;
        // Find the first key strictly past input. A held sample changes at
        // the exact key, including when seeking directly onto the boundary.
        let (mut low, mut high) = (0, key_count);
        while low < high {
            let mid = low + (high - low) / 2;
            if self.data[mid * stride] <= input {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let left = low.saturating_sub(1) * stride;
        let right = low.min(key_count - 1) * stride;
        let alpha = if !self.linear || left == right {
            0.0
        } else {
            // Use wider intermediates for the full finite f32 input range.
            let start = f64::from(self.data[left]);
            (f64::from(input) - start) / (f64::from(self.data[right]) - start)
        };
        let value = |offset| {
            let from = f64::from(self.data[left + offset]);
            let to = f64::from(self.data[right + offset]);
            (from + (to - from) * alpha) as f32
        };
        match self.kind {
            SampleKind::Scalar => ReactiveValue::Scalar(value(1)),
            SampleKind::Vec2 => ReactiveValue::Vec2(Vec2::new(value(1), value(2))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampled_channels_hold_and_interpolate_uneven_keys() {
        let data = vec![1.0, 0.0, 1.0, 2.0, 2.0, 5.0, 5.0, 8.0, 11.0];
        let linear = SignalSamples::new(ValueKind::Vec2, true, data.clone()).unwrap();
        let held = SignalSamples::new(ValueKind::Vec2, false, data).unwrap();
        let point = |x, y| ReactiveValue::Vec2(Vec2::new(x, y));
        assert_eq!(linear.sample(0.0), point(0.0, 1.0));
        assert_eq!(linear.sample(3.5), point(5.0, 8.0));
        assert_eq!(linear.sample(9.0), point(8.0, 11.0));
        assert_eq!(held.sample(3.5), point(2.0, 5.0));
        assert_eq!(held.sample(5.0), point(8.0, 11.0));
        assert!(SignalSamples::new(ValueKind::Scalar, false, vec![1.0, 0.0, 1.0, 3.0]).is_none());
        assert!(SignalSamples::new(ValueKind::Scalar, false, vec![0.0, f32::INFINITY]).is_none());
    }
}
