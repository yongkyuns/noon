//! Pango-style markup normalization for native retained text.
//!
//! This module owns only author-time normalization: markup is decoded into plain
//! UTF-8 source plus source-byte style spans. It deliberately does not shape text,
//! mutate semantic scene state, or apply spans to retained resources. Those layers
//! can consume this representation without introducing frontend or renderer markup
//! semantics.

use std::{fmt, sync::Arc};

use noon_core::TextSourceSpan;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkupTextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub monospace: bool,
    /// Relative Pango convenience-tag size steps. `<big>` increments and `<small>`
    /// decrements this value; nested tags compose additively.
    pub size_steps: i16,
    /// Relative baseline steps. `<sup>` increments and `<sub>` decrements this value.
    pub baseline_steps: i16,
    /// Color spelling is preserved for the authoring layer to resolve through its
    /// canonical Manim/Pango color parser.
    pub foreground: Option<Arc<str>>,
    pub font_family: Option<Arc<str>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkupStyleSpan {
    pub source_span: TextSourceSpan,
    pub style: MarkupTextStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedMarkupText {
    pub source: Arc<str>,
    pub spans: Arc<[MarkupStyleSpan]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkupTextError {
    UnterminatedTag { offset: usize },
    MalformedTag { offset: usize },
    UnsupportedTag { tag: String, offset: usize },
    UnexpectedClosingTag { tag: String, offset: usize },
    MismatchedClosingTag {
        expected: String,
        found: String,
        offset: usize,
    },
    UnclosedTag { tag: String, offset: usize },
    UnsupportedAttribute {
        tag: String,
        attribute: String,
        offset: usize,
    },
    InvalidAttribute {
        tag: String,
        attribute: String,
        value: String,
        offset: usize,
    },
    UnterminatedEntity { offset: usize },
    InvalidEntity { entity: String, offset: usize },
    SourceTooLarge,
}

impl fmt::Display for MarkupTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnterminatedTag { offset } => write!(f, "unterminated markup tag at byte {offset}"),
            Self::MalformedTag { offset } => write!(f, "malformed markup tag at byte {offset}"),
            Self::UnsupportedTag { tag, offset } => {
                write!(f, "unsupported markup tag <{tag}> at byte {offset}")
            }
            Self::UnexpectedClosingTag { tag, offset } => {
                write!(f, "unexpected closing markup tag </{tag}> at byte {offset}")
            }
            Self::MismatchedClosingTag {
                expected,
                found,
                offset,
            } => write!(
                f,
                "mismatched closing markup tag </{found}> at byte {offset}; expected </{expected}>"
            ),
            Self::UnclosedTag { tag, offset } => {
                write!(f, "unclosed markup tag <{tag}> opened at byte {offset}")
            }
            Self::UnsupportedAttribute {
                tag,
                attribute,
                offset,
            } => write!(
                f,
                "unsupported attribute {attribute:?} on <{tag}> at byte {offset}"
            ),
            Self::InvalidAttribute {
                tag,
                attribute,
                value,
                offset,
            } => write!(
                f,
                "invalid value {value:?} for attribute {attribute:?} on <{tag}> at byte {offset}"
            ),
            Self::UnterminatedEntity { offset } => {
                write!(f, "unterminated markup entity at byte {offset}")
            }
            Self::InvalidEntity { entity, offset } => {
                write!(f, "invalid markup entity &{entity}; at byte {offset}")
            }
            Self::SourceTooLarge => write!(f, "decoded markup source exceeds Noon's text span space"),
        }
    }
}

impl std::error::Error for MarkupTextError {}

#[derive(Clone, Debug)]
struct OpenTag {
    name: String,
    offset: usize,
    previous_style: MarkupTextStyle,
}

