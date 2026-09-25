//! Rebind graph wrappers after the ordinary semantic family copy. Geometry,
//! paint, topology, and endpoint dependencies all use that single copy path.

use super::*;
use crate::FamilyCopy;

impl<K: Clone + Eq + Hash> RetainedGraph<K> {
    fn rebind_copy(&self, copy: &FamilyCopy) -> Result<Self, GraphAuthoringError> {
        let vertices = self
            .vertices
            .iter()
            .map(|entry| {
                Ok(GraphVertexEntry {
                    key: entry.key.clone(),
                    id: entry.id,
                    object: copy.mobject(&entry.object)?,
                })
            })
            .collect::<Result<_, GraphAuthoringError>>()?;
        let edges = self
            .edges
            .iter()
            .map(|entry| {
                let object = match &entry.object {
                    GraphEdgeMobject::Line { family, line } => GraphEdgeMobject::Line {
                        family: copy.family(family)?,
                        line: copy.mobject(line)?,
                    },
                    GraphEdgeMobject::Arrow(arrow) => {
                        GraphEdgeMobject::Arrow(copy.rebind_manim_arrow(arrow)?)
                    }
                };
                Ok(GraphEdgeEntry {
                    edge: entry.edge,
                    object,
                })
            })
            .collect::<Result<_, GraphAuthoringError>>()?;
        Ok(Self {
            directed: self.directed,
            family: copy.family(&self.family)?,
            vertices,
            vertex_lookup: self.vertex_lookup.clone(),
            edges,
            edge_lookup: self.edge_lookup.clone(),
            options: self.options.clone(),
        })
    }
}

impl<K: Clone + Eq + Hash> Graph<K> {
    /// Copy retained geometry, appearance, and topology into independent IDs.
    pub fn copy(&self) -> Result<Self, GraphAuthoringError> {
        let copied = self.family().copy_family()?;
        Ok(Self {
            inner: self.inner.rebind_copy(&copied)?,
        })
    }

    /// Copy the current effective state through the existing live publisher.
    pub fn copy_live(
        &self,
        live: &mut crate::LiveSession<'_>,
    ) -> Result<Self, GraphAuthoringError> {
        let copied = live.copy_family(self.family())?;
        Ok(Self {
            inner: self.inner.rebind_copy(&copied)?,
        })
    }
}

impl<K: Clone + Eq + Hash> DiGraph<K> {
    /// Copy retained geometry, appearance, and topology into independent IDs.
    pub fn copy(&self) -> Result<Self, GraphAuthoringError> {
        let copied = self.family().copy_family()?;
        Ok(Self {
            inner: self.inner.rebind_copy(&copied)?,
        })
    }

    /// Copy the current effective state through the existing live publisher.
    pub fn copy_live(
        &self,
        live: &mut crate::LiveSession<'_>,
    ) -> Result<Self, GraphAuthoringError> {
        let copied = live.copy_family(self.family())?;
        Ok(Self {
            inner: self.inner.rebind_copy(&copied)?,
        })
    }
}
