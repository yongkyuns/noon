//! Immutable presentation choices resolved before ordinary scene admission.
//!
//! A theme is an authoring value, not an ambient setting or runtime style layer.
//! Using one does not change existing objects, primitive defaults, camera state,
//! or renderer configuration. See #2302 for the remaining presentation work.

use crate::Color;
use std::str::FromStr;

/// A text's purpose within a presentation. Placement remains ordinary layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    Title,
    Subtitle,
    Body,
    Caption,
    Label,
    Code,
}

/// A color's purpose, independent of the selected light or dark palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorRole {
    Background,
    Surface,
    RaisedSurface,
    Foreground,
    Muted,
    Outline,
    Accent,
    Positive,
    Warning,
    Negative,
}

/// Reusable spacing in scene units. These values do not place objects themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpacingRole {
    Page,
    Panel,
    Small,
    Normal,
    Large,
}

/// A resolved, immutable native-text presentation. Explicit Text builders may
/// override individual fields after a theme selects these defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    font_family: &'static str,
    font_size: f32,
    line_spacing: f32,
    color: Color,
}

impl TextStyle {
    pub const fn font_family(self) -> &'static str {
        self.font_family
    }

    /// Typographic points, using the existing native Text unit conversion.
    pub const fn font_size(self) -> f32 {
        self.font_size
    }

    /// Native Text line spacing; -1 selects the font's normal line metrics.
    pub const fn line_spacing(self) -> f32 {
        self.line_spacing
    }

    pub const fn color(self) -> Color {
        self.color
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThemeKind {
    Dark,
    Light,
}

/// A small immutable collection of semantic presentation defaults.
///
/// Selection is explicit and local. Constructing or copying a theme allocates
/// no scene, font, GPU resource, or semantic identity. Resolution has no frame
/// cost: the resulting colors and text use the normal shared engine paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    kind: ThemeKind,
}

struct Palette {
    background: Color,
    surface: Color,
    raised_surface: Color,
    foreground: Color,
    muted: Color,
    outline: Color,
    accent: Color,
    positive: Color,
    warning: Color,
    negative: Color,
    categorical: [Color; 8],
}

const DARK: Palette = Palette {
    background: Color::from_hex(0x0B1020),
    surface: Color::from_hex(0x141C2F),
    raised_surface: Color::from_hex(0x1D2941),
    foreground: Color::from_hex(0xEEF3FF),
    muted: Color::from_hex(0xA8B4CC),
    outline: Color::from_hex(0x52627F),
    accent: Color::from_hex(0x70C7F4),
    positive: Color::from_hex(0x7ADBA6),
    warning: Color::from_hex(0xF2C27B),
    negative: Color::from_hex(0xFF968A),
    categorical: [
        Color::from_hex(0x70C7F4),
        Color::from_hex(0x81DDD2),
        Color::from_hex(0x7ADBA6),
        Color::from_hex(0xF2C27B),
        Color::from_hex(0xF7AD77),
        Color::from_hex(0xFF968A),
        Color::from_hex(0xF59CC5),
        Color::from_hex(0xBAABFF),
    ],
};

const LIGHT: Palette = Palette {
    background: Color::from_hex(0xF7F9FC),
    surface: Color::WHITE,
    raised_surface: Color::from_hex(0xE8EEF7),
    foreground: Color::from_hex(0x172238),
    muted: Color::from_hex(0x53627C),
    outline: Color::from_hex(0x8A9AB4),
    accent: Color::from_hex(0x166A9F),
    positive: Color::from_hex(0x20784E),
    warning: Color::from_hex(0x916000),
    negative: Color::from_hex(0xB83F38),
    categorical: [
        Color::from_hex(0x166A9F),
        Color::from_hex(0x087C78),
        Color::from_hex(0x20784E),
        Color::from_hex(0x916000),
        Color::from_hex(0xA9511D),
        Color::from_hex(0xB83F38),
        Color::from_hex(0xA33875),
        Color::from_hex(0x6B4BC5),
    ],
};

impl Theme {
    pub const fn dark() -> Self {
        Self {
            kind: ThemeKind::Dark,
        }
    }

    pub const fn light() -> Self {
        Self {
            kind: ThemeKind::Light,
        }
    }

