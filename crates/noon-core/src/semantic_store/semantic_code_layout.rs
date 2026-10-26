//! Sparse allocated Code rows on ordinary semantic family roots.
//!
//! Metrics remain in the admitted panel's local frame. The geometry handle is
//! only an identity guard for that frame; the panel object owns its resource.

use crate::{
    CodeSyntaxColors, Color, GeometryResourceHandle, NativeFontMetricsProfile,
    SemanticLocalResourceToken, SemanticNodeId, SemanticTransactionNodeRef,
};
use std::{collections::BTreeMap, sync::Arc};

/// Immutable authoring inputs for editing an allocated listing. Font bytes are
/// shared immutable inputs, not a resource handle or another text scene. Current
/// source remains authoritative on the ordinary text members.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticCodeSourceRecipe {
    pub language: Option<Arc<str>>,
    pub formatter_style: Arc<str>,
    pub tab_width: usize,
    pub font_family: Arc<str>,
    pub font_data: Arc<[u8]>,
    pub font_face_index: u32,
    pub font_metrics_profile: NativeFontMetricsProfile,
    pub font_size: f32,
    pub line_spacing: f32,
    pub disable_ligatures: bool,
    /// Logical column zero, before glyph ink centering, in the panel frame.
    pub logical_source_x: f64,
    pub syntax_colors: CodeSyntaxColors,
    pub token_colors: BTreeMap<String, Color>,
}

impl SemanticCodeSourceRecipe {
    pub fn is_valid(&self) -> bool {
        self.tab_width <= 256
            && !self.font_family.is_empty()
            && !self.font_data.is_empty()
            && self.font_size.is_finite()
            && self.font_size > 0.0
            && self.line_spacing.is_finite()
            && self.line_spacing >= -1.0
            && self.logical_source_x.is_finite()
            && self.syntax_colors.is_valid()
            && self.token_colors.iter().all(|(token, color)| {
                crate::is_valid_code_token_selector(token)
                    && [color.red, color.green, color.blue, color.alpha]
                        .into_iter()
                        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
            })
    }
}

/// Named ordinary nodes that make one listing's durable query structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticCodeLayoutNodes<N = SemanticNodeId> {
    pub panel: N,
    pub background: N,
    pub highlights: N,
    pub code_lines: N,
    pub line_numbers: Option<N>,
}

impl<N: Copy> SemanticCodeLayoutNodes<N> {
    pub fn referenced_nodes(&self) -> impl Iterator<Item = N> + '_ {
        std::iter::once(self.panel).chain(self.family_references())
    }

    pub fn family_references(&self) -> impl Iterator<Item = N> + '_ {
        [self.background, self.highlights, self.code_lines]
            .into_iter()
            .chain(self.line_numbers)
    }

    pub fn map_nodes<M: Copy>(self, mut map: impl FnMut(N) -> M) -> SemanticCodeLayoutNodes<M> {
        SemanticCodeLayoutNodes {
            panel: map(self.panel),
            background: map(self.background),
            highlights: map(self.highlights),
            code_lines: map(self.code_lines),
            line_numbers: self.line_numbers.map(map),
        }
    }
}

/// Allocated listing slots, independent of each line's current ink or content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticCodeLayoutMetrics {
    pub line_count: usize,
    pub first_baseline: f64,
    pub baseline_step: f64,
    pub row_bottom: f64,
    pub row_top: f64,
    pub body_left: f64,
    pub body_right: f64,
    pub gutter_left: f64,
}

impl SemanticCodeLayoutMetrics {
    pub fn is_valid(&self) -> bool {
        self.line_count > 0
            && [
                self.first_baseline,
                self.baseline_step,
                self.row_bottom,
                self.row_top,
                self.body_left,
                self.body_right,
                self.gutter_left,
            ]
            .into_iter()
            .all(f64::is_finite)
            && self.baseline_step > 0.0
            && self.row_top > self.row_bottom
            && (self.baseline_step * self.line_count.saturating_sub(1) as f64
                + (self.row_top - self.row_bottom))
                .is_finite()
            && self.gutter_left <= self.body_left
            && self.body_left <= self.body_right
            && (self.body_right - self.gutter_left).is_finite()
            && (self.first_baseline + self.row_top).is_finite()
            && (self.first_baseline - self.baseline_step * self.line_count.saturating_sub(1) as f64
                + self.row_bottom)
                .is_finite()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticCodeLayout {
    nodes: SemanticCodeLayoutNodes,
    metrics: SemanticCodeLayoutMetrics,
    panel_geometry: GeometryResourceHandle,
    source_recipe: Option<Arc<SemanticCodeSourceRecipe>>,
}

impl SemanticCodeLayout {
    pub const fn new(
        nodes: SemanticCodeLayoutNodes,
        metrics: SemanticCodeLayoutMetrics,
        panel_geometry: GeometryResourceHandle,
    ) -> Self {
        Self {
            nodes,
            metrics,
            panel_geometry,
            source_recipe: None,
        }
    }

    pub fn with_source_recipe(mut self, recipe: Arc<SemanticCodeSourceRecipe>) -> Self {
        self.source_recipe = Some(recipe);
        self
    }

    pub fn source_recipe(&self) -> Option<&Arc<SemanticCodeSourceRecipe>> {
        self.source_recipe.as_ref()
    }

    pub const fn nodes(&self) -> &SemanticCodeLayoutNodes {
        &self.nodes
    }

    pub const fn metrics(&self) -> &SemanticCodeLayoutMetrics {
        &self.metrics
    }

    pub const fn panel_geometry(&self) -> GeometryResourceHandle {
        self.panel_geometry
    }

    pub fn referenced_nodes(&self) -> impl Iterator<Item = SemanticNodeId> + '_ {
        self.nodes.referenced_nodes()
    }

    pub(crate) fn remap_resource_namespace(&mut self, namespace: u64) {
        // This also remaps a retired guard: replacing the panel must not turn
        // a copied invalidated layout into a newly valid frame declaration.
        self.panel_geometry.arena = namespace;
    }
}

/// A guard resolves through the transaction's existing path admission scope.
/// It does not introduce a resource declaration or retain a resource payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticCodePanelGeometry {
    Existing(GeometryResourceHandle),
    Pending(SemanticLocalResourceToken),
}

