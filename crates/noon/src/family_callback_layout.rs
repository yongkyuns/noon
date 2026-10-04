//! Shared effective-layout replacement for callback phase overlays.

use crate::{
    AuthoringError, ExecutionSessionCallbackError, LayoutAnchor, SemanticNodeId, Transform2D,
};
use noon_core::{Rect, SceneRevision, SemanticLoweringError, SemanticVec3};
use std::collections::BTreeMap;

/// One fully staged callback replacement row. The caller publishes all rows only
/// after the complete operation has succeeded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CallbackLayoutChange {
    pub node: SemanticNodeId,
    pub transform: Transform2D,
    pub bounds: Option<Rect>,
}

#[derive(Debug)]
pub enum CallbackLayoutError {
    Authoring(AuthoringError),
    Callback(ExecutionSessionCallbackError),
    Family(crate::FamilyCallbackPaintError),
    Empty,
    MissingBounds(SemanticNodeId),
}

impl std::fmt::Display for CallbackLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(error) => error.fmt(f),
            Self::Callback(error) => error.fmt(f),
            Self::Family(error) => error.fmt(f),
            Self::Empty => f.write_str("callback layout anchor has no semantic leaves"),
            Self::MissingBounds(node) => write!(f, "callback layout has no bounds for {node:?}"),
        }
    }
}
impl std::error::Error for CallbackLayoutError {}
impl From<AuthoringError> for CallbackLayoutError {
    fn from(error: AuthoringError) -> Self {
        Self::Authoring(error)
    }
}
impl From<ExecutionSessionCallbackError> for CallbackLayoutError {
    fn from(error: ExecutionSessionCallbackError) -> Self {
        Self::Callback(error)
    }
}
impl From<crate::FamilyCallbackPaintError> for CallbackLayoutError {
    fn from(error: crate::FamilyCallbackPaintError) -> Self {
        Self::Family(error)
    }
}
impl From<SemanticLoweringError> for CallbackLayoutError {
    fn from(error: SemanticLoweringError) -> Self {
        Self::Authoring(error.into())
    }
}

fn addressed_nodes(
    anchor: &LayoutAnchor,
    revision: SceneRevision,
) -> Result<Vec<SemanticNodeId>, CallbackLayoutError> {
    let node = anchor.resolve()?;
    let store = anchor.integration_store().borrow();
    if matches!(
        store.node(node).map(|item| item.kind()),
        Some(noon_core::SemanticNodeKind::Family(_))
    ) {
        Ok(
            crate::MobjectFamily::from_node(std::rc::Rc::clone(anchor.integration_store()), node)?
                .callback_leaf_nodes(revision)?,
        )
    } else {
        Ok(vec![node])
    }
}

fn union_bounds(
    nodes: &[SemanticNodeId],
    rows: &BTreeMap<SemanticNodeId, (Transform2D, Option<Rect>)>,
) -> Result<Rect, CallbackLayoutError> {
    let mut bounds: Option<Rect> = None;
    for node in nodes {
        let (_, row_bounds) = rows
            .get(node)
            .copied()
            .ok_or(ExecutionSessionCallbackError::UnknownObject(*node))?;
        let next = row_bounds.ok_or(CallbackLayoutError::MissingBounds(*node))?;
        bounds = Some(match bounds {
            None => next,
            Some(mut value) => {
                value.min.x = value.min.x.min(next.min.x);
                value.min.y = value.min.y.min(next.min.y);
                value.max.x = value.max.x.max(next.max.x);
                value.max.y = value.max.y.max(next.max.y);
                value
            }
        });
    }
    bounds.ok_or(CallbackLayoutError::Empty)
}

