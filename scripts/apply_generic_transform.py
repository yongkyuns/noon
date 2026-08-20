from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"expected source fragment not found in {path}:\n{old[:500]}")
    file.write_text(text.replace(old, new, 1))


# noon-core: detached object snapshots are identity-free Transform endpoints.
replace_once(
    "crates/noon-core/src/lib.rs",
    """}\n\n#[derive(Clone, Debug, Default, PartialEq)]\npub struct SceneDefinition {\n""",
    """}\n\n#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]\npub struct ObjectSnapshot {\n    pub geometry: GeometryRef,\n    pub transform: Transform2D,\n    pub style: Style,\n}\n\nimpl ObjectSnapshot {\n    pub fn new(geometry: GeometryRef) -> Self {\n        Self {\n            geometry,\n            transform: Transform2D::default(),\n            style: Style::default(),\n        }\n    }\n}\n\nimpl From<&ObjectDefinition> for ObjectSnapshot {\n    fn from(value: &ObjectDefinition) -> Self {\n        Self {\n            geometry: value.geometry.clone(),\n            transform: value.transform,\n            style: value.style,\n        }\n    }\n}\n\n#[derive(Clone, Debug, Default, PartialEq)]\npub struct SceneDefinition {\n""",
)

# Timeline: one atomic Transform track carries full source/target snapshots.
replace_once(
    "crates/noon-core/src/timeline.rs",
    "use crate::{ObjectId, SceneDefinition, TrackId, Vec2};\n",
    "use crate::{ObjectId, ObjectSnapshot, SceneDefinition, TrackId, Vec2};\n",
)
replace_once(
    "crates/noon-core/src/timeline.rs",
    """pub enum Property {\n    Position,\n    Rotation,\n    Opacity,\n    Reveal,\n    Morph,\n}\n""",
    """pub enum Property {\n    Transform,\n    Position,\n    Rotation,\n    Opacity,\n    Reveal,\n    Morph,\n}\n""",
)
replace_once(
    "crates/noon-core/src/timeline.rs",
    """pub enum ValueKind {\n    Scalar,\n    Vec2,\n}\n""",
    """pub enum ValueKind {\n    Scalar,\n    Vec2,\n    Object,\n}\n""",
)
replace_once(
    "crates/noon-core/src/timeline.rs",
    """        match self {\n            Self::Position => ValueKind::Vec2,\n            Self::Rotation | Self::Opacity | Self::Reveal | Self::Morph => ValueKind::Scalar,\n        }\n""",
    """        match self {\n            Self::Transform => ValueKind::Object,\n            Self::Position => ValueKind::Vec2,\n            Self::Rotation | Self::Opacity | Self::Reveal | Self::Morph => ValueKind::Scalar,\n        }\n""",
)
replace_once(
    "crates/noon-core/src/timeline.rs",
    """#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]\n#[serde(rename_all = \"snake_case\")]\npub enum TrackValues {\n    Scalar { from: f32, to: f32 },\n    Vec2 { from: Vec2, to: Vec2 },\n}\n\nimpl TrackValues {\n    pub const fn value_kind(self) -> ValueKind {\n        match self {\n            Self::Scalar { .. } => ValueKind::Scalar,\n            Self::Vec2 { .. } => ValueKind::Vec2,\n        }\n    }\n}\n""",
    """#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]\n#[serde(rename_all = \"snake_case\")]\npub enum TrackValues {\n    Scalar { from: f32, to: f32 },\n    Vec2 { from: Vec2, to: Vec2 },\n    Object { from: ObjectSnapshot, to: ObjectSnapshot },\n}\n\nimpl TrackValues {\n    pub const fn value_kind(&self) -> ValueKind {\n        match self {\n            Self::Scalar { .. } => ValueKind::Scalar,\n            Self::Vec2 { .. } => ValueKind::Vec2,\n            Self::Object { .. } => ValueKind::Object,\n        }\n    }\n}\n""",
)
replace_once(
    "crates/noon-core/src/timeline.rs",
    """    pub fn animate_position(\n        &mut self,\n        object: ObjectId,\n        from: Vec2,\n        to: Vec2,\n        timing: TrackTiming,\n    ) -> Result<TrackId, TimelineError> {\n""",
    """    pub fn animate_transform(\n        &mut self,\n        object: ObjectId,\n        from: ObjectSnapshot,\n        to: ObjectSnapshot,\n        timing: TrackTiming,\n    ) -> Result<TrackId, TimelineError> {\n        self.add_track(\n            object,\n            Property::Transform,\n            TrackValues::Object { from, to },\n            timing,\n        )\n    }\n\n    pub fn animate_position(\n        &mut self,\n        object: ObjectId,\n        from: Vec2,\n        to: Vec2,\n        timing: TrackTiming,\n    ) -> Result<TrackId, TimelineError> {\n""",
)

