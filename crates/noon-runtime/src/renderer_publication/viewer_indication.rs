//! Presentation-only Indicate samples borrowed by the typed renderer.
use noon_core::{Color, ObjectId, SemanticCamera3D, SemanticVec3, Style, Transform2D, Vec2};

use crate::{FrameChanges, FrameState, RendererPublication, SceneInstance};

/// One restoring viewer effect sampled independently of the authored timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewerIndication {
    pub object: ObjectId,
    pub scale: f64,
    pub color: Color,
    pub strength: f32,
    pub center: Option<SemanticVec3>,
}

impl ViewerIndication {
    fn scale_about(self, value: f64, center: f64) -> f64 {
        center + (value - center) * self.scale
    }

    pub fn planar_pose(self, mut pose: Transform2D) -> Transform2D {
        let scale = pose.scale * self.scale as f32;
        let translation = self.center.map_or(pose.translation, |center| {
            Vec2::new(
                self.scale_about(f64::from(pose.translation.x), center.x) as f32,
                self.scale_about(f64::from(pose.translation.y), center.y) as f32,
            )
        });
        if [scale.x, scale.y, translation.x, translation.y]
            .into_iter()
            .all(f32::is_finite)
        {
            pose.scale = scale;
            pose.translation = translation;
        }
        pose
    }

    pub fn scale_spatial_pose(
        self,
        translation: &mut [f64; 3],
        scale: &mut [f64; 3],
        anchor: &mut Option<[f64; 3]>,
    ) {
        let translate = |point| {
            self.center.map_or(point, |center| {
                let [x, y, z] = point;
                [
                    self.scale_about(x, center.x),
                    self.scale_about(y, center.y),
                    self.scale_about(z, center.z),
                ]
            })
        };
        let next_translation = translate(*translation);
        let next_scale = scale.map(|value| value * self.scale);
        let next_anchor = anchor.map(translate);
        if next_scale
            .iter()
            .chain(&next_translation)
            .chain(next_anchor.iter().flatten())
            .all(|value| value.is_finite())
        {
            *translation = next_translation;
            *scale = next_scale;
            *anchor = next_anchor;
        }
    }

    pub fn tint(self, style: &mut Style, has_fill_paint: bool, is_image: bool) {
        // An image's fill encodes alpha. Give it geometric feedback only.
        if is_image {
            return;
        }
        let mix = |from, to| from * (1.0 - self.strength) + to * self.strength;
        let recolor = |color: Color| {
            Color::rgba(
                mix(color.red, self.color.red),
                mix(color.green, self.color.green),
                mix(color.blue, self.color.blue),
                color.alpha,
            )
        };
        if !has_fill_paint {
            style.fill = style.fill.map(recolor);
        }
        style.stroke = style.stroke.map(recolor);
    }
}

/// Render-only history of the last submitted samples, for sparse restoration.
/// Authored and effective values are always read fresh from the runtime.
#[derive(Clone, Debug, Default)]
pub struct ViewerPresentation {
    previous: Vec<ViewerIndication>,
    camera: Option<(usize, SemanticCamera3D)>,
}

impl ViewerPresentation {
    pub fn row_changes(
        &self,
        current: &[ViewerIndication],
        camera: Option<(usize, SemanticCamera3D)>,
        mut object_index: impl FnMut(ObjectId) -> Option<usize>,
        frame: &FrameState,
    ) -> FrameChanges {
        let mut indices = Vec::new();
        if self.previous != current {
            indices.extend(
                self.previous
                    .iter()
                    .chain(current)
                    .filter_map(|sample| object_index(sample.object)),
            );
        }
        if self.camera != camera {
            indices.extend(
                self.camera
                    .into_iter()
                    .chain(camera)
                    .filter_map(|(index, _)| {
                        frame.objects.get(index)?.camera_projection().map(|_| index)
                    }),
            );
        }
        FrameChanges::objects(indices)
    }

    pub fn presented(
        &mut self,
        current: &[ViewerIndication],
        camera: Option<(usize, SemanticCamera3D)>,
    ) {
        self.previous.clear();
        self.previous.extend_from_slice(current);
        self.camera = camera;
    }
}

