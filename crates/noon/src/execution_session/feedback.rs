use noon_core::{
    GeometryRef, GeometryResource, GeometryResourceLookup, Rect, SemanticStore, SourceSite,
    TextResourceLookup, Vec2,
};
use noon_runtime::frame_object_conservative_bounds_with_resources;

use super::{ExecutionSession, PointerFrameSnapshot};

const MAX_ROWS: usize = 512;
const MAX_PATH_COMMANDS: usize = 128;
const MAX_TARGETS: usize = 4;

#[derive(Debug, PartialEq, Eq)]
pub struct FeedbackTarget {
    pub kind: &'static str,
    pub text: Option<String>,
    pub source: Option<SourceSite>,
}

#[derive(Debug, Default)]
pub struct FeedbackTargets {
    pub targets: Vec<FeedbackTarget>,
    pub limited: bool,
}

impl ExecutionSession {
    /// Observe a normalized rectangle on the selected planar frame. Traverse a
    /// bounded painter prefix from the top without rebuilding a dirty index.
    pub fn feedback_targets(
        &self,
        displayed: &PointerFrameSnapshot,
        region: [f32; 4],
        source: &SemanticStore,
    ) -> Option<FeedbackTargets> {
        let [x, y, width, height] = region;
        if !region.iter().all(|v| v.is_finite() && *v >= 0.0)
            || width <= 0.0
            || height <= 0.0
            || x + width > 1.0
            || y + height > 1.0
        {
            return None;
        }
        let view = displayed.view();
        displayed.validate_presentation(self, view).ok()?;
        let viewport = view.viewport();
        let a = displayed
            .position(Vec2::new(x * viewport.x, y * viewport.y))
            .ok()?
            .scene();
        let b = displayed
            .position(Vec2::new(
                (x + width) * viewport.x,
                (y + height) * viewport.y,
            ))
            .ok()?
            .scene();
        let selection = Rect::new(
            Vec2::new(a.x.min(b.x), a.y.min(b.y)),
            Vec2::new(a.x.max(b.x), a.y.max(b.y)),
        );
        let frame = self.frame();
        let order = self.painter_order();
        let mut ranked = Vec::with_capacity(MAX_TARGETS + 1);
        let mut limited = order.len() > MAX_ROWS;
        let geometry_resources = self.runtime.geometry_resources();
        for &index in order.iter().rev().take(MAX_ROWS) {
            let index = index as usize;
            let row = &frame.objects[index];
            if !frame.is_present(index)
                || !row.draw_enabled
                || row.style.opacity <= 0.0
                || row.appearance <= 0.0
                || row.spatial.is_some()
                || Some(row.id) == self.camera_object
                || row.content.composed_source().is_some()
            {
                continue;
            }
            let geometry = frame.render_geometry(index);
            let path = match geometry {
                Some(GeometryRef::VectorPath(path)) => Some(path.as_ref()),
                Some(GeometryRef::External(id)) => geometry_resources
                    .current_handle(*id)
                    .and_then(|handle| geometry_resources.get(handle))
                    .and_then(|resource| match resource {
                        GeometryResource::VectorPath(path) => Some(path.as_ref()),
                        GeometryResource::Mesh(_) => None,
                    }),
                _ => None,
            };
            if path.is_some_and(|path| path.commands().len() > MAX_PATH_COMMANDS) {
                limited = true;
                continue;
            }
            let Some(bounds) =
                frame_object_conservative_bounds_with_resources(frame, index, geometry_resources)
            else {
                continue;
            };
            let overlap = Rect::new(
                Vec2::new(
                    bounds.min.x.max(selection.min.x),
                    bounds.min.y.max(selection.min.y),
                ),
                Vec2::new(
                    bounds.max.x.min(selection.max.x),
                    bounds.max.y.min(selection.max.y),
                ),
            );
            let overlap_area = area(overlap);
            if overlap_area <= 0.0 {
                continue;
            }
            // Intersection over union favors objects that fit the marked area.
            // Equal scores retain the frame's topmost-first ordering.
            let score = overlap_area / (area(bounds) + area(selection) - overlap_area);
            let rank = ranked.partition_point(|(existing, _)| *existing >= score);
            if rank >= MAX_TARGETS {
                continue;
            }
            let kind = match geometry {
                Some(GeometryRef::Circle { .. }) => "Circle",
                Some(GeometryRef::Rectangle { .. }) => "Rectangle",
                Some(GeometryRef::Line { .. }) => "Line",
                Some(_) => "Path",
                None if frame.text(index).is_some() => "Text",
                None => "Image",
            };
            let hint = FeedbackTarget {
                kind,
                text: frame
                    .text(index)
                    .and_then(|handle| self.text_resources().get(handle))
                    .map(|text| {
                        text.source
                            .chars()
                            .take(96)
                            .map(|c| {
                                if c.is_control() || c.is_whitespace() {
                                    ' '
                                } else {
                                    c
                                }
                            })
                            .collect()
                    }),
                source: self
                    .interaction_semantic_object_id(row.id)
                    .and_then(|id| source.source_site(id)),
            };
            if let Some(existing) = ranked.iter().position(|(_, old)| old == &hint) {
                if existing < rank {
                    continue;
                }
                ranked.remove(existing);
            }
            ranked.insert(rank, (score, hint));
            ranked.truncate(MAX_TARGETS);
        }
        Some(FeedbackTargets {
            targets: ranked.into_iter().map(|(_, hint)| hint).collect(),
            limited,
        })
    }
}

fn area(bounds: Rect) -> f64 {
    (f64::from(bounds.max.x) - f64::from(bounds.min.x)).max(0.0)
        * (f64::from(bounds.max.y) - f64::from(bounds.min.y)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_frame_keeps_hints_local_and_bounded() {
        let mut scene = crate::Scene::new();
        let backdrop = scene.rectangle(12.0, 7.0).unwrap();
        scene.add_many(&[(&backdrop).into()]).unwrap();
        for _ in 0..MAX_ROWS {
            let outside = scene.circle(0.15).unwrap();
            scene.shift(&outside, Vec2::new(5.0, 0.0)).unwrap();
            scene.add_many(&[(&outside).into()]).unwrap();
        }
        for line in 1..=6 {
            let circle = scene.circle(0.4).unwrap();
            scene
                .integration_store()
                .borrow_mut()
                .record_source_site(circle.node_id(), line, 20)
                .unwrap();
            scene.add_many(&[(&circle).into()]).unwrap();
        }
        let source = std::rc::Rc::clone(scene.integration_store());
        let mut execution = scene.into_execution().unwrap();
        let session = execution.session_mut();
        let view = session
            .inspection_pointer_view(1, Vec2::new(800.0, 450.0))
            .unwrap();
        let frame = session.capture_pointer_frame(view).unwrap();
        let result = session
            .feedback_targets(&frame, [0.45, 0.43, 0.10, 0.14], &source.borrow())
            .unwrap();
        assert!(result.limited);
        assert_eq!(result.targets.len(), MAX_TARGETS);
        for (hint, line) in result.targets.iter().zip((3..=6).rev()) {
            assert_eq!(hint.kind, "Circle");
            assert_eq!(hint.source.unwrap().line.get(), line);
        }
        assert!(session
            .feedback_targets(&frame, [0.95, 0.4, 0.1, 0.1], &source.borrow())
            .is_none());
    }
}