# Compiler: validate Transform geometry once and cache a stable runtime geometry pair.
replace_once(
    "crates/noon-compile/src/lib.rs",
    """pub struct DynamicProperties {\n    pub position: bool,\n""",
    """pub struct DynamicProperties {\n    pub transform: bool,\n    pub position: bool,\n""",
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    """        match property {\n            Property::Position => self.position = true,\n""",
    """        match property {\n            Property::Transform => self.transform = true,\n            Property::Position => self.position = true,\n""",
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    """        self.position || self.rotation || self.opacity || self.reveal || self.morph\n""",
    """        self.transform\n            || self.position\n            || self.rotation\n            || self.opacity\n            || self.reveal\n            || self.morph\n""",
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    """pub struct CompiledTrack {\n    pub id: TrackId,\n    pub object_index: u32,\n    pub property: Property,\n    pub values: TrackValues,\n    pub timing: TrackTiming,\n}\n""",
    """pub struct CompiledTrack {\n    pub id: TrackId,\n    pub object_index: u32,\n    pub property: Property,\n    pub values: TrackValues,\n    pub timing: TrackTiming,\n    /// Stable geometry used by an atomic Transform. For path morphing this is\n    /// the source path carrying its target correspondence; it does not change\n    /// during steady-state playback.\n    pub transform_geometry: Option<GeometryRef>,\n}\n""",
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    """pub enum CompileError {\n    TooManyObjects(usize),\n    UnknownObject(ObjectId),\n}\n""",
    """pub enum CompileError {\n    TooManyObjects(usize),\n    UnknownObject(ObjectId),\n    UnsupportedTransformGeometry(TrackId),\n    PathTransformRequiresRetessellation(TrackId),\n}\n""",
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    """            Self::UnknownObject(id) => {\n                write!(formatter, \"track references unknown object {}\", id.get())\n            }\n""",
    """            Self::UnknownObject(id) => {\n                write!(formatter, \"track references unknown object {}\", id.get())\n            }\n            Self::UnsupportedTransformGeometry(id) => write!(\n                formatter,\n                \"transform track {} uses unsupported geometry interpolation\",\n                id.get()\n            ),\n            Self::PathTransformRequiresRetessellation(id) => write!(\n                formatter,\n                \"transform track {} changes path fill topology or stroke width\",\n                id.get()\n            ),\n""",
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    """pub enum CompilePatchError {\n    TooManyObjects(usize),\n    DuplicateObject(ObjectId),\n    UnknownObject(ObjectId),\n    DuplicateTrack(TrackId),\n    UnknownTrack(TrackId),\n    InvalidTrack(TimelineError),\n}\n""",
    """pub enum CompilePatchError {\n    TooManyObjects(usize),\n    DuplicateObject(ObjectId),\n    UnknownObject(ObjectId),\n    DuplicateTrack(TrackId),\n    UnknownTrack(TrackId),\n    InvalidTrack(TimelineError),\n    UnsupportedTransformGeometry(TrackId),\n    PathTransformRequiresRetessellation(TrackId),\n}\n""",
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    """            Self::InvalidTrack(error) => write!(formatter, \"invalid track: {error}\"),\n""",
    """            Self::InvalidTrack(error) => write!(formatter, \"invalid track: {error}\"),\n            Self::UnsupportedTransformGeometry(id) => write!(\n                formatter,\n                \"transform track {} uses unsupported geometry interpolation\",\n                id.get()\n            ),\n            Self::PathTransformRequiresRetessellation(id) => write!(\n                formatter,\n                \"transform track {} changes path fill topology or stroke width\",\n                id.get()\n            ),\n""",
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    """            objects[object_index as usize].dynamic.mark(track.property);\n            tracks.push(compile_track(track, object_index));\n""",
    """            objects[object_index as usize].dynamic.mark(track.property);\n            tracks.push(compile_track(track, object_index).map_err(|error| {\n                compile_error(track.id, error)\n            })?);\n""",
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    """        Ok(compile_track(track, object_index))\n""",
    """        compile_track(track, object_index).map_err(|error| compile_patch_error(track.id, error))\n""",
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    """fn compile_track(track: &TrackDefinition, object_index: u32) -> CompiledTrack {\n    CompiledTrack {\n        id: track.id,\n        object_index,\n        property: track.property,\n        values: track.values,\n        timing: track.timing,\n    }\n}\n""",
    """#[derive(Clone, Copy, Debug, PartialEq, Eq)]\nenum TransformCompileFailure {\n    UnsupportedGeometry,\n    RequiresRetessellation,\n}\n\nfn compile_track(\n    track: &TrackDefinition,\n    object_index: u32,\n) -> Result<CompiledTrack, TransformCompileFailure> {\n    Ok(CompiledTrack {\n        id: track.id,\n        object_index,\n        property: track.property,\n        values: track.values.clone(),\n        timing: track.timing,\n        transform_geometry: compile_transform_geometry(track)?,\n    })\n}\n\nfn compile_transform_geometry(\n    track: &TrackDefinition,\n) -> Result<Option<GeometryRef>, TransformCompileFailure> {\n    if track.property != Property::Transform {\n        return Ok(None);\n    }\n    let TrackValues::Object { from, to } = &track.values else {\n        unreachable!(\"validated Transform track must contain object snapshots\");\n    };\n\n    if from.geometry == to.geometry {\n        return Ok(Some(from.geometry.clone()));\n    }\n\n    match (&from.geometry, &to.geometry) {\n        (GeometryRef::VectorPath(source), GeometryRef::VectorPath(target)) => {\n            if from.style.fill != to.style.fill\n                || from.style.stroke_width.to_bits() != to.style.stroke_width.to_bits()\n            {\n                return Err(TransformCompileFailure::RequiresRetessellation);\n            }\n            Ok(Some(GeometryRef::path(\n                source.clone().with_morph_target(target.clone()),\n            )))\n        }\n        _ => Err(TransformCompileFailure::UnsupportedGeometry),\n    }\n}\n\nfn compile_error(id: TrackId, error: TransformCompileFailure) -> CompileError {\n    match error {\n        TransformCompileFailure::UnsupportedGeometry => {\n            CompileError::UnsupportedTransformGeometry(id)\n        }\n        TransformCompileFailure::RequiresRetessellation => {\n            CompileError::PathTransformRequiresRetessellation(id)\n        }\n    }\n}\n\nfn compile_patch_error(id: TrackId, error: TransformCompileFailure) -> CompilePatchError {\n    match error {\n        TransformCompileFailure::UnsupportedGeometry => {\n            CompilePatchError::UnsupportedTransformGeometry(id)\n        }\n        TransformCompileFailure::RequiresRetessellation => {\n            CompilePatchError::PathTransformRequiresRetessellation(id)\n        }\n    }\n}\n""",
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    """    match property {\n        Property::Position => 0,\n        Property::Rotation => 1,\n        Property::Opacity => 2,\n        Property::Reveal => 3,\n        Property::Morph => 4,\n    }\n""",
    """    match property {\n        Property::Transform => 0,\n        Property::Position => 1,\n        Property::Rotation => 2,\n        Property::Opacity => 3,\n        Property::Reveal => 4,\n        Property::Morph => 5,\n    }\n""",
)