/// Only the borrowed frame storage moves. The GPU visitor can see the scoped
/// presentation, but cannot access or mutate the runtime. Drop restores just
/// the rows we replaced, including if renderer preparation fails or unwinds.
struct ViewerFrame<'a> {
    runtime: &'a mut SceneInstance,
    frame: FrameState,
    canonical: Vec<(usize, crate::FrameObjectState, Option<Transform2D>)>,
}

impl Drop for ViewerFrame<'_> {
    fn drop(&mut self) {
        for (index, object, render_transform) in self.canonical.drain(..).rev() {
            self.frame.objects[index] = object;
            self.frame.render_transforms[index] = render_transform;
        }
        std::mem::swap(&mut self.runtime.frame, &mut self.frame);
    }
}

impl SceneInstance {
    fn show_viewer_samples<T>(
        &mut self,
        samples: &[ViewerIndication],
        camera: Option<(usize, SemanticCamera3D)>,
        changes: FrameChanges,
        viewer_rows_changed: bool,
        draw: impl FnOnce(RendererPublication<'_>) -> T,
    ) -> T {
        if samples.is_empty() && camera.is_none() {
            let mut publication = self.publication_with_changes(changes);
            publication.viewer_rows_changed = viewer_rows_changed;
            return draw(publication);
        }
        let frame = std::mem::take(&mut self.frame);
        let mut view = ViewerFrame {
            runtime: self,
            frame,
            canonical: Vec::with_capacity(samples.len() + usize::from(camera.is_some())),
        };
        for &sample in samples {
            let Some(index) = view.runtime.frame_index_for_object(sample.object) else {
                continue;
            };
            let object = &mut view.frame.objects[index];
            let render_pose = &mut view.frame.render_transforms[index];
            view.canonical.push((index, object.clone(), *render_pose));
            if let Some(spatial) = object.spatial.as_mut().filter(|spatial| {
                spatial.composition_domain
                    != noon_core::SemanticSpatialCompositionDomain::FixedFrame
            }) {
                let xyz = |v: SemanticVec3| [v.x, v.y, v.z];
                let vec = |[x, y, z]: [f64; 3]| SemanticVec3::new(x, y, z);
                let mut translation = xyz(spatial.world.translation);
                let mut scale = xyz(spatial.world.scale);
                let mut anchor = spatial.fixed_orientation_center.map(xyz);
                sample.scale_spatial_pose(&mut translation, &mut scale, &mut anchor);
                spatial.world.translation = vec(translation);
                spatial.world.scale = vec(scale);
                spatial.fixed_orientation_center = anchor.map(vec);
            } else {
                object.transform = sample.planar_pose(object.transform);
                *render_pose = render_pose.map(|pose| sample.planar_pose(pose));
            }
            sample.tint(
                &mut object.style,
                object.fill_paint.is_some(),
                object.content.image().is_some(),
            );
        }
        if let Some((index, camera)) = camera {
            if let Some(row) = view
                .frame
                .objects
                .get_mut(index)
                .filter(|row| row.camera_projection().is_some())
            {
                view.canonical
                    .push((index, row.clone(), view.frame.render_transforms[index]));
                let spatial = row
                    .spatial
                    .as_mut()
                    .expect("camera projection has a spatial row");
                spatial.world.translation = camera.position;
                spatial.world.rotation = camera.orientation;
                spatial.camera_projection = Some(camera.projection);
            }
        }
        let mut publication = view.runtime.publication_for_frame(&view.frame, changes);
        publication.viewer_indications = samples;
        publication.viewer_rows_changed = viewer_rows_changed;
        draw(publication)
    }

    /// Submit a borrowed, render-only view of the selected effective frame.
    /// The normal publication is used for exports, visibility and picking.
    pub fn with_viewer_renderer_publication<T>(
        &mut self,
        samples: &[ViewerIndication],
        camera: Option<(usize, SemanticCamera3D)>,
        previous: &ViewerPresentation,
        followup_redraw: bool,
        draw: impl FnOnce(RendererPublication<'_>) -> T,
    ) -> T {
        let mut changes = self.take_frame_changes();
        let viewer_changes = previous.row_changes(
            samples,
            camera,
            |id| self.frame_index_for_object(id),
            &self.frame,
        );
        let viewer_rows_changed = !viewer_changes.is_empty();
        changes.accumulate(viewer_changes);
        if followup_redraw {
            self.changes = FrameChanges::presentation_redraw();
        }
        self.show_viewer_samples(samples, camera, changes, viewer_rows_changed, draw)
    }
}
