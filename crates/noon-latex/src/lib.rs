#![forbid(unsafe_code)]

//! Real-LaTeX backend contract for Noon.
//!
//! This crate deliberately stops at the authoring/compile boundary. A concrete
//! provider may use an in-process/WASM TeX engine or another deterministic compile
//! service, but it must normalize the result into Noon's retained `TextResource`,
//! immutable font arena, and shared vector geometry arena before returning. DVI,
//! XDV, PDF, or SVG therefore never become permanent runtime text representations.

use std::{fmt, sync::Arc};

use noon_core::{
    FontResourceArena, GeometryResourceArena, TextLayoutArtifact, TextLayoutBackend,
    TextLayoutBackendKind, TextResource, TextResourceValidationError, TextSourceKind,
};

pub const LATEX_CONTRACT_VERSION: &str = "noon-latex-contract-v1";
const DEFAULT_DOCUMENT_CLASS: &str = r"\documentclass[preview]{standalone}";
const DEFAULT_PREAMBLE: &str =
    "\\usepackage[english]{babel}\n\\usepackage{amsmath}\n\\usepackage{amssymb}";
const DEFAULT_PLACEHOLDER: &str = "YourTextHere";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LatexSourceMode {
    Tex,
    MathTex,
}

impl LatexSourceMode {
    pub const fn source_kind(self) -> TextSourceKind {
        match self {
            Self::Tex => TextSourceKind::Tex,
            Self::MathTex => TextSourceKind::MathTex,
        }
    }

    /// ManimCE's normal `MathTex` source is compiled inside `align*`; ordinary
    /// `Tex` source is inserted directly unless the caller supplies an environment.
    pub const fn default_environment(self) -> Option<&'static str> {
        match self {
            Self::Tex => None,
            Self::MathTex => Some("align*"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LatexCompilerKind {
    Latex,
    PdfLatex,
    LuaTex,
    LuaLatex,
    XeLatex,
}

impl LatexCompilerKind {
    pub const fn command_name(self) -> &'static str {
        match self {
            Self::Latex => "latex",
            Self::PdfLatex => "pdflatex",
            Self::LuaTex => "luatex",
            Self::LuaLatex => "lualatex",
            Self::XeLatex => "xelatex",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LatexOutputFormat {
    Dvi,
    Xdv,
    Pdf,
}

impl LatexOutputFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Dvi => ".dvi",
            Self::Xdv => ".xdv",
            Self::Pdf => ".pdf",
        }
    }
}

/// Compiler/template identity needed before any concrete TeX engine is selected.
///
/// Defaults match ManimCE's ordinary `TexTemplate`: classic `latex` producing DVI,
/// standalone preview document class, and the babel/amsmath/amssymb preamble.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LatexTemplateSpec {
    pub compiler: LatexCompilerKind,
    pub output_format: LatexOutputFormat,
    pub document_class: Arc<str>,
    pub preamble: Arc<str>,
    pub post_document_commands: Arc<str>,
    pub placeholder: Arc<str>,
    /// When set, this complete body is authoritative and the structured template
    /// fields remain identity metadata only, matching Manim's fixed-body behavior.
    pub body_override: Option<Arc<str>>,
}

impl Default for LatexTemplateSpec {
    fn default() -> Self {
        Self {
            compiler: LatexCompilerKind::Latex,
            output_format: LatexOutputFormat::Dvi,
            document_class: Arc::from(DEFAULT_DOCUMENT_CLASS),
            preamble: Arc::from(DEFAULT_PREAMBLE),
            post_document_commands: Arc::from(""),
            placeholder: Arc::from(DEFAULT_PLACEHOLDER),
            body_override: None,
        }
    }
}

impl LatexTemplateSpec {
    pub fn body(&self) -> String {
        if let Some(body) = &self.body_override {
            return body.to_string();
        }
        let mut sections = vec![self.document_class.as_ref(), self.preamble.as_ref()];
        sections.push(r"\begin{document}");
        if !self.post_document_commands.is_empty() {
            sections.push(self.post_document_commands.as_ref());
        }
        sections.push(self.placeholder.as_ref());
        sections.push(r"\end{document}");
        sections.join("\n")
    }

