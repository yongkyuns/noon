mod artwork;

pub use artwork::{RigArtwork2d, RigArtworkTarget2d};

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    f64::consts::{PI, TAU},
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

/// A pointing gesture prepared from one or more scene-space targets.
pub struct LookAndPoint2d<'a> {
    pub wrist: &'a str,
    /// A separate joint whose local +X axis follows the target.
    pub gaze: &'a str,
    /// Ordered (phase, point) samples from first contact through release.
    /// Repeat the point at a later phase to hold a stationary target.
    pub targets: &'a [(f32, [f64; 2])],
    /// The gaze starts turning at the first phase. The arm starts reaching at the second.
    pub look: [f32; 2],
    /// Phase at which both the arm and gaze are back at rest.
    pub rest: f32,
    pub fingertip: f64,
    pub bend_left: bool,
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
#[derive(Clone)]
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

    /// Prepare rotation tracks for a shoulder, elbow, and wrist from scene-space targets.
    ///
    /// Targets are (phase, point) pairs in increasing order. The named wrist
    /// must have an elbow parent and a shoulder grandparent. The fingertip is
    /// measured along the wrist's local +X axis and follows the forearm heading.
    /// `bend_left` puts the elbow left of the shoulder-to-target ray. Any
    /// ancestors of the shoulder use the existing clip at each target phase.
    ///
    /// The tip meets each key. Angles interpolate between keys and keep both
    /// arm lengths fixed. Sample a moving target more densely when it needs to
    /// stay close between keys.
    pub fn tracks_for_pointing(
        &self,
        wrist: &str,
        targets: &[(f32, [f64; 2])],
        fingertip: f64,
        bend_left: bool,
    ) -> Result<[JointTrack2d; 3], AuthoringError> {
        let skeleton = &self.skeleton;
        let wrist_index = *skeleton
            .names
            .get(wrist)
            .ok_or_else(|| invalid(format!("unknown 2D pointing wrist `{wrist}`")))?;
        let parents = &skeleton.parents;
        let (elbow, shoulder) = parents[wrist_index]
            .and_then(|elbow| parents[elbow].map(|shoulder| (elbow, shoulder)))
            .ok_or_else(|| {
                invalid(format!(
                    "2D pointing wrist `{wrist}` needs a parent and grandparent"
                ))
            })?;
        let [shoulder_joint, elbow_joint, wrist_joint] =
            [shoulder, elbow, wrist_index].map(|index| &skeleton.joints[index]);
        let [ux, uy] = elbow_joint.offset;
        let [fx, fy] = wrist_joint.offset;
        let (upper, forearm) = (ux.hypot(uy), fx.hypot(fy));
        if (upper as f32) == 0.0 || (forearm as f32) == 0.0 {
            return Err(invalid(format!(
                "2D pointing wrist `{wrist}` needs two nonzero arm segments"
            )));
        }
        if !(fingertip as f32).is_finite() || fingertip < 0.0 {
            return Err(invalid(
                "2D pointing fingertip must be finite and nonnegative",
            ));
        }
        if targets.is_empty() || targets.len() > SignalSamples::MAX_KEYS {
            return Err(invalid(
                "2D pointing requires a supported number of target keys",
            ));
        }

        let mut tracks = [shoulder_joint, elbow_joint, wrist_joint].map(|joint| JointTrack2d {
            joint: joint.name.clone(),
            keys: Vec::with_capacity(targets.len()),
        });
        // The hand extends the second side of the triangle along the forearm.
        // Wrist placement in playback still uses the original joint offset.
        let lower = forearm + fingertip;
        let reach = upper + lower;
        let inner = (upper - lower).abs();
        let tolerance = reach * (8.0 * f64::EPSILON);
        let side = if bend_left { 1.0 } else { -1.0 };
        let upper_rest_axis = uy.atan2(ux);
        let lower_rest_axis = fy.atan2(fx);
        let mut previous_phase = f32::NEG_INFINITY;
        let mut previous_angles = [0.0; 3];
        for &(phase, target) in targets {
            if !phase.is_finite()
                || phase <= previous_phase
                || !target
                    .iter()
                    .all(|&coordinate| (coordinate as f32).is_finite())
            {
                return Err(invalid(
                    "2D pointing keys require increasing finite phases and finite targets",
                ));
            }
            previous_phase = phase;
            let (origin, parent_heading) = match parents[shoulder] {
                Some(parent) => {
                    let base = self.pose(phase)?;
                    (base.posed[shoulder].position, base.posed[parent].heading)
                }
                None => (shoulder_joint.offset, 0.0),
            };
            let [dx, dy] = subtract(target, origin);
            let distance = dx.hypot(dy);
            if (distance as f32) == 0.0
                || distance < inner - tolerance
                || distance > reach + tolerance
            {
                return Err(invalid(format!(
                    "2D pointing target at phase {phase} for wrist `{wrist}` is out of reach or has no direction"
                )));
            }
            let cosine = (distance * distance + (upper - lower) * (upper + lower))
                / (2.0 * upper * distance);
            let upper_axis = dy.atan2(dx) + side * cosine.clamp(-1.0, 1.0).acos();
            let [ex, ey] = rotate([upper, 0.0], upper_axis);
            let lower_axis = (dy - ey).atan2(dx - ex);
            let shoulder_heading = upper_axis - upper_rest_axis;
            let elbow_heading = lower_axis - lower_rest_axis;
            let angles = [
                shoulder_heading - parent_heading - shoulder_joint.rest_angle,
                elbow_heading - shoulder_heading - elbow_joint.rest_angle,
                lower_rest_axis - wrist_joint.rest_angle,
            ];
            for ((track, previous), angle) in
                tracks.iter_mut().zip(&mut previous_angles).zip(angles)
            {
                // Unwrap adjacent keys across the atan2 seam.
                *previous = nearest_angle(angle, *previous);
                track.keys.push([phase, *previous as f32]);
            }
        }
        Ok(tracks)
    }

    /// Prepare one gaze channel and three arm channels for an ordinary rig binding.
    /// The arm interpolates joint angles on the way out and back, so neither
    /// segment stretches. Contact is exact at the authored target samples.
    pub fn look_and_point(
        &self,
        cue: LookAndPoint2d<'_>,
    ) -> Result<[JointTrack2d; 4], AuthoringError> {
        let [(contact, first_target), tail @ ..] = cue.targets else {
            return Err(invalid("look and point requires a target"));
        };
        let release = tail.last().map_or(*contact, |&(phase, _)| phase);
        let [start, ready] = cue.look;
        if !start.is_finite()
            || !cue.rest.is_finite()
            || !(start < ready && ready < *contact && release < cue.rest)
        {
            return Err(invalid(
                "look and point phases must order gaze, reach, target samples, then rest",
            ));
        }
        if cue.targets.len() > SignalSamples::MAX_KEYS - 3 {
            return Err(invalid("look and point has too many target samples"));
        }
        let index = |name: &str| {
            self.skeleton
                .names
                .get(name)
                .copied()
                .ok_or_else(|| invalid(format!("unknown look and point joint `{name}`")))
        };
        let gaze = index(cue.gaze)?;
        let [mut shoulder, mut elbow, mut wrist] =
            self.tracks_for_pointing(cue.wrist, cue.targets, cue.fingertip, cue.bend_left)?;
        let parents = &self.skeleton.parents;
        let is_ancestor = |ancestor, child| {
            std::iter::successors(Some(child), |&joint| parents[joint])
                .any(|joint| joint == ancestor)
        };
        if is_ancestor(gaze, index(cue.wrist)?) || is_ancestor(index(&shoulder.joint)?, gaze) {
            return Err(invalid(
                "look and point requires a gaze joint on a separate branch from the arm",
            ));
        }
        for track in [&mut shoulder, &mut elbow, &mut wrist] {
            track.keys.insert(0, [ready, 0.0]);
            return_to_rest(&mut track.keys, cue.rest);
        }
        let mut keys = Vec::with_capacity(cue.targets.len() + 3);
        keys.push([start, 0.0]);
        let mut angle = 0.0;
        for (phase, target) in
            std::iter::once((ready, *first_target)).chain(cue.targets.iter().copied())
        {
            let base = self.pose(phase)?;
            let [dx, dy] = subtract(target, base.posed[gaze].position);
            if (dx.hypot(dy) as f32) == 0.0 {
                return Err(invalid(format!(
                    "look target at phase {phase} coincides with gaze joint `{}`",
                    cue.gaze
                )));
            }
            let parent = parents[gaze].map_or(0.0, |parent| base.posed[parent].heading);
            angle = nearest_angle(
                dy.atan2(dx) - parent - self.skeleton.joints[gaze].rest_angle,
                angle,
            );
            keys.push([phase, angle as f32]);
        }
        return_to_rest(&mut keys, cue.rest);
        Ok([
            shoulder,
            elbow,
            wrist,
            JointTrack2d {
                joint: cue.gaze.into(),
                keys,
            },
        ])
    }

    /// Prepare ordered look-and-point cues as one set of rig tracks.
    /// Each cue must reach rest before the next begins. Cues may use the same
    /// arm or different arms and may share a gaze joint.
    pub fn look_and_point_sequence<'a>(
        &self,
        cues: impl IntoIterator<Item = LookAndPoint2d<'a>>,
    ) -> Result<Vec<JointTrack2d>, AuthoringError> {
        let mut tracks: Vec<JointTrack2d> = Vec::new();
        let mut indices = HashMap::new();
        let mut previous_rest = f32::NEG_INFINITY;
        for cue in cues {
            if cue.look[0] < previous_rest {
                return Err(invalid(
                    "look and point cues must be ordered and cannot overlap",
                ));
            }
            previous_rest = cue.rest;
            for track in self.look_and_point(cue)? {
                let index = *indices.entry(track.joint.clone()).or_insert(tracks.len());
                if let Some(existing) = tracks.get_mut(index) {
                    let [phase, angle] = *existing.keys.last().expect("prepared keys");
                    // The rest orientation can be a full turn away from zero.
                    // Keep that orientation through the gap and the next cue.
                    let skip = usize::from(phase == track.keys[0][0]);
                    if existing.keys.len() + track.keys.len() - skip > SignalSamples::MAX_KEYS {
                        return Err(invalid(
                            "look and point sequence has too many keys for a joint",
                        ));
                    }
                    existing.keys.extend(
                        track
                            .keys
                            .into_iter()
                            .skip(skip)
                            .map(|[t, a]| [t, a + angle]),
                    );
                } else {
                    tracks.push(track);
                }
            }
        }
        if tracks.is_empty() {
            return Err(invalid("look and point sequence requires a cue"));
        }
        Ok(tracks)
    }
}

