//! Joint-local cue sampling and styled vector targets.

use super::{copy_with_paths, into_local_coordinates, invalid, rotate, RetargetedRig2d};
use crate::{AuthoringError, CompositeEntryHandle, MobjectTarget, Scene, SemanticCopy};
use noon_core::{SemanticNodeId, VectorPath};
use std::{collections::HashMap, ops::Range};

/// Named drawings with matching point correspondence, attached to one target joint.
pub struct RigArtwork2d {
    rig: RetargetedRig2d,
    joint: usize,
    poses: Vec<Vec<[f64; 2]>>,
    cues: Vec<(f32, usize)>,
}

impl RetargetedRig2d {
    /// Prepare a sequence of named drawings in joint-local coordinates. Each
    /// drawing supplies the same ordered set of anchors and control points.
    /// The cue phases use the same units as the joint tracks.
    pub fn artwork_track(
        &self,
        joint: &str,
        poses: Vec<(String, Vec<[f64; 2]>)>,
        cues: &[(f32, String)],
    ) -> Result<RigArtwork2d, AuthoringError> {
        let joint = *self
            .skeleton
            .names
            .get(joint)
            .ok_or_else(|| invalid(format!("unknown artwork joint `{joint}`")))?;
        let point_count = poses.first().map_or(0, |(_, points)| points.len());
        let mut names = HashMap::new();
        for (index, (name, points)) in poses.iter().enumerate() {
            if name.is_empty() || names.insert(name.as_str(), index).is_some() {
                return Err(invalid("2D artwork pose names must be nonempty and unique"));
            }
            if point_count == 0 || points.len() != point_count {
                return Err(invalid(
                    "2D artwork poses require matching nonempty point arrays",
                ));
            }
            if !points
                .iter()
                .flatten()
                .all(|&value| (value as f32).is_finite())
            {
                return Err(AuthoringError::NonFiniteGeometry);
            }
        }
        if cues.is_empty()
            || cues.iter().any(|(phase, _)| !phase.is_finite())
            || cues.windows(2).any(|pair| pair[0].0 >= pair[1].0)
        {
            return Err(invalid(
                "2D artwork cues require finite, strictly increasing phases",
            ));
        }
        let cues = cues
            .iter()
            .map(|(phase, name)| {
                let index = names
                    .get(name.as_str())
                    .ok_or_else(|| invalid(format!("unknown artwork pose `{name}`")))?;
                Ok((*phase, *index))
            })
            .collect::<Result<_, AuthoringError>>()?;
        Ok(RigArtwork2d {
            rig: self.clone(),
            joint,
            cues,
            poses: poses.into_iter().map(|(_, points)| points).collect(),
        })
    }
}

impl RigArtwork2d {
    /// Interpolate the named drawings locally, then place the points in scene
    /// space using the exact sampled joint pose. Cue endpoints clamp.
    pub fn points_at(&self, phase: f32) -> Result<Vec<[f64; 2]>, AuthoringError> {
        let joint = self.rig.pose(phase)?.posed[self.joint];
        let upper = self.cues.partition_point(|(key, _)| *key <= phase);
        let (start, from) = self.cues[upper.saturating_sub(1)];
        let (end, to) = self.cues[upper.min(self.cues.len() - 1)];
        let progress = if from == to {
            0.0
        } else {
            (f64::from(phase) - f64::from(start)) / (f64::from(end) - f64::from(start))
        };
        let points = self.poses[from].iter().zip(&self.poses[to]).map(|(a, b)| {
            let local = std::array::from_fn(|axis| a[axis] + (b[axis] - a[axis]) * progress);
            let [x, y] = rotate(local, joint.heading);
            let point = [joint.position[0] + x, joint.position[1] + y];
            if !point.iter().all(|&value| (value as f32).is_finite()) {
                return Err(AuthoringError::NonFiniteGeometry);
            }
            Ok(point)
        });
        points.collect()
    }
}

/// A vector artwork cue track. The first drawing supplies the target hierarchy,
/// presentation, and styles. The paths of every drawing are captured at preparation.
pub struct RigArtworkTarget2d {
    track: RigArtwork2d,
    template: CompositeEntryHandle,
    layout: ArtworkLayout,
    paths: HashMap<SemanticNodeId, ArtworkPath>,
}

struct ArtworkPath {
    path: VectorPath,
    range: Range<usize>,
}

type ArtworkLayout = Vec<(SemanticNodeId, Vec<usize>)>;

struct ArtworkDrawing {
    paths: Vec<Option<ArtworkPath>>,
    points: Vec<[f64; 2]>,
}