/// Normalize a bounded, deterministic subset of Manim/Pango markup.
///
/// Supported convenience tags are `b`, `i`, `u`, `s`, `tt`, `big`, `small`,
/// `sup`, and `sub`. `<span>` currently accepts `foreground`/`fgcolor` and
/// `font_family`. Unsupported tags and attributes are rejected rather than ignored.
/// XML named entities and numeric decimal/hex entities are decoded into the returned
/// plain UTF-8 source.
pub fn normalize_markup_text(markup: &str) -> Result<NormalizedMarkupText, MarkupTextError> {
    let mut source = String::with_capacity(markup.len());
    let mut spans = Vec::<MarkupStyleSpan>::new();
    let mut style = MarkupTextStyle::default();
    let mut stack = Vec::<OpenTag>::new();
    let mut index = 0_usize;

    while index < markup.len() {
        let byte = markup.as_bytes()[index];
        if byte == b'<' {
            let end = find_tag_end(markup, index)?;
            let raw = &markup[index + 1..end];
            parse_tag(raw, index, &mut style, &mut stack)?;
            index = end + 1;
            continue;
        }
        if byte == b'&' {
            let end = markup[index + 1..]
                .find(';')
                .map(|relative| index + 1 + relative)
                .ok_or(MarkupTextError::UnterminatedEntity { offset: index })?;
            let entity = &markup[index + 1..end];
            let decoded = decode_entity(entity)
                .ok_or_else(|| MarkupTextError::InvalidEntity {
                    entity: entity.to_owned(),
                    offset: index,
                })?;
            let start = source.len();
            source.push(decoded);
            push_span(&mut spans, start, source.len(), &style)?;
            index = end + 1;
            continue;
        }

        let next = markup[index..]
            .find(['<', '&'])
            .map(|relative| index + relative)
            .unwrap_or(markup.len());
        let start = source.len();
        source.push_str(&markup[index..next]);
        push_span(&mut spans, start, source.len(), &style)?;
        index = next;
    }

    if let Some(open) = stack.last() {
        return Err(MarkupTextError::UnclosedTag {
            tag: open.name.clone(),
            offset: open.offset,
        });
    }

    Ok(NormalizedMarkupText {
        source: Arc::from(source),
        spans: spans.into(),
    })
}

fn find_tag_end(markup: &str, start: usize) -> Result<usize, MarkupTextError> {
    let bytes = markup.as_bytes();
    let mut quote = None::<u8>;
    let mut index = start + 1;
    while index < bytes.len() {
        match (quote, bytes[index]) {
            (Some(expected), actual) if actual == expected => quote = None,
            (Some(_), _) => {}
            (None, b'\'' | b'"') => quote = Some(bytes[index]),
            (None, b'>') => return Ok(index),
            _ => {}
        }
        index += 1;
    }
    Err(MarkupTextError::UnterminatedTag { offset: start })
}

fn parse_tag(
    raw: &str,
    offset: usize,
    style: &mut MarkupTextStyle,
    stack: &mut Vec<OpenTag>,
) -> Result<(), MarkupTextError> {
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with('!') || raw.starts_with('?') {
        return Err(MarkupTextError::MalformedTag { offset });
    }
    if let Some(rest) = raw.strip_prefix('/') {
        let name = rest.trim();
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            return Err(MarkupTextError::MalformedTag { offset });
        }
        let Some(open) = stack.pop() else {
            return Err(MarkupTextError::UnexpectedClosingTag {
                tag: name.to_owned(),
                offset,
            });
        };
        if open.name != name {
            let expected = open.name;
            stack.push(open);
            return Err(MarkupTextError::MismatchedClosingTag {
                expected,
                found: name.to_owned(),
                offset,
            });
        }
        *style = open.previous_style;
        return Ok(());
    }
    if raw.ends_with('/') {
        return Err(MarkupTextError::MalformedTag { offset });
    }

    let name_end = raw
        .find(char::is_whitespace)
        .unwrap_or(raw.len());
    let name = &raw[..name_end];
    if !is_supported_tag(name) {
        return Err(MarkupTextError::UnsupportedTag {
            tag: name.to_owned(),
            offset,
        });
    }

    let attributes = parse_attributes(&raw[name_end..], name, offset)?;
    let previous_style = style.clone();
    apply_tag(name, &attributes, style, offset)?;
    stack.push(OpenTag {
        name: name.to_owned(),
        offset,
        previous_style,
    });
    Ok(())
}

fn is_supported_tag(name: &str) -> bool {
    matches!(
        name,
        "b" | "i" | "u" | "s" | "tt" | "big" | "small" | "sup" | "sub" | "span"
    )
}

fn parse_attributes(
    input: &str,
    tag: &str,
    offset: usize,
) -> Result<Vec<(String, String)>, MarkupTextError> {
    let mut attributes = Vec::new();
    let bytes = input.as_bytes();
    let mut index = 0_usize;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index == bytes.len() {
            break;
        }
        let name_start = index;
        while index < bytes.len()
            && !bytes[index].is_ascii_whitespace()
            && bytes[index] != b'='
        {
            index += 1;
        }
        if name_start == index {
            return Err(MarkupTextError::MalformedTag { offset });
        }
        let name = &input[name_start..index];
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if bytes.get(index) != Some(&b'=') {
            return Err(MarkupTextError::MalformedTag { offset });
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let Some(&quote @ (b'\'' | b'"')) = bytes.get(index) else {
            return Err(MarkupTextError::MalformedTag { offset });
        };
        index += 1;
        let value_start = index;
        while index < bytes.len() && bytes[index] != quote {
            index += 1;
        }
        if index == bytes.len() {
            return Err(MarkupTextError::MalformedTag { offset });
        }
        let raw_value = &input[value_start..index];
        let value = decode_attribute_value(raw_value, offset)?;
        index += 1;
        if attributes.iter().any(|(existing, _)| existing == name) {
            return Err(MarkupTextError::InvalidAttribute {
                tag: tag.to_owned(),
                attribute: name.to_owned(),
                value,
                offset,
            });
        }
        attributes.push((name.to_owned(), value));
    }
    Ok(attributes)
}