    const fn palette(self) -> &'static Palette {
        match self.kind {
            ThemeKind::Dark => &DARK,
            ThemeKind::Light => &LIGHT,
        }
    }

    pub const fn color(self, role: ColorRole) -> Color {
        let palette = self.palette();
        match role {
            ColorRole::Background => palette.background,
            ColorRole::Surface => palette.surface,
            ColorRole::RaisedSurface => palette.raised_surface,
            ColorRole::Foreground => palette.foreground,
            ColorRole::Muted => palette.muted,
            ColorRole::Outline => palette.outline,
            ColorRole::Accent => palette.accent,
            ColorRole::Positive => palette.positive,
            ColorRole::Warning => palette.warning,
            ColorRole::Negative => palette.negative,
        }
    }

    pub const fn spacing(self, role: SpacingRole) -> f64 {
        match role {
            SpacingRole::Page | SpacingRole::Large => 0.5,
            SpacingRole::Panel => 0.3,
            SpacingRole::Small => 0.1,
            SpacingRole::Normal => 0.25,
        }
    }

    pub const fn categorical_count(self) -> usize {
        self.palette().categorical.len()
    }

    /// Select a categorical color without silently reusing an earlier category.
    /// A caller that deliberately wants cycling can apply its own modulo.
    pub fn categorical(self, index: usize) -> Result<Color, PresentationError> {
        self.palette().categorical.get(index).copied().ok_or(
            PresentationError::CategoricalIndexOutOfRange {
                index,
                count: self.categorical_count(),
            },
        )
    }

    pub const fn text_style(self, role: TextRole) -> TextStyle {
        let (font_size, color_role) = match role {
            TextRole::Title => (36.0, ColorRole::Foreground),
            TextRole::Subtitle => (28.0, ColorRole::Muted),
            TextRole::Body => (24.0, ColorRole::Foreground),
            TextRole::Caption => (22.0, ColorRole::Muted),
            TextRole::Label => (18.0, ColorRole::Foreground),
            TextRole::Code => (20.0, ColorRole::Foreground),
        };
        TextStyle {
            // This deterministic face is already available in bundled-fonts.
            font_family: "DejaVu Sans Mono",
            font_size,
            line_spacing: -1.0,
            color: self.color(color_role),
        }
    }

    /// Resolve a role into the ordinary native Text builder. Later builder calls
    /// take precedence; neither admission nor rendering needs the theme again.
    #[cfg(feature = "native-text")]
    pub fn text(self, source: impl Into<std::sync::Arc<str>>, role: TextRole) -> crate::Text {
        let style = self.text_style(role);
        crate::Text::new(source)
            .with_font(style.font_family())
            .with_font_size(style.font_size())
            .with_line_spacing(style.line_spacing())
            .color(style.color())
    }
}

/// Invalid presentation names at an optional language boundary, or a category
/// outside the finite palette. Typed Rust role lookup itself cannot fail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresentationError {
    UnknownName { kind: &'static str, name: String },
    CategoricalIndexOutOfRange { index: usize, count: usize },
}

impl std::fmt::Display for PresentationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownName { kind, name } => {
                write!(formatter, "unknown presentation {kind} {name:?}")
            }
            Self::CategoricalIndexOutOfRange { index, count } => write!(
                formatter,
                "categorical color index {index} is outside the palette of {count} colors"
            ),
        }
    }
}

impl std::error::Error for PresentationError {}

fn unknown_name(kind: &'static str, name: &str) -> PresentationError {
    PresentationError::UnknownName {
        kind,
        name: name.into(),
    }
}

impl FromStr for Theme {
    type Err = PresentationError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "dark" => Ok(Self::dark()),
            "light" => Ok(Self::light()),
            _ => Err(unknown_name("theme", name)),
        }
    }
}

impl FromStr for TextRole {
    type Err = PresentationError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "title" => Ok(Self::Title),
            "subtitle" => Ok(Self::Subtitle),
            "body" => Ok(Self::Body),
            "caption" => Ok(Self::Caption),
            "label" => Ok(Self::Label),
            "code" => Ok(Self::Code),
            _ => Err(unknown_name("text role", name)),
        }
    }
}

impl FromStr for ColorRole {
    type Err = PresentationError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "background" => Ok(Self::Background),
            "surface" => Ok(Self::Surface),
            "raised_surface" => Ok(Self::RaisedSurface),
            "foreground" => Ok(Self::Foreground),
            "muted" => Ok(Self::Muted),
            "outline" => Ok(Self::Outline),
            "accent" => Ok(Self::Accent),
            "positive" => Ok(Self::Positive),
            "warning" => Ok(Self::Warning),
            "negative" => Ok(Self::Negative),
            _ => Err(unknown_name("color role", name)),
        }
    }
}

impl FromStr for SpacingRole {
    type Err = PresentationError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "page" => Ok(Self::Page),
            "panel" => Ok(Self::Panel),
            "small" => Ok(Self::Small),
            "normal" => Ok(Self::Normal),
            "large" => Ok(Self::Large),
            _ => Err(unknown_name("spacing role", name)),
        }
    }
}

#[cfg(test)]
mod tests;
