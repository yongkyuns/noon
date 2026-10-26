//! Authored Code color choices, retained independently of a language wrapper.

use crate::Color;

/// Optional color overrides for broad source-syntax categories.
///
/// `foreground` changes the theme's unclassified source fallback. Explicit
/// theme rules retain their colors unless their category is also overridden.
/// Comments and strings keep lexical precedence over exact token overrides.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CodeSyntaxColors {
    pub foreground: Option<Color>,
    pub comment: Option<Color>,
    /// Keywords, storage/type names and language constants, excluding operators.
    pub keyword: Option<Color>,
    pub number: Option<Color>,
    pub string: Option<Color>,
    pub operator: Option<Color>,
}

impl CodeSyntaxColors {
    pub fn is_valid(&self) -> bool {
        [
            self.foreground,
            self.comment,
            self.keyword,
            self.number,
            self.string,
            self.operator,
        ]
        .into_iter()
        .flatten()
        .all(is_valid_code_color)
    }
}

/// Code color values follow ordinary finite normalized RGBA channels.
pub fn is_valid_code_color(color: Color) -> bool {
    [color.red, color.green, color.blue, color.alpha]
        .into_iter()
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
}

/// Whether an exact token selector is an identifier or a dotted identifier path.
///
/// Each component follows Unicode XID rules, with `_` also permitted as a start.
/// Whitespace, empty components, numeric starts and arbitrary expressions are
/// rejected. Matching never searches inside a longer identifier.
pub fn is_valid_code_token_selector(selector: &str) -> bool {
    selector.split('.').all(|component| {
        let mut characters = component.chars();
        characters
            .next()
            .is_some_and(|first| first == '_' || unicode_ident::is_xid_start(first))
            && characters.all(unicode_ident::is_xid_continue)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_token_selectors_have_unicode_identifier_boundaries() {
        for valid in ["in", "threadIdx.x", "_", "café", "cafe\u{301}.値", "a.b.c"] {
            assert!(is_valid_code_token_selector(valid), "{valid}");
        }
        for invalid in [
            "", " ", "0", "1x", ".x", "x.", "a..b", "a b", "a[0]", "a.*", "\u{301}a",
        ] {
            assert!(!is_valid_code_token_selector(invalid), "{invalid}");
        }
    }

    #[test]
    fn optional_code_colors_require_finite_normalized_channels() {
        assert!(CodeSyntaxColors::default().is_valid());
        assert!(CodeSyntaxColors {
            foreground: Some(Color::TRANSPARENT),
            comment: Some(Color::rgba(0.12345, 0.5, 1.0, 0.75)),
            ..CodeSyntaxColors::default()
        }
        .is_valid());
        for channel in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 1.01] {
            assert!(!CodeSyntaxColors {
                number: Some(Color::rgba(0.0, channel, 0.0, 1.0)),
                ..CodeSyntaxColors::default()
            }
            .is_valid());
        }
    }
}
