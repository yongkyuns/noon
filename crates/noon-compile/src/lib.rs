//! Compiler from Noon's authoring-oriented scene definition to dense runtime data.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use noon_core::{
    GeometryRef, ObjectId, Property, SceneDefinition, ScenePatch, Style, TimelineError,
    TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, Vec2, VectorPath,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicProperties {
    pub presence: bool,
    pub transform: bool,
    pub position: bool,
    pub rotation: bool,
    pub opacity: bool,
    pub appearance: bool,
    pub reveal: bool,
    pub morph: bool,
}

impl DynamicProperties {
    fn mark(&mut self, property: Property) {
        match property {
            Property::Presence => self.presence = true,
            Property::Transform => self.transform = true,
            Property::Position => self.position = true,
            Property::Rotation => self.rotation = true,
            Property::Opacity => self.opacity = true,
            Property::Appearance => self.appearance = true,
            Property::Reveal => self.reveal = true,
            Property::Morph => self.morph = true,
        }
    }

    pub const fn any(self) -> bool {
        self.presence
            || self.transform
            || self.position
            || self.rotation
            || self.opacity
            || self.appearance
            || self.reveal
            || self.morph
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledObject {
    pub id: ObjectId,
    pub geometry: GeometryRef,
    pub base_transform: Transform2D,
    pub base_style: Style,
    pub dynamic: DynamicProperties,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TransformGeometryPlan {
    Static,
    Circle {
        from_radius: f32,
        to_radius: f32,
    },
    Rectangle {
        from_size: noon_core::Vec2,
        to_size: noon_core::Vec2,
    },
    Line {
        from_start: noon_core::Vec2,
        from_end: noon_core::Vec2,
        to_start: noon_core::Vec2,
        to_end: noon_core::Vec2,
    },
    /// Fixed source/target topology prepared once for the path renderer.
    PathPair(GeometryRef),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledTrack {
    pub id: TrackId,
    pub object_index: u32,
    pub property: Property,
    pub values: TrackValues,
    pub timing: TrackTiming,
    /// Compiler-selected geometry interpolation strategy for an atomic Transform.
    /// Non-Transform tracks carry `None`.
    pub transform_geometry_plan: Option<TransformGeometryPlan>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledScene {
    objects: Vec<CompiledObject>,
    tracks: Vec<CompiledTrack>,
    object_indices: BTreeMap<ObjectId, u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    TooManyObjects(usize),
    UnknownObject(ObjectId),
    DiscontinuousPresence { previous: TrackId, next: TrackId },
    UnsupportedTransformGeometry(TrackId),
    PathTransformRequiresRetessellation(TrackId),
    UnsafeFilledPathTransform(TrackId),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyObjects(count) => {
                write!(formatter, "scene contains too many objects: {count}")
            }
            Self::UnknownObject(id) => {
                write!(formatter, "track references unknown object {}", id.get())
            }
            Self::DiscontinuousPresence { previous, next } => write!(
                formatter,
                "presence track {} does not hand off continuously to track {}",
                previous.get(),
                next.get()
            ),
            Self::UnsupportedTransformGeometry(id) => write!(
                formatter,
                "transform track {} uses unsupported geometry interpolation",
                id.get()
            ),
            Self::PathTransformRequiresRetessellation(id) => write!(
                formatter,
                "transform track {} changes path fill presence, stroke topology, or stroke width",
                id.get()
            ),
            Self::UnsafeFilledPathTransform(id) => write!(
                formatter,
                "transform track {} uses filled path geometry without a stable fixed triangulation",
                id.get()
            ),
        }
    }
}

impl std::error::Error for CompileError {}

#[derive(Clone, Debug, PartialEq)]
pub enum CompilePatchError {
    TooManyObjects(usize),
    DuplicateObject(ObjectId),
    UnknownObject(ObjectId),
    DuplicateTrack(TrackId),
    UnknownTrack(TrackId),
    InvalidTrack(TimelineError),
    DiscontinuousPresence { previous: TrackId, next: TrackId },
    UnsupportedTransformGeometry(TrackId),
    PathTransformRequiresRetessellation(TrackId),
    UnsafeFilledPathTransform(TrackId),
}

impl std::fmt::Display for CompilePatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyObjects(count) => {
                write!(formatter, "scene contains too many objects: {count}")
            }
            Self::DuplicateObject(id) => write!(formatter, "duplicate object id {}", id.get()),
            Self::UnknownObject(id) => write!(formatter, "unknown object id {}", id.get()),
            Self::DuplicateTrack(id) => write!(formatter, "duplicate track id {}", id.get()),
            Self::UnknownTrack(id) => write!(formatter, "unknown track id {}", id.get()),
            Self::InvalidTrack(error) => write!(formatter, "invalid track: {error}"),
            Self::DiscontinuousPresence { previous, next } => write!(
                formatter,
                "presence track {} does not hand off continuously to track {}",
                previous.get(),
                next.get()
            ),
            Self::UnsupportedTransformGeometry(id) => write!(
                formatter,
                "transform track {} uses unsupported geometry interpolation",
                id.get()
            ),
            Self::PathTransformRequiresRetessellation(id) => write!(
                formatter,
                "transform track {} changes path fill presence, stroke topology, or stroke width",
                id.get()
            ),
            Self::UnsafeFilledPathTransform(id) => write!(
                formatter,
                "transform track {} uses filled path geometry without a stable fixed triangulation",
                id.get()
            ),
        }
    }
}

