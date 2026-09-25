//! Shared source and template preparation for the real LaTeX compiler.
//!
//! Hosts execute the returned document; they do not rewrite expressions, infer
//! geometry, or substitute another math language. Presentation is deliberately
//! absent, so color, size and placement can reuse one compiled artifact.

use std::{fmt, sync::Arc};

use noon_core::{TextSourceKind, TextSourceSpan};

pub const LATEX_TEMPLATE_VERSION: &str = "noon-latex-box-v1";
pub const MAX_LATEX_DOCUMENT_BYTES: usize = 1024 * 1024;
const MAX_LATEX_ARGUMENTS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LatexFormat {
    /// An engine whose immutable format already contains the document class.
    Preloaded,
    /// A conventional native LaTeX executable with the article class available.
    Article,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LatexDocument {
    source: Arc<str>,
    kind: TextSourceKind,
    preamble: Arc<str>,
    environment: Option<Arc<str>>,
    part_spans: Arc<[TextSourceSpan]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LatexDocumentError {
    InvalidSourceKind,
    InvalidEnvironment,
    TooLarge,
    NulByte,
}

impl fmt::Display for LatexDocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidSourceKind => "LaTeX source must be Tex or MathTex",
            Self::InvalidEnvironment => "invalid LaTeX environment name",
            Self::TooLarge => "LaTeX document exceeds 1 MiB",
            Self::NulByte => "LaTeX source contains a NUL byte",
        })
    }
}
impl std::error::Error for LatexDocumentError {}

impl LatexDocument {
    pub fn new(
        source: impl Into<Arc<str>>,
        kind: TextSourceKind,
    ) -> Result<Self, LatexDocumentError> {
        let environment = match kind {
            TextSourceKind::Tex => "center",
            TextSourceKind::MathTex => "align*",
            _ => return Err(LatexDocumentError::InvalidSourceKind),
        };
        let source = source.into();
        let (source, part_spans) = normalize_arguments([source], kind)?;
        let document = Self {
            source,
            kind,
            preamble: Arc::from("\\usepackage{amsmath}\n\\usepackage{amssymb}"),
            environment: Some(Arc::from(environment)),
            part_spans,
        };
        document.validate()?;
        Ok(document)
    }

    /// Build one source from explicit Manim-style string arguments. MathTex
    /// arguments remain separately addressable text parts; `{{...}}` within an
    /// argument removes its delimiter braces and creates an isolated part.
    pub fn from_arguments<I, S>(
        arguments: I,
        kind: TextSourceKind,
    ) -> Result<Self, LatexDocumentError>
    where
        I: IntoIterator<Item = S>,
        S: Into<Arc<str>>,
    {
        let environment = match kind {
            TextSourceKind::Tex => "center",
            TextSourceKind::MathTex => "align*",
            _ => return Err(LatexDocumentError::InvalidSourceKind),
        };
        let (source, part_spans) = normalize_arguments(arguments, kind)?;
        let document = Self {
            source,
            kind,
            preamble: Arc::from("\\usepackage{amsmath}\n\\usepackage{amssymb}"),
            environment: Some(Arc::from(environment)),
            part_spans,
        };
        document.validate()?;
        Ok(document)
    }

    pub fn with_preamble(
        mut self,
        preamble: impl Into<Arc<str>>,
    ) -> Result<Self, LatexDocumentError> {
        self.preamble = preamble.into();
        self.validate()?;
        Ok(self)
    }

    pub fn with_environment(
        mut self,
        environment: Option<Arc<str>>,
    ) -> Result<Self, LatexDocumentError> {
        self.environment = environment;
        self.validate()?;
        Ok(self)
    }

    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn kind(&self) -> TextSourceKind {
        self.kind
    }
    pub fn part_spans(&self) -> &[TextSourceSpan] {
        &self.part_spans
    }

