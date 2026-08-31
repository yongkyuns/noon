use std::fmt;

use serde::{Deserialize, Serialize};

/// Current wire version for browser/native input delivery.
pub const NATIVE_INPUT_PROTOCOL_VERSION: u16 = 1;

/// Ordered language-neutral input updates delivered from a host to native execution.
///
/// `sampled` contains latest-value state and may be coalesced by producers before
/// delivery. `events` contains discrete occurrences and its vector order is part of
/// the contract. Batches themselves are ordered by `sequence` at the transport layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativeInputBatch {
    pub version: u16,
    pub sequence: u64,
    #[serde(default)]
    pub sampled: Vec<NativeInputSample>,
    #[serde(default)]
    pub events: Vec<NativeInputEvent>,
}

impl NativeInputBatch {
    pub fn new(sequence: u64) -> Self {
        Self {
            version: NATIVE_INPUT_PROTOCOL_VERSION,
            sequence,
            sampled: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), NativeInputContractError> {
        if self.version != NATIVE_INPUT_PROTOCOL_VERSION {
            return Err(NativeInputContractError::UnsupportedVersion {
                found: self.version,
                supported: NATIVE_INPUT_PROTOCOL_VERSION,
            });
        }

        for (index, sample) in self.sampled.iter().enumerate() {
            sample.validate(index)?;
        }
        for (index, event) in self.events.iter().enumerate() {
            event.validate(index)?;
        }
        Ok(())
    }
}

/// Latest-value input suitable for native reactive signals.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeInputSample {
    Pointer {
        pointer_id: i32,
        x: f64,
        y: f64,
        buttons: u16,
    },
    Key {
        /// Browser `KeyboardEvent.code`, chosen because it is layout-independent.
        code: String,
        down: bool,
    },
    Viewport {
        /// Canvas CSS-pixel extent. Zero is valid during transient layout collapse.
        width: f64,
        height: f64,
        device_pixel_ratio: f64,
    },
    ControlScalar {
        /// Stable authoring/runtime signal identity; interpretation is not host-owned.
        signal_id: u64,
        value: f64,
    },
}

impl NativeInputSample {
    fn validate(&self, index: usize) -> Result<(), NativeInputContractError> {
        match self {
            Self::Pointer { x, y, .. } => {
                finite("sampled.pointer.x", index, *x)?;
                finite("sampled.pointer.y", index, *y)
            }
            Self::Key { code, .. } => nonempty_code("sampled.key.code", index, code),
            Self::Viewport {
                width,
                height,
                device_pixel_ratio,
            } => {
                finite_nonnegative("sampled.viewport.width", index, *width)?;
                finite_nonnegative("sampled.viewport.height", index, *height)?;
                finite_positive(
                    "sampled.viewport.device_pixel_ratio",
                    index,
                    *device_pixel_ratio,
                )
            }
            Self::ControlScalar { value, .. } => finite("sampled.control_scalar.value", index, *value),
        }
    }
}

/// Discrete input occurrence. Unlike sampled state, entries must not be coalesced
/// across semantic event boundaries and retain their order within a batch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeInputEvent {
    PointerButton {
        pointer_id: i32,
        button: u16,
        phase: PointerButtonPhase,
        x: f64,
        y: f64,
    },
    Key {
        code: String,
        phase: KeyPhase,
        #[serde(default)]
        repeat: bool,
    },
    Wheel {
        delta_x: f64,
        delta_y: f64,
    },
}