impl std::error::Error for CompilePatchError {}

impl CompiledScene {
    pub fn compile(scene: &SceneDefinition) -> Result<Self, CompileError> {
        let mut object_indices = BTreeMap::new();
        let mut objects = Vec::with_capacity(scene.objects().len());

        for (index, object) in scene.objects().iter().enumerate() {
            let index = u32::try_from(index)
                .map_err(|_| CompileError::TooManyObjects(scene.objects().len()))?;
            object_indices.insert(object.id, index);
            objects.push(CompiledObject {
                id: object.id,
                geometry: object.geometry.clone(),
                base_transform: object.transform,
                base_style: object.style,
                dynamic: DynamicProperties::default(),
            });
        }

        let mut tracks = Vec::with_capacity(scene.tracks().len());
        for track in scene.tracks() {
            let object_index = *object_indices
                .get(&track.object)
                .ok_or(CompileError::UnknownObject(track.object))?;
            objects[object_index as usize].dynamic.mark(track.property);
            tracks.push(
                compile_track(track, object_index)
                    .map_err(|error| compile_error(track.id, error))?,
            );
        }
        sort_tracks(&mut tracks);
        validate_presence_chains(&tracks)
            .map_err(|(previous, next)| CompileError::DiscontinuousPresence { previous, next })?;

        Ok(Self {
            objects,
            tracks,
            object_indices,
        })
    }

    pub fn objects(&self) -> &[CompiledObject] {
        &self.objects
    }

    pub fn tracks(&self) -> &[CompiledTrack] {
        &self.tracks
    }

