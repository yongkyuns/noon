//! ManimCE's SVG parser stores paint opacity in an eight-bit color alpha.
//!
//! Normalize numeric declarations before `usvg` reduces them to f32, so decimal
//! half-byte ties retain Python's ties-to-even behavior (notably 0.3 -> 76/255).
//! CSS parsing and cascade still use the same parser as `usvg`; selectors and
//! nonnumeric values are left intact. Ordinary post-import style edits bypass
//! this conversion.

use std::ops::Range;

fn paint_opacity(name: &str) -> bool {
    matches!(name, "fill-opacity" | "stroke-opacity")
}

fn numeric_opacity(value: &str) -> Option<String> {
    let value: f64 = value.trim().parse().ok()?;
    value.is_finite().then(|| byte_opacity(value).to_string())
}

pub(crate) fn byte_opacity(value: f64) -> f64 {
    (value.clamp(0.0, 1.0) * 255.0).round_ties_even() / 255.0
}

fn declaration_edits<'a>(
    source: &'a str,
    declarations: impl Iterator<Item = simplecss::Declaration<'a>>,
    edits: &mut Vec<(Range<usize>, String)>,
) {
    for declaration in declarations {
        if !paint_opacity(declaration.name) {
            continue;
        }
        if let Some(value) = numeric_opacity(declaration.value) {
            // Declaration values borrow the original CSS string, including when
            // comments, quoted strings, or !important surround the declaration.
            let start = declaration.value.as_ptr() as usize - source.as_ptr() as usize;
            edits.push((start..start + declaration.value.len(), value));
        }
    }
}

pub(crate) fn normalize(source: &str, document: &usvg::roxmltree::Document<'_>) -> String {
    let mut edits = Vec::new();
    for node in document.descendants().filter(|node| node.is_element()) {
        for attribute in node.attributes() {
            if paint_opacity(attribute.name()) {
                if let Some(value) = numeric_opacity(attribute.value()) {
                    edits.push((attribute.range_value(), value));
                }
            } else if attribute.name() == "style" {
                let range = attribute.range_value();
                // Work on lexical source, avoiding offsets into XML-unescaped
                // attribute storage. Entity-containing declarations are left
                // to the existing SVG parser.
                let css = &source[range.clone()];
                declaration_edits(
                    source,
                    simplecss::DeclarationTokenizer::from(css),
                    &mut edits,
                );
            }
        }
        if node.has_tag_name("style")
            && node.attribute("type").is_none_or(|kind| kind == "text/css")
        {
            for child in node.children().filter(|child| child.is_text()) {
                let css = &source[child.range()];
                let sheet = simplecss::StyleSheet::parse(css);
                declaration_edits(
                    source,
                    sheet
                        .rules
                        .iter()
                        .flat_map(|rule| rule.declarations.iter().copied()),
                    &mut edits,
                );
            }
        }
    }
    edits.sort_by_key(|(range, _)| range.start);
    edits.dedup_by(|left, right| left.0 == right.0);
    let mut normalized = source.to_owned();
    for (range, value) in edits.into_iter().rev() {
        normalized.replace_range(range, &value);
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_alpha_uses_original_decimal_and_preserves_css_structure() {
        let source = r#"<svg fill-opacity="0.3"><style>.a { fill-opacity: 0.25 !important; stroke-opacity: .5 } .b { content: 'fill-opacity: .3' }</style><path style="fill-opacity: .3; stroke-opacity: .5"/></svg>"#;
        let document = usvg::roxmltree::Document::parse(source).unwrap();
        let result = normalize(source, &document);
        assert!(result.contains("fill-opacity=\"0.2980392156862745\""));
        assert!(result.contains("fill-opacity: 0.25098039215686274 !important"));
        assert!(result.contains("content: 'fill-opacity: .3'"));
        assert!(result.contains("stroke-opacity: 0.5019607843137255"));
        assert!(result.contains("style=\"fill-opacity: 0.2980392156862745;"));
    }
}
