//! Compatibility re-export of the authoritative renderer-independent graph topology.
//!
//! Graph identity, insertion order, tombstoned removal, and incident-edge indexing
//! are owned by noon-core so semantic declarations and authoring use one topology
//! implementation rather than parallel models.

pub use noon_core::{GraphEdge, GraphEdgeId, GraphTopology, GraphTopologyError, GraphVertexId};