    pub fn object_index(&self, id: ObjectId) -> Option<u32> {
        self.object_indices.get(&id).copied()
    }

    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<(), CompilePatchError> {
        match patch {
            ScenePatch::CreateObject(object) => {
                if self.object_indices.contains_key(&object.id) {
                    return Err(CompilePatchError::DuplicateObject(object.id));
                }
                let index = u32::try_from(self.objects.len())
                    .map_err(|_| CompilePatchError::TooManyObjects(self.objects.len()))?;
                self.objects.push(CompiledObject {
                    id: object.id,
                    geometry: object.geometry.clone(),
                    base_transform: object.transform,
                    base_style: object.style,
                    dynamic: DynamicProperties::default(),
                });
                self.object_indices.insert(object.id, index);
            }
            ScenePatch::RemoveObject(id) => {
                let index = self
                    .object_index(*id)
                    .ok_or(CompilePatchError::UnknownObject(*id))?;
                self.objects.remove(index as usize);
                self.tracks.retain(|track| track.object_index != index);
                for track in &mut self.tracks {
                    if track.object_index > index {
                        track.object_index -= 1;
                    }
                }
                self.rebuild_object_indices();
                self.recompute_dynamic();
            }
            ScenePatch::SetTransform { object, transform } => {
                let index = self
                    .object_index(*object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                self.objects[index as usize].base_transform = *transform;
            }
            ScenePatch::SetStyle { object, style } => {
                let index = self
                    .object_index(*object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                self.objects[index as usize].base_style = *style;
            }
            ScenePatch::AddTrack(track) => {
                if self.tracks.iter().any(|existing| existing.id == track.id) {
                    return Err(CompilePatchError::DuplicateTrack(track.id));
                }
                let compiled = self.compile_patch_track(track)?;
                let mut tracks = self.tracks.clone();
                tracks.push(compiled);
                sort_tracks(&mut tracks);
                validate_presence_chains(&tracks).map_err(|(previous, next)| {
                    CompilePatchError::DiscontinuousPresence { previous, next }
                })?;
                self.tracks = tracks;
                self.recompute_dynamic();
            }
            ScenePatch::ReplaceTrack(track) => {
                let position = self
                    .tracks
                    .iter()
                    .position(|existing| existing.id == track.id)
                    .ok_or(CompilePatchError::UnknownTrack(track.id))?;
                let compiled = self.compile_patch_track(track)?;
                let mut tracks = self.tracks.clone();
                tracks[position] = compiled;
                sort_tracks(&mut tracks);
                validate_presence_chains(&tracks).map_err(|(previous, next)| {
                    CompilePatchError::DiscontinuousPresence { previous, next }
                })?;
                self.tracks = tracks;
                self.recompute_dynamic();
            }
            ScenePatch::RemoveTrack(id) => {
                let position = self
                    .tracks
                    .iter()
                    .position(|track| track.id == *id)
                    .ok_or(CompilePatchError::UnknownTrack(*id))?;
                let mut tracks = self.tracks.clone();
                tracks.remove(position);
                validate_presence_chains(&tracks).map_err(|(previous, next)| {
                    CompilePatchError::DiscontinuousPresence { previous, next }
                })?;
                self.tracks = tracks;
                self.recompute_dynamic();
            }
        }
        Ok(())
    }

    fn compile_patch_track(
        &self,
        track: &TrackDefinition,
    ) -> Result<CompiledTrack, CompilePatchError> {
        let object_index = self
            .object_index(track.object)
            .ok_or(CompilePatchError::UnknownObject(track.object))?;
        if !track.timing.start_time.is_finite() {
            return Err(CompilePatchError::InvalidTrack(
                TimelineError::InvalidStartTime(track.timing.start_time),
            ));
        }
        if !track.timing.duration.is_finite() {
            return Err(CompilePatchError::InvalidTrack(
                TimelineError::InvalidDuration(track.timing.duration),
            ));
        }
        if track.property.is_instant() {
            if track.timing.duration != 0.0 {
                return Err(CompilePatchError::InvalidTrack(
                    TimelineError::InvalidInstantDuration {
                        property: track.property,
                        duration: track.timing.duration,
                    },
                ));
            }
        } else if track.timing.duration <= 0.0 {
            return Err(CompilePatchError::InvalidTrack(
                TimelineError::InvalidDuration(track.timing.duration),
            ));
        }
        let expected = track.property.value_kind();
        let actual = track.values.value_kind();
        if expected != actual {
            return Err(CompilePatchError::InvalidTrack(
                TimelineError::ValueTypeMismatch {
                    property: track.property,
                    expected,
                    actual,
                },
            ));
        }
        compile_track(track, object_index).map_err(|error| compile_patch_error(track.id, error))
    }

    fn rebuild_object_indices(&mut self) {
        self.object_indices.clear();
        for (index, object) in self.objects.iter().enumerate() {
            let index = u32::try_from(index).expect("compiled object count already validated");
            self.object_indices.insert(object.id, index);
        }
    }

    fn recompute_dynamic(&mut self) {
        for object in &mut self.objects {
            object.dynamic = DynamicProperties::default();
        }
        for track in &self.tracks {
            self.objects[track.object_index as usize]
                .dynamic
                .mark(track.property);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransformCompileFailure {
    UnsupportedGeometry,
    RequiresRetessellation,
    UnsafeFilledPath,
}

fn compile_track(
    track: &TrackDefinition,
    object_index: u32,
) -> Result<CompiledTrack, TransformCompileFailure> {
    Ok(CompiledTrack {
        id: track.id,
        object_index,
        property: track.property,
        values: track.values.clone(),
        timing: track.timing,
        transform_geometry_plan: compile_transform_geometry_plan(track)?,
    })
}

fn compile_transform_geometry_plan(
    track: &TrackDefinition,
) -> Result<Option<TransformGeometryPlan>, TransformCompileFailure> {
    if track.property != Property::Transform {
        return Ok(None);
    }
    let TrackValues::Object { from, to } = &track.values else {
        unreachable!("validated Transform track must contain object snapshots");
    };

    if let (GeometryRef::VectorPath(_), GeometryRef::VectorPath(_)) = (&from.geometry, &to.geometry)
    {
        if path_style_requires_retessellation(from.style, to.style) {
            return Err(TransformCompileFailure::RequiresRetessellation);
        }
    }

    if from.geometry == to.geometry {
        return Ok(Some(TransformGeometryPlan::Static));
    }

    let plan = match (&from.geometry, &to.geometry) {
        (
            GeometryRef::Circle {
                radius: from_radius,
            },
            GeometryRef::Circle { radius: to_radius },
        ) => TransformGeometryPlan::Circle {
            from_radius: *from_radius,
            to_radius: *to_radius,
        },
        (GeometryRef::Rectangle { size: from_size }, GeometryRef::Rectangle { size: to_size }) => {
            TransformGeometryPlan::Rectangle {
                from_size: *from_size,
                to_size: *to_size,
            }
        }
        (
            GeometryRef::Line {
                start: from_start,
                end: from_end,
            },
            GeometryRef::Line {
                start: to_start,
                end: to_end,
            },
        ) => TransformGeometryPlan::Line {
            from_start: *from_start,
            from_end: *from_end,
            to_start: *to_start,
            to_end: *to_end,
        },
        (GeometryRef::VectorPath(source), GeometryRef::VectorPath(target)) => {
            compile_path_pair(from, to, source.clone(), target.clone())?
        }
        (GeometryRef::Circle { .. }, GeometryRef::Rectangle { .. })
        | (GeometryRef::Rectangle { .. }, GeometryRef::Circle { .. }) => {
            let source = closed_analytic_path(&from.geometry)
                .expect("closed analytic source geometry must convert to a path");
            let target = closed_analytic_path(&to.geometry)
                .expect("closed analytic target geometry must convert to a path");
            compile_path_pair(from, to, source, target)?
        }
        _ => return Err(TransformCompileFailure::UnsupportedGeometry),
    };
    Ok(Some(plan))
}

fn path_style_requires_retessellation(from: Style, to: Style) -> bool {
    from.stroke_width.to_bits() != to.stroke_width.to_bits()
        || from.stroke_join != to.stroke_join
        || from.stroke_cap != to.stroke_cap
        || from.fill.is_some() != to.fill.is_some()
}

fn compile_path_pair(
    from: &noon_core::ObjectSnapshot,
    to: &noon_core::ObjectSnapshot,
    source: VectorPath,
    target: VectorPath,
) -> Result<TransformGeometryPlan, TransformCompileFailure> {
    if path_style_requires_retessellation(from.style, to.style) {
        return Err(TransformCompileFailure::RequiresRetessellation);
    }
    if from.style.fill.is_some()
        && noon_geometry::plan_filled_morph(&source, &target, noon_geometry::MorphOptions::DEFAULT)
            .is_err()
    {
        return Err(TransformCompileFailure::UnsafeFilledPath);
    }
    Ok(TransformGeometryPlan::PathPair(GeometryRef::path(
        source.with_morph_target(target),
    )))
}

fn closed_analytic_path(geometry: &GeometryRef) -> Option<VectorPath> {
    match geometry {
        GeometryRef::Circle { radius } => Some(circle_path(*radius)),
        GeometryRef::Rectangle { size } => Some(rectangle_path(*size)),
        _ => None,
    }
}

fn circle_path(radius: f32) -> VectorPath {
    // Standard four-cubic approximation. Cross-kind transforms intentionally
    // use path rendering only while active; same-kind Circle transforms retain
    // the exact analytic fast path.
    let handle = radius * 0.552_284_8;
    VectorPath::new()
        .move_to(Vec2::new(radius, 0.0))
        .cubic_to(
            Vec2::new(radius, handle),
            Vec2::new(handle, radius),
            Vec2::new(0.0, radius),
        )
        .cubic_to(
            Vec2::new(-handle, radius),
            Vec2::new(-radius, handle),
            Vec2::new(-radius, 0.0),
        )
        .cubic_to(
            Vec2::new(-radius, -handle),
            Vec2::new(-handle, -radius),
            Vec2::new(0.0, -radius),
        )
        .cubic_to(
            Vec2::new(handle, -radius),
            Vec2::new(radius, -handle),
            Vec2::new(radius, 0.0),
        )
        .close()
}

fn rectangle_path(size: Vec2) -> VectorPath {
    let half = size * 0.5;
    // Start at the right midpoint and include side midpoints so deterministic
    // correspondence lines up the rectangle's cardinal directions with the
    // circle's four cubic endpoints.
    VectorPath::new()
        .move_to(Vec2::new(half.x, 0.0))
        .line_to(Vec2::new(half.x, half.y))
        .line_to(Vec2::new(0.0, half.y))
        .line_to(Vec2::new(-half.x, half.y))
        .line_to(Vec2::new(-half.x, 0.0))
        .line_to(Vec2::new(-half.x, -half.y))
        .line_to(Vec2::new(0.0, -half.y))
        .line_to(Vec2::new(half.x, -half.y))
        .close()
}

fn compile_error(id: TrackId, error: TransformCompileFailure) -> CompileError {
    match error {
        TransformCompileFailure::UnsupportedGeometry => {
            CompileError::UnsupportedTransformGeometry(id)
        }
        TransformCompileFailure::RequiresRetessellation => {
            CompileError::PathTransformRequiresRetessellation(id)
        }
        TransformCompileFailure::UnsafeFilledPath => CompileError::UnsafeFilledPathTransform(id),
    }
}

fn compile_patch_error(id: TrackId, error: TransformCompileFailure) -> CompilePatchError {
    match error {
        TransformCompileFailure::UnsupportedGeometry => {
            CompilePatchError::UnsupportedTransformGeometry(id)
        }
        TransformCompileFailure::RequiresRetessellation => {
            CompilePatchError::PathTransformRequiresRetessellation(id)
        }
        TransformCompileFailure::UnsafeFilledPath => {
            CompilePatchError::UnsafeFilledPathTransform(id)
        }
    }
}

fn sort_tracks(tracks: &mut [CompiledTrack]) {
    tracks.sort_by(|left, right| {
        left.object_index
            .cmp(&right.object_index)
            .then_with(|| property_rank(left.property).cmp(&property_rank(right.property)))
            .then_with(|| left.timing.start_time.total_cmp(&right.timing.start_time))
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn validate_presence_chains(tracks: &[CompiledTrack]) -> Result<(), (TrackId, TrackId)> {
    let mut previous: Option<(u32, TrackId, bool)> = None;

    for track in tracks
        .iter()
        .filter(|track| track.property == Property::Presence)
    {
        let TrackValues::Bool { from, to } = &track.values else {
            unreachable!("validated Presence track must contain bool values");
        };

        if let Some((object_index, previous_id, previous_to)) = previous {
            if object_index == track.object_index && previous_to != *from {
                return Err((previous_id, track.id));
            }
        }
        previous = Some((track.object_index, track.id, *to));
    }

    Ok(())
}

const fn property_rank(property: Property) -> u8 {
    match property {
        Property::Presence => 0,
        Property::Transform => 1,
        Property::Position => 2,
        Property::Rotation => 3,
        Property::Opacity => 4,
        Property::Appearance => 5,
        Property::Reveal => 6,
        Property::Morph => 7,
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        Easing, GeometryRef, ObjectDefinition, Property, ScenePatch, TrackTiming, TrackValues, Vec2,
    };

    use super::*;

    fn filled_loop() -> noon_core::VectorPath {
        noon_core::VectorPath::new()
            .move_to(Vec2::new(0.0, 1.5))
            .cubic_to(
                Vec2::new(1.0, 1.5),
                Vec2::new(1.5, 1.0),
                Vec2::new(1.5, 0.0),
            )
            .cubic_to(
                Vec2::new(1.5, -1.0),
                Vec2::new(1.0, -1.5),
                Vec2::new(0.0, -1.5),
            )
            .cubic_to(
                Vec2::new(-1.0, -1.5),
                Vec2::new(-1.5, -1.0),
                Vec2::new(-1.5, 0.0),
            )
            .cubic_to(
                Vec2::new(-1.5, 1.0),
                Vec2::new(-1.0, 1.5),
                Vec2::new(0.0, 1.5),
            )
            .close()
    }

    fn filled_star() -> noon_core::VectorPath {
        noon_core::VectorPath::new()
            .move_to(Vec2::new(0.0, 1.9))
            .line_to(Vec2::new(0.45, 0.62))
            .line_to(Vec2::new(1.8, 0.58))
            .line_to(Vec2::new(0.72, -0.24))
            .line_to(Vec2::new(1.12, -1.54))
            .line_to(Vec2::new(0.0, -0.78))
            .line_to(Vec2::new(-1.12, -1.54))
            .line_to(Vec2::new(-0.72, -0.24))
            .line_to(Vec2::new(-1.8, 0.58))
            .line_to(Vec2::new(-0.45, 0.62))
            .close()
    }

    #[test]
    fn safe_filled_path_transform_compiles_to_fixed_path_pair() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(filled_loop()));
        let mut from = noon_core::ObjectSnapshot::new(GeometryRef::path(filled_loop()));
        let mut to = noon_core::ObjectSnapshot::new(GeometryRef::path(filled_star()));
        from.style.fill = Some(noon_core::Color::WHITE);
        to.style.fill = Some(noon_core::Color::BLACK);
        scene
            .add_track(
                object,
                Property::Transform,
                TrackValues::Object { from, to },
                TrackTiming::new(0.0, 2.0, Easing::Linear),
            )
            .expect("safe filled Transform track must be valid");

        let compiled = CompiledScene::compile(&scene).expect("safe filled path must compile");
        assert!(matches!(
            compiled.tracks()[0].transform_geometry_plan,
            Some(TransformGeometryPlan::PathPair(_))
        ));
    }

    #[test]
    fn filled_path_transform_rejects_fill_presence_change() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(filled_loop()));
        let from = noon_core::ObjectSnapshot::new(GeometryRef::path(filled_loop()));
        let mut to = noon_core::ObjectSnapshot::new(GeometryRef::path(filled_star()));
        to.style.fill = None;
        let mut from = from;
        from.style.fill = Some(noon_core::Color::WHITE);
        scene
            .add_track(
                object,
                Property::Transform,
                TrackValues::Object { from, to },
                TrackTiming::new(0.0, 2.0, Easing::Linear),
            )
            .expect("semantic track is valid before compilation");
        assert!(matches!(
            CompiledScene::compile(&scene),
            Err(CompileError::PathTransformRequiresRetessellation(_))
        ));
    }

    #[test]
    fn object_ids_resolve_to_dense_indices() {
        let mut scene = SceneDefinition::new();
        let circle = scene.add(GeometryRef::circle(1.0));
        let rectangle = scene.add(GeometryRef::rectangle(2.0, 3.0));

        let compiled = CompiledScene::compile(&scene).expect("scene must compile");

        assert_eq!(compiled.object_index(circle), Some(0));
        assert_eq!(compiled.object_index(rectangle), Some(1));
        assert_eq!(compiled.objects()[0].id, circle);
        assert_eq!(compiled.objects()[1].id, rectangle);
    }

    #[test]
    fn tracks_are_sorted_for_runtime_access() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_position(
                object,
                Vec2::new(5.0, 0.0),
                Vec2::new(6.0, 0.0),
                TrackTiming::new(5.0, 1.0, Easing::Linear),
            )
            .expect("valid track");
        scene
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::ONE,
                TrackTiming::new(1.0, 1.0, Easing::Linear),
            )
            .expect("valid track");

        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        let starts: Vec<f64> = compiled
            .tracks()
            .iter()
            .map(|track| track.timing.start_time)
            .collect();

        assert_eq!(starts, vec![1.0, 5.0]);
    }

    #[test]
    fn only_animated_properties_are_marked_dynamic() {
        let mut scene = SceneDefinition::new();
        let animated = scene.add(GeometryRef::circle(1.0));
        let static_object = scene.add(GeometryRef::rectangle(2.0, 2.0));
        scene
            .animate_scalar(
                animated,
                Property::Opacity,
                1.0,
                0.0,
                TrackTiming::new(0.0, 1.0, Easing::Linear),
            )
            .expect("valid track");

        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        let animated_index = compiled.object_index(animated).expect("known object") as usize;
        let static_index = compiled.object_index(static_object).expect("known object") as usize;

        assert_eq!(
            compiled.objects()[animated_index].dynamic,
            DynamicProperties {
                presence: false,
                transform: false,
                position: false,
                rotation: false,
                opacity: true,
                appearance: false,
                reveal: false,
                morph: false,
            }
        );
        assert!(!compiled.objects()[static_index].dynamic.any());
    }

    #[test]
    fn appearance_tracks_mark_only_appearance_dynamic() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_appearance(object, 0.0, 1.0, TrackTiming::new(0.0, 1.0, Easing::Linear))
            .expect("valid appearance track");

        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        assert_eq!(
            compiled.objects()[0].dynamic,
            DynamicProperties {
                appearance: true,
                ..DynamicProperties::default()
            }
        );
    }

    #[test]
    fn presence_tracks_mark_only_presence_dynamic() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .set_presence_at(object, false, true, 2.0)
            .expect("valid presence event");

        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        assert_eq!(
            compiled.objects()[0].dynamic,
            DynamicProperties {
                presence: true,
                ..DynamicProperties::default()
            }
        );
        assert_eq!(compiled.tracks()[0].timing.duration, 0.0);
    }

