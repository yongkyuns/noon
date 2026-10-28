//! Typed candidate queries borrowed from the execution-owned spatial index.
use super::*;

/// Ephemeral broad-phase query product. It contains only existing identities;
/// filtering/composed-source expansion remains renderer preparation work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PainterPrefixGeometryQuery {
    pub objects: Vec<noon_core::ObjectId>,
    pub stats: crate::SpatialQueryStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PainterPrefixGeometryQueryError {
    MissingSpatialIndex,
    InvalidBounds,
    InvalidAnchor(noon_core::ObjectId),
    StaleSpatialEntry(noon_core::ObjectId),
}

impl RendererPublication<'_> {
    /// Query raw geometry in an expanded world window, strictly before an
    /// existing physical painter anchor. This is not the complete filtered
    /// prefix: the renderer must add earlier effect contributions and complete
    /// isolated intervals before using it as a capture selection.
    pub fn query_painter_prefix_geometry(
        &self,
        bounds: Rect,
        anchor: noon_core::ObjectId,
    ) -> Result<PainterPrefixGeometryQuery, PainterPrefixGeometryQueryError> {
        use PainterPrefixGeometryQueryError as Error;
        if ![bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
            .into_iter()
            .all(f32::is_finite)
            || bounds.min.x >= bounds.max.x
            || bounds.min.y >= bounds.max.y
        {
            return Err(Error::InvalidBounds);
        }
        let anchor_row = self
            .object_index(anchor)
            .ok_or(Error::InvalidAnchor(anchor))?;
        let row = &self.frame.objects[anchor_row];
        if row.content.composed_source().is_some()
            || row.spatial.is_some()
            || !self.frame.is_present(anchor_row)
        {
            return Err(Error::InvalidAnchor(anchor));
        }
        let anchor_rank = self
            .painter_rank(anchor_row)
            .ok_or(Error::InvalidAnchor(anchor))?;
        let index = self.spatial_index.ok_or(Error::MissingSpatialIndex)?;
        let query = index.query_rect(bounds);
        let mut objects = Vec::with_capacity(query.slots().len());
        for &slot in query.slots() {
            let object = index
                .object_for_slot(slot)
                .expect("query references its borrowed index");
            let row = self
                .object_index(object)
                .ok_or(Error::StaleSpatialEntry(object))?;
            let rank = self
                .painter_rank(row)
                .ok_or(Error::StaleSpatialEntry(object))?;
            if rank < anchor_rank {
                objects.push((rank, object));
            }
        }
        objects.sort_unstable_by_key(|&(rank, _)| rank);
        let objects = objects
            .into_iter()
            .map(|(_, object)| object)
            .collect::<Vec<_>>();
        let mut stats = query.stats();
        stats.results = objects.len();
        Ok(PainterPrefixGeometryQuery { objects, stats })
    }
}
