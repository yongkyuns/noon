use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

use noon_core::{
    ReactiveValue, SemanticNodeId, SemanticObjectProperty, SemanticSignalBindingError,
    SemanticSignalExpr as Expr, SemanticSignalValueKind, SemanticStore, SemanticVec3,
    SignalSamples, ValueKind,
};

use crate::{
    family_copy::SemanticCopy, AuthoringError, FamilyCopy, Mobject, MobjectFamily, MobjectTarget,
    Scene, ValueTracker,
};

/// A joint in a parent-first planar skeleton. Offsets are in parent coordinates
/// (scene coordinates for the root), and rest angles are local radians.
#[derive(Clone)]
pub struct Joint2d {
    pub name: String,
    pub parent: Option<String>,
    pub offset: [f64; 2],
    pub rest_angle: f64,
}

/// A reusable joint channel. Each key is [tracker value, radians from rest].
/// Angles interpolate as authored, so unwrapped angles can make full turns.
pub struct JointTrack2d {
    pub joint: String,
    pub keys: Vec<[f32; 2]>,
}

/// A rigid drawing attached to a joint. Its origin is offset in joint coordinates
/// and its local +X axis faces along the joint. Its scale and styling are independent.
pub struct JointAttachment2d {
    pub joint: String,
    pub object: Mobject,
    pub offset: [f64; 2],
}

impl Scene {
    /// Bind planar artwork to a named joint hierarchy during scene setup.
    /// Reuse the same tracks on another rig by giving its corresponding joints
    /// the same names. Each rig keeps its own offsets and rest angles. Untracked
    /// joints hold their local rest angle and inherit movement from their parent.
    /// The existing Rust timeline and signal graph evaluate poses during play/seek.
    pub fn bind_rig_2d(
        &mut self,
        phase: &ValueTracker,
        joints: &[Joint2d],
        tracks: &[JointTrack2d],
        attachments: &[JointAttachment2d],
    ) -> Result<(), AuthoringError> {
        self.require_signal_setup()?;
        phase.require_store(self.integration_store())?;
        if attachments.is_empty() {
            return Err(invalid("a 2D rig requires at least one attachment"));
        }
        let names = joint_names(joints)?;
        let parents = joint_parents(joints, &names)?;
        let samples = rotation_samples(&names, tracks, &[])?;

        let store = self.integration_store();
        let mut targets = HashSet::new();
        let mut attachment_joints = Vec::with_capacity(attachments.len());
        for attachment in attachments {
            self.require_object(&attachment.object)?;
            let target = attachment.object.node_id();
            if !targets.insert(target) || !self.contains(&attachment.object)? {
                return Err(invalid(
                    "2D rig attachments must be distinct objects in this Scene",
                ));
            }
            if !attachment
                .offset
                .into_iter()
                .all(|value| (value as f32).is_finite())
            {
                return Err(invalid(
                    "2D rig attachment offsets must be finite and in range",
                ));
            }
            attachment_joints.push(
                *names
                    .get(attachment.joint.as_str())
                    .ok_or_else(|| invalid("a 2D rig attachment names an unknown joint"))?,
            );
            let state = store.borrow();
            for binding in state.semantic_object_signal_bindings(target)? {
                if matches!(
                    binding.property(),
                    SemanticObjectProperty::Translation | SemanticObjectProperty::RotationZ
                ) {
                    return Err(SemanticSignalBindingError::PropertyAlreadyBound {
                        target,
                        property: binding.property(),
                        existing_signal: binding.signal(),
                    }
                    .into());
                }
            }
        }

        let mut store = store.borrow_mut();
        if !store.is_semantic_signal_scoped(self.root(), phase.node_id())
            || store.semantic_signal_value_kind(phase.node_id())? != SemanticSignalValueKind::Scalar
        {
            return Err(invalid("the 2D rig tracker must belong to this Scene"));
        }
        // All input, reference, and property conflicts are checked above. The
        // expressions below always produce a vector for position and a scalar
        // for heading, with dependencies only on previously allocated signals.
        let mut poses = Vec::<(SemanticNodeId, SemanticNodeId)>::with_capacity(joints.len());
        for ((joint, parent), samples) in joints.iter().zip(parents).zip(samples) {
            let mut heading = Expr::scalar(joint.rest_angle);
            if let Some(samples) = samples {
                heading = add(
                    heading,
                    Expr::Sample(Box::new(Expr::signal(phase.node_id())), samples),
                );
            }
            let position = if let Some(parent) = parent {
                let (parent_position, parent_heading) = poses[parent];
                heading = add(Expr::signal(parent_heading), heading);
                offset_from(parent_position, parent_heading, joint.offset)
            } else {
                vector(joint.offset)
            };
            poses.push((derived(&mut store, position), derived(&mut store, heading)));
        }
        for (attachment, joint) in attachments.iter().zip(attachment_joints) {
            let (mut position, heading) = poses[joint];
            if attachment.offset != [0.0, 0.0] {
                position = derived(
                    &mut store,
                    offset_from(position, heading, attachment.offset),
                );
            }
            let target = attachment.object.node_id();
            for (signal, property) in [
                (position, SemanticObjectProperty::Translation),
                (heading, SemanticObjectProperty::RotationZ),
            ] {
                store
                    .bind_semantic_signal(signal, target, property)
                    .expect("validated 2D rig binding");
            }
        }
        Ok(())
    }
}