impl RetargetedRig2d {
    /// Prepare named joint-local vector drawings for ordinary Transform targets.
    /// Each drawing must have corresponding ordered members and path controls.
    /// All drawings belong to the same store as the sampled targets.
    pub fn artwork_track_for(
        &self,
        joint: &str,
        poses: &[(String, MobjectTarget<'_>)],
        cues: &[(f32, String)],
    ) -> Result<RigArtworkTarget2d, AuthoringError> {
        let Some(((first_name, first), other_poses)) = poses.split_first() else {
            return Err(invalid("2D artwork requires at least one vector drawing"));
        };
        let store = first.integration_store();
        let template = CompositeEntryHandle::from_node(store.clone(), first.node_id())?;
        let layout = artwork_layout(*first)?;
        let first = artwork_drawing(*first, &layout)?;
        let mut drawings = vec![(first_name.clone(), first.points)];
        for (name, pose) in other_poses {
            pose.require_store(store)?;
            let other_layout = artwork_layout(*pose)?;
            if !layout
                .iter()
                .map(|(_, members)| members)
                .eq(other_layout.iter().map(|(_, members)| members))
            {
                return Err(invalid(format!(
                    "2D artwork pose `{name}` must match the ordered members of `{first_name}`"
                )));
            }
            let drawing = artwork_drawing(*pose, &other_layout)?;
            let matches = first
                .paths
                .iter()
                .zip(&drawing.paths)
                .all(|(a, b)| match (a, b) {
                    (Some(a), Some(b)) => a.corresponds(b),
                    (None, None) => true,
                    _ => false,
                });
            if !matches {
                return Err(invalid(format!(
                    "2D artwork pose `{name}` must match the path contours of `{first_name}`"
                )));
            }
            drawings.push((name.clone(), drawing.points));
        }
        let track = self.artwork_track(joint, drawings, cues)?;
        let paths = layout
            .iter()
            .zip(first.paths)
            .filter_map(|((id, _), path)| path.map(|path| (*id, path)))
            .collect();
        Ok(RigArtworkTarget2d {
            track,
            template,
            layout,
            paths,
        })
    }
}

impl RigArtworkTarget2d {
    /// Sample a styled detached vector target at the clip phase.
    pub fn target_at(
        &self,
        scene: &mut Scene,
        phase: f32,
    ) -> Result<SemanticCopy<CompositeEntryHandle>, AuthoringError> {
        self.target_at_with_references(scene, phase, &[])
    }

    /// Carry detached wrapper metadata through the same semantic copy.
    pub fn target_at_with_references(
        &self,
        scene: &mut Scene,
        phase: f32,
        references: &[MobjectTarget<'_>],
    ) -> Result<SemanticCopy<CompositeEntryHandle>, AuthoringError> {
        let source = self.template.as_target();
        source.require_store(scene.integration_store())?;
        if artwork_layout(source)? != self.layout {
            return Err(invalid(
                "2D artwork template membership changed after preparation",
            ));
        }
        let points = self.track.points_at(phase)?;
        copy_with_paths(
            scene,
            source,
            references,
            CompositeEntryHandle::from_node,
            |object, state| {
                let Some(shape) = self.paths.get(&object.node_id()) else {
                    return Ok(None);
                };
                let transform = state
                    .transform
                    .as_planar()
                    .filter(|transform| transform.scale.x != 0.0 && transform.scale.y != 0.0)
                    .ok_or_else(|| {
                        invalid("2D artwork requires an invertible planar template transform")
                    })?;
                let mut points = points[shape.range.clone()].iter().copied();
                let path = shape.path.map_xy(|_, _| {
                    into_local_coordinates(
                        transform,
                        points.next().expect("stored point correspondence"),
                    )
                });
                if !path.is_finite() {
                    return Err(AuthoringError::NonFiniteGeometry);
                }
                Ok(Some(path))
            },
        )
    }
}

impl ArtworkPath {
    fn corresponds(&self, other: &Self) -> bool {
        self.range.len() == other.range.len()
            && self.path.manim_points().is_some() == other.path.manim_points().is_some()
            && self
                .path
                .commands()
                .iter()
                .map(std::mem::discriminant)
                .eq(other.path.commands().iter().map(std::mem::discriminant))
    }
}

fn artwork_layout(source: MobjectTarget<'_>) -> Result<ArtworkLayout, AuthoringError> {
    let nodes = source.family_nodes()?;
    let indices: HashMap<_, _> = nodes.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let store = source.integration_store().borrow();
    nodes
        .into_iter()
        .map(|id| {
            let node = store.semantic_authoring_node_checked(id)?;
            Ok((
                id,
                node.members_iter().map(|member| indices[&member]).collect(),
            ))
        })
        .collect()
}

fn artwork_drawing(
    source: MobjectTarget<'_>,
    layout: &ArtworkLayout,
) -> Result<ArtworkDrawing, AuthoringError> {
    let store = source.integration_store().borrow();
    let mut points = Vec::new();
    let paths = layout
        .iter()
        .map(|(id, _)| {
            let node = store.semantic_authoring_node_checked(*id)?;
            let Some(state) = node.semantic_object_state() else {
                return Ok(None);
            };
            let transform = state
                .transform
                .as_planar()
                .ok_or_else(|| invalid("2D artwork drawings require planar transforms"))?;
            let path = crate::path_queries::content_path(&store, state.content)?.into_owned();
            if path.morph_target().is_some() {
                return Err(invalid("2D artwork drawings require ordinary vector paths"));
            }
            let start = points.len();
            // Visit the same controls in the same order used to rebuild the sampled path.
            let _ = path.map_xy(|x, y| {
                let (local_x, local_y) = transform.transform_xy(x, y);
                points.push([local_x, local_y]);
                (x, y)
            });
            Ok(Some(ArtworkPath {
                path,
                range: start..points.len(),
            }))
        })
        .collect::<Result<_, AuthoringError>>()?;
    Ok(ArtworkDrawing { paths, points })
}
