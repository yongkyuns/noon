use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    AnimationOptions, CompositionTimeMap, CompositionTimeMapStep, MutationTransaction, ObjectId,
    PatchError, Property, RateFunction, SceneDefinition, ScenePatch, TimelineError,
    TrackDefinition, TrackId, TrackTiming, TrackValues,
};

/// Stable semantic identity for a retained animation node.
///
/// IDs are independent from flattened track IDs. Reusing a removed slot bumps its
/// generation so stale authoring handles cannot silently bind to a new animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AnimationNodeId {
    slot: u32,
    generation: u32,
}

impl AnimationNodeId {
    pub const fn new(slot: u32, generation: u32) -> Self {
        Self { slot, generation }
    }

    pub const fn slot(self) -> u32 {
        self.slot
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

/// One leaf track before composition timing has been flattened.
///
/// `timing` is local to the leaf. Composition and root timing are retained by the
/// graph and only materialized into a root timing + `CompositionTimeMap` when the
/// leaf is lowered for execution.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimationTrackTemplate {
    pub object: ObjectId,
    pub property: Property,
    pub values: TrackValues,
    pub timing: TrackTiming,
}

impl AnimationTrackTemplate {
    pub const fn new(
        object: ObjectId,
        property: Property,
        values: TrackValues,
        timing: TrackTiming,
    ) -> Self {
        Self {
            object,
            property,
            values,
            timing,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationNodeKind {
    Leaf {
        tracks: Vec<AnimationTrackTemplate>,
    },
    Parallel {
        children: Vec<AnimationNodeId>,
    },
    Sequence {
        children: Vec<AnimationNodeId>,
    },
    Lagged {
        children: Vec<AnimationNodeId>,
        lag_ratio: f64,
    },
}

impl AnimationNodeKind {
    pub fn children(&self) -> &[AnimationNodeId] {
        match self {
            Self::Leaf { .. } => &[],
            Self::Parallel { children }
            | Self::Sequence { children }
            | Self::Lagged { children, .. } => children,
        }
    }

    fn children_mut(&mut self) -> Option<&mut Vec<AnimationNodeId>> {
        match self {
            Self::Leaf { .. } => None,
            Self::Parallel { children }
            | Self::Sequence { children }
            | Self::Lagged { children, .. } => Some(children),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimationNode {
    id: AnimationNodeId,
    kind: AnimationNodeKind,
    #[serde(default)]
    options: AnimationOptions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<AnimationNodeId>,
}

impl AnimationNode {
    pub const fn id(&self) -> AnimationNodeId {
        self.id
    }

    pub fn kind(&self) -> &AnimationNodeKind {
        &self.kind
    }

    pub const fn options(&self) -> AnimationOptions {
        self.options
    }

    pub const fn parent(&self) -> Option<AnimationNodeId> {
        self.parent
    }
}

#[derive(Clone, Debug)]
struct AnimationSlot {
    generation: u32,
    node: Option<AnimationNode>,
    next_free: Option<u32>,
}

/// Retained authoring graph. Execution never walks this structure per frame.
#[derive(Clone, Debug, Default)]
pub struct AnimationGraph {
    slots: Vec<AnimationSlot>,
    free_head: Option<u32>,
    live_nodes: usize,
}

impl AnimationGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_leaf(&mut self, tracks: Vec<AnimationTrackTemplate>) -> AnimationNodeId {
        self.insert_kind(AnimationNodeKind::Leaf { tracks })
    }

    pub fn insert_parallel(
        &mut self,
        children: Vec<AnimationNodeId>,
    ) -> Result<AnimationNodeId, AnimationGraphError> {
        self.insert_composition(AnimationNodeKind::Parallel { children })
    }

    pub fn insert_sequence(
        &mut self,
        children: Vec<AnimationNodeId>,
    ) -> Result<AnimationNodeId, AnimationGraphError> {
        self.insert_composition(AnimationNodeKind::Sequence { children })
    }

    pub fn insert_lagged(
        &mut self,
        children: Vec<AnimationNodeId>,
        lag_ratio: f64,
    ) -> Result<AnimationNodeId, AnimationGraphError> {
        validate_lag_ratio(lag_ratio)?;
        self.insert_composition(AnimationNodeKind::Lagged {
            children,
            lag_ratio,
        })
    }

    fn insert_composition(
        &mut self,
        kind: AnimationNodeKind,
    ) -> Result<AnimationNodeId, AnimationGraphError> {
        if kind.children().is_empty() {
            return Err(AnimationGraphError::EmptyComposition);
        }
        let mut unique = HashSet::with_capacity(kind.children().len());
        for &child in kind.children() {
            let node = self
                .node(child)
                .ok_or(AnimationGraphError::UnknownNode(child))?;
            if node.parent.is_some() {
                return Err(AnimationGraphError::AlreadyParented(child));
            }
            if !unique.insert(child) {
                return Err(AnimationGraphError::DuplicateChild(child));
            }
        }
        let children = kind.children().to_vec();
        let id = self.insert_kind(kind);
        for child in children {
            self.node_mut(child)
                .expect("composition children validated above")
                .parent = Some(id);
        }
        Ok(id)
    }

    fn insert_kind(&mut self, kind: AnimationNodeKind) -> AnimationNodeId {
        let (slot_index, generation) = if let Some(slot_index) = self.free_head {
            let slot = &mut self.slots[slot_index as usize];
            self.free_head = slot.next_free.take();
            (slot_index, slot.generation)
        } else {
            let slot_index = u32::try_from(self.slots.len())
                .expect("Noon animation semantic node slot space exhausted");
            self.slots.push(AnimationSlot {
                generation: 0,
                node: None,
                next_free: None,
            });
            (slot_index, 0)
        };
        let id = AnimationNodeId::new(slot_index, generation);
        self.slots[slot_index as usize].node = Some(AnimationNode {
            id,
            kind,
            options: AnimationOptions::new(),
            parent: None,
        });
        self.live_nodes += 1;
        id
    }

    pub fn node(&self, id: AnimationNodeId) -> Option<&AnimationNode> {
        let slot = self.slots.get(id.slot as usize)?;
        (slot.generation == id.generation)
            .then_some(slot.node.as_ref())
            .flatten()
    }

    fn node_mut(&mut self, id: AnimationNodeId) -> Option<&mut AnimationNode> {
        let slot = self.slots.get_mut(id.slot as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.node.as_mut()
    }

    pub fn set_options(
        &mut self,
        id: AnimationNodeId,
        options: AnimationOptions,
    ) -> Result<(), AnimationGraphError> {
        self.node_mut(id)
            .ok_or(AnimationGraphError::UnknownNode(id))?
            .options = options;
        Ok(())
    }

    pub fn set_lag_ratio(
        &mut self,
        id: AnimationNodeId,
        lag_ratio: f64,
    ) -> Result<(), AnimationGraphError> {
        validate_lag_ratio(lag_ratio)?;
        let node = self
            .node_mut(id)
            .ok_or(AnimationGraphError::UnknownNode(id))?;
        let AnimationNodeKind::Lagged {
            lag_ratio: current, ..
        } = &mut node.kind
        else {
            return Err(AnimationGraphError::NotLagged(id));
        };
        *current = lag_ratio;
        Ok(())
    }

    pub fn replace_child(
        &mut self,
        parent: AnimationNodeId,
        index: usize,
        replacement: AnimationNodeId,
    ) -> Result<AnimationNodeId, AnimationGraphError> {
        let replacement_node = self
            .node(replacement)
            .ok_or(AnimationGraphError::UnknownNode(replacement))?;
        if replacement_node.parent.is_some() {
            return Err(AnimationGraphError::AlreadyParented(replacement));
        }
        if self.contains(replacement, parent) {
            return Err(AnimationGraphError::Cycle {
                parent,
                child: replacement,
            });
        }
        let old = {
            let node = self
                .node(parent)
                .ok_or(AnimationGraphError::UnknownNode(parent))?;
            *node
                .kind
                .children()
                .get(index)
                .ok_or(AnimationGraphError::ChildIndex { parent, index })?
        };
        self.node_mut(parent)
            .and_then(|node| node.kind.children_mut())
            .expect("parent with indexed child must be a composition")[index] = replacement;
        self.node_mut(old)
            .expect("existing child must remain live")
            .parent = None;
        self.node_mut(replacement)
            .expect("replacement validated above")
            .parent = Some(parent);
        Ok(old)
    }

    pub fn remove_node(
        &mut self,
        id: AnimationNodeId,
    ) -> Result<AnimationNode, AnimationGraphError> {
        let node = self
            .node(id)
            .ok_or(AnimationGraphError::UnknownNode(id))?
            .clone();
        if node.parent.is_some() {
            return Err(AnimationGraphError::StillParented(id));
        }
        for &child in node.kind.children() {
            self.node_mut(child)
                .expect("live composition child must exist")
                .parent = None;
        }
        let slot = &mut self.slots[id.slot as usize];
        let removed = slot.node.take().expect("node validated above");
        slot.generation = slot.generation.wrapping_add(1);
        slot.next_free = self.free_head;
        self.free_head = Some(id.slot);
        self.live_nodes -= 1;
        Ok(removed)
    }

    pub fn root_for(&self, id: AnimationNodeId) -> Result<AnimationNodeId, AnimationGraphError> {
        let mut current = id;
        loop {
            let node = self
                .node(current)
                .ok_or(AnimationGraphError::UnknownNode(current))?;
            match node.parent {
                Some(parent) => current = parent,
                None => return Ok(current),
            }
        }
    }

    pub fn len(&self) -> usize {
        self.live_nodes
    }

    pub fn is_empty(&self) -> bool {
        self.live_nodes == 0
    }

    fn contains(&self, root: AnimationNodeId, target: AnimationNodeId) -> bool {
        if root == target {
            return true;
        }
        let Some(node) = self.node(root) else {
            return false;
        };
        node.kind
            .children()
            .iter()
            .copied()
            .any(|child| self.contains(child, target))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnimationTrackOrigin {
    pub leaf: AnimationNodeId,
    pub track_index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationLoweringContext {
    pub start_time: f64,
    /// `Scene.play`-level overrides. They apply to the root while child options
    /// remain independently retained on their semantic nodes.
    pub play_options: AnimationOptions,
}

impl AnimationLoweringContext {
    pub const fn new(start_time: f64) -> Self {
        Self {
            start_time,
            play_options: AnimationOptions::new(),
        }
    }

    pub const fn with_play_options(mut self, options: AnimationOptions) -> Self {
        self.play_options = options;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimationRelowerStats {
    pub nodes_visited: usize,
    pub tracks_added: usize,
    pub tracks_replaced: usize,
    pub tracks_removed: usize,
    pub tracks_unchanged: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimationRelowerResult {
    pub patches: Vec<ScenePatch>,
    pub stats: AnimationRelowerStats,
}

#[derive(Clone, Debug, Default)]
pub struct AnimationLowering {
    origin_tracks: HashMap<AnimationTrackOrigin, TrackId>,
    track_origins: HashMap<TrackId, AnimationTrackOrigin>,
    root_origins: HashMap<AnimationNodeId, Vec<AnimationTrackOrigin>>,
    root_contexts: HashMap<AnimationNodeId, AnimationLoweringContext>,
}

impl AnimationLowering {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn track_for_origin(&self, origin: AnimationTrackOrigin) -> Option<TrackId> {
        self.origin_tracks.get(&origin).copied()
    }

    pub fn origin_for_track(&self, track: TrackId) -> Option<AnimationTrackOrigin> {
        self.track_origins.get(&track).copied()
    }

    pub fn lower_root(
        &mut self,
        graph: &AnimationGraph,
        scene: &mut SceneDefinition,
        root: AnimationNodeId,
        context: AnimationLoweringContext,
    ) -> Result<AnimationRelowerResult, AnimationGraphError> {
        if graph.root_for(root)? != root {
            return Err(AnimationGraphError::NotRoot(root));
        }
        self.relower_root_with_context(graph, scene, root, context)
    }

    pub fn relower_root(
        &mut self,
        graph: &AnimationGraph,
        scene: &mut SceneDefinition,
        root: AnimationNodeId,
    ) -> Result<AnimationRelowerResult, AnimationGraphError> {
        let context = self
            .root_contexts
            .get(&root)
            .copied()
            .ok_or(AnimationGraphError::RootNotLowered(root))?;
        self.relower_root_with_context(graph, scene, root, context)
    }

    /// Re-lower only the composition root containing `edited`.
    ///
    /// Unrelated roots are neither walked nor rewritten. The returned patches can
    /// be fed directly to the runtime transaction path, whose event scheduler then
    /// re-lowers only the affected execution channels.
    pub fn relower_edited_subtree(
        &mut self,
        graph: &AnimationGraph,
        scene: &mut SceneDefinition,
        edited: AnimationNodeId,
    ) -> Result<AnimationRelowerResult, AnimationGraphError> {
        let root = graph.root_for(edited)?;
        self.relower_root(graph, scene, root)
    }

    fn relower_root_with_context(
        &mut self,
        graph: &AnimationGraph,
        scene: &mut SceneDefinition,
        root: AnimationNodeId,
        context: AnimationLoweringContext,
    ) -> Result<AnimationRelowerResult, AnimationGraphError> {
        if !context.start_time.is_finite() || context.start_time < 0.0 {
            return Err(AnimationGraphError::InvalidStartTime(context.start_time));
        }
        let mut stats = AnimationRelowerStats::default();
        let root_duration = node_duration(graph, root, Some(context.play_options), &mut stats)?;
        if !root_duration.is_finite() || root_duration <= 0.0 {
            return Err(AnimationGraphError::InvalidRunTime(root_duration));
        }
        let mut candidates = Vec::new();
        lower_node(
            graph,
            root,
            LowerRootContext {
                root,
                context,
                root_duration,
            },
            &[],
            &mut candidates,
            &mut stats,
        )?;

        let old_origins = self.root_origins.get(&root).cloned().unwrap_or_default();
        let candidate_origins = candidates
            .iter()
            .map(|candidate| candidate.origin)
            .collect::<Vec<_>>();
        let candidate_set = candidate_origins.iter().copied().collect::<HashSet<_>>();
        let mut patches = Vec::new();

        for origin in old_origins {
            if candidate_set.contains(&origin) {
                continue;
            }
            if let Some(track) = self.origin_tracks.get(&origin).copied() {
                patches.push(ScenePatch::RemoveTrack(track));
                stats.tracks_removed += 1;
            }
        }

        let mut next_track_id = scene.next_track_id;
        let mut assignments = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if scene.object(candidate.track.object).is_none() {
                return Err(AnimationGraphError::UnknownObject(candidate.track.object));
            }
            if let Some(track_id) = self.origin_tracks.get(&candidate.origin).copied() {
                let mut track = candidate.track;
                track.id = track_id;
                let existing = scene
                    .tracks
                    .iter()
                    .find(|existing| existing.id == track_id)
                    .ok_or(AnimationGraphError::MissingLoweredTrack(track_id))?;
                if existing == &track {
                    stats.tracks_unchanged += 1;
                } else {
                    patches.push(ScenePatch::ReplaceTrack(track));
                    stats.tracks_replaced += 1;
                }
                assignments.push((candidate.origin, track_id));
            } else {
                let id = TrackId::new(next_track_id);
                next_track_id = next_track_id
                    .checked_add(1)
                    .ok_or(AnimationGraphError::TrackIdExhausted)?;
                let mut track = candidate.track;
                track.id = id;
                patches.push(ScenePatch::AddTrack(track));
                stats.tracks_added += 1;
                assignments.push((candidate.origin, id));
            }
        }

        let transaction = MutationTransaction::from_mutations(patches.iter().cloned());
        scene.apply_transaction(&transaction)?;

        if let Some(previous) = self.root_origins.insert(root, candidate_origins) {
            for origin in previous {
                if !candidate_set.contains(&origin) {
                    if let Some(track) = self.origin_tracks.remove(&origin) {
                        self.track_origins.remove(&track);
                    }
                }
            }
        }
        for (origin, track) in assignments {
            self.origin_tracks.insert(origin, track);
            self.track_origins.insert(track, origin);
        }
        self.root_contexts.insert(root, context);
        Ok(AnimationRelowerResult { patches, stats })
    }
}

#[derive(Clone, Debug)]
struct CandidateTrack {
    origin: AnimationTrackOrigin,
    track: TrackDefinition,
}

#[derive(Clone, Copy)]
struct LowerRootContext {
    root: AnimationNodeId,
    context: AnimationLoweringContext,
    root_duration: f64,
}

fn lower_node(
    graph: &AnimationGraph,
    node_id: AnimationNodeId,
    lowering: LowerRootContext,
    inherited_steps: &[CompositionTimeMapStep],
    output: &mut Vec<CandidateTrack>,
    stats: &mut AnimationRelowerStats,
) -> Result<(), AnimationGraphError> {
    let root = lowering.root;
    let context = lowering.context;
    let root_duration = lowering.root_duration;
    let node = graph
        .node(node_id)
        .ok_or(AnimationGraphError::UnknownNode(node_id))?;
    stats.nodes_visited += 1;
    match &node.kind {
        AnimationNodeKind::Leaf { tracks } => {
            let intrinsic = leaf_intrinsic_run_time(tracks)?;
            let rate_func = if node_id == root {
                context
                    .play_options
                    .rate_func
                    .or(node.options.rate_func)
                    .unwrap_or(RateFunction::Linear)
            } else {
                node.options.rate_func.unwrap_or(RateFunction::Linear)
            };
            for (index, template) in tracks.iter().enumerate() {
                let origin = AnimationTrackOrigin {
                    leaf: node_id,
                    track_index: u32::try_from(index)
                        .expect("animation leaf track count exceeds u32 limits"),
                };
                if template.property.is_instant() {
                    let local_alpha = if intrinsic > 0.0 {
                        template.timing.start_time / intrinsic
                    } else {
                        0.0
                    };
                    let root_alpha = invert_steps_for_boundary(inherited_steps, local_alpha)?;
                    let start_time = context.start_time + root_duration * root_alpha;
                    let track = TrackDefinition {
                        id: TrackId::new(0),
                        object: template.object,
                        property: template.property,
                        values: template.values.clone(),
                        timing: TrackTiming::instant(start_time),
                        origin: Some(origin),
                        time_map: CompositionTimeMap::identity(),
                    };
                    crate::validate_track_definition(&track)?;
                    output.push(CandidateTrack { origin, track });
                    continue;
                }
                let mut steps = inherited_steps.to_vec();
                let local_start = template.timing.start_time / intrinsic.max(f64::EPSILON);
                let local_duration = template.timing.duration / intrinsic.max(f64::EPSILON);
                steps.push(CompositionTimeMapStep::new(
                    local_start.clamp(0.0, 1.0),
                    local_duration.clamp(f64::EPSILON, 1.0),
                    rate_func,
                ));
                let track = TrackDefinition {
                    id: TrackId::new(0),
                    object: template.object,
                    property: template.property,
                    values: template.values.clone(),
                    timing: TrackTiming::new(
                        context.start_time,
                        root_duration,
                        template.timing.easing,
                    ),
                    origin: Some(origin),
                    time_map: CompositionTimeMap::from_steps(steps),
                };
                crate::validate_track_definition(&track)?;
                output.push(CandidateTrack { origin, track });
            }
        }
        AnimationNodeKind::Parallel { children }
        | AnimationNodeKind::Sequence { children }
        | AnimationNodeKind::Lagged { children, .. } => {
            let child_durations = children
                .iter()
                .copied()
                .map(|child| node_duration(graph, child, None, stats))
                .collect::<Result<Vec<_>, _>>()?;
            let option_lag_ratio = if node_id == root {
                context.play_options.lag_ratio.or(node.options.lag_ratio)
            } else {
                node.options.lag_ratio
            };
            let lag_ratio = match &node.kind {
                AnimationNodeKind::Parallel { .. } => option_lag_ratio.unwrap_or(0.0),
                AnimationNodeKind::Sequence { .. } => option_lag_ratio.unwrap_or(1.0),
                AnimationNodeKind::Lagged { lag_ratio, .. } => {
                    option_lag_ratio.unwrap_or(*lag_ratio)
                }
                AnimationNodeKind::Leaf { .. } => unreachable!(),
            };
            validate_lag_ratio(lag_ratio)?;
            let intrinsic_schedule =
                crate::resolve_composition_schedule(&child_durations, lag_ratio, None)?;
            let requested_run_time = if node_id == root {
                context.play_options.run_time.or(node.options.run_time)
            } else {
                node.options.run_time
            };
            let node_run_time = requested_run_time.unwrap_or(intrinsic_schedule.run_time);
            if !node_run_time.is_finite() || node_run_time <= 0.0 {
                return Err(AnimationGraphError::InvalidRunTime(node_run_time));
            }
            let rate_func = if node_id == root {
                context
                    .play_options
                    .rate_func
                    .or(node.options.rate_func)
                    .unwrap_or(RateFunction::Linear)
            } else {
                node.options.rate_func.unwrap_or(RateFunction::Linear)
            };
            let schedule = crate::resolve_composition_schedule(
                &child_durations,
                lag_ratio,
                Some(node_run_time),
            )?;
            for (&child, interval) in children.iter().zip(&schedule.intervals) {
                let mut steps = inherited_steps.to_vec();
                steps.push(CompositionTimeMapStep::new(
                    interval.start_time / node_run_time,
                    interval.duration / node_run_time,
                    rate_func,
                ));
                lower_node(graph, child, lowering, &steps, output, stats)?;
            }
        }
    }
    Ok(())
}

fn node_duration(
    graph: &AnimationGraph,
    node_id: AnimationNodeId,
    root_play_options: Option<AnimationOptions>,
    stats: &mut AnimationRelowerStats,
) -> Result<f64, AnimationGraphError> {
    let node = graph
        .node(node_id)
        .ok_or(AnimationGraphError::UnknownNode(node_id))?;
    stats.nodes_visited += 1;
    let intrinsic = match &node.kind {
        AnimationNodeKind::Leaf { tracks } => leaf_intrinsic_run_time(tracks)?,
        AnimationNodeKind::Parallel { children }
        | AnimationNodeKind::Sequence { children }
        | AnimationNodeKind::Lagged { children, .. } => {
            let child_durations = children
                .iter()
                .copied()
                .map(|child| node_duration(graph, child, None, stats))
                .collect::<Result<Vec<_>, _>>()?;
            let option_lag_ratio = root_play_options
                .and_then(|options| options.lag_ratio)
                .or(node.options.lag_ratio);
            let lag_ratio = match &node.kind {
                AnimationNodeKind::Parallel { .. } => option_lag_ratio.unwrap_or(0.0),
                AnimationNodeKind::Sequence { .. } => option_lag_ratio.unwrap_or(1.0),
                AnimationNodeKind::Lagged { lag_ratio, .. } => {
                    option_lag_ratio.unwrap_or(*lag_ratio)
                }
                AnimationNodeKind::Leaf { .. } => unreachable!(),
            };
            validate_lag_ratio(lag_ratio)?;
            crate::resolve_composition_schedule(&child_durations, lag_ratio, None)?.run_time
        }
    };
    let requested = root_play_options
        .and_then(|options| options.run_time)
        .or(node.options.run_time);
    node_effective_run_time(intrinsic, requested)
}

fn node_effective_run_time(
    intrinsic: f64,
    requested: Option<f64>,
) -> Result<f64, AnimationGraphError> {
    let value = requested.unwrap_or(intrinsic);
    if !value.is_finite() || value <= 0.0 {
        return Err(AnimationGraphError::InvalidRunTime(value));
    }
    Ok(value)
}

fn leaf_intrinsic_run_time(tracks: &[AnimationTrackTemplate]) -> Result<f64, AnimationGraphError> {
    if tracks.is_empty() {
        return Err(AnimationGraphError::EmptyLeaf);
    }
    let mut end = 0.0_f64;
    for template in tracks {
        crate::validate_track_definition(&TrackDefinition {
            id: TrackId::new(0),
            object: template.object,
            property: template.property,
            values: template.values.clone(),
            timing: template.timing,
            origin: None,
            time_map: CompositionTimeMap::identity(),
        })?;
        end = end.max(template.timing.start_time + template.timing.duration);
    }
    if end <= 0.0 {
        // An all-instant lifecycle leaf still needs a finite execution interval so
        // its boundary can be placed in a composition.
        Ok(1.0)
    } else {
        Ok(end)
    }
}

/// Map a local instant boundary back through linear composition intervals.
/// Nonlinear/reversing parent rates make an instant boundary non-unique; callers
/// must represent such lifecycle behavior explicitly rather than silently choose
/// a different semantic event time.
fn invert_steps_for_boundary(
    steps: &[CompositionTimeMapStep],
    mut local_alpha: f64,
) -> Result<f64, AnimationGraphError> {
    for step in steps.iter().rev() {
        if step.rate_func != RateFunction::Linear {
            return Err(AnimationGraphError::NonlinearInstantBoundary);
        }
        local_alpha = step.start + local_alpha * step.duration;
    }
    Ok(local_alpha.clamp(0.0, 1.0))
}

fn validate_lag_ratio(value: f64) -> Result<(), AnimationGraphError> {
    if !value.is_finite() || value < 0.0 {
        return Err(AnimationGraphError::InvalidLagRatio(value));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub enum AnimationGraphError {
    UnknownNode(AnimationNodeId),
    UnknownObject(ObjectId),
    AlreadyParented(AnimationNodeId),
    StillParented(AnimationNodeId),
    DuplicateChild(AnimationNodeId),
    EmptyComposition,
    EmptyLeaf,
    NotLagged(AnimationNodeId),
    NotRoot(AnimationNodeId),
    RootNotLowered(AnimationNodeId),
    ChildIndex {
        parent: AnimationNodeId,
        index: usize,
    },
    Cycle {
        parent: AnimationNodeId,
        child: AnimationNodeId,
    },
    MissingLoweredTrack(TrackId),
    InvalidStartTime(f64),
    InvalidRunTime(f64),
    InvalidLagRatio(f64),
    NonlinearInstantBoundary,
    TrackIdExhausted,
    Timeline(TimelineError),
    Patch(PatchError),
    Composition(crate::CompositionError),
}

impl std::fmt::Display for AnimationGraphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownNode(id) => write!(formatter, "unknown animation node {id:?}"),
            Self::UnknownObject(id) => write!(formatter, "unknown animation object {}", id.get()),
            Self::AlreadyParented(id) => write!(formatter, "animation node {id:?} already has a parent"),
            Self::StillParented(id) => write!(formatter, "animation node {id:?} must be detached before removal"),
            Self::DuplicateChild(id) => write!(formatter, "animation node {id:?} appears twice in one composition"),
            Self::EmptyComposition => formatter.write_str("animation composition requires children"),
            Self::EmptyLeaf => formatter.write_str("animation leaf requires at least one track"),
            Self::NotLagged(id) => write!(formatter, "animation node {id:?} is not a Lagged composition"),
            Self::NotRoot(id) => write!(formatter, "animation node {id:?} is not a graph root"),
            Self::RootNotLowered(id) => write!(formatter, "animation root {id:?} has not been lowered"),
            Self::ChildIndex { parent, index } => write!(formatter, "animation child index {index} is out of range for {parent:?}"),
            Self::Cycle { parent, child } => write!(formatter, "adding {child:?} below {parent:?} would create a cycle"),
            Self::MissingLoweredTrack(id) => write!(formatter, "lowered track {} is missing from the scene", id.get()),
            Self::InvalidStartTime(value) => write!(formatter, "animation start time must be finite and non-negative, got {value}"),
            Self::InvalidRunTime(value) => write!(formatter, "animation run time must be finite and positive, got {value}"),
            Self::InvalidLagRatio(value) => write!(formatter, "animation lag ratio must be finite and non-negative, got {value}"),
            Self::NonlinearInstantBoundary => formatter.write_str("instant lifecycle events under nonlinear composition timing require an explicit semantic boundary"),
            Self::TrackIdExhausted => formatter.write_str("Noon track ID space exhausted"),
            Self::Timeline(error) => error.fmt(formatter),
            Self::Patch(error) => error.fmt(formatter),
            Self::Composition(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for AnimationGraphError {}

impl From<TimelineError> for AnimationGraphError {
    fn from(value: TimelineError) -> Self {
        Self::Timeline(value)
    }
}

impl From<PatchError> for AnimationGraphError {
    fn from(value: PatchError) -> Self {
        Self::Patch(value)
    }
}

impl From<crate::CompositionError> for AnimationGraphError {
    fn from(value: crate::CompositionError) -> Self {
        Self::Composition(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GeometryRef, Vec2};

    fn position_leaf(
        graph: &mut AnimationGraph,
        object: ObjectId,
        from: f32,
        to: f32,
        duration: f64,
    ) -> AnimationNodeId {
        graph.insert_leaf(vec![AnimationTrackTemplate::new(
            object,
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::new(from, 0.0),
                to: Vec2::new(to, 0.0),
            },
            TrackTiming::new(0.0, duration, RateFunction::Linear),
        )])
    }

    #[test]
    fn node_ids_are_stable_and_stale_generations_do_not_alias() {
        let mut graph = AnimationGraph::new();
        let first = graph.insert_leaf(vec![AnimationTrackTemplate::new(
            ObjectId::new(0),
            Property::Presence,
            TrackValues::Bool {
                from: false,
                to: true,
            },
            TrackTiming::instant(0.0),
        )]);
        graph.remove_node(first).unwrap();
        let second = graph.insert_leaf(vec![AnimationTrackTemplate::new(
            ObjectId::new(0),
            Property::Presence,
            TrackValues::Bool {
                from: false,
                to: true,
            },
            TrackTiming::instant(0.0),
        )]);
        assert_eq!(first.slot(), second.slot());
        assert_ne!(first.generation(), second.generation());
        assert!(graph.node(first).is_none());
    }

    #[test]
    fn relowering_one_root_preserves_unrelated_track_identity() {
        let mut scene = SceneDefinition::new();
        let first_object = scene.add(GeometryRef::circle(1.0));
        let second_object = scene.add(GeometryRef::circle(1.0));
        let mut graph = AnimationGraph::new();
        let first = position_leaf(&mut graph, first_object, 0.0, 10.0, 1.0);
        let second = position_leaf(&mut graph, second_object, 0.0, 20.0, 1.0);
        let mut lowering = AnimationLowering::new();
        lowering
            .lower_root(
                &graph,
                &mut scene,
                first,
                AnimationLoweringContext::new(0.0),
            )
            .unwrap();
        lowering
            .lower_root(
                &graph,
                &mut scene,
                second,
                AnimationLoweringContext::new(0.0),
            )
            .unwrap();
        let second_origin = AnimationTrackOrigin {
            leaf: second,
            track_index: 0,
        };
        let second_track = lowering.track_for_origin(second_origin).unwrap();
        let second_definition = scene
            .tracks()
            .iter()
            .find(|track| track.id == second_track)
            .unwrap()
            .clone();

        graph
            .set_options(first, AnimationOptions::new().run_time(2.0))
            .unwrap();
        let result = lowering
            .relower_edited_subtree(&graph, &mut scene, first)
            .unwrap();
        assert_eq!(result.stats.tracks_replaced, 1);
        assert_eq!(result.stats.tracks_added, 0);
        assert_eq!(result.stats.tracks_removed, 0);
        assert_eq!(lowering.track_for_origin(second_origin), Some(second_track));
        assert_eq!(
            scene.tracks().iter().find(|track| track.id == second_track),
            Some(&second_definition)
        );
    }

    #[test]
    fn child_replacement_changes_only_affected_origins() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let mut graph = AnimationGraph::new();
        let first = position_leaf(&mut graph, object, 0.0, 1.0, 1.0);
        let second = position_leaf(&mut graph, object, 1.0, 2.0, 1.0);
        let root = graph.insert_sequence(vec![first, second]).unwrap();
        let mut lowering = AnimationLowering::new();
        lowering
            .lower_root(&graph, &mut scene, root, AnimationLoweringContext::new(0.0))
            .unwrap();
        let first_origin = AnimationTrackOrigin {
            leaf: first,
            track_index: 0,
        };
        let first_track = lowering.track_for_origin(first_origin).unwrap();

        let replacement = position_leaf(&mut graph, object, 1.0, 3.0, 1.0);
        assert_eq!(graph.replace_child(root, 1, replacement).unwrap(), second);
        let result = lowering
            .relower_edited_subtree(&graph, &mut scene, root)
            .unwrap();
        assert_eq!(lowering.track_for_origin(first_origin), Some(first_track));
        assert_eq!(result.stats.tracks_removed, 1);
        assert_eq!(result.stats.tracks_added, 1);
    }

    #[test]
    fn nested_composition_lowers_origin_metadata_and_time_maps() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let mut graph = AnimationGraph::new();
        let first = position_leaf(&mut graph, object, 0.0, 1.0, 1.0);
        let second = position_leaf(&mut graph, object, 1.0, 2.0, 1.0);
        let lagged = graph.insert_lagged(vec![first, second], 0.5).unwrap();
        graph
            .set_options(lagged, AnimationOptions::new().run_time(3.0))
            .unwrap();
        let mut lowering = AnimationLowering::new();
        lowering
            .lower_root(
                &graph,
                &mut scene,
                lagged,
                AnimationLoweringContext::new(2.0),
            )
            .unwrap();

        assert_eq!(scene.tracks().len(), 2);
        for track in scene.tracks() {
            assert_eq!(track.timing.start_time, 2.0);
            assert_eq!(track.timing.duration, 3.0);
            assert!(!track.time_map.is_identity());
            assert!(lowering.origin_for_track(track.id).is_some());
        }
    }
    #[test]
    fn scene_play_lag_ratio_overrides_root_composition_option() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let mut graph = AnimationGraph::new();
        let first = position_leaf(&mut graph, object, 0.0, 1.0, 1.0);
        let second = position_leaf(&mut graph, object, 1.0, 2.0, 1.0);
        let root = graph.insert_parallel(vec![first, second]).unwrap();
        graph
            .set_options(root, AnimationOptions::new().lag_ratio(0.25))
            .unwrap();
        let mut lowering = AnimationLowering::new();
        lowering
            .lower_root(
                &graph,
                &mut scene,
                root,
                AnimationLoweringContext::new(0.0)
                    .with_play_options(AnimationOptions::new().lag_ratio(0.5)),
            )
            .unwrap();

        assert_eq!(scene.tracks().len(), 2);
        assert!(scene
            .tracks()
            .iter()
            .all(|track| (track.timing.duration - 1.5).abs() < 1e-12));
    }
}