# Runtime: atomic Transform evaluation precedes narrower property overrides.
replace_once(
    "crates/noon-runtime/src/lib.rs",
    """use noon_core::{\n    Easing, GeometryRef, ObjectId, Property, ScenePatch, Style, TrackValues, Transform2D, Vec2,\n};\n""",
    """use noon_core::{\n    Color, Easing, GeometryRef, ObjectId, ObjectSnapshot, Property, ScenePatch, Style, TrackValues,\n    Transform2D, Vec2,\n};\n""",
)
replace_once(
    "crates/noon-runtime/src/lib.rs",
    """                self.frame.objects[index].transform = *transform;\n                self.reapply_properties(index, &[Property::Position, Property::Rotation]);\n""",
    """                self.frame.objects[index].transform = *transform;\n                self.reapply_properties(\n                    index,\n                    &[Property::Transform, Property::Position, Property::Rotation],\n                );\n""",
)
replace_once(
    "crates/noon-runtime/src/lib.rs",
    """                self.frame.objects[index].style = *style;\n                self.reapply_properties(index, &[Property::Opacity]);\n""",
    """                self.frame.objects[index].style = *style;\n                self.reapply_properties(index, &[Property::Transform, Property::Opacity]);\n""",
)
replace_once(
    "crates/noon-runtime/src/lib.rs",
    """        let TrackValues::Scalar { from, .. } = track.values else {\n            unreachable!(\"compiled scalar property must contain scalar values\");\n        };\n        values[index] = from.clamp(0.0, 1.0);\n""",
    """        let TrackValues::Scalar { from, .. } = &track.values else {\n            unreachable!(\"compiled scalar property must contain scalar values\");\n        };\n        values[index] = from.clamp(0.0, 1.0);\n""",
)
replace_once(
    "crates/noon-runtime/src/lib.rs",
    """    let track = &tracks[group.cursor - 1];\n    let value = interpolate(track, time);\n\n    match (group.property, value) {\n""",
    """    let track = &tracks[group.cursor - 1];\n    if group.property == Property::Transform {\n        return apply_transform_track(frame, group.object_index, track, time);\n    }\n    let value = interpolate(track, time);\n\n    match (group.property, value) {\n""",
)
replace_once(
    "crates/noon-runtime/src/lib.rs",
    """fn interpolate(track: &CompiledTrack, time: f64) -> EvaluatedValue {\n    let raw = ((time - track.timing.start_time) / track.timing.duration).clamp(0.0, 1.0) as f32;\n    let progress = apply_easing(track.timing.easing, raw);\n    match track.values {\n        TrackValues::Scalar { from, to } => EvaluatedValue::Scalar(lerp(from, to, progress)),\n        TrackValues::Vec2 { from, to } => EvaluatedValue::Vec2(Vec2::new(\n            lerp(from.x, to.x, progress),\n            lerp(from.y, to.y, progress),\n        )),\n    }\n}\n""",
    """fn apply_transform_track(\n    frame: &mut FrameState,\n    object_index: usize,\n    track: &CompiledTrack,\n    time: f64,\n) -> bool {\n    let TrackValues::Object { from, to } = &track.values else {\n        unreachable!(\"compiled Transform track must contain object snapshots\");\n    };\n    let progress = track_progress(track, time);\n    let next_geometry = track\n        .transform_geometry\n        .as_ref()\n        .expect(\"compiled Transform track must carry prepared geometry\");\n    let before_object = frame.objects[object_index].clone();\n    let before_morph = frame.morphs[object_index];\n\n    let object = &mut frame.objects[object_index];\n    if object.geometry != *next_geometry {\n        object.geometry = next_geometry.clone();\n    }\n    object.transform = interpolate_transform(from.transform, to.transform, progress);\n    object.style = interpolate_style(from.style, to.style, progress);\n    frame.morphs[object_index] = if path_geometry_morphs(from, to) {\n        progress\n    } else {\n        0.0\n    };\n\n    frame.objects[object_index] != before_object || frame.morphs[object_index] != before_morph\n}\n\nfn path_geometry_morphs(from: &ObjectSnapshot, to: &ObjectSnapshot) -> bool {\n    from.geometry != to.geometry\n        && matches!(\n            (&from.geometry, &to.geometry),\n            (GeometryRef::VectorPath(_), GeometryRef::VectorPath(_))\n        )\n}\n\nfn interpolate_transform(from: Transform2D, to: Transform2D, progress: f32) -> Transform2D {\n    Transform2D {\n        translation: Vec2::new(\n            lerp(from.translation.x, to.translation.x, progress),\n            lerp(from.translation.y, to.translation.y, progress),\n        ),\n        rotation: lerp(from.rotation, to.rotation, progress),\n        scale: Vec2::new(\n            lerp(from.scale.x, to.scale.x, progress),\n            lerp(from.scale.y, to.scale.y, progress),\n        ),\n    }\n}\n\nfn interpolate_style(from: Style, to: Style, progress: f32) -> Style {\n    Style {\n        fill: interpolate_optional_color(from.fill, to.fill, progress),\n        stroke: interpolate_optional_color(from.stroke, to.stroke, progress),\n        stroke_width: lerp(from.stroke_width, to.stroke_width, progress),\n        opacity: lerp(from.opacity, to.opacity, progress),\n    }\n}\n\nfn interpolate_optional_color(\n    from: Option<Color>,\n    to: Option<Color>,\n    progress: f32,\n) -> Option<Color> {\n    if progress <= 0.0 {\n        return from;\n    }\n    if progress >= 1.0 {\n        return to;\n    }\n    match (from, to) {\n        (None, None) => None,\n        (Some(from), Some(to)) => Some(interpolate_color(from, to, progress)),\n        (None, Some(to)) => Some(interpolate_color(\n            Color::rgba(to.red, to.green, to.blue, 0.0),\n            to,\n            progress,\n        )),\n        (Some(from), None) => Some(interpolate_color(\n            from,\n            Color::rgba(from.red, from.green, from.blue, 0.0),\n            progress,\n        )),\n    }\n}\n\nfn interpolate_color(from: Color, to: Color, progress: f32) -> Color {\n    Color::rgba(\n        lerp(from.red, to.red, progress),\n        lerp(from.green, to.green, progress),\n        lerp(from.blue, to.blue, progress),\n        lerp(from.alpha, to.alpha, progress),\n    )\n}\n\nfn track_progress(track: &CompiledTrack, time: f64) -> f32 {\n    let raw = ((time - track.timing.start_time) / track.timing.duration).clamp(0.0, 1.0) as f32;\n    apply_easing(track.timing.easing, raw)\n}\n\nfn interpolate(track: &CompiledTrack, time: f64) -> EvaluatedValue {\n    let progress = track_progress(track, time);\n    match &track.values {\n        TrackValues::Scalar { from, to } => {\n            EvaluatedValue::Scalar(lerp(*from, *to, progress))\n        }\n        TrackValues::Vec2 { from, to } => EvaluatedValue::Vec2(Vec2::new(\n            lerp(from.x, to.x, progress),\n            lerp(from.y, to.y, progress),\n        )),\n        TrackValues::Object { .. } => {\n            unreachable!(\"Transform tracks are evaluated atomically\")\n        }\n    }\n}\n""",
)