impl NativeInputEvent {
    fn validate(&self, index: usize) -> Result<(), NativeInputContractError> {
        match self {
            Self::PointerButton { x, y, .. } => {
                finite("events.pointer_button.x", index, *x)?;
                finite("events.pointer_button.y", index, *y)
            }
            Self::Key { code, .. } => nonempty_code("events.key.code", index, code),
            Self::Wheel { delta_x, delta_y } => {
                finite("events.wheel.delta_x", index, *delta_x)?;
                finite("events.wheel.delta_y", index, *delta_y)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerButtonPhase {
    Down,
    Up,
    Click,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyPhase {
    Down,
    Up,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NativeInputContractError {
    UnsupportedVersion { found: u16, supported: u16 },
    NonFinite { field: &'static str, index: usize },
    Negative { field: &'static str, index: usize },
    NonPositive { field: &'static str, index: usize },
    EmptyCode { field: &'static str, index: usize },
}

impl fmt::Display for NativeInputContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion { found, supported } => write!(
                formatter,
                "unsupported native input protocol version {found}; supported version is {supported}"
            ),
            Self::NonFinite { field, index } => {
                write!(formatter, "native input {field} at index {index} must be finite")
            }
            Self::Negative { field, index } => write!(
                formatter,
                "native input {field} at index {index} must be non-negative"
            ),
            Self::NonPositive { field, index } => write!(
                formatter,
                "native input {field} at index {index} must be positive"
            ),
            Self::EmptyCode { field, index } => write!(
                formatter,
                "native input {field} at index {index} must not be empty"
            ),
        }
    }
}

impl std::error::Error for NativeInputContractError {}

fn finite(field: &'static str, index: usize, value: f64) -> Result<(), NativeInputContractError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(NativeInputContractError::NonFinite { field, index })
    }
}

fn finite_nonnegative(
    field: &'static str,
    index: usize,
    value: f64,
) -> Result<(), NativeInputContractError> {
    finite(field, index, value)?;
    if value >= 0.0 {
        Ok(())
    } else {
        Err(NativeInputContractError::Negative { field, index })
    }
}

fn finite_positive(
    field: &'static str,
    index: usize,
    value: f64,
) -> Result<(), NativeInputContractError> {
    finite(field, index, value)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(NativeInputContractError::NonPositive { field, index })
    }
}

fn nonempty_code(
    field: &'static str,
    index: usize,
    code: &str,
) -> Result<(), NativeInputContractError> {
    if code.is_empty() {
        Err(NativeInputContractError::EmptyCode { field, index })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_sampled_state_and_discrete_event_order() {
        let batch = NativeInputBatch {
            version: NATIVE_INPUT_PROTOCOL_VERSION,
            sequence: 42,
            sampled: vec![
                NativeInputSample::Pointer {
                    pointer_id: 7,
                    x: 12.5,
                    y: -3.0,
                    buttons: 1,
                },
                NativeInputSample::Key {
                    code: "KeyA".to_owned(),
                    down: true,
                },
                NativeInputSample::Viewport {
                    width: 640.0,
                    height: 360.0,
                    device_pixel_ratio: 2.0,
                },
                NativeInputSample::ControlScalar {
                    signal_id: 9,
                    value: 0.25,
                },
            ],
            events: vec![
                NativeInputEvent::PointerButton {
                    pointer_id: 7,
                    button: 0,
                    phase: PointerButtonPhase::Down,
                    x: 12.5,
                    y: -3.0,
                },
                NativeInputEvent::Key {
                    code: "Space".to_owned(),
                    phase: KeyPhase::Down,
                    repeat: false,
                },
                NativeInputEvent::Wheel {
                    delta_x: 1.5,
                    delta_y: -2.0,
                },
            ],
        };
        batch.validate().unwrap();

        let json = serde_json::to_string(&batch).unwrap();
        let restored: NativeInputBatch = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, batch);
        assert!(json.contains("\"kind\":\"pointer\""));
        assert!(json.contains("\"kind\":\"pointer_button\""));
    }

    #[test]
    fn future_protocol_versions_fail_explicitly() {
        let batch = NativeInputBatch {
            version: NATIVE_INPUT_PROTOCOL_VERSION + 1,
            ..NativeInputBatch::new(0)
        };
        assert_eq!(
            batch.validate().unwrap_err(),
            NativeInputContractError::UnsupportedVersion {
                found: NATIVE_INPUT_PROTOCOL_VERSION + 1,
                supported: NATIVE_INPUT_PROTOCOL_VERSION,
            }
        );
    }

    #[test]
    fn transient_zero_viewport_is_valid_but_invalid_numerics_are_rejected() {
        let mut batch = NativeInputBatch::new(3);
        batch.sampled.push(NativeInputSample::Viewport {
            width: 0.0,
            height: 0.0,
            device_pixel_ratio: 1.0,
        });
        batch.validate().unwrap();

        batch.sampled[0] = NativeInputSample::Viewport {
            width: -1.0,
            height: 0.0,
            device_pixel_ratio: 1.0,
        };
        assert!(matches!(
            batch.validate().unwrap_err(),
            NativeInputContractError::Negative { .. }
        ));

        batch.sampled[0] = NativeInputSample::Viewport {
            width: 0.0,
            height: 0.0,
            device_pixel_ratio: 0.0,
        };
        assert!(matches!(
            batch.validate().unwrap_err(),
            NativeInputContractError::NonPositive { .. }
        ));

        batch.sampled[0] = NativeInputSample::Pointer {
            pointer_id: 1,
            x: f64::NAN,
            y: 0.0,
            buttons: 0,
        };
        assert!(matches!(
            batch.validate().unwrap_err(),
            NativeInputContractError::NonFinite { .. }
        ));
    }

    #[test]
    fn keyboard_codes_and_discrete_numeric_payloads_are_validated() {
        let mut batch = NativeInputBatch::new(4);
        batch.sampled.push(NativeInputSample::Key {
            code: String::new(),
            down: true,
        });
        assert!(matches!(
            batch.validate().unwrap_err(),
            NativeInputContractError::EmptyCode { .. }
        ));

        batch.sampled.clear();
        batch.events.push(NativeInputEvent::Wheel {
            delta_x: 0.0,
            delta_y: f64::INFINITY,
        });
        assert!(matches!(
            batch.validate().unwrap_err(),
            NativeInputContractError::NonFinite { .. }
        ));
    }
}