    #[test]
    fn continuous_presence_chain_compiles() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .set_presence_at(object, false, true, 1.0)
            .expect("valid first presence event");
        scene
            .set_presence_at(object, true, false, 2.0)
            .expect("valid second presence event");

        CompiledScene::compile(&scene).expect("continuous presence chain must compile");
    }

    #[test]
    fn discontinuous_presence_chain_is_rejected() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let previous = scene
            .set_presence_at(object, false, true, 1.0)
            .expect("valid first presence event");
        let next = scene
            .set_presence_at(object, false, true, 2.0)
            .expect("each presence event is individually valid");

        assert_eq!(
            CompiledScene::compile(&scene),
            Err(CompileError::DiscontinuousPresence { previous, next })
        );
    }

    #[test]
    fn patch_rejects_discontinuous_presence_without_mutating_tracks() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let previous = scene
            .set_presence_at(object, false, true, 1.0)
            .expect("valid first presence event");
        let mut compiled = CompiledScene::compile(&scene).expect("scene must compile");
        let before = compiled.tracks().to_vec();
        let next = TrackId::new(9);

        let track = TrackDefinition {
            id: next,
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::new(2.0, 0.0, Easing::Linear),
        };
        assert_eq!(
            compiled.apply_patch(&ScenePatch::AddTrack(track)),
            Err(CompilePatchError::DiscontinuousPresence { previous, next })
        );
        assert_eq!(compiled.tracks(), before);
    }

    #[test]
    fn patch_rejects_removing_required_presence_handoff_without_mutating_tracks() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let first = scene
            .set_presence_at(object, false, true, 1.0)
            .expect("valid first presence event");
        let middle = scene
            .set_presence_at(object, true, false, 2.0)
            .expect("valid middle presence event");
        let last = scene
            .set_presence_at(object, false, true, 3.0)
            .expect("valid last presence event");
        let mut compiled = CompiledScene::compile(&scene).expect("scene must compile");
        let before = compiled.tracks().to_vec();

        assert_eq!(
            compiled.apply_patch(&ScenePatch::RemoveTrack(middle)),
            Err(CompilePatchError::DiscontinuousPresence {
                previous: first,
                next: last,
            })
        );
        assert_eq!(compiled.tracks(), before);
    }

    #[test]
    fn reveal_tracks_mark_only_reveal_dynamic() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::ONE),
        ));
        scene
            .animate_reveal(object, 0.0, 1.0, TrackTiming::new(0.0, 1.0, Easing::Linear))
            .expect("valid reveal track");

        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        assert_eq!(
            compiled.objects()[0].dynamic,
            DynamicProperties {
                presence: false,
                transform: false,
                position: false,
                rotation: false,
                opacity: false,
                appearance: false,
                reveal: true,
                morph: false,
            }
        );
    }

    #[test]
    fn morph_tracks_mark_only_morph_dynamic() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::ONE),
        ));
        scene
            .animate_morph(object, 0.0, 1.0, TrackTiming::new(0.0, 1.0, Easing::Linear))
            .expect("valid morph track");

        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        assert!(compiled.objects()[0].dynamic.morph);
        assert!(!compiled.objects()[0].dynamic.reveal);
        assert!(!compiled.objects()[0].dynamic.appearance);
    }

    #[test]
    fn identical_input_compiles_identically() {
        fn build() -> SceneDefinition {
            let mut scene = SceneDefinition::new();
            let object = scene.add(GeometryRef::circle(2.0));
            scene
                .animate_position(
                    object,
                    Vec2::ZERO,
                    Vec2::new(3.0, 4.0),
                    TrackTiming::new(0.5, 2.0, Easing::EaseInOutCubic),
                )
                .expect("valid track");
            scene
        }

        assert_eq!(
            CompiledScene::compile(&build()).expect("scene must compile"),
            CompiledScene::compile(&build()).expect("scene must compile")
        );
    }

    #[test]
    fn compiled_patches_preserve_dense_identity_and_dynamic_flags() {
        let mut scene = SceneDefinition::new();
        let first = scene.add(GeometryRef::circle(1.0));
        let second = scene.add(GeometryRef::rectangle(2.0, 2.0));
        let mut compiled = CompiledScene::compile(&scene).expect("scene must compile");

        compiled
            .apply_patch(&ScenePatch::CreateObject(ObjectDefinition::new(
                ObjectId::new(7),
                GeometryRef::circle(3.0),
            )))
            .expect("valid patch");
        assert_eq!(compiled.object_index(first), Some(0));
        assert_eq!(compiled.object_index(second), Some(1));
        assert_eq!(compiled.object_index(ObjectId::new(7)), Some(2));

        let track = TrackDefinition {
            id: TrackId::new(9),
            object: second,
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 1.0, to: 0.0 },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
        };
        compiled
            .apply_patch(&ScenePatch::AddTrack(track))
            .expect("valid patch");
        assert!(compiled.objects()[1].dynamic.opacity);
    }
}