fn nearest_angle(angle: f64, previous: f64) -> f64 {
    previous + (angle - previous + PI).rem_euclid(TAU) - PI
}

fn return_to_rest(keys: &mut Vec<[f32; 2]>, phase: f32) {
    let previous = f64::from(keys.last().expect("prepared keys")[1]);
    keys.push([phase, nearest_angle(0.0, previous) as f32]);
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
        copy_with_paths(scene, source, references, from_node, |object, state| {
            if !members.contains(&object.node_id()) {
                return Ok(None);
            }
            let store = object.integration_store().borrow();
            deform_path(&store, state, &deform_point)
        })
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

fn copy_with_paths<R>(
    scene: &mut Scene,
    source: MobjectTarget<'_>,
    references: &[MobjectTarget<'_>],
    from_node: fn(Rc<RefCell<SemanticStore>>, SemanticNodeId) -> Result<R, AuthoringError>,
    mut path_for: impl FnMut(
        &Mobject,
        &noon_core::SemanticObjectState,
    ) -> Result<Option<noon_core::VectorPath>, AuthoringError>,
) -> Result<SemanticCopy<R>, AuthoringError> {
    let mut paths = Vec::new();
    let mut changed = Vec::new();
    let (mut transaction, copy) =
        scene.prepare_target_copy(source, references, |object, state| {
            if let Some(path) = path_for(object, state)? {
                paths.push(path);
                changed.push(object.node_id());
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
        into_local_coordinates(transform, [new_x, new_y])
    });
    if !warped.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok((warped != *path).then_some(warped))
}

fn into_local_coordinates(
    transform: noon_core::SemanticTransform2_5D,
    [x, y]: [f64; 2],
) -> (f64, f64) {
    let [x, y] = rotate(
        [x - transform.translation.x, y - transform.translation.y],
        -transform.rotation_z,
    );
    (x / transform.scale.x, y / transform.scale.y)
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
    fn artwork_cues_blink_on_a_turning_head_with_different_eye_offsets() {
        let cues = [
            (0.0, "open".into()),
            (1.0, "shut".into()),
            (2.0, "open".into()),
        ];
        let poses = || {
            vec![
                ("open".into(), vec![[0.0, 0.0], [0.0, 0.4], [0.5, 0.0]]),
                ("shut".into(), vec![[0.0, 0.0], [0.0, 0.0], [0.5, 0.0]]),
            ]
        };
        for eye_offset in [0.7, 1.2] {
            let rig = RetargetedRig2d::new(
                vec![
                    joint("head", None, [0.4, -0.3], 0.0),
                    joint("eye", Some("head"), [eye_offset, 0.0], 0.0),
                ],
                &[JointTrack2d {
                    joint: "head".into(),
                    keys: vec![[0.0, 0.0], [1.0, FRAC_PI_2]],
                }],
                &[],
            )
            .unwrap();
            let blink = rig.artwork_track("eye", poses(), &cues).unwrap();
            for (phase, opening) in [(0.5, 0.2), (1.0, 0.0), (1.5, 0.2), (0.5, 0.2)] {
                let [x, y, heading] = rig.pose(phase).unwrap().joint("eye").unwrap();
                let points = blink.points_at(phase).unwrap();
                assert_eq!(points[0], [x, y]);
                assert!((points[1][0] - (x - opening * heading.sin())).abs() < 1e-10);
                assert!((points[1][1] - (y + opening * heading.cos())).abs() < 1e-10);
            }
            assert_eq!(blink.points_at(2.0).unwrap(), blink.points_at(3.0).unwrap());
            let error = rig
                .artwork_track("eye", poses(), &[(0.0, "missing".into())])
                .err()
                .unwrap();
            assert!(error.to_string().contains("missing"));
            let mut mismatched = poses();
            mismatched[1].1.pop();
            assert!(rig.artwork_track("eye", mismatched, &cues).is_err());
        }
    }

    #[test]
    fn styled_artwork_preserves_nested_members_and_samples_corresponding_contours() {
        use noon_core::{Color, PathCommand, SemanticPaint, SemanticStyle, VectorPath};

        let mut scene = Scene::new();
        let ink = SemanticStyle {
            stroke: Some(SemanticPaint::Solid(Color::WHITE)),
            stroke_width: 0.15,
            ..Default::default()
        };
        let drawing = |extent, close| {
            let path = VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(extent, 0.0));
            (if close { path.close() } else { path })
                .move_to(Vec2::new(0.0, 1.0))
                .line_to(Vec2::new(extent, 1.0))
        };
        let a = scene.path(drawing(2.0, true), ink.clone()).unwrap();
        let b = scene
            .path(drawing(4.0, true), SemanticStyle::default())
            .unwrap();
        let bad = scene.path(drawing(4.0, false), ink.clone()).unwrap();
        let inner_a = scene.family(&[(&a).into()]).unwrap();
        let inner_b = scene.family(&[(&b).into()]).unwrap();
        let first = scene.family(&[(&a).into(), (&inner_a).into()]).unwrap();
        let second = scene.family(&[(&b).into(), (&inner_b).into()]).unwrap();
        let rig =
            RetargetedRig2d::new(vec![joint("root", None, [10.0, -2.0], 0.0)], &[], &[]).unwrap();
        let cues = [(0.0, "a".into()), (1.0, "b".into())];
        let poses = [
            ("a".into(), (&first).into()),
            ("b".into(), (&second).into()),
        ];
        let track = rig.artwork_track_for("root", &poses, &cues).unwrap();
        let sample = track.target_at(&mut scene, 0.5).unwrap();
        let copy = sample.mobject(&a).unwrap();
        let nested = sample.family(&inner_a).unwrap();
        let root = sample.family(&first).unwrap();
        let store = scene.integration_store().borrow();
        assert_eq!(
            store
                .semantic_family_members_checked(root.node_id())
                .unwrap(),
            [copy.node_id(), nested.node_id()]
        );
        assert_eq!(
            store
                .semantic_family_members_checked(nested.node_id())
                .unwrap(),
            [copy.node_id()]
        );
        let state = store.semantic_object_state_checked(copy.node_id()).unwrap();
        assert_eq!(state.style.stroke, ink.stroke);
        assert_eq!(state.style.stroke_width, ink.stroke_width);
        let path = crate::path_queries::content_path(&store, state.content).unwrap();
        assert_eq!(
            path.commands(),
            &[
                PathCommand::MoveTo {
                    to: Vec2::new(10.0, -2.0)
                },
                PathCommand::LineTo {
                    to: Vec2::new(13.0, -2.0)
                },
                PathCommand::Close,
                PathCommand::MoveTo {
                    to: Vec2::new(10.0, -1.0)
                },
                PathCommand::LineTo {
                    to: Vec2::new(13.0, -1.0)
                },
            ]
        );
        drop(store);
        let incompatible = [("a".into(), (&a).into()), ("b".into(), (&bad).into())];
        let Err(error) = rig.artwork_track_for("root", &incompatible, &cues) else {
            panic!("an open contour cannot replace a closed one");
        };
        assert!(error
            .to_string()
            .contains("pose `b` must match the path contours"));
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
            assert_eq!(frame.objects.len(), 4);
            let (pairs, _) = frame.objects.as_chunks::<2>();
            for (pair, (x, length, rest)) in pairs.iter().zip([(-2.0, 1.0, 0.0), (2.0, 1.8, 0.3)]) {
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
    fn tracks_for_pointing_reach_with_fixed_lengths_during_play_and_seek() {
        let mut scene = Scene::new();
        let clock = scene.value_tracker(0.0).unwrap();
        let targets: Vec<_> = (0..=16_u8)
            .map(|i| {
                let t = f32::from(i) / 16.0;
                (t, [0.5 * f64::from(t), 0.9 * f64::from(t)])
            })
            .collect();
        let proportions = [(-2.4, 1.42, 1.55, false), (2.6, 1.81, 1.03, true)];
        let fingertip = 0.23;
        for (x, upper, lower, bend_left) in proportions {
            let joints = [
                joint("shoulder", None, [x, 0.0], 0.31),
                joint("elbow", Some("shoulder"), [0.0, upper], -0.46),
                joint("wrist", Some("elbow"), [-lower, 0.0], 0.72),
            ];
            let rig = RetargetedRig2d::new(joints.to_vec(), &[], &[]).unwrap();
            let tracks = rig
                .tracks_for_pointing("wrist", &targets, fingertip, bend_left)
                .unwrap();
            let elbow = scene.circle(0.1).unwrap();
            let wrist = scene.circle(0.1).unwrap();
            let tip = scene.circle(0.1).unwrap();
            scene
                .add_many(&[(&elbow).into(), (&wrist).into(), (&tip).into()])
                .unwrap();
            scene
                .bind_rig_2d(
                    &clock,
                    &joints,
                    &tracks,
                    &[
                        attachment("elbow", &elbow),
                        attachment("wrist", &wrist),
                        JointAttachment2d {
                            offset: [fingertip, 0.0],
                            ..attachment("wrist", &tip)
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
        let mut forward = Vec::new();
        for i in 0..=32_u8 {
            let t = f32::from(i) / 32.0;
            session.seek(f64::from(t)).unwrap();
            let positions: Vec<_> = session
                .frame()
                .objects
                .iter()
                .map(|object| object.transform.translation)
                .collect();
            let (arms, remainder) = positions.as_chunks::<3>();
            assert!(remainder.is_empty());
            assert_eq!(arms.len(), proportions.len());
            for (&[elbow, wrist, tip], (x, upper, lower, bend_left)) in arms.iter().zip(proportions)
            {
                let shoulder = Vec2::new(x as f32, 0.0);
                let target = Vec2::new(0.5 * t, 0.9 * t);
                assert!(((elbow - shoulder).length() - upper as f32).abs() < 1e-5);
                assert!(((wrist - elbow).length() - lower as f32).abs() < 1e-5);
                assert!(((tip - wrist).length() - fingertip as f32).abs() < 1e-5);
                assert!((tip - target).length() < if i % 2 == 0 { 1e-5 } else { 0.002 });
                let ray = target - shoulder;
                let offset = elbow - shoulder;
                assert_eq!(ray.x * offset.y - ray.y * offset.x > 0.0, bend_left);
            }
            forward.push(positions);
        }
        for (i, expected) in forward.into_iter().enumerate().rev() {
            session.seek(i as f64 / 32.0).unwrap();
            assert!(session
                .frame()
                .objects
                .iter()
                .map(|object| object.transform.translation)
                .eq(expected));
        }
    }

    #[test]
    fn pointing_rejects_unreachable_targets_and_unwraps_adjacent_angles() {
        let rig = RetargetedRig2d::new(
            vec![
                joint("shoulder", None, [0.0, 0.0], 0.0),
                joint("elbow", Some("shoulder"), [2.0, 0.0], 0.0),
                joint("wrist", Some("elbow"), [1.0, 0.0], 0.0),
            ],
            &[],
            &[],
        )
        .unwrap();
        for target in [[4.0, 0.0], [0.1, 0.0], [0.0, 0.0]] {
            let message = rig
                .tracks_for_pointing("wrist", &[(3.0, target)], 0.0, true)
                .err()
                .unwrap()
                .to_string();
            assert!(message.contains("phase 3 for wrist `wrist` is out of reach"));
        }
        assert!(rig
            .tracks_for_pointing("elbow", &[(0.0, [2.0, 0.0])], 0.0, true)
            .is_err());
        assert!(rig
            .tracks_for_pointing("wrist", &[(0.0, [2.0, 0.0]); 2], 0.0, true)
            .is_err());
        // With equal effective lengths, the reach interval includes zero.
        // A direction that vanishes at runtime precision cannot define a pose.
        assert!(rig
            .tracks_for_pointing("wrist", &[(0.0, [f64::MIN_POSITIVE, 0.0])], 1.0, true)
            .is_err());
        let [shoulder, _, _] = rig
            .tracks_for_pointing(
                "wrist",
                &[(0.0, [-3.0, 0.0]), (1.0, [-2.99, -0.01])],
                0.0,
                true,
            )
            .unwrap();
        assert!((shoulder.keys[1][1] - shoulder.keys[0][1]).abs() < 0.2);
    }

    #[test]
    fn look_and_point_leads_with_gaze_and_returns_both_branches_to_rest() {
        let mut scene = Scene::new();
        let clock = scene.value_tracker(0.0).unwrap();
        let gaze_origin = Vec2::new(0.0, 0.65);
        let proportions = [(-2.4, 1.42, 1.55, false), (2.6, 1.81, 1.03, true)];
        let fingertip = 0.23;
        let targets = [(1.0, [0.0, 0.0]), (2.0, [0.5, 0.9]), (3.0, [0.5, 0.9])];
        for (x, upper, lower, bend_left) in proportions {
            let joints = [
                joint("body", None, [0.0, 0.0], 0.0),
                joint("shoulder", Some("body"), [x, 0.0], -0.8),
                joint("elbow", Some("shoulder"), [upper, 0.0], 0.47),
                joint("wrist", Some("elbow"), [lower, 0.0], 0.0),
                joint("gaze", Some("body"), [0.0, f64::from(gaze_origin.y)], 0.0),
            ];
            let rig = RetargetedRig2d::new(joints.to_vec(), &[], &[]).unwrap();
            let cue = LookAndPoint2d {
                wrist: "wrist",
                gaze: "gaze",
                targets: &targets,
                look: [0.0, 0.4],
                rest: 4.0,
                fingertip,
                bend_left,
            };
            assert!(rig
                .look_and_point(LookAndPoint2d {
                    gaze: "body",
                    ..cue
                })
                .is_err());
            let clip = rig.look_and_point(cue).unwrap();
            let attachments = [
                ("elbow", 0.0),
                ("wrist", 0.0),
                ("wrist", fingertip),
                ("gaze", 1.0),
            ]
            .map(|(joint, along)| {
                let point = scene.circle(0.1).unwrap();
                scene.add(&point).unwrap();
                JointAttachment2d {
                    offset: [along, 0.0],
                    ..attachment(joint, &point)
                }
            });
            scene
                .bind_rig_2d(&clock, &joints, &clip, &attachments)
                .unwrap();
        }
        scene
            .play_value(&clock, 4.0)
            .rate_func(RateFunction::Linear)
            .run_time(4.0)
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let checkpoints = [0.0, 0.4, 0.75, 1.0, 2.0, 2.6, 3.0, 3.5, 4.0];
        let mut forward = Vec::<Vec<Vec2>>::new();
        for time in checkpoints {
            session.seek(time).unwrap();
            let positions: Vec<_> = session
                .frame()
                .objects
                .iter()
                .map(|object| object.transform.translation)
                .collect();
            let (arms, remainder) = positions.as_chunks::<4>();
            assert!(remainder.is_empty());
            assert_eq!(arms.len(), proportions.len());
            for (i, (&[elbow, wrist, tip, eye], (x, upper, lower, _))) in
                arms.iter().zip(proportions).enumerate()
            {
                let shoulder = Vec2::new(x as f32, 0.0);
                assert!(((elbow - shoulder).length() - upper as f32).abs() < 1e-5);
                assert!(((wrist - elbow).length() - lower as f32).abs() < 1e-5);
                assert!(((tip - wrist).length() - fingertip as f32).abs() < 1e-5);
                if time == 0.4 {
                    assert!((tip - forward[0][4 * i + 2]).length() < 1e-5);
                    assert!((eye - (gaze_origin + Vec2::new(0.0, -1.0))).length() < 1e-5);
                }
                if (1.0..=3.0).contains(&time) {
                    let [x, y] = if time == 1.0 {
                        targets[0].1
                    } else {
                        targets[1].1
                    };
                    assert!((tip - Vec2::new(x as f32, y as f32)).length() < 1e-5);
                }
            }
            if time == 4.0 {
                assert!(positions
                    .iter()
                    .zip(&forward[0])
                    .all(|(&a, &b)| (a - b).length() < 1e-5));
            }
            forward.push(positions);
        }
        for (time, expected) in checkpoints.into_iter().zip(forward).rev() {
            session.seek(time).unwrap();
            assert!(session
                .frame()
                .objects
                .iter()
                .map(|object| object.transform.translation)
                .eq(expected));
        }
    }

    #[test]
    fn consecutive_cues_keep_the_same_neutral_angle_at_a_shared_boundary() {
        let joints = vec![
            joint("base", None, [0.0, 0.0], 0.0),
            joint("shoulder", Some("base"), [0.0, 0.0], 0.0),
            joint("elbow", Some("shoulder"), [1.0, 0.0], 0.0),
            joint("wrist", Some("elbow"), [1.0, 0.0], 0.0),
            joint("gaze", Some("base"), [0.0, 0.0], 0.0),
        ];
        let rig = RetargetedRig2d::new(joints.clone(), &[], &[]).unwrap();
        let around = [
            (1.0, [1.0, 0.0]),
            (2.0, [0.0, 1.0]),
            (3.0, [-1.0, 0.0]),
            (4.0, [0.0, -1.0]),
            (5.0, [1.0, 0.0]),
        ];
        let up = [(7.0, [0.0, 1.0])];
        let first = LookAndPoint2d {
            wrist: "wrist",
            gaze: "gaze",
            targets: &around,
            look: [0.0, 0.5],
            rest: 6.0,
            fingertip: 0.2,
            bend_left: true,
        };
        let second = LookAndPoint2d {
            targets: &up,
            look: [6.0, 6.5],
            rest: 8.0,
            ..first
        };
        assert!(rig
            .look_and_point_sequence([
                LookAndPoint2d { ..first },
                LookAndPoint2d {
                    look: [5.9, 6.5],
                    ..second
                },
            ])
            .is_err());
        let tracks = rig.look_and_point_sequence([first, second]).unwrap();
        assert_eq!(tracks.len(), 4);
        let rig = RetargetedRig2d::new(joints, &tracks, &[]).unwrap();
        let neutral = rig.pose(6.0).unwrap();
        let lead = rig.pose(6.25).unwrap();
        assert!((neutral.joint("gaze").unwrap()[2] - TAU).abs() < 1e-5);
        assert!((lead.joint("gaze").unwrap()[2] - (TAU + PI / 4.0)).abs() < 1e-5);
        let [nx, ny, _] = neutral.joint("wrist").unwrap();
        let [lx, ly, _] = lead.joint("wrist").unwrap();
        assert!((nx - lx).hypot(ny - ly) < 1e-5);
        let [x, y, angle] = rig.pose(7.0).unwrap().joint("wrist").unwrap();
        assert!((x + 0.2 * angle.cos()).hypot(y + 0.2 * angle.sin() - 1.0) < 1e-5);
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