/// Prepare a replace/stretch from phase-effective rows. Both anchors may address
/// ordinary objects or families. No overlay write is produced until every row,
/// factor, and transformed bound has passed validation.
pub fn prepare_callback_layout_replace(
    source: &LayoutAnchor,
    target: &LayoutAnchor,
    revision: SceneRevision,
    dimension: crate::LayoutDimension,
    stretch: bool,
    mut read: impl FnMut(
        SemanticNodeId,
    ) -> Result<(Transform2D, Option<Rect>), ExecutionSessionCallbackError>,
) -> Result<Vec<CallbackLayoutChange>, CallbackLayoutError> {
    if !std::rc::Rc::ptr_eq(source.integration_store(), target.integration_store()) {
        return Err(AuthoringError::ForeignStore.into());
    }
    let source_nodes = addressed_nodes(source, revision)?;
    let target_nodes = addressed_nodes(target, revision)?;
    let mut rows = BTreeMap::new();
    for node in source_nodes.iter().chain(&target_nodes).copied() {
        if let std::collections::btree_map::Entry::Vacant(entry) = rows.entry(node) {
            entry.insert(read(node)?);
        }
    }
    let source_bounds = union_bounds(&source_nodes, &rows)?;
    let target_bounds = union_bounds(&target_nodes, &rows)?;
    let (sx, sy) = if stretch {
        let sx = if source_bounds.width() == 0.0 {
            1.0
        } else {
            f64::from(target_bounds.width() / source_bounds.width())
        };
        let sy = if source_bounds.height() == 0.0 {
            1.0
        } else {
            f64::from(target_bounds.height() / source_bounds.height())
        };
        (sx, sy)
    } else {
        let (from, to) = match dimension {
            crate::LayoutDimension::Width => (source_bounds.width(), target_bounds.width()),
            crate::LayoutDimension::Height => (source_bounds.height(), target_bounds.height()),
        };
        let factor = if from == 0.0 {
            1.0
        } else {
            f64::from(to / from)
        };
        (factor, factor)
    };
    crate::semantic_mobject::authoring_render_f64("callback replace scale.x", sx)?;
    crate::semantic_mobject::authoring_render_f64("callback replace scale.y", sy)?;
    let source_center = (
        (f64::from(source_bounds.min.x) + f64::from(source_bounds.max.x)) * 0.5,
        (f64::from(source_bounds.min.y) + f64::from(source_bounds.max.y)) * 0.5,
    );
    let target_center = (
        (f64::from(target_bounds.min.x) + f64::from(target_bounds.max.x)) * 0.5,
        (f64::from(target_bounds.min.y) + f64::from(target_bounds.max.y)) * 0.5,
    );
    let mut changes = Vec::with_capacity(source_nodes.len());
    for node in source_nodes {
        let (previous, bounds) = rows[&node];
        let (local_x, local_y) =
            crate::dimension_fit::world_scale_factors(f64::from(previous.rotation), sx, sy)?;
        let scale = SemanticVec3::new(
            f64::from(previous.scale.x) * local_x,
            f64::from(previous.scale.y) * local_y,
            0.0,
        )
        .lower_xy_f32()?;
        let translation = SemanticVec3::new(
            target_center.0 + (f64::from(previous.translation.x) - source_center.0) * sx,
            target_center.1 + (f64::from(previous.translation.y) - source_center.1) * sy,
            0.0,
        )
        .lower_xy_f32()?;
        let transform = Transform2D {
            translation,
            scale,
            ..previous
        };
        let bounds = bounds
            .map(|rect| scale_bounds(rect, source_center, target_center, sx, sy))
            .transpose()?;
        changes.push(CallbackLayoutChange {
            node,
            transform,
            bounds,
        });
    }
    Ok(changes)
}

fn scale_bounds(
    bounds: Rect,
    source: (f64, f64),
    target: (f64, f64),
    sx: f64,
    sy: f64,
) -> Result<Rect, SemanticLoweringError> {
    let point = |x: f32, y: f32| -> Result<noon_core::Vec2, SemanticLoweringError> {
        SemanticVec3::new(
            target.0 + (f64::from(x) - source.0) * sx,
            target.1 + (f64::from(y) - source.1) * sy,
            0.0,
        )
        .lower_xy_f32()
    };
    let points = [
        point(bounds.min.x, bounds.min.y)?,
        point(bounds.min.x, bounds.max.y)?,
        point(bounds.max.x, bounds.min.y)?,
        point(bounds.max.x, bounds.max.y)?,
    ];
    let mut min = points[0];
    let mut max = points[0];
    for point in points.into_iter().skip(1) {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    Ok(Rect::new(min, max))
}