    fn validate(&self) -> Result<(), LatexDocumentError> {
        if self.environment.as_deref().is_some_and(|name| {
            name.is_empty()
                || name.len() > 64
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'*' | b'@'))
        }) {
            return Err(LatexDocumentError::InvalidEnvironment);
        }
        if self.source.contains('\0') || self.preamble.contains('\0') {
            return Err(LatexDocumentError::NulByte);
        }
        if self.source.len().saturating_add(self.preamble.len()) > MAX_LATEX_DOCUMENT_BYTES {
            return Err(LatexDocumentError::TooLarge);
        }
        Ok(())
    }

    /// The full generated document is part of compilation identity alongside
    /// the engine/format/package/font identity supplied by the host.
    pub fn render(&self, format: LatexFormat) -> Result<String, LatexDocumentError> {
        self.validate()?;
        let mut document = String::new();
        if format == LatexFormat::Article {
            document.push_str("\\documentclass[10pt]{article}\n");
        }
        document.push_str(&self.preamble);
        document.push_str("\n\\begin{document}\n\\setbox0=\\vbox{\\hsize=15cm\n");
        if let Some(environment) = &self.environment {
            document.push_str(&format!("\\begin{{{environment}}}\n"));
        }
        let mut cursor = 0;
        for span in self.part_spans.iter().copied() {
            let start = span.start as usize;
            let end = span.end as usize;
            if cursor < start {
                document.push_str(&format!("\\special{{noon:source:{cursor}:{start}}}"));
                document.push_str(&self.source[cursor..start]);
            }
            document.push_str(&format!(
                "\\special{{noon:part:{start}:{end}}}\\special{{noon:source:{start}:{end}}}"
            ));
            document.push_str(&self.source[start..end]);
            cursor = end;
        }
        if cursor < self.source.len() {
            document.push_str(&format!(
                "\\special{{noon:source:{cursor}:{}}}",
                self.source.len()
            ));
            document.push_str(&self.source[cursor..]);
        }
        // The newline also ends a final TeX comment before generated commands.
        document.push('\n');
        if let Some(environment) = &self.environment {
            document.push_str(&format!("\\end{{{environment}}}\n"));
        }
        document.push_str("}\n\\shipout\\box0\n\\end{document}\n");
        if document.len() > MAX_LATEX_DOCUMENT_BYTES {
            return Err(LatexDocumentError::TooLarge);
        }
        Ok(document)
    }
}

fn normalize_arguments<I, S>(
    arguments: I,
    kind: TextSourceKind,
) -> Result<(Arc<str>, Arc<[TextSourceSpan]>), LatexDocumentError>
where
    I: IntoIterator<Item = S>,
    S: Into<Arc<str>>,
{
    let mut source = String::new();
    let mut parts = Vec::new();
    for (argument_index, argument) in arguments.into_iter().enumerate() {
        if argument_index >= MAX_LATEX_ARGUMENTS {
            return Err(LatexDocumentError::TooLarge);
        }
        let argument = argument.into();
        let additional = argument.len().saturating_add(usize::from(
            argument_index > 0 && kind == TextSourceKind::MathTex,
        ));
        if source.len().saturating_add(additional) > MAX_LATEX_DOCUMENT_BYTES {
            return Err(LatexDocumentError::TooLarge);
        }
        if argument_index > 0 && kind == TextSourceKind::MathTex {
            source.push(' ');
        }
        if kind == TextSourceKind::MathTex {
            append_mathtex_parts(&mut source, &mut parts, &argument);
        } else {
            append_part(&mut source, &mut parts, &argument);
        }
    }
    if parts.is_empty() {
        parts.push(TextSourceSpan::new(0, source_len_u32(source.len())?));
    }
    Ok((source.into(), parts.into()))
}

fn append_mathtex_parts(source: &mut String, parts: &mut Vec<TextSourceSpan>, argument: &str) {
    let mut cursor = 0;
    while cursor < argument.len() {
        let Some(open) = next_isolating_open(argument, cursor) else {
            append_part(source, parts, &argument[cursor..]);
            return;
        };
        let Some(close) = matching_double_brace(argument, open + 2) else {
            append_part(source, parts, &argument[cursor..]);
            return;
        };
        if open > cursor {
            append_part(source, parts, &argument[cursor..open]);
        }
        append_part(source, parts, &argument[open + 2..close]);
        cursor = close + 2;
    }
}

fn next_isolating_open(value: &str, cursor: usize) -> Option<usize> {
    let mut index = cursor;
    while let Some(offset) = value[index..].find("{{") {
        let open = index + offset;
        if open == 0
            || value[..open]
                .chars()
                .last()
                .is_some_and(char::is_whitespace)
        {
            return Some(open);
        }
        index = open + 2;
    }
    None
}

