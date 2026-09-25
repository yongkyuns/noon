//! Renderer-independent DecimalNumber formatting.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecimalFormat {
    pub decimal_places: u32,
    pub include_sign: bool,
    pub group_with_commas: bool,
    pub show_ellipsis: bool,
    pub unit: Option<String>,
}

impl Default for DecimalFormat {
    fn default() -> Self {
        Self {
            decimal_places: 2,
            include_sign: false,
            group_with_commas: true,
            show_ellipsis: false,
            unit: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NumericFormatError {
    NonFinite { value_bits: u64 },
    DecimalPlaces { requested: u32 },
}

impl std::fmt::Display for NumericFormatError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinite { .. } => formatter.write_str("DecimalNumber value must be finite"),
            Self::DecimalPlaces { requested } => write!(
                formatter,
                "decimal places {requested} exceeds the supported limit"
            ),
        }
    }
}
impl std::error::Error for NumericFormatError {}

pub fn format_decimal(value: f64, options: &DecimalFormat) -> Result<String, NumericFormatError> {
    if !value.is_finite() {
        return Err(NumericFormatError::NonFinite {
            value_bits: value.to_bits(),
        });
    }
    if options.decimal_places > 12 {
        return Err(NumericFormatError::DecimalPlaces {
            requested: options.decimal_places,
        });
    }
    let precision = options.decimal_places as usize;
    let mut text = format!("{value:.precision$}");
    if options.group_with_commas {
        let sign_len = usize::from(matches!(text.as_bytes().first(), Some(b'-' | b'+')));
        let decimal = text[sign_len..]
            .find('.')
            .map_or(text.len(), |index| sign_len + index);
        let mut grouped = String::with_capacity(text.len() + decimal / 3);
        grouped.push_str(&text[..sign_len]);
        for (index, digit) in text[sign_len..decimal].bytes().enumerate() {
            if index > 0 && (decimal - sign_len - index) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(char::from(digit));
        }
        grouped.push_str(&text[decimal..]);
        text = grouped;
    }
    if options.include_sign && !text.starts_with('-') {
        text.insert(0, '+');
    }
    // Manim uses NumPy's rounding only to decide whether a formatted negative
    // value loses its sign. Keep that decision independent of Rust's formatted
    // digits: `-0.005` at two places displays `0.01`, not `-0.01`.
    let rounded_for_sign = (value * 10_f64.powi(options.decimal_places as i32)).round_ties_even();
    if text.starts_with('-') && rounded_for_sign == 0.0 {
        text.remove(0);
        if options.include_sign {
            text.insert(0, '+');
        }
    }
    if options.show_ellipsis {
        text.push_str("...");
    }
    if let Some(unit) = &options.unit {
        text.push_str(unit);
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_manim_numeric_display_bits() {
        assert_eq!(
            format_decimal(12_345.6, &DecimalFormat::default()).unwrap(),
            "12,345.60"
        );
        let format = DecimalFormat {
            decimal_places: 0,
            include_sign: true,
            show_ellipsis: true,
            unit: Some("m".into()),
            ..Default::default()
        };
        assert_eq!(format_decimal(-0.004, &format).unwrap(), "+0...m");
    }

    #[test]
    fn keeps_manim_negative_zero_sign_rule_separate_from_formatted_digits() {
        assert_eq!(
            format_decimal(-0.005, &DecimalFormat::default()).unwrap(),
            "0.01"
        );
        assert_eq!(
            format_decimal(
                -0.005,
                &DecimalFormat {
                    include_sign: true,
                    ..Default::default()
                },
            )
            .unwrap(),
            "+0.01"
        );
    }
}
