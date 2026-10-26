//! Native syntax-highlighted source listings over ordinary retained text.
//!
//! Layout and highlighting happen once, at admission. A `Code` is a normal
//! semantic family with a background, optional line numbers, and text lines;
//! transforms, copies, animations and rendering use the shared engine paths.

mod admission;
mod highlight;
mod regions;
mod source_update;
#[cfg(all(test, feature = "bundled-fonts"))]
mod tests;

use crate::{
    AuthoringError, Color, LiveSession, LiveSessionError, Mobject, MobjectFamily, NativeFontFace,
    Scene, TextAuthoringError, TextPartAuthoringError, DEFAULT_NATIVE_TEXT_FONT_FAMILY,
};
pub use noon_core::CodeSyntaxColors;
use noon_core::SemanticStore;
use std::{cell::RefCell, collections::BTreeMap, path::Path, rc::Rc, sync::Arc};

/// Background presentation, using ordinary retained vector geometry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodeBackground {
    #[default]
    Rectangle,
    Window,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodeBackgroundConfig {
    pub buff: f64,
    pub corner_radius: f64,
    /// `None` uses the selected syntax theme's background.
    pub fill_color: Option<Color>,
    pub fill_opacity: f64,
    pub stroke_color: Color,
    /// Scene-space width, following other native Rust geometry options.
    pub stroke_width: f64,
    pub stroke_opacity: f64,
}
impl Default for CodeBackgroundConfig {
    fn default() -> Self {
        Self {
            buff: 0.3,
            corner_radius: 0.2,
            fill_color: None,
            fill_opacity: 1.0,
            stroke_color: Color::WHITE,
            stroke_width: 0.01,
            stroke_opacity: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodeParagraphConfig {
    pub font: String,
    pub font_face: Option<NativeFontFace>,
    pub font_size: f32,
    pub line_spacing: f32,
    pub disable_ligatures: bool,
}
impl Default for CodeParagraphConfig {
    fn default() -> Self {
        Self {
            font: DEFAULT_NATIVE_TEXT_FONT_FAMILY.into(),
            font_face: None,
            font_size: 24.0,
            line_spacing: 0.5,
            disable_ligatures: true,
        }
    }
}

/// Source-display options. Source is preserved, not formatted or dedented.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeOptions {
    pub language: Option<String>,
    /// A filename hint for language detection; admission performs no file I/O.
    pub source_path: Option<String>,
    pub formatter_style: String,
    /// Character-column tab stops. Zero removes tabs; at most 256 is supported.
    pub tab_width: usize,
    pub add_line_numbers: bool,
    pub line_numbers_from: i32,
    pub background: CodeBackground,
    pub background_config: CodeBackgroundConfig,
    pub paragraph_config: CodeParagraphConfig,
    /// A same-row-count source variant whose extent is reserved before fitting.
    /// This is measured, never displayed. Row edits cannot exceed these slots.
    pub reserved_source: Option<String>,
    pub syntax_colors: CodeSyntaxColors,
    /// Exact whole identifiers or dotted paths; comments and strings are protected.
    pub token_colors: BTreeMap<String, Color>,
}
impl Default for CodeOptions {
    fn default() -> Self {
        Self {
            language: None,
            source_path: None,
            formatter_style: "vim".into(),
            tab_width: 4,
            add_line_numbers: true,
            line_numbers_from: 1,
            background: CodeBackground::Rectangle,
            background_config: CodeBackgroundConfig::default(),
            paragraph_config: CodeParagraphConfig::default(),
            reserved_source: None,
            syntax_colors: CodeSyntaxColors::default(),
            token_colors: BTreeMap::new(),
        }
    }
}

/// Presentation of an ordinary rectangle over allocated source-row slots.
///
/// Padding is measured in the listing's local scene units, before its current
/// panel transform. These options do not change the Code layout or its text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CodeHighlightOptions {
    pub include_line_numbers: bool,
    pub padding: f64,
    pub color: Color,
    pub fill_opacity: f64,
}
impl Default for CodeHighlightOptions {
    fn default() -> Self {
        Self {
            include_line_numbers: false,
            padding: 0.05,
            color: Color::YELLOW,
            fill_opacity: 0.2,
        }
    }
}

#[derive(Debug)]
pub enum CodeAuthoringError {
    InvalidOption {
        name: &'static str,
        value: String,
    },
    Highlight(String),
    InvalidStructure,
    LineOutOfRange {
        index: usize,
        count: usize,
    },
    InvalidLineRange {
        start: usize,
        end: usize,
        count: usize,
    },
    UnsupportedLineRegion(&'static str),
    UnsupportedSourceUpdate(&'static str),
    SourceDoesNotFit {
        index: usize,
    },
    Text(TextAuthoringError),
    TextPart(TextPartAuthoringError),
    Semantic(AuthoringError),
    LiveSession(LiveSessionError),
    Io(std::io::Error),
}
impl std::fmt::Display for CodeAuthoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOption { name, value } => write!(f, "invalid Code {name}: {value}"),
            Self::Highlight(error) => f.write_str(error),
            Self::InvalidStructure => f.write_str("semantic family is not a valid Code listing"),
            Self::LineOutOfRange { index, count } => {
                write!(
                    f,
                    "Code line index {index} is out of range for {count} lines"
                )
            }
            Self::InvalidLineRange { start, end, count } => write!(
                f,
                "Code line range {start}..{end} must be nonempty and within 0..{count}"
            ),
            Self::UnsupportedLineRegion(reason) => {
                write!(f, "Code line region is unavailable: {reason}")
            }
            Self::UnsupportedSourceUpdate(reason) => {
                write!(f, "Code source update is unavailable: {reason}")
            }
            Self::SourceDoesNotFit { index } => {
                write!(f, "Code line {index} exceeds its reserved layout")
            }
            Self::Text(error) => error.fmt(f),
            Self::TextPart(error) => error.fmt(f),
            Self::Semantic(error) => error.fmt(f),
            Self::LiveSession(error) => error.fmt(f),
            Self::Io(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for CodeAuthoringError {}
impl From<AuthoringError> for CodeAuthoringError {
    fn from(value: AuthoringError) -> Self {
        Self::Semantic(value)
    }
}
impl From<TextAuthoringError> for CodeAuthoringError {
    fn from(value: TextAuthoringError) -> Self {
        Self::Text(value)
    }
}
impl From<TextPartAuthoringError> for CodeAuthoringError {
    fn from(value: TextPartAuthoringError) -> Self {
        Self::TextPart(value)
    }
}

/// A source listing with independently addressable retained text lines.
#[derive(Clone, Debug)]
pub struct Code {
    family: MobjectFamily,
    background: MobjectFamily,
    highlights: MobjectFamily,
    code_lines: MobjectFamily,
    line_numbers: Option<MobjectFamily>,
}

/// The removed and replacement rows of one atomic Code edit. The previous row
/// leaves this listing; other scene/family memberships remain unchanged. It is
/// still usable for an ordinary exit animation. The current row occupies the
/// same rank in the listing's `code_lines` family.
#[derive(Clone, Debug)]
pub struct CodeLineReplacement {
    pub previous: Mobject,
    pub current: Mobject,
}
impl Code {
    pub fn new(
        scene: &mut Scene,
        source: &str,
        options: CodeOptions,
    ) -> Result<Self, CodeAuthoringError> {
        admission::publish(admission::Publisher::Scene(scene), source, options)
    }

    pub fn in_store(
        store: Rc<RefCell<SemanticStore>>,
        source: &str,
        options: CodeOptions,
    ) -> Result<Self, CodeAuthoringError> {
        admission::publish(admission::Publisher::Store(store), source, options)
    }

    pub fn in_live_session(
        live: &mut LiveSession<'_>,
        source: &str,
        options: CodeOptions,
    ) -> Result<Self, CodeAuthoringError> {
        admission::publish(admission::Publisher::Live(live), source, options)
    }

    /// Read a UTF-8 file at the native host input boundary, then use shared admission.
    pub fn from_file(
        scene: &mut Scene,
        path: impl AsRef<Path>,
        mut options: CodeOptions,
    ) -> Result<Self, CodeAuthoringError> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path).map_err(CodeAuthoringError::Io)?;
        options.source_path = Some(path.to_string_lossy().into_owned());
        Self::new(scene, &source, options)
    }

    /// Recover the listing view from its ordinary durable family topology.
    pub fn from_family(family: MobjectFamily) -> Result<Self, CodeAuthoringError> {
        family.validate()?;
        let store = Rc::clone(family.integration_store());
        let layout = store
            .borrow()
            .semantic_code_layout(family.node_id())
            .map_err(AuthoringError::from)?
            .ok_or(CodeAuthoringError::InvalidStructure)?;
        let nodes = layout.nodes();
        let (background, highlights, numbers, lines) = (
            nodes.background,
            nodes.highlights,
            nodes.line_numbers,
            nodes.code_lines,
        );
        let listing = Self {
            family,
            background: MobjectFamily::from_node(Rc::clone(&store), background)?,
            highlights: MobjectFamily::from_node(Rc::clone(&store), highlights)?,
            code_lines: MobjectFamily::from_node(Rc::clone(&store), lines)?,
            line_numbers: numbers
                .map(|node| MobjectFamily::from_node(Rc::clone(&store), node))
                .transpose()?,
        };
        let semantic = store.borrow();
        let line_count = validate_text_family(&semantic, lines)?;
        if line_count == 0
            || line_count != layout.metrics().line_count
            || numbers
                .map(|node| validate_text_family(&semantic, node))
                .transpose()?
                .is_some_and(|count| count != line_count)
        {
            return Err(CodeAuthoringError::InvalidStructure);
        }
        let background = semantic
            .semantic_family_checked(background)
            .map_err(AuthoringError::from)?;
        if !matches!(background.member_count(), 1 | 4) {
            return Err(CodeAuthoringError::InvalidStructure);
        }
        for member in background.members_iter() {
            if semantic
                .semantic_object_state_checked(member)
                .map_err(AuthoringError::from)?
                .content
                .geometry()
                .is_none()
            {
                return Err(CodeAuthoringError::InvalidStructure);
            }
        }
        drop(semantic);
        listing.layout()?;
        Ok(listing)
    }

    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }
    pub fn code_lines(&self) -> &MobjectFamily {
        &self.code_lines
    }
    pub fn line_numbers(&self) -> Option<&MobjectFamily> {
        self.line_numbers.as_ref()
    }
    pub fn background(&self) -> &MobjectFamily {
        &self.background
    }
    /// Ordinary highlight geometry, painted after the panel and before text.
    pub fn highlights(&self) -> &MobjectFamily {
        &self.highlights
    }