/// Route one named clip channel to a target joint. A gain of 1 preserves the
/// authored rotation; negative gains can mirror a gesture.
pub struct JointMapping2d {
    pub source: String,
    pub target: String,
    pub gain: f32,
}

/// A rotation clip prepared against the target's own bind skeleton.
pub struct RetargetedRig2d {
    skeleton: Arc<RigSkeleton>,
    samples: Vec<Option<SignalSamples>>,
}

struct RigSkeleton {
    joints: Vec<Joint2d>,
    parents: Vec<Option<usize>>,
    names: HashMap<String, usize>,
    bind: Vec<JointPose>,
}

/// An authored pose used to prepare vector drawings for ordinary transforms.
pub struct RigPose2d {
    skeleton: Arc<RigSkeleton>,
    posed: Vec<JointPose>,
}

#[derive(Clone, Copy)]
struct JointPose {
    position: [f64; 2],
    heading: f64,
}

impl RetargetedRig2d {
    /// Match tracks by name unless they have an explicit mapping. Other
    /// target joints keep their local bind angle and inherit parent motion.
    pub fn new(
        joints: Vec<Joint2d>,
        tracks: &[JointTrack2d],
        mapping: &[JointMapping2d],
    ) -> Result<Self, AuthoringError> {
        let names = joint_names(&joints)?;
        let parents = joint_parents(&joints, &names)?;
        let samples = rotation_samples(&names, tracks, mapping)?;
        let names = names
            .into_iter()
            .map(|(name, index)| (name.to_owned(), index))
            .collect();
        let bind = forward_kinematics(&joints, &parents, |_| 0.0);
        Ok(Self {
            skeleton: Arc::new(RigSkeleton {
                joints,
                parents,
                names,
                bind,
            }),
            samples,
        })
    }

    /// Sample linearly interpolated, unwrapped angles. Phases outside the
    /// clip clamp to its first or last keys, independently for each track.
    pub fn pose(&self, phase: f32) -> Result<RigPose2d, AuthoringError> {
        if !phase.is_finite() {
            return Err(invalid("the 2D rig phase must be finite"));
        }
        let posed =
            forward_kinematics(
                &self.skeleton.joints,
                &self.skeleton.parents,
                |index| match self.samples[index]
                    .as_ref()
                    .map(|samples| samples.sample(phase))
                {
                    Some(ReactiveValue::Scalar(value)) => f64::from(value),
                    None => 0.0,
                    _ => unreachable!("rotation tracks are scalar"),
                },
            );
        Ok(RigPose2d {
            skeleton: Arc::clone(&self.skeleton),
            posed,
        })
    }
}

impl RigPose2d {
    /// The joint's world-space [x, y, heading in radians]. Use this to
    /// position authored drawings such as alternate hand views at a wrist.
    pub fn joint(&self, name: &str) -> Result<[f64; 3], AuthoringError> {
        let JointPose {
            position: [x, y],
            heading,
        } = self.posed[self.index(name)?];
        Ok([x, y, heading])
    }