fn decode_attribute_value(value: &str, offset: usize) -> Result<String, MarkupTextError> {
    let mut decoded = String::with_capacity(value.len());
    let mut index = 0_usize;
    while index < value.len() {
        if value.as_bytes()[index] != b'&' {
            let next = value[index..]
                .find('&')
                .map(|relative| index + relative)
                .unwrap_or(value.len());
            decoded.push_str(&value[index..next]);
            index = next;
            continue;
        }
        let end = value[index + 1..]
            .find(';')
            .map(|relative| index + 1 + relative)
            .ok_or(MarkupTextError::UnterminatedEntity { offset })?;
        let entity = &value[index + 1..end];
        decoded.push(decode_entity(entity).ok_or_else(|| MarkupTextError::InvalidEntity {
            entity: entity.to_owned(),
            offset,
        })?);
        index = end + 1;
    }
    Ok(decoded)
}

fn apply_tag(
    tag: &str,
    attributes: &[(String, String)],
    style: &mut MarkupTextStyle,
    offset: usize,
) -> Result<(), MarkupTextError> {
    if tag != "span" && !attributes.is_empty() {
        return Err(MarkupTextError::UnsupportedAttribute {
            tag: tag.to_owned(),
            attribute: attributes[0].0.clone(),
            offset,
        });
    }
    match tag {
        "b" => style.bold = true,
        "i" => style.italic = true,
        "u" => style.underline = true,
        "s" => style.strikethrough = true,
        "tt" => style.monospace = true,
        "big" => style.size_steps = style.size_steps.saturating_add(1),
        "small" => style.size_steps = style.size_steps.saturating_sub(1),
        "sup" => style.baseline_steps = style.baseline_steps.saturating_add(1),
        "sub" => style.baseline_steps = style.baseline_steps.saturating_sub(1),
        "span" => {
            for (name, value) in attributes {
                match name.as_str() {
                    "foreground" | "fgcolor" => {
                        if value.is_empty() {
                            return Err(MarkupTextError::InvalidAttribute {
                                tag: tag.to_owned(),
                                attribute: name.clone(),
                                value: value.clone(),
                                offset,
                            });
                        }
                        style.foreground = Some(Arc::from(value.as_str()));
                    }
                    "font_family" => {
                        if value.is_empty() {
                            return Err(MarkupTextError::InvalidAttribute {
                                tag: tag.to_owned(),
                                attribute: name.clone(),
                                value: value.clone(),
                                offset,
                            });
                        }
                        style.font_family = Some(Arc::from(value.as_str()));
                    }
                    _ => {
                        return Err(MarkupTextError::UnsupportedAttribute {
                            tag: tag.to_owned(),
                            attribute: name.clone(),
                            offset,
                        })
                    }
                }
            }
        }
        _ => unreachable!("tag support checked before apply_tag"),
    }
    Ok(())
}