impl From<GeometryResourceHandle> for SemanticCodePanelGeometry {
    fn from(handle: GeometryResourceHandle) -> Self {
        Self::Existing(handle)
    }
}

impl From<SemanticLocalResourceToken> for SemanticCodePanelGeometry {
    fn from(token: SemanticLocalResourceToken) -> Self {
        Self::Pending(token)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticTransactionCodeLayout {
    nodes: SemanticCodeLayoutNodes<SemanticTransactionNodeRef>,
    metrics: SemanticCodeLayoutMetrics,
    panel_geometry: SemanticCodePanelGeometry,
    source_recipe: Option<Arc<SemanticCodeSourceRecipe>>,
}

impl From<SemanticCodeLayout> for SemanticTransactionCodeLayout {
    fn from(layout: SemanticCodeLayout) -> Self {
        Self::map_existing(layout, Into::into)
    }
}

impl SemanticTransactionCodeLayout {
    pub fn new(
        nodes: SemanticCodeLayoutNodes<SemanticTransactionNodeRef>,
        metrics: SemanticCodeLayoutMetrics,
        panel_geometry: impl Into<SemanticCodePanelGeometry>,
    ) -> Self {
        Self {
            nodes,
            metrics,
            panel_geometry: panel_geometry.into(),
            source_recipe: None,
        }
    }

    pub fn with_source_recipe(mut self, recipe: Arc<SemanticCodeSourceRecipe>) -> Self {
        self.source_recipe = Some(recipe);
        self
    }

    pub fn map_existing(
        layout: SemanticCodeLayout,
        map: impl FnMut(SemanticNodeId) -> SemanticTransactionNodeRef,
    ) -> Self {
        Self {
            nodes: layout.nodes.map_nodes(map),
            metrics: layout.metrics,
            panel_geometry: layout.panel_geometry.into(),
            source_recipe: layout.source_recipe,
        }
    }

    pub(crate) fn nodes(&self) -> &SemanticCodeLayoutNodes<SemanticTransactionNodeRef> {
        &self.nodes
    }

    pub(crate) fn node_references(&self) -> impl Iterator<Item = SemanticTransactionNodeRef> + '_ {
        self.nodes.referenced_nodes()
    }

    pub(crate) fn family_references(
        &self,
    ) -> impl Iterator<Item = SemanticTransactionNodeRef> + '_ {
        self.nodes.family_references()
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.metrics.is_valid()
            && self.source_recipe.as_ref().is_none_or(|recipe| {
                recipe.is_valid()
                    && (self.metrics.body_left..=self.metrics.body_right)
                        .contains(&recipe.logical_source_x)
            })
    }

    pub(crate) fn panel_geometry(&self) -> SemanticCodePanelGeometry {
        self.panel_geometry
    }

    pub(crate) fn materialize_panel_geometry(
        &mut self,
        resolve: impl FnOnce(SemanticLocalResourceToken) -> GeometryResourceHandle,
    ) {
        if let SemanticCodePanelGeometry::Pending(token) = self.panel_geometry {
            self.panel_geometry = SemanticCodePanelGeometry::Existing(resolve(token));
        }
    }

    pub(crate) fn resolve(
        self,
        resolve: impl FnMut(SemanticTransactionNodeRef) -> SemanticNodeId,
    ) -> SemanticCodeLayout {
        let SemanticCodePanelGeometry::Existing(panel_geometry) = self.panel_geometry else {
            unreachable!("Code panel guard materializes with its pending panel path")
        };
        SemanticCodeLayout {
            nodes: self.nodes.map_nodes(resolve),
            metrics: self.metrics,
            panel_geometry,
            source_recipe: self.source_recipe,
        }
    }
}

#[cfg(test)]
mod tests;