# Python authoring: detached Mobjects + atomic object-valued Transform tracks.
replace_once(
    "web/python/noon.py",
    "import json\nimport math\n",
    "import copy\nimport json\nimport math\n",
)
replace_once(
    "web/python/noon.py",
    """@dataclass(frozen=True, slots=True)\nclass Object:\n    \"\"\"Stable reference to an object owned by one Scene.\"\"\"\n\n    id: int\n    _owner: object\n\n\nclass VectorPath:\n""",
    """@dataclass(frozen=True, slots=True)\nclass Mobject:\n    \"\"\"Detached semantic object snapshot usable as a Transform target.\"\"\"\n\n    geometry: dict[str, Any]\n    transform: dict[str, Any]\n    style: dict[str, Any]\n\n    def to_ir(self) -> dict[str, Any]:\n        return {\n            \"geometry\": copy.deepcopy(self.geometry),\n            \"transform\": copy.deepcopy(self.transform),\n            \"style\": copy.deepcopy(self.style),\n        }\n\n\n@dataclass(frozen=True, slots=True)\nclass Object:\n    \"\"\"Stable reference to an object owned by one Scene.\"\"\"\n\n    id: int\n    _owner: object\n\n\nclass VectorPath:\n""",
)
replace_once(
    "web/python/noon.py",
    """@dataclass(frozen=True, slots=True)\nclass Transform:\n    \"\"\"Transform one scene object toward a target shape.\n\n    The first implementation supports VectorPath targets. Scene.play lowers\n    this authoring object into deterministic Noon IR; Python is not used during\n    frame playback.\n    \"\"\"\n\n    source: Object\n    target: VectorPath\n    key: str | None = None\n\n\nclass Scene:\n""",
    """def _make_mobject(\n    geometry: dict[str, Any],\n    *,\n    position: tuple[float, float] = (0.0, 0.0),\n    rotation: float = 0.0,\n    scale: tuple[float, float] = (1.0, 1.0),\n    fill: Color | None = Color(1.0, 1.0, 1.0),\n    stroke: Color | None = None,\n    stroke_width: float = 1.0,\n    opacity: float = 1.0,\n) -> Mobject:\n    if fill is not None and not isinstance(fill, Color):\n        raise TypeError(\"fill must be a Color or None\")\n    if stroke is not None and not isinstance(stroke, Color):\n        raise TypeError(\"stroke must be a Color or None\")\n    width = _finite_number(\"stroke_width\", stroke_width)\n    if width < 0.0:\n        raise ValueError(\"stroke_width must be non-negative\")\n    return Mobject(\n        geometry=copy.deepcopy(geometry),\n        transform={\n            \"translation\": _vec2(\"position\", position),\n            \"rotation\": _finite_number(\"rotation\", rotation),\n            \"scale\": _vec2(\"scale\", scale),\n        },\n        style={\n            \"fill\": None if fill is None else fill.to_ir(),\n            \"stroke\": None if stroke is None else stroke.to_ir(),\n            \"stroke_width\": width,\n            \"opacity\": _finite_number(\"opacity\", opacity),\n        },\n    )\n\n\ndef Circle(radius: float, **kwargs: Any) -> Mobject:\n    return _make_mobject(\n        {\"circle\": {\"radius\": _positive_number(\"radius\", radius)}},\n        **kwargs,\n    )\n\n\ndef Rectangle(width: float, height: float, **kwargs: Any) -> Mobject:\n    return _make_mobject(\n        {\n            \"rectangle\": {\n                \"size\": {\n                    \"x\": _positive_number(\"width\", width),\n                    \"y\": _positive_number(\"height\", height),\n                }\n            }\n        },\n        **kwargs,\n    )\n\n\ndef Line(\n    start: tuple[float, float],\n    end: tuple[float, float],\n    **kwargs: Any,\n) -> Mobject:\n    kwargs.setdefault(\"fill\", None)\n    kwargs.setdefault(\"stroke\", Color(1.0, 1.0, 1.0))\n    kwargs.setdefault(\"stroke_width\", 0.1)\n    return _make_mobject(\n        {\"line\": {\"start\": _vec2(\"start\", start), \"end\": _vec2(\"end\", end)}},\n        **kwargs,\n    )\n\n\ndef Path(path: VectorPath, **kwargs: Any) -> Mobject:\n    if not isinstance(path, VectorPath):\n        raise TypeError(\"path must be a VectorPath\")\n    kwargs.setdefault(\"stroke_width\", 0.1)\n    return _make_mobject({\"vector_path\": path.to_ir()}, **kwargs)\n\n\n@dataclass(frozen=True, slots=True)\nclass Transform:\n    \"\"\"Atomically transform one scene object toward a detached target snapshot.\"\"\"\n\n    source: Object\n    target: Mobject | VectorPath\n    key: str | None = None\n\n\nclass Scene:\n""",
)
replace_once(
    "web/python/noon.py",
    """        self._object_keys: dict[int, str] = {}\n        self._track_keys: dict[int, str] = {}\n\n    def circle(\n""",
    """        self._object_keys: dict[int, str] = {}\n        self._track_keys: dict[int, str] = {}\n        self._scheduled_transform_targets: dict[int, dict[str, Any]] = {}\n        self._scheduled_transform_ends: dict[int, float] = {}\n\n    def add(self, mobject: Mobject, *, key: str | None = None) -> Object:\n        if not isinstance(mobject, Mobject):\n            raise TypeError(\"add expects a detached Mobject\")\n        return self._append_snapshot(mobject.to_ir(), key)\n\n    def circle(\n""",
)
replace_once(
    "web/python/noon.py",
    """        target = animation.target\n        if not isinstance(obj, Object) or obj._owner is not self._owner:\n            raise ValueError(\"transformed object must belong to this Scene\")\n        if not isinstance(target, VectorPath):\n            raise TypeError(\"Transform target must currently be a VectorPath\")\n        geometry = self._objects[obj.id][\"geometry\"]\n        source = geometry.get(\"vector_path\")\n        if source is None:\n            raise ValueError(\"the current Transform renderer supports vector paths only\")\n        if \"morph_target\" in source:\n            raise ValueError(\"a path can currently have one geometric Transform target\")\n        source[\"morph_target\"] = target.to_ir()\n        self._add_scalar_track(\n            obj,\n            \"morph\",\n            0.0,\n            1.0,\n            start_time,\n            duration,\n            easing,\n            animation.key,\n        )\n""",
    """        target = animation.target\n        if not isinstance(obj, Object) or obj._owner is not self._owner:\n            raise ValueError(\"transformed object must belong to this Scene\")\n\n        start = _finite_number(\"start_time\", start_time)\n        run_duration = _positive_number(\"duration\", duration)\n        previous_end = self._scheduled_transform_ends.get(obj.id)\n        if previous_end is not None and start < previous_end:\n            raise ValueError(\"generic Transform tracks for one object must not overlap\")\n\n        source_snapshot = copy.deepcopy(\n            self._scheduled_transform_targets.get(obj.id, self._snapshot_for_object(obj))\n        )\n        if isinstance(target, VectorPath):\n            target_snapshot = copy.deepcopy(source_snapshot)\n            target_snapshot[\"geometry\"] = {\"vector_path\": target.to_ir()}\n        elif isinstance(target, Mobject):\n            target_snapshot = target.to_ir()\n        else:\n            raise TypeError(\"Transform target must be a detached Mobject or VectorPath\")\n\n        self._add_track(\n            obj,\n            \"transform\",\n            {\n                \"object\": {\n                    \"from\": source_snapshot,\n                    \"to\": target_snapshot,\n                }\n            },\n            start,\n            run_duration,\n            easing,\n            animation.key,\n        )\n        self._scheduled_transform_targets[obj.id] = copy.deepcopy(target_snapshot)\n        self._scheduled_transform_ends[obj.id] = start + run_duration\n""",
)
replace_once(
    "web/python/noon.py",
    """    def _add_object(\n        self,\n        geometry: dict[str, Any],\n""",
    """    def _snapshot_for_object(self, obj: Object) -> dict[str, Any]:\n        stored = self._objects[obj.id]\n        return {\n            \"geometry\": copy.deepcopy(stored[\"geometry\"]),\n            \"transform\": copy.deepcopy(stored[\"transform\"]),\n            \"style\": copy.deepcopy(stored[\"style\"]),\n        }\n\n    def _append_snapshot(\n        self, snapshot: dict[str, Any], key: str | None\n    ) -> Object:\n        object_id = len(self._objects)\n        authoring_key = _authoring_key(\"key\", key, f\"@object:{object_id}\")\n        if authoring_key in self._object_keys.values():\n            raise ValueError(f\"duplicate object key: {authoring_key}\")\n        self._object_keys[object_id] = authoring_key\n        stored = copy.deepcopy(snapshot)\n        stored[\"id\"] = object_id\n        self._objects.append(stored)\n        return Object(object_id, self._owner)\n\n    def _add_object(\n        self,\n        geometry: dict[str, Any],\n""",
)
replace_once(
    "web/python/noon.py",
    """        object_id = len(self._objects)\n        authoring_key = _authoring_key(\"key\", key, f\"@object:{object_id}\")\n        if authoring_key in self._object_keys.values():\n            raise ValueError(f\"duplicate object key: {authoring_key}\")\n        self._object_keys[object_id] = authoring_key\n        self._objects.append(\n            {\n                \"id\": object_id,\n                \"geometry\": geometry,\n                \"transform\": {\n                    \"translation\": _vec2(\"position\", position),\n                    \"rotation\": _finite_number(\"rotation\", rotation),\n                    \"scale\": _vec2(\"scale\", scale),\n                },\n                \"style\": {\n                    \"fill\": None if fill is None else fill.to_ir(),\n                    \"stroke\": None if stroke is None else stroke.to_ir(),\n                    \"stroke_width\": width,\n                    \"opacity\": _finite_number(\"opacity\", opacity),\n                },\n            }\n        )\n        return Object(object_id, self._owner)\n""",
    """        return self._append_snapshot(\n            {\n                \"geometry\": geometry,\n                \"transform\": {\n                    \"translation\": _vec2(\"position\", position),\n                    \"rotation\": _finite_number(\"rotation\", rotation),\n                    \"scale\": _vec2(\"scale\", scale),\n                },\n                \"style\": {\n                    \"fill\": None if fill is None else fill.to_ir(),\n                    \"stroke\": None if stroke is None else stroke.to_ir(),\n                    \"stroke_width\": width,\n                    \"opacity\": _finite_number(\"opacity\", opacity),\n                },\n            },\n            key,\n        )\n""",
)