fn push_span(
    spans: &mut Vec<MarkupStyleSpan>,
    start: usize,
    end: usize,
    style: &MarkupTextStyle,
) -> Result<(), MarkupTextError> {
    if start == end {
        return Ok(());
    }
    let start = u32::try_from(start).map_err(|_| MarkupTextError::SourceTooLarge)?;
    let end = u32::try_from(end).map_err(|_| MarkupTextError::SourceTooLarge)?;
    if let Some(last) = spans.last_mut() {
        if last.style == *style && last.source_span.end == start {
            last.source_span.end = end;
            return Ok(());
        }
    }
    spans.push(MarkupStyleSpan {
        source_span: TextSourceSpan::new(start, end),
        style: style.clone(),
    });
    Ok(())
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let codepoint = if let Some(hex) = entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
            {
                u32::from_str_radix(hex, 16).ok()?
            } else if let Some(decimal) = entity.strip_prefix('#') {
                decimal.parse::<u32>().ok()?
            } else {
                return None;
            };
            char::from_u32(codepoint)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style_for<'a>(normalized: &'a NormalizedMarkupText, text: &str) -> &'a MarkupTextStyle {
        let start = normalized.source.find(text).unwrap() as u32;
        normalized
            .spans
            .iter()
            .find(|span| span.source_span.start <= start && start < span.source_span.end)
            .map(|span| &span.style)
            .unwrap()
    }

    #[test]
    fn nested_convenience_tags_normalize_to_plain_source_and_inherited_styles() {
        let normalized = normalize_markup_text("<b>bold <i>both</i></b> plain").unwrap();
        assert_eq!(normalized.source.as_ref(), "bold both plain");
        assert!(style_for(&normalized, "bold").bold);
        assert!(!style_for(&normalized, "bold").italic);
        assert!(style_for(&normalized, "both").bold);
        assert!(style_for(&normalized, "both").italic);
        assert_eq!(style_for(&normalized, "plain"), &MarkupTextStyle::default());
    }

    #[test]
    fn convenience_tags_compose_relative_size_and_baseline_steps() {
        let normalized = normalize_markup_text(
            "<big>A<big>B</big></big><small>C</small>H<sub>2</sub>O<sup>+</sup>",
        )
        .unwrap();
        assert_eq!(normalized.source.as_ref(), "ABCH2O+");
        assert_eq!(style_for(&normalized, "A").size_steps, 1);
        assert_eq!(style_for(&normalized, "B").size_steps, 2);
        assert_eq!(style_for(&normalized, "C").size_steps, -1);
        assert_eq!(style_for(&normalized, "2").baseline_steps, -1);
        assert_eq!(style_for(&normalized, "+").baseline_steps, 1);
    }

    #[test]
    fn span_metadata_is_inherited_and_restored() {
        let normalized = normalize_markup_text(
            "<span foreground=\"red\" font_family='serif'>red <b>bold</b></span> default",
        )
        .unwrap();
        let red = style_for(&normalized, "red");
        assert_eq!(red.foreground.as_deref(), Some("red"));
        assert_eq!(red.font_family.as_deref(), Some("serif"));
        let bold = style_for(&normalized, "bold");
        assert!(bold.bold);
        assert_eq!(bold.foreground.as_deref(), Some("red"));
        assert_eq!(style_for(&normalized, "default").foreground, None);
    }

    #[test]
    fn named_and_numeric_entities_decode_before_utf8_spans_are_recorded() {
        let normalized = normalize_markup_text("caf&#233; &amp; &#x1F642; &lt;x&gt;").unwrap();
        assert_eq!(normalized.source.as_ref(), "café & 🙂 <x>");
        assert_eq!(normalized.spans.len(), 1);
        assert_eq!(normalized.spans[0].source_span.start, 0);
        assert_eq!(normalized.spans[0].source_span.end, normalized.source.len() as u32);
        assert!(normalized
            .spans
            .iter()
            .all(|span| normalized.source.is_char_boundary(span.source_span.start as usize)
                && normalized.source.is_char_boundary(span.source_span.end as usize)));
    }

    #[test]
    fn entity_decoding_inside_attributes_is_deterministic() {
        let normalized = normalize_markup_text(
            "<span font_family='A&amp;B' foreground='#fff'>x</span>",
        )
        .unwrap();
        let style = style_for(&normalized, "x");
        assert_eq!(style.font_family.as_deref(), Some("A&B"));
        assert_eq!(style.foreground.as_deref(), Some("#fff"));
    }

    #[test]
    fn malformed_nesting_is_rejected_instead_of_recovered() {
        assert!(matches!(
            normalize_markup_text("<b><i>x</b></i>"),
            Err(MarkupTextError::MismatchedClosingTag { .. })
        ));
        assert!(matches!(
            normalize_markup_text("<b>x"),
            Err(MarkupTextError::UnclosedTag { .. })
        ));
    }

    #[test]
    fn unsupported_markup_is_rejected_explicitly() {
        assert!(matches!(
            normalize_markup_text("<gradient from='RED' to='BLUE'>x</gradient>"),
            Err(MarkupTextError::UnsupportedTag { .. })
        ));
        assert!(matches!(
            normalize_markup_text("<span underline='double'>x</span>"),
            Err(MarkupTextError::UnsupportedAttribute { .. })
        ));
        assert!(matches!(
            normalize_markup_text("<b class='x'>x</b>"),
            Err(MarkupTextError::UnsupportedAttribute { .. })
        ));
    }

    #[test]
    fn invalid_entities_are_rejected_explicitly() {
        assert!(matches!(
            normalize_markup_text("x &bogus; y"),
            Err(MarkupTextError::InvalidEntity { .. })
        ));
        assert!(matches!(
            normalize_markup_text("x &#x110000; y"),
            Err(MarkupTextError::InvalidEntity { .. })
        ));
    }
}
