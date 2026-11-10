//! Resource and coordinate constructors through the existing Scene admission path.
//!
//! The capability borrows the one Scene only for the operation. Preparation,
//! semantic identity, rollback, and running publication remain owned by that
//! Scene; constructing an object does not implicitly add it to the display.

use std::sync::Arc;

use super::{ScriptError, ScriptScene};
use crate::{
    ImageMobjectOptions, ManimAxes, ManimAxesOptions, ManimNumberLine, ManimNumberLineOptions,
    ManimNumberPlane, ManimNumberPlaneOptions, Mobject,
};

impl ScriptScene {
    /// Shape and admit detached native text, before or after a completed boundary.
    ///
    /// Accepts a string, [`crate::Text`], or [`crate::MarkupText`]. Font selection
    /// and validation use the same providers as [`crate::Scene::text`].
    #[cfg(feature = "native-text")]
    pub fn text(&self, text: impl Into<crate::Text>) -> Result<Mobject, ScriptError> {
        self.with_scene(|scene| Ok(scene.text(text)?))
    }

    /// Compile and admit detached Typst markup through shared resource admission.
    #[cfg(feature = "typst")]
    pub fn typst(&self, text: crate::Typst) -> Result<Mobject, ScriptError> {
        self.with_scene(|scene| Ok(scene.typst(text)?))
    }

    /// Compile and admit detached Typst math through shared resource admission.
    #[cfg(feature = "typst")]
    pub fn math_typst(&self, text: crate::MathTypst) -> Result<Mobject, ScriptError> {
        self.with_scene(|scene| Ok(scene.math_typst(text)?))
    }

    /// Admit an immutable prepared image without adding it to the display.
    ///
    /// With `image-decode`, `ImageMobjectOptions::encoded` prepares PNG/JPEG
    /// bytes. File and URL access remain with the calling host.
    pub fn image(&self, options: ImageMobjectOptions) -> Result<Mobject, ScriptError> {
        self.with_scene(|scene| Ok(scene.image(options)?))
    }

    /// Admit tightly packed, top-to-bottom, straight-alpha RGBA8 pixels.
    /// Invalid dimensions or byte counts publish no object or image resource.
    pub fn image_rgba8(
        &self,
        width: u32,
        height: u32,
        pixels: impl Into<Arc<[u8]>>,
    ) -> Result<Mobject, ScriptError> {
        self.with_scene(|scene| Ok(scene.image_rgba8(width, height, pixels)?))
    }

    /// Construct a detached listing with the shared syntax and text providers.
    /// Add its [`crate::Code::family`] through the ordinary family membership API.
    #[cfg(feature = "code")]
    pub fn code(
        &self,
        source: &str,
        options: crate::CodeOptions,
    ) -> Result<crate::Code, ScriptError> {
        self.with_scene(|scene| Ok(crate::Code::new(scene, source, options)?))
    }

    /// Construct a detached coordinate line, including its retained ticks and tips.
    pub fn number_line(
        &self,
        options: &ManimNumberLineOptions,
    ) -> Result<ManimNumberLine, ScriptError> {
        self.with_scene(|scene| Ok(scene.number_line(options)?))
    }

    /// Construct detached axes through one shared semantic transaction.
    pub fn axes(&self, options: &ManimAxesOptions) -> Result<ManimAxes, ScriptError> {
        self.with_scene(|scene| Ok(scene.axes(options)?))
    }

    /// Construct a detached retained coordinate plane through one transaction.
    pub fn number_plane(
        &self,
        options: &ManimNumberPlaneOptions,
    ) -> Result<ManimNumberPlane, ScriptError> {
        self.with_scene(|scene| Ok(scene.number_plane(options)?))
    }
}

#[cfg(test)]
mod tests;
