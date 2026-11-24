//! Rebind graph wrappers after the ordinary semantic family copy. Geometry,
//! paint, topology, and endpoint dependencies all use that single copy path.

use super::*;
use crate::{integration::graph::HostGraph, SemanticCopy};

impl<K: Clone + Eq + Hash> RetainedGraph<K> {
    fn rebind_copy<Root>(&self, copy: &SemanticCopy<Root>) -> Result<Self, GraphAuthoringError> {
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

impl HostGraph {
    /// Rebind the host wrapper to the nodes in an existing family copy.
    pub fn rebind_copy<Root>(
        &self,
        copy: &SemanticCopy<Root>,
    ) -> Result<Self, crate::integration::AuthoringFailure> {
        match self {
            Self::Undirected(graph) => graph
                .inner
                .rebind_copy(copy)
                .map(|inner| Self::Undirected(Graph { inner })),
            Self::Directed(graph) => graph
                .inner
                .rebind_copy(copy)
                .map(|inner| Self::Directed(DiGraph { inner })),
        }
        .map_err(|error| crate::integration::AuthoringFailure::unclassified("graph.copy", &error))
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

    /// Copy geometry, appearance and topology through the owning Scene.
    pub fn copy_in_scene(&self, scene: &mut Scene) -> Result<Self, GraphAuthoringError> {
        let copied = scene.copy_family(self.family())?;
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

    /// Copy geometry, appearance and topology through the owning Scene.
    pub fn copy_in_scene(&self, scene: &mut Scene) -> Result<Self, GraphAuthoringError> {
        let copied = scene.copy_family(self.family())?;
        Ok(Self {
            inner: self.inner.rebind_copy(&copied)?,
        })
    }
}