    pub fn fingerprint(&self) -> Arc<str> {
        let mut identity = format!(
            "{}\0{}\0{}\0{}\0{}\0{}\0{}",
            self.compiler.command_name(),
            self.output_format.extension(),
            self.document_class,
            self.preamble,
            self.post_document_commands,
            self.placeholder,
            self.body_override.as_deref().unwrap_or("")
        );
        identity.push('\0');
        identity.push_str(LATEX_CONTRACT_VERSION);
        Arc::from(fingerprint_hex(identity.as_bytes()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LatexCompileRequest {
    pub source: Arc<str>,
    pub mode: LatexSourceMode,
    pub template: LatexTemplateSpec,
    /// Explicit environment override. `None` selects the source mode's default.
    pub environment: Option<Arc<str>>,
}

impl LatexCompileRequest {
    pub fn new(source: impl Into<Arc<str>>, mode: LatexSourceMode) -> Self {
        Self {
            source: source.into(),
            mode,
            template: LatexTemplateSpec::default(),
            environment: None,
        }
    }

    pub fn resolved_environment(&self) -> Option<&str> {
        self.environment
            .as_deref()
            .or_else(|| self.mode.default_environment())
    }

    /// Deterministic complete TeX input for a concrete compiler provider.
    pub fn prepared_source(&self) -> Result<String, LatexBackendError> {
        if self.template.placeholder.is_empty() {
            return Err(LatexBackendError::InvalidTemplate(
                "LaTeX template placeholder must not be empty".into(),
            ));
        }
        let expression = match self.resolved_environment() {
            Some(environment) => {
                validate_environment(environment)?;
                format!(
                    "\\begin{{{environment}}}\n{}\n\\end{{{environment}}}",
                    self.source
                )
            }
            None => self.source.to_string(),
        };
        Ok(self
            .template
            .body()
            .replace(self.template.placeholder.as_ref(), &expression))
    }

    pub fn compile_fingerprint(
        &self,
        backend_version: &str,
    ) -> Result<Arc<str>, LatexBackendError> {
        let prepared = self.prepared_source()?;
        let identity = format!(
            "{LATEX_CONTRACT_VERSION}\0{backend_version}\0{:?}\0{}\0{}\0{}",
            self.mode,
            self.template.compiler.command_name(),
            self.template.output_format.extension(),
            prepared
        );
        Ok(Arc::from(fingerprint_hex(identity.as_bytes())))
    }

    pub fn layout_artifact(
        &self,
        backend_version: impl Into<Arc<str>>,
        normalized_resource_fingerprint: &str,
    ) -> Result<TextLayoutArtifact, LatexBackendError> {
        let backend_version = backend_version.into();
        let compile = self.compile_fingerprint(backend_version.as_ref())?;
        let artifact_identity = format!("{}\0{normalized_resource_fingerprint}", compile);
        Ok(TextLayoutArtifact {
            backend: TextLayoutBackend {
                kind: TextLayoutBackendKind::Latex,
                version: backend_version,
            },
            template_fingerprint: self.template.fingerprint(),
            artifact_fingerprint: Arc::from(fingerprint_hex(artifact_identity.as_bytes())),
            backend_payload_key: None,
        })
    }
}

/// Fully normalized output accepted from a concrete real-LaTeX provider.
///
/// There is intentionally no DVI/PDF/SVG field here. Backend-native intermediate
/// data may exist while compiling, but the retained runtime receives only semantic
/// text resources plus exact immutable dependencies.
#[derive(Clone, Debug)]
pub struct LatexResourceArtifact {
    pub resource: TextResource,
    pub fonts: FontResourceArena,
    pub geometry: GeometryResourceArena,
}

impl LatexResourceArtifact {
    pub fn validate_for(&self, request: &LatexCompileRequest) -> Result<(), LatexBackendError> {
        self.resource
            .validate()
            .map_err(LatexBackendError::InvalidResource)?;
        if self.resource.source.as_ref() != request.source.as_ref() {
            return Err(LatexBackendError::ContractViolation(
                "normalized LaTeX resource source differs from compile request".into(),
            ));
        }
        if self.resource.kind != request.mode.source_kind() {
            return Err(LatexBackendError::ContractViolation(
                "normalized LaTeX source kind differs from compile request".into(),
            ));
        }
        let layout = self.resource.layout_artifact.as_ref().ok_or_else(|| {
            LatexBackendError::ContractViolation(
                "normalized LaTeX resource is missing layout backend identity".into(),
            )
        })?;
        if layout.backend.kind != TextLayoutBackendKind::Latex {
            return Err(LatexBackendError::ContractViolation(
                "normalized Tex/MathTex resource must identify the LaTeX backend".into(),
            ));
        }
        for run in self.resource.runs.iter() {
            if self.fonts.get_for_face(&run.font).is_none() {
                return Err(LatexBackendError::ContractViolation(
                    "normalized LaTeX glyph run is missing exact retained font bytes".into(),
                ));
            }
        }
        for vector in self.resource.vector_items.iter() {
            if self.geometry.get(vector.geometry).is_none() {
                return Err(LatexBackendError::ContractViolation(
                    "normalized LaTeX vector item references missing geometry".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Concrete engine/service boundary. Implementations compile only at authoring or
/// resource-build time; renderers never call this trait.
pub trait LatexBackendProvider {
    fn backend_version(&self) -> &str;

    fn compile_resource(
        &mut self,
        request: &LatexCompileRequest,
    ) -> Result<LatexResourceArtifact, LatexBackendError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LatexBackendError {
    BackendUnavailable(Arc<str>),
    InvalidTemplate(Arc<str>),
    InvalidEnvironment(Arc<str>),
    Compile(Arc<str>),
    UnsupportedResource(Arc<str>),
    ContractViolation(Arc<str>),
    InvalidResource(TextResourceValidationError),
}

impl fmt::Display for LatexBackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BackendUnavailable(message) => {
                write!(formatter, "LaTeX backend unavailable: {message}")
            }
            Self::InvalidTemplate(message) => {
                write!(formatter, "invalid LaTeX template: {message}")
            }
            Self::InvalidEnvironment(message) => {
                write!(formatter, "invalid LaTeX environment: {message}")
            }
            Self::Compile(message) => write!(formatter, "LaTeX compilation failed: {message}"),
            Self::UnsupportedResource(message) => {
                write!(formatter, "unsupported LaTeX resource: {message}")
            }
            Self::ContractViolation(message) => {
                write!(formatter, "LaTeX backend contract violation: {message}")
            }
            Self::InvalidResource(error) => {
                write!(formatter, "invalid normalized LaTeX resource: {error}")
            }
        }
    }
}

impl std::error::Error for LatexBackendError {}

fn validate_environment(environment: &str) -> Result<(), LatexBackendError> {
    if environment.is_empty()
        || environment.contains(['\n', '\r', '\\', '{', '}'])
        || environment.chars().any(char::is_whitespace)
    {
        return Err(LatexBackendError::InvalidEnvironment(Arc::from(
            environment,
        )));
    }
    Ok(())
}

/// Stable FNV-1a fingerprint used only as deterministic cache/resource identity.
fn fingerprint_hex(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_classic_manim_latex_to_dvi_contract() {
        let template = LatexTemplateSpec::default();
        assert_eq!(template.compiler, LatexCompilerKind::Latex);
        assert_eq!(template.output_format, LatexOutputFormat::Dvi);
        assert!(template.document_class.contains("standalone"));
        assert!(template.preamble.contains("amsmath"));
        assert!(template.preamble.contains("amssymb"));
    }

    #[test]
    fn tex_and_mathtex_keep_distinct_source_semantics() {
        assert_eq!(LatexSourceMode::Tex.source_kind(), TextSourceKind::Tex);
        assert_eq!(
            LatexSourceMode::MathTex.source_kind(),
            TextSourceKind::MathTex
        );
        assert_ne!(
            LatexSourceMode::MathTex.source_kind(),
            TextSourceKind::MathTypst
        );
    }

    #[test]
    fn mathtex_defaults_to_align_star_but_tex_does_not() {
        let tex = LatexCompileRequest::new("x^2", LatexSourceMode::Tex);
        let math = LatexCompileRequest::new("x^2", LatexSourceMode::MathTex);
        assert_eq!(tex.resolved_environment(), None);
        assert_eq!(math.resolved_environment(), Some("align*"));
        assert!(!tex.prepared_source().unwrap().contains(r"\begin{align*}"));
        assert!(math.prepared_source().unwrap().contains(r"\begin{align*}"));
    }

    #[test]
    fn template_and_compile_fingerprints_capture_semantic_inputs() {
        let first = LatexCompileRequest::new("x", LatexSourceMode::MathTex);
        let mut second = first.clone();
        second.template.preamble = Arc::from("\\usepackage{amsmath}\n\\usepackage{physics}");
        assert_ne!(first.template.fingerprint(), second.template.fingerprint());
        assert_ne!(
            first.compile_fingerprint("engine-1").unwrap(),
            second.compile_fingerprint("engine-1").unwrap()
        );
        assert_ne!(
            first.compile_fingerprint("engine-1").unwrap(),
            first.compile_fingerprint("engine-2").unwrap()
        );
    }

    #[test]
    fn invalid_environment_is_rejected_before_compilation() {
        let mut request = LatexCompileRequest::new("x", LatexSourceMode::Tex);
        request.environment = Some(Arc::from("align*\\evil"));
        assert!(matches!(
            request.prepared_source(),
            Err(LatexBackendError::InvalidEnvironment(_))
        ));
    }

    #[test]
    fn layout_artifact_is_explicitly_latex_and_payload_free() {
        let request = LatexCompileRequest::new("x", LatexSourceMode::MathTex);
        let artifact = request
            .layout_artifact("test-engine-1", "normalized-glyph-vector-set")
            .unwrap();
        assert_eq!(artifact.backend.kind, TextLayoutBackendKind::Latex);
        assert_eq!(artifact.backend.version.as_ref(), "test-engine-1");
        assert!(artifact.backend_payload_key.is_none());
    }
}