# Existing stress tests now inspect atomic Transform tracks rather than embedded targets.
replace_once(
    "web/python/test_examples.py",
    """        self.assertEqual(properties.count(\"morph\"), 600)\n        self.assertEqual(properties.count(\"rotation\"), 600)\n\n        morph_geometries = {\n            json.dumps(\n                obj[\"geometry\"][\"vector_path\"],\n                sort_keys=True,\n                separators=(\",\", \":\"),\n            )\n            for obj in document[\"objects\"]\n        }\n        self.assertEqual(len(morph_geometries), 12)\n""",
    """        self.assertEqual(properties.count(\"transform\"), 600)\n        self.assertEqual(properties.count(\"rotation\"), 600)\n\n        morph_geometries = {\n            json.dumps(\n                track[\"values\"][\"object\"][\"to\"][\"geometry\"][\"vector_path\"],\n                sort_keys=True,\n                separators=(\",\", \":\"),\n            )\n            for track in document[\"tracks\"]\n            if track[\"property\"] == \"transform\"\n        }\n        self.assertEqual(len(morph_geometries), 12)\n""",
)
replace_once(
    "web/python/test_examples.py",
    """                self.assertEqual(properties.count(\"morph\"), object_count)\n                self.assertEqual(properties.count(\"rotation\"), object_count)\n\n                morph_geometries = {\n                    json.dumps(\n                        obj[\"geometry\"][\"vector_path\"],\n                        sort_keys=True,\n                        separators=(\",\", \":\"),\n                    )\n                    for obj in document[\"objects\"]\n                }\n                self.assertEqual(len(morph_geometries), 12)\n""",
    """                self.assertEqual(properties.count(\"transform\"), object_count)\n                self.assertEqual(properties.count(\"rotation\"), object_count)\n\n                morph_geometries = {\n                    json.dumps(\n                        track[\"values\"][\"object\"][\"to\"][\"geometry\"][\"vector_path\"],\n                        sort_keys=True,\n                        separators=(\",\", \":\"),\n                    )\n                    for track in document[\"tracks\"]\n                    if track[\"property\"] == \"transform\"\n                }\n                self.assertEqual(len(morph_geometries), 12)\n""",
)