    /// Select one line through the store's rank lookup, without copying its siblings.
    pub fn line(&self, index: usize) -> Result<Mobject, CodeAuthoringError> {
        let store = self.family.integration_store();
        let semantic = store.borrow();
        let id = semantic
            .semantic_family_member_at_checked(self.code_lines.node_id(), index)
            .map_err(AuthoringError::from)?
            .ok_or_else(|| CodeAuthoringError::LineOutOfRange {
                index,
                count: semantic
                    .node(self.code_lines.node_id())
                    .map_or(0, |node| node.member_count()),
            })?;
        drop(semantic);
        Ok(Mobject::from_node(Rc::clone(store), id)?)
    }
    pub fn line_source(&self, index: usize) -> Result<Arc<str>, CodeAuthoringError> {
        Ok(self.line(index)?.text_source()?)
    }
    /// Read all source lines in one traversal, including empty lines.
    pub fn line_sources(&self) -> Result<Vec<Arc<str>>, CodeAuthoringError> {
        family_sources(&self.code_lines)
    }
    pub fn line_number_sources(&self) -> Result<Vec<Arc<str>>, CodeAuthoringError> {
        self.line_numbers
            .as_ref()
            .map(family_sources)
            .transpose()
            .map(Option::unwrap_or_default)
    }
    pub fn get_styles_list() -> Vec<String> {
        highlight::style_names()
    }
    pub fn get_languages_list() -> Vec<String> {
        highlight::language_names()
    }
}

fn validate_text_family(
    store: &SemanticStore,
    family: noon_core::SemanticNodeId,
) -> Result<usize, CodeAuthoringError> {
    let node = store
        .semantic_family_checked(family)
        .map_err(AuthoringError::from)?;
    for member in node.members_iter() {
        if store
            .semantic_object_state_checked(member)
            .map_err(AuthoringError::from)?
            .content
            .text()
            .is_none()
        {
            return Err(CodeAuthoringError::InvalidStructure);
        }
    }
    Ok(node.member_count())
}

fn family_sources(family: &MobjectFamily) -> Result<Vec<Arc<str>>, CodeAuthoringError> {
    let store = family.integration_store().borrow();
    store
        .semantic_family_checked(family.node_id())
        .map_err(AuthoringError::from)?
        .members_iter()
        .map(|member| {
            let handle = store
                .semantic_object_state_checked(member)
                .map_err(AuthoringError::from)?
                .content
                .text()
                .ok_or(CodeAuthoringError::InvalidStructure)?;
            let resource = store
                .text_resources()
                .get(handle)
                .ok_or(AuthoringError::MissingTextResource(handle))?;
            Ok(Arc::clone(&resource.source))
        })
        .collect()
}

fn invalid(name: &'static str, value: impl ToString) -> CodeAuthoringError {
    CodeAuthoringError::InvalidOption {
        name,
        value: value.to_string(),
    }
}

impl CodeOptions {
    fn validate(&self) -> Result<(), CodeAuthoringError> {
        if self.tab_width > 256 {
            return Err(invalid("tab_width (0..=256)", self.tab_width));
        }
        if !self.syntax_colors.is_valid() {
            return Err(invalid(
                "syntax_colors",
                "expected finite RGBA channels in 0..=1",
            ));
        }
        for (selector, color) in &self.token_colors {
            if !noon_core::is_valid_code_token_selector(selector)
                || !noon_core::is_valid_code_color(*color)
            {
                return Err(invalid("token_colors", selector));
            }
        }
        let paragraph = &self.paragraph_config;
        if !paragraph.font_size.is_finite() || paragraph.font_size <= 0.0 {
            return Err(invalid("font_size", paragraph.font_size));
        }
        if !paragraph.line_spacing.is_finite() || paragraph.line_spacing < -1.0 {
            return Err(invalid("line_spacing", paragraph.line_spacing));
        }
        let background = &self.background_config;
        for (name, value) in [
            ("buff", background.buff),
            ("corner_radius", background.corner_radius),
            ("stroke_width", background.stroke_width),
        ] {
            if !value.is_finite() || value < 0.0 || value > f64::from(f32::MAX) {
                return Err(invalid(name, value));
            }
        }
        for (name, value) in [
            ("fill_opacity", background.fill_opacity),
            ("stroke_opacity", background.stroke_opacity),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(invalid(name, value));
            }
        }
        for (name, color) in [
            ("fill_color", background.fill_color),
            ("stroke_color", Some(background.stroke_color)),
        ] {
            if let Some(color) = color {
                for channel in [color.red, color.green, color.blue, color.alpha] {
                    if !channel.is_finite() || !(0.0..=1.0).contains(&channel) {
                        return Err(invalid(name, format!("{color:?}")));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Normalize newline encodings and expand tabs at real character-column stops.
/// Keep leading/trailing whitespace and every empty line, including the last.
pub(super) fn normalize_source(
    source: &str,
    tab_width: usize,
) -> Result<String, CodeAuthoringError> {
    if tab_width > 256 {
        return Err(invalid("tab_width (0..=256)", tab_width));
    }
    let mut output = String::new();
    output
        .try_reserve(source.len())
        .map_err(|error| invalid("source", error))?;
    let mut column = 0usize;
    let mut chars = source.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\r' | '\n' => {
                if character == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                output.push('\n');
                column = 0;
            }
            '\t' => {
                let count = if tab_width == 0 {
                    0
                } else {
                    tab_width - column % tab_width
                };
                output.extend(std::iter::repeat_n(' ', count));
                column = column
                    .checked_add(count)
                    .ok_or_else(|| invalid("source", "too long"))?;
            }
            other => {
                output.push(other);
                column = column
                    .checked_add(1)
                    .ok_or_else(|| invalid("source", "too long"))?;
            }
        }
        if output.len() > u32::MAX as usize {
            return Err(invalid("source", "exceeds retained text source range"));
        }
    }
    Ok(output)
}