fn matching_double_brace(value: &str, mut index: usize) -> Option<usize> {
    let mut depth = 0_u32;
    while index < value.len() {
        let byte = value.as_bytes()[index];
        if byte == b'\\' {
            index += 1;
            if index < value.len() {
                index += value[index..].chars().next()?.len_utf8();
            }
            continue;
        }
        if byte == b'{' {
            depth = depth.checked_add(1)?;
            index += 1;
            continue;
        }
        if byte == b'}' {
            if depth == 0 && value[index..].starts_with("}}") {
                return Some(index);
            }
            if depth > 0 {
                depth -= 1;
            }
        }
        index += value[index..].chars().next()?.len_utf8();
    }
    None
}

fn append_part(source: &mut String, parts: &mut Vec<TextSourceSpan>, value: &str) {
    if value.is_empty() {
        return;
    }
    let start = source_len_u32(source.len()).expect("bounded LaTeX source");
    source.push_str(value);
    let end = source_len_u32(source.len()).expect("bounded LaTeX source");
    parts.push(TextSourceSpan::new(start, end));
}

fn source_len_u32(length: usize) -> Result<u32, LatexDocumentError> {
    u32::try_from(length).map_err(|_| LatexDocumentError::TooLarge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_identity_uses_utf8_offsets_and_preserves_tex() {
        let document = LatexDocument::new("é % trailing comment", TextSourceKind::Tex).unwrap();
        let rendered = document.render(LatexFormat::Preloaded).unwrap();
        assert!(rendered.contains("noon:source:0:21}"));
        assert!(rendered.contains("é % trailing comment\n\\end{center}"));
        assert!(!rendered.contains("documentclass"));
        assert!(document
            .render(LatexFormat::Article)
            .unwrap()
            .starts_with("\\documentclass[10pt]{article}"));
    }

    #[test]
    fn mathtex_arguments_and_balanced_double_braces_emit_isolated_part_markers() {
        let document =
            LatexDocument::from_arguments(["x{{+}}y", "{{ a^{b^{c}} }}"], TextSourceKind::MathTex)
                .unwrap();
        assert_eq!(document.source(), "x{{+}}y  a^{b^{c}} ");
        assert_eq!(
            document.part_spans(),
            [TextSourceSpan::new(0, 7), TextSourceSpan::new(8, 19),]
        );
        let rendered = document.render(LatexFormat::Preloaded).unwrap();
        for marker in ["noon:part:0:7}", "noon:part:8:19}", "noon:source:7:8}"] {
            assert!(rendered.contains(marker), "missing {marker}");
        }
        assert!(rendered.contains("x{{+}}y"));
        assert!(!rendered.contains("{{ a^{b^{c}} }}"));
    }

    #[test]
    fn mathtex_isolation_ignores_escaped_braces() {
        let document =
            LatexDocument::from_arguments([r"{{ \{x\} }}"], TextSourceKind::MathTex).unwrap();
        assert_eq!(document.source(), r" \{x\} ");
        assert_eq!(document.part_spans(), [TextSourceSpan::new(0, 7)]);
    }

    #[test]
    fn rejects_unbounded_argument_batches_before_partition_growth() {
        assert!(matches!(
            LatexDocument::from_arguments(
                std::iter::repeat("").take(MAX_LATEX_ARGUMENTS + 1),
                TextSourceKind::MathTex,
            ),
            Err(LatexDocumentError::TooLarge)
        ));
    }

    #[test]
    fn tex_arguments_concatenate_while_mathtex_uses_a_space_separator() {
        assert_eq!(
            LatexDocument::from_arguments(["x", "y"], TextSourceKind::Tex)
                .unwrap()
                .source(),
            "xy"
        );
        assert_eq!(
            LatexDocument::from_arguments(["x", "y"], TextSourceKind::MathTex)
                .unwrap()
                .source(),
            "x y"
        );
    }

    #[test]
    fn rejects_bad_configuration_before_invoking_a_host() {
        assert!(matches!(
            LatexDocument::new("x", TextSourceKind::Plain),
            Err(LatexDocumentError::InvalidSourceKind)
        ));
        let document = LatexDocument::new("x", TextSourceKind::MathTex).unwrap();
        assert!(document
            .clone()
            .with_environment(Some(Arc::from("align*}\\end{document}")))
            .is_err());
        assert!(document.clone().with_preamble("\0").is_err());
        assert!(document
            .with_preamble("x".repeat(MAX_LATEX_DOCUMENT_BYTES))
            .is_err());
    }
}
