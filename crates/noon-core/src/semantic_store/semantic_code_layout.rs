//! Sparse allocated Code rows on ordinary semantic family roots.
//!
//! Metrics remain in the admitted panel's local frame. The geometry handle is
//! only an identity guard for that frame; the panel object owns its resource.

use crate::{
    GeometryResourceHandle, SemanticLocalResourceToken, SemanticNodeId, SemanticTransactionNodeRef,
};

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
        }
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
        }
    }

    pub fn map_existing(
        layout: SemanticCodeLayout,
        map: impl FnMut(SemanticNodeId) -> SemanticTransactionNodeRef,
    ) -> Self {
        Self::new(
            layout.nodes.map_nodes(map),
            layout.metrics,
            layout.panel_geometry,
        )
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
        SemanticCodeLayout::new(self.nodes.map_nodes(resolve), self.metrics, panel_geometry)
    }
}

#[cfg(test)]
mod tests;