    /// Prepare a detached vector object as a Transform target.
    /// Landmarks are in scene space. Omit the chain for an unbranched rig.
    pub fn target_for(
        &self,
        scene: &mut Scene,
        source: &Mobject,
        chain: Option<&[&str]>,
    ) -> Result<Mobject, AuthoringError> {
        self.target_copy(scene, source.into(), &[], chain, Mobject::from_node)
            .map(|copy| copy.root().clone())
    }

    /// Prepare a styled copy of a planar vector family with one common chain.
    /// Nested membership and shared descendants are preserved.
    pub fn target_for_family(
        &self,
        scene: &mut Scene,
        source: &MobjectFamily,
        chain: Option<&[&str]>,
    ) -> Result<FamilyCopy, AuthoringError> {
        self.target_for_family_with_references(scene, source, &[], chain)
    }

    /// Carry detached metadata references through the same copy. Only the
    /// addressed family and its descendants are deformed.
    pub fn target_for_family_with_references(
        &self,
        scene: &mut Scene,
        source: &MobjectFamily,
        references: &[MobjectTarget<'_>],
        chain: Option<&[&str]>,
    ) -> Result<FamilyCopy, AuthoringError> {
        self.target_copy(
            scene,
            source.into(),
            references,
            chain,
            MobjectFamily::from_node,
        )
    }

    fn target_copy<R>(
        &self,
        scene: &mut Scene,
        source: MobjectTarget<'_>,
        references: &[MobjectTarget<'_>],
        chain: Option<&[&str]>,
        from_node: fn(Rc<RefCell<SemanticStore>>, SemanticNodeId) -> Result<R, AuthoringError>,
    ) -> Result<SemanticCopy<R>, AuthoringError> {
        source.require_store(scene.integration_store())?;
        let joints = match chain {
            Some(names) => self.resolve_chain(names)?,
            None if self.skeleton.parents[1..]
                .iter()
                .enumerate()
                .all(|(index, parent)| *parent == Some(index)) =>
            {
                (0..self.posed.len()).collect()
            }
            None => {
                return Err(invalid(
                    "a branched 2D rig requires an explicit deformation chain",
                ))
            }
        };
        let deform_point = self.point_deformer(&joints)?;
        let members: HashSet<_> = source.family_nodes()?.into_iter().collect();
        let mut paths = Vec::new();
        let mut changed = Vec::new();
        let (mut transaction, copy) =
            scene.prepare_target_copy(source, references, |object, state| {
                let id = object.node_id();
                if members.contains(&id) {
                    let store = object.integration_store().borrow();
                    if let Some(path) = deform_path(&store, state, &deform_point)? {
                        paths.push(path);
                        changed.push(id);
                    }
                }
                Ok(())
            })?;
        let result = if paths.is_empty() {
            scene.apply_semantic_transaction(transaction)?
        } else {
            scene.require_resource_admission()?;
            scene.with_semantic_publication(|store, publish| {
                store.with_geometry_paths(paths, |store, handles| {
                    for (id, &handle) in changed.into_iter().zip(handles) {
                        transaction.replace_content(
                            copy.copied_token(id),
                            noon_core::StoredGeometry::Resource(handle),
                        );
                    }
                    publish(store, transaction)
                })
            })?
        };
        copy.resolve(&result, from_node)
    }

    /// Warp bind-space points with a parent-to-child chain. A single joint
    /// attaches points rigidly. Otherwise the nearest bind segment selects
    /// the two influences, with smoothstep weights along that segment.
    /// The point order and count are preserved, including Bézier handles.
    pub fn deform_points(
        &self,
        chain: &[impl AsRef<str>],
        points: &[[f64; 2]],
    ) -> Result<Vec<[f64; 2]>, AuthoringError> {
        let joints = self.resolve_chain(chain)?;
        let deform_point = self.point_deformer(&joints)?;
        points
            .iter()
            .map(|&point| {
                if !point.iter().all(|&value| (value as f32).is_finite()) {
                    return Err(AuthoringError::NonFiniteGeometry);
                }
                Ok(deform_point(point))
            })
            .collect()
    }

    fn resolve_chain(&self, chain: &[impl AsRef<str>]) -> Result<Vec<usize>, AuthoringError> {
        chain.iter().map(|name| self.index(name.as_ref())).collect()
    }

    fn point_deformer(
        &self,
        joints: &[usize],
    ) -> Result<impl Fn([f64; 2]) -> [f64; 2] + use<'_>, AuthoringError> {
        let Some(&root) = joints.first() else {
            return Err(invalid(
                "a 2D deformation chain requires at least one target joint",
            ));
        };
        let mut segments = Vec::with_capacity(joints.len() - 1);
        for pair in joints.windows(2) {
            let (parent, child) = (pair[0], pair[1]);
            let start = self.skeleton.bind[parent].position;
            let axis = subtract(self.skeleton.bind[child].position, start);
            let length_squared = dot(axis, axis);
            if self.skeleton.parents[child] != Some(parent) || length_squared == 0.0 {
                return Err(invalid(format!(
                    "2D deformation chain requires a nonzero parent-to-child segment from `{}` to `{}`",
                    self.skeleton.joints[parent].name, self.skeleton.joints[child].name,
                )));
            }
            segments.push((parent, child, start, axis, length_squared));
        }

        Ok(move |point| {
            let mut nearest = f64::INFINITY;
            let mut influence = (root, root, 0.0);
            for &(parent, child, start, axis, length_squared) in &segments {
                let offset = subtract(point, start);
                let weight = (dot(offset, axis) / length_squared).clamp(0.0, 1.0);
                let distance = subtract(offset, axis.map(|value| weight * value));
                let squared = dot(distance, distance);
                if squared < nearest {
                    nearest = squared;
                    influence = (parent, child, weight * weight * (3.0 - 2.0 * weight));
                }
            }
            let (a, b, weight) = influence;
            let a = self.skin_point(a, point);
            let b = self.skin_point(b, point);
            [a[0] + (b[0] - a[0]) * weight, a[1] + (b[1] - a[1]) * weight]
        })
    }

    fn index(&self, name: &str) -> Result<usize, AuthoringError> {
        self.skeleton
            .names
            .get(name)
            .copied()
            .ok_or_else(|| invalid(format!("unknown 2D rig target joint `{name}`")))
    }

    fn skin_point(&self, index: usize, point: [f64; 2]) -> [f64; 2] {
        let bind = self.skeleton.bind[index];
        let posed = self.posed[index];
        let offset = subtract(point, bind.position);
        // Apply the change from bind, preserving exact points at rest.
        let rotated = rotate(offset, posed.heading - bind.heading);
        let shift = subtract(posed.position, bind.position);
        [
            point[0] + shift[0] + (rotated[0] - offset[0]),
            point[1] + shift[1] + (rotated[1] - offset[1]),
        ]
    }
}

fn deform_path(
    store: &SemanticStore,
    state: &noon_core::SemanticObjectState,
    deform_point: &impl Fn([f64; 2]) -> [f64; 2],
) -> Result<Option<noon_core::VectorPath>, AuthoringError> {
    let transform = state
        .transform
        .as_planar()
        .filter(|transform| transform.scale.x != 0.0 && transform.scale.y != 0.0)
        .ok_or_else(|| invalid("2D deformation requires an invertible planar object transform"))?;
    let path = crate::path_queries::content_path(store, state.content)?;
    let warped = path.map_xy(|x, y| {
        let (world_x, world_y) = transform.transform_xy(x, y);
        let [new_x, new_y] = deform_point([world_x, world_y]);
        if [new_x, new_y] == [world_x, world_y] {
            return (x, y);
        }
        // Keep object-relative paint and stroke scaling in the original local frame.
        let [x, y] = rotate(
            [
                new_x - transform.translation.x,
                new_y - transform.translation.y,
            ],
            -transform.rotation_z,
        );
        (x / transform.scale.x, y / transform.scale.y)
    });
    if !warped.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok((warped != *path).then_some(warped))
}

fn joint_names(joints: &[Joint2d]) -> Result<HashMap<&str, usize>, AuthoringError> {
    if joints.is_empty() {
        return Err(invalid("a 2D rig requires a root"));
    }
    let mut names = HashMap::new();
    for (index, joint) in joints.iter().enumerate() {
        if joint.name.trim().is_empty() || names.insert(joint.name.as_str(), index).is_some() {
            return Err(invalid("2D rig joint names must be nonempty and unique"));
        }
        if !(joint.rest_angle as f32).is_finite()
            || !joint.offset.iter().all(|&value| (value as f32).is_finite())
        {
            return Err(invalid(format!(
                "2D rig joint `{}` requires a finite transform in range",
                joint.name
            )));
        }
    }
    Ok(names)
}

fn joint_parents(
    joints: &[Joint2d],
    names: &HashMap<&str, usize>,
) -> Result<Vec<Option<usize>>, AuthoringError> {
    joints
        .iter()
        .enumerate()
        .map(|(index, joint)| match &joint.parent {
            Some(name) => names
                .get(name.as_str())
                .copied()
                .filter(|&parent| parent < index)
                .map(Some)
                .ok_or_else(|| {
                    invalid(format!(
                        "2D rig parent `{name}` must precede its child `{}`",
                        joint.name
                    ))
                }),
            None if index == 0 => Ok(None),
            None => Err(invalid("a 2D rig requires exactly one root")),
        })
        .collect()
}

fn rotation_samples(
    names: &HashMap<&str, usize>,
    tracks: &[JointTrack2d],
    mapping: &[JointMapping2d],
) -> Result<Vec<Option<SignalSamples>>, AuthoringError> {
    let mut mapped = HashMap::new();
    for entry in mapping {
        let role = &entry.source;
        let target = &entry.target;
        if !entry.gain.is_finite() || mapped.insert(role.as_str(), entry).is_some() {
            return Err(invalid(format!("invalid mapping from 2D rig source `{role}` to target `{target}`: gain must be finite and source unique")));
        }
    }

    let mut samples = vec![None; names.len()];
    let mut sources = vec![None; names.len()];
    let mut seen = HashSet::new();
    for track in tracks {
        let role = track.joint.as_str();
        let (target, gain) = mapped
            .get(role)
            .map_or((role, 1.0), |entry| (entry.target.as_str(), entry.gain));
        if !seen.insert(role) {
            return Err(invalid(format!(
                "duplicate 2D rig source track `{role}` for target `{target}`"
            )));
        }
        let index = *names.get(target).ok_or_else(|| {
            invalid(format!(
                "2D rig source `{role}` maps to unknown target joint `{target}`"
            ))
        })?;
        if let Some(previous) = sources[index].replace(role) {
            return Err(invalid(format!("2D rig source `{role}` and source `{previous}` both map to target joint `{target}`")));
        }
        samples[index] = Some(
            SignalSamples::new(
                ValueKind::Scalar,
                true,
                track.keys.iter().flat_map(|&[phase, angle]| [phase, angle * gain]).collect(),
            ).ok_or_else(|| invalid(format!("2D rig keys from source `{role}` to target `{target}` must be finite and strictly increasing")))?,
        );
    }
    if let Some(entry) = mapping
        .iter()
        .find(|entry| !seen.contains(entry.source.as_str()))
    {
        return Err(invalid(format!(
            "2D rig mapping to target `{}` names unknown source `{}`",
            entry.target, entry.source,
        )));
    }
    Ok(samples)
}

fn forward_kinematics(
    joints: &[Joint2d],
    parents: &[Option<usize>],
    angle_at: impl Fn(usize) -> f64,
) -> Vec<JointPose> {
    let mut result = Vec::<JointPose>::with_capacity(joints.len());
    for (index, (joint, parent)) in joints.iter().zip(parents).enumerate() {
        let angle = joint.rest_angle + angle_at(index);
        let pose = match parent {
            Some(parent) => {
                let parent = result[*parent];
                let [dx, dy] = rotate(joint.offset, parent.heading);
                JointPose {
                    position: [parent.position[0] + dx, parent.position[1] + dy],
                    heading: parent.heading + angle,
                }
            }
            None => JointPose {
                position: joint.offset,
                heading: angle,
            },
        };
        result.push(pose);
    }
    result
}

fn subtract(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn rotate([x, y]: [f64; 2], angle: f64) -> [f64; 2] {
    let (sin, cos) = angle.sin_cos();
    [cos * x - sin * y, sin * x + cos * y]
}

fn invalid(message: impl Into<String>) -> AuthoringError {
    AuthoringError::InvalidRig2d(message.into())
}

fn derived(store: &mut SemanticStore, expression: Expr) -> SemanticNodeId {
    store
        .insert_semantic_derived_signal(expression)
        .expect("validated 2D rig expression")
}

fn add(a: Expr, b: Expr) -> Expr {
    Expr::Add(Box::new(a), Box::new(b))
}

fn vector([x, y]: [f64; 2]) -> Expr {
    Expr::Constant(SemanticVec3::new(x, y, 0.0).into())
}

fn offset_from(position: SemanticNodeId, heading: SemanticNodeId, [x, y]: [f64; 2]) -> Expr {
    if [x, y] == [0.0, 0.0] {
        return Expr::signal(position);
    }
    let cosine = Expr::Cos(Box::new(Expr::signal(heading)));
    let sine = Expr::Sin(Box::new(Expr::signal(heading)));
    let along = Expr::Mul(Box::new(cosine), Box::new(vector([x, y])));
    let across = Expr::Mul(Box::new(sine), Box::new(vector([-y, x])));
    add(Expr::signal(position), add(along, across))
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{RateFunction, Vec2};
    use std::f32::consts::FRAC_PI_2;

    fn joint(name: &str, parent: Option<&str>, offset: [f64; 2], rest_angle: f64) -> Joint2d {
        Joint2d {
            name: name.into(),
            parent: parent.map(str::to_owned),
            offset,
            rest_angle,
        }
    }

    fn attachment(joint: &str, object: &Mobject) -> JointAttachment2d {
        JointAttachment2d {
            joint: joint.into(),
            object: object.clone(),
            offset: [0.0, 0.0],
        }
    }

    #[test]
    fn invalid_family_target_leaves_geometry_and_scene_revision_unchanged() {
        let rig = RetargetedRig2d::new(
            vec![joint("root", None, [0.0, 0.0], 0.0)],
            &[JointTrack2d {
                joint: "root".into(),
                keys: vec![[0.0, 0.5]],
            }],
            &[],
        )
        .unwrap();
        let pose = rig.pose(0.0).unwrap();
        let mut scene = Scene::new();
        let good = scene.square(1.0).unwrap();
        let bad = scene.circle(1.0).unwrap();
        scene.set_scale(&bad, 0.0, 1.0).unwrap();
        let family = scene.family(&[(&good).into(), (&bad).into()]).unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let mut execution = scene.into_execution().unwrap();
        let revision = execution.scene().revision();
        let stats = execution
            .scene()
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats();
        assert!(pose
            .target_for_family(execution.scene_mut(), &family, None)
            .is_err());
        assert_eq!(execution.scene().revision(), revision);
        assert_eq!(
            execution
                .scene()
                .integration_store()
                .borrow()
                .geometry_resources()
                .stats(),
            stats
        );
    }

    #[test]
    fn retarget_pose_keeps_bind_points_and_maps_a_named_rotation() {
        let skeleton = || {
            vec![
                joint("base", None, [1.0, -1.0], 0.0),
                joint("torso", Some("base"), [0.0, 2.0], 0.0),
                joint("tip", Some("torso"), [0.0, 2.0], 0.0),
            ]
        };
        let tracks = [JointTrack2d {
            joint: "shoulder".into(),
            keys: vec![[0.0, 0.0], [1.0, FRAC_PI_2]],
        }];
        let mapping = |target: &str| JointMapping2d {
            source: "shoulder".into(),
            target: target.into(),
            gain: -1.0,
        };
        let rig = RetargetedRig2d::new(skeleton(), &tracks, &[mapping("torso")]).unwrap();
        let chain = ["base", "torso", "tip"];
        let outline = [[0.8, -1.4], [1.0, 0.0], [1.0, 3.0]];
        assert_eq!(
            rig.pose(-1.0)
                .unwrap()
                .deform_points(&chain, &outline)
                .unwrap(),
            outline
        );
        let pose = rig.pose(2.0).unwrap();
        let warped = pose.deform_points(&chain, &outline).unwrap();
        assert_eq!(warped[0], outline[0]);
        assert!((warped[1][0] - 0.5).abs() < 1e-6);
        let [x, y, heading] = pose.joint("tip").unwrap();
        assert!((x - 3.0).abs() < 1e-6 && (y - 1.0).abs() < 1e-6);
        assert!((heading + f64::from(FRAC_PI_2)).abs() < 1e-9);
        assert!((x - warped[2][0]).abs() < 1e-9 && (y - warped[2][1]).abs() < 1e-9);
        assert!(pose.deform_points(&["base", "tip"], &outline).is_err());

        let Err(error) = RetargetedRig2d::new(skeleton(), &tracks, &[mapping("missing")]) else {
            panic!("an explicit mapping to an unknown joint must fail");
        };
        assert!(error.to_string().contains("source `shoulder`"));
        assert!(error.to_string().contains("target joint `missing`"));
    }

    #[test]
    fn one_clip_reaches_different_distances_and_stays_connected_between_keys() {
        let mut scene = Scene::new();
        let clock = scene.value_tracker(0.0).unwrap();
        let tracks = [
            JointTrack2d {
                joint: "shoulder".into(),
                keys: vec![[0.0, 0.0], [1.0, FRAC_PI_2]],
            },
            JointTrack2d {
                joint: "elbow".into(),
                keys: vec![[0.0, 0.0], [1.0, -FRAC_PI_2]],
            },
        ];
        for (base, length, rest) in [(-2.0, 1.0, 0.0), (2.0, 1.8, 0.3)] {
            let elbow = scene.circle(0.1).unwrap();
            let hand = scene.circle(0.15).unwrap();
            scene.add(&elbow).unwrap();
            scene.add(&hand).unwrap();
            scene
                .bind_rig_2d(
                    &clock,
                    &[
                        joint("shoulder", None, [base, 0.0], rest),
                        joint("elbow", Some("shoulder"), [length, 0.0], 0.2),
                    ],
                    &tracks,
                    &[
                        attachment("elbow", &elbow),
                        JointAttachment2d {
                            offset: [0.7, 0.0],
                            ..attachment("elbow", &hand)
                        },
                    ],
                )
                .unwrap();
        }
        scene
            .play_value(&clock, 1.0)
            .rate_func(RateFunction::Linear)
            .run_time(1.0)
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        for time in [0.0, 0.37, 1.0, 0.37, 0.0] {
            session.seek(time).unwrap();
            let frame = session.frame();
            for (pair, (x, length, rest)) in frame
                .objects
                .chunks_exact(2)
                .zip([(-2.0, 1.0, 0.0), (2.0, 1.8, 0.3)])
            {
                let shoulder = rest + time as f32 * FRAC_PI_2;
                let elbow = Vec2::new(x, 0.0) + length * Vec2::new(shoulder.cos(), shoulder.sin());
                let forearm_angle: f32 = rest + 0.2;
                let hand = elbow + 0.7 * Vec2::new(forearm_angle.cos(), forearm_angle.sin());
                assert!((pair[0].transform.translation - elbow).length() < 1e-5);
                assert!((pair[1].transform.translation - hand).length() < 1e-5);
                assert!((pair[1].transform.rotation - forearm_angle).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn a_late_attachment_conflict_leaves_the_scene_unmodified() {
        let mut scene = Scene::new();
        let clock = scene.value_tracker(0.0).unwrap();
        let free = scene.circle(0.1).unwrap();
        let bound = scene.circle(0.2).unwrap();
        scene.add(&free).unwrap();
        scene.add(&bound).unwrap();
        scene.bind_rotation(&bound, &clock).unwrap();
        let before = scene.integration_store().borrow().len();
        assert!(matches!(
            scene.bind_rig_2d(
                &clock,
                &[joint("root", None, [0.0, 0.0], 0.0)],
                &[],
                &[attachment("root", &free), attachment("root", &bound)],
            ),
            Err(AuthoringError::SignalBinding(
                SemanticSignalBindingError::PropertyAlreadyBound { .. }
            ))
        ));
        let store = scene.integration_store().borrow();
        assert_eq!(store.len(), before);
        assert!(store
            .semantic_object_signal_bindings(free.node_id())
            .unwrap()
            .is_empty());
    }
}
