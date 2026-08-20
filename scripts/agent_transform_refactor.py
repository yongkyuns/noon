from pathlib import Path


def replace(path: str, old: str, new: str, count: int = 1) -> None:
    p = Path(path)
    text = p.read_text()
    if old not in text:
        raise SystemExit(f"expected snippet not found in {path}: {old[:120]!r}")
    p.write_text(text.replace(old, new, count))


def replace_all(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if old not in text:
        raise SystemExit(f"expected snippet not found in {path}: {old[:120]!r}")
    p.write_text(text.replace(old, new))


# Core timeline: morph is semantically independent from reveal.
replace(
    "crates/noon-core/src/timeline.rs",
    "    Opacity,\n    Reveal,\n}",
    "    Opacity,\n    Reveal,\n    Morph,\n}",
)
replace(
    "crates/noon-core/src/timeline.rs",
    "            Self::Rotation | Self::Opacity | Self::Reveal => ValueKind::Scalar,",
    "            Self::Rotation | Self::Opacity | Self::Reveal | Self::Morph => ValueKind::Scalar,",
)
replace(
    "crates/noon-core/src/timeline.rs",
    "    pub fn animate_reveal(\n        &mut self,\n        object: ObjectId,\n        from: f32,\n        to: f32,\n        timing: TrackTiming,\n    ) -> Result<TrackId, TimelineError> {\n        self.animate_scalar(object, Property::Reveal, from, to, timing)\n    }\n\n    pub fn tracks(&self) -> &[TrackDefinition] {",
    "    pub fn animate_reveal(\n        &mut self,\n        object: ObjectId,\n        from: f32,\n        to: f32,\n        timing: TrackTiming,\n    ) -> Result<TrackId, TimelineError> {\n        self.animate_scalar(object, Property::Reveal, from, to, timing)\n    }\n\n    pub fn animate_morph(\n        &mut self,\n        object: ObjectId,\n        from: f32,\n        to: f32,\n        timing: TrackTiming,\n    ) -> Result<TrackId, TimelineError> {\n        self.animate_scalar(object, Property::Morph, from, to, timing)\n    }\n\n    pub fn tracks(&self) -> &[TrackDefinition] {",
)
replace(
    "crates/noon-core/src/timeline.rs",
    "    #[test]\n    fn unknown_objects_are_rejected() {",
    '''    #[test]
    fn morph_is_a_distinct_scalar_timeline_property() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(
            crate::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::ONE),
        ));
        scene
            .animate_morph(object, 0.0, 1.0, timing())
            .expect("valid morph track");

        assert_eq!(scene.tracks()[0].property, Property::Morph);
        assert_eq!(
            scene.tracks()[0].values,
            TrackValues::Scalar { from: 0.0, to: 1.0 }
        );
    }

    #[test]
    fn unknown_objects_are_rejected() {''',
)

# Compiler: track morph independently and keep deterministic ordering.
replace(
    "crates/noon-compile/src/lib.rs",
    "    pub opacity: bool,\n    pub reveal: bool,\n}",
    "    pub opacity: bool,\n    pub reveal: bool,\n    pub morph: bool,\n}",
)
replace(
    "crates/noon-compile/src/lib.rs",
    "            Property::Opacity => self.opacity = true,\n            Property::Reveal => self.reveal = true,",
    "            Property::Opacity => self.opacity = true,\n            Property::Reveal => self.reveal = true,\n            Property::Morph => self.morph = true,",
)
replace(
    "crates/noon-compile/src/lib.rs",
    "        self.position || self.rotation || self.opacity || self.reveal",
    "        self.position || self.rotation || self.opacity || self.reveal || self.morph",
)
replace(
    "crates/noon-compile/src/lib.rs",
    "        Property::Reveal => 3,\n    }",
    "        Property::Reveal => 3,\n        Property::Morph => 4,\n    }",
)
replace_all(
    "crates/noon-compile/src/lib.rs",
    "                reveal: false,\n",
    "                reveal: false,\n                morph: false,\n",
)
replace(
    "crates/noon-compile/src/lib.rs",
    "    #[test]\n    fn identical_input_compiles_identically() {",
    '''    #[test]
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
    }

    #[test]
    fn identical_input_compiles_identically() {''',
)

# Runtime: independent reveal and morph sidecars.
replace(
    "crates/noon-runtime/src/lib.rs",
    "    /// geometry types that do not support reveal yet.\n    pub reveals: Vec<f32>,\n}",
    "    /// geometry types that do not support reveal yet.\n    pub reveals: Vec<f32>,\n    /// Normalized per-object morph progress, independent from reveal.\n    pub morphs: Vec<f32>,\n}",
)
replace(
    "crates/noon-runtime/src/lib.rs",
    "    pub fn reveal(&self, object_index: usize) -> f32 {\n        self.reveals[object_index]\n    }\n}",
    "    pub fn reveal(&self, object_index: usize) -> f32 {\n        self.reveals[object_index]\n    }\n\n    pub fn morph(&self, object_index: usize) -> f32 {\n        self.morphs[object_index]\n    }\n}",
)
replace(
    "crates/noon-runtime/src/lib.rs",
    '''    FrameState {
        time,
        reveals: initial_reveals(compiled, objects.len()),
        objects,
    }
}

fn initial_reveals(compiled: &CompiledScene, object_count: usize) -> Vec<f32> {
    let mut reveals = vec![1.0; object_count];
    let mut initialized = vec![false; object_count];
    for track in compiled
        .tracks()
        .iter()
        .filter(|track| track.property == Property::Reveal)
    {
        let index = track.object_index as usize;
        if initialized[index] {
            continue;
        }
        let TrackValues::Scalar { from, .. } = track.values else {
            unreachable!("compiled reveal track must contain scalar values");
        };
        reveals[index] = from.clamp(0.0, 1.0);
        initialized[index] = true;
    }
    reveals
}''',
    '''    FrameState {
        time,
        reveals: initial_scalar_property(compiled, objects.len(), Property::Reveal, 1.0),
        morphs: initial_scalar_property(compiled, objects.len(), Property::Morph, 0.0),
        objects,
    }
}

fn initial_scalar_property(
    compiled: &CompiledScene,
    object_count: usize,
    property: Property,
    default: f32,
) -> Vec<f32> {
    let mut values = vec![default; object_count];
    let mut initialized = vec![false; object_count];
    for track in compiled
        .tracks()
        .iter()
        .filter(|track| track.property == property)
    {
        let index = track.object_index as usize;
        if initialized[index] {
            continue;
        }
        let TrackValues::Scalar { from, .. } = track.values else {
            unreachable!("compiled scalar property must contain scalar values");
        };
        values[index] = from.clamp(0.0, 1.0);
        initialized[index] = true;
    }
    values
}''',
)
replace(
    "crates/noon-runtime/src/lib.rs",
    '''        (Property::Reveal, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = frame.reveals[group.object_index] != value;
            frame.reveals[group.object_index] = value;
            changed
        }
        (Property::Position, EvaluatedValue::Vec2(value)) => {''',
    '''        (Property::Reveal, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = frame.reveals[group.object_index] != value;
            frame.reveals[group.object_index] = value;
            changed
        }
        (Property::Morph, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = frame.morphs[group.object_index] != value;
            frame.morphs[group.object_index] = value;
            changed
        }
        (Property::Position, EvaluatedValue::Vec2(value)) => {''',
)
replace(
    "crates/noon-runtime/src/lib.rs",
    "    #[test]\n    fn backward_and_forward_seeks_are_deterministic() {",
    '''    #[test]
    fn reveal_and_morph_progress_are_independent() {
        let source = noon_core::VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 0.0));
        let target = noon_core::VectorPath::new()
            .move_to(Vec2::new(0.0, -1.0))
            .line_to(Vec2::new(0.0, 1.0));
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(source.with_morph_target(target)));
        scene
            .animate_reveal(object, 0.0, 1.0, TrackTiming::new(0.0, 2.0, Easing::Linear))
            .expect("valid reveal track");
        scene
            .animate_morph(object, 0.0, 1.0, TrackTiming::new(0.0, 4.0, Easing::Linear))
            .expect("valid morph track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));

        let frame = instance.seek(1.0).expect("valid time");
        assert_eq!(frame.reveal(0), 0.5);
        assert_eq!(frame.morph(0), 0.25);
    }

    #[test]
    fn backward_and_forward_seeks_are_deterministic() {''',
)

# Renderer: carry reveal and morph independently in the path instance.
replace(
    "crates/noon-render-wgpu/src/lib.rs",
    "const PATH_PROGRESS_MAX: u32 = 16_777_215;\nconst PATH_MORPH_BIT: u32 = 1 << 25;",
    "const PATH_PROGRESS_MAX: u32 = 16_777_215;",
)
replace(
    "crates/noon-render-wgpu/src/lib.rs",
    "pub struct PathInstance {\n    pub transform: PackedTransform,\n    pub style: PackedStyle,\n}",
    "pub struct PathInstance {\n    pub transform: PackedTransform,\n    pub style: PackedStyle,\n    /// x = reveal, y = morph progress. Both are normalized and independent.\n    pub path_params: [f32; 2],\n}",
)
replace_all(
    "crates/noon-render-wgpu/src/lib.rs",
    "pack_path(object, frame.reveal(object_index))",
    "pack_path(object, frame.reveal(object_index), frame.morph(object_index))",
)
replace(
    "crates/noon-render-wgpu/src/lib.rs",
    "                surface: pack_path_surface(vertex.surface, vertex.path_progress, mesh.morphing),",
    "                surface: pack_path_surface(vertex.surface, vertex.path_progress),",
)
replace(
    "crates/noon-render-wgpu/src/lib.rs",
    '''fn pack_path(object: &FrameObjectState, reveal: f32) -> PathInstance {
    debug_assert!(matches!(object.geometry, GeometryRef::VectorPath(_)));
    let mut style: PackedStyle = object.style.into();
    // Path stroke width is baked into the cached mesh, so this otherwise-unused
    // GPU field carries normalized reveal without growing the instance stride.
    style.stroke_width = reveal.clamp(0.0, 1.0);
    PathInstance {
        transform: object.transform.into(),
        style,
    }
}

fn pack_path_surface(surface: PathSurface, progress: f32, morphing: bool) -> u32 {
    let progress = (progress.clamp(0.0, 1.0) * PATH_PROGRESS_MAX as f32).round() as u32;
    let mut packed = (progress << 1)
        | match surface {
            PathSurface::Fill => 0,
            PathSurface::Stroke => 1,
        };
    if morphing {
        packed |= PATH_MORPH_BIT;
    }
    packed
}''',
    '''fn pack_path(object: &FrameObjectState, reveal: f32, morph: f32) -> PathInstance {
    debug_assert!(matches!(object.geometry, GeometryRef::VectorPath(_)));
    PathInstance {
        transform: object.transform.into(),
        style: object.style.into(),
        path_params: [reveal.clamp(0.0, 1.0), morph.clamp(0.0, 1.0)],
    }
}

fn pack_path_surface(surface: PathSurface, progress: f32) -> u32 {
    let progress = (progress.clamp(0.0, 1.0) * PATH_PROGRESS_MAX as f32).round() as u32;
    (progress << 1)
        | match surface {
            PathSurface::Fill => 0,
            PathSurface::Stroke => 1,
        }
}''',
)
replace(
    "crates/noon-render-wgpu/src/lib.rs",
    '''        let reveals = vec![1.0; objects.len()];
        FrameState {
            time: 1.25,
            objects,
            reveals,
        }''',
    '''        let reveals = vec![1.0; objects.len()];
        let morphs = vec![0.0; objects.len()];
        FrameState {
            time: 1.25,
            objects,
            reveals,
            morphs,
        }''',
)
replace(
    "crates/noon-render-wgpu/src/lib.rs",
    "        assert_eq!(std::mem::size_of::<PathInstance>(), 72);",
    "        assert_eq!(std::mem::size_of::<PathInstance>(), 80);",
)
replace(
    "crates/noon-render-wgpu/src/lib.rs",
    "        assert_eq!(prepared.paths[0].style.stroke_width, 0.35);",
    "        assert_eq!(prepared.paths[0].path_params[0], 0.35);",
)
replace(
    "crates/noon-render-wgpu/src/lib.rs",
    "    #[test]\n    fn one_hundred_thousand_circles_form_one_batch() {",
    '''    #[test]
    fn path_morph_changes_only_dirty_the_instance_record() {
        let target = VectorPath::new()
            .move_to(Vec2::new(0.0, -1.0))
            .line_to(Vec2::new(0.0, 1.0));
        let source = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 0.0))
            .with_morph_target(target);
        let mut state = object(7, GeometryRef::path(source));
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.2;
        let mut frame = frame(vec![state]);
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);

        frame.morphs[0] = 0.6;
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));

        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert_eq!(prepared.stats.dirty_instance_count, 1);
        assert!(!prepared.path_geometry_dirty);
        assert_eq!(prepared.paths[0].path_params, [1.0, 0.6]);
    }

    #[test]
    fn one_hundred_thousand_circles_form_one_batch() {''',
)

# WGPU layout: add one vec2 path parameter attribute at the end.
replace(
    "crates/noon-render-wgpu/src/gpu.rs",
    "const PATH_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 7] = [",
    "const PATH_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 8] = [",
)
replace(
    "crates/noon-render-wgpu/src/gpu.rs",
    '''    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32x2,
        offset: 64,
        shader_location: 9,
    },
];

struct AnalyticPipelineDescriptor''',
    '''    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32x2,
        offset: 64,
        shader_location: 9,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 72,
        shader_location: 10,
    },
];

struct AnalyticPipelineDescriptor''',
)
replace(
    "crates/noon-render-wgpu/src/gpu.rs",
    "        assert_eq!(path_instance_layout.array_stride, 72);",
    "        assert_eq!(path_instance_layout.array_stride, 80);",
)
replace(
    "crates/noon-render-wgpu/src/gpu.rs",
    '''        assert_eq!(path_instance_layout.attributes.len(), 7);
        assert_eq!(path_instance_layout.attributes[0].shader_location, 3);
        assert_eq!(path_instance_layout.attributes[6].shader_location, 9);''',
    '''        assert_eq!(path_instance_layout.attributes.len(), 8);
        assert_eq!(path_instance_layout.attributes[0].shader_location, 3);
        assert_eq!(path_instance_layout.attributes[6].shader_location, 9);
        assert_eq!(path_instance_layout.attributes[7].shader_location, 10);''',
)

# Shader: always interpolate dual positions with morph progress; reveal stays separate.
Path("crates/noon-render-wgpu/src/path.wgsl").write_text('''struct Camera {
    center: vec2<f32>,
    clip_scale: vec2<f32>,
    viewport_size: vec2<f32>,
    padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct PathVertexInput {
    @location(0) local: vec2<f32>,
    @location(1) target_local: vec2<f32>,
    @location(2) surface_and_progress: u32,
    @location(3) translation: vec2<f32>,
    @location(4) scale: vec2<f32>,
    @location(5) rotation: f32,
    @location(6) fill: vec4<f32>,
    @location(7) stroke: vec4<f32>,
    @location(8) metrics: vec2<f32>,
    @location(9) flags: vec2<u32>,
    @location(10) path_params: vec2<f32>,
};

struct PathVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) path_progress: f32,
    @location(2) reveal: f32,
};

fn premultiplied(color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(color.rgb * color.a, color.a);
}

@vertex
fn vs_path(input: PathVertexInput) -> PathVertexOutput {
    let is_stroke = (input.surface_and_progress & 1u) == 1u;
    let encoded_progress = input.surface_and_progress >> 1u;
    let path_progress = f32(encoded_progress) / 16777215.0;
    let morph = clamp(input.path_params.y, 0.0, 1.0);
    let local = mix(input.local, input.target_local, morph);

    let c = cos(input.rotation);
    let s = sin(input.rotation);
    let scaled = local * input.scale;
    let world = vec2<f32>(
        c * scaled.x - s * scaled.y,
        s * scaled.x + c * scaled.y,
    ) + input.translation;

    var output: PathVertexOutput;
    output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);
    let enabled = select(input.flags.x != 0u, input.flags.y != 0u, is_stroke);
    let color = select(input.fill, input.stroke, is_stroke);
    output.color = select(vec4<f32>(0.0), premultiplied(color) * input.metrics.y, enabled);
    output.path_progress = path_progress;
    output.reveal = clamp(input.path_params.x, 0.0, 1.0);
    return output;
}

@fragment
fn fs_path(input: PathVertexOutput) -> @location(0) vec4<f32> {
    if input.reveal <= 0.0 {
        return vec4<f32>(0.0);
    }
    if input.reveal >= 1.0 {
        return input.color;
    }

    let edge = max(fwidth(input.path_progress), 0.00001);
    let coverage = 1.0 - smoothstep(input.reveal, input.reveal + edge, input.path_progress);
    return input.color * coverage;
}
''')

# Python: Manim-like Transform/Scene.play surface.
p = Path("web/python/noon.py")
text = p.read_text()
marker = 'class Scene:\n    """Complete, versioned Noon scene document."""'
if marker not in text:
    raise SystemExit("Scene insertion marker missing")
text = text.replace(
    marker,
    '''@dataclass(frozen=True, slots=True)
class Transform:
    """Transform one scene object toward a target shape.

    The first implementation supports VectorPath targets. Scene.play lowers
    this authoring object into deterministic Noon IR; Python is not used during
    frame playback.
    """

    source: Object
    target: VectorPath
    key: str | None = None


class Scene:
    """Complete, versioned Noon scene document."""''',
    1,
)
play_marker = "    def animate_position(\n"
if play_marker not in text:
    raise SystemExit("animate_position marker missing")
text = text.replace(
    play_marker,
    '''    def play(
        self,
        *animations: Transform,
        duration: float,
        start_time: float = 0.0,
        easing: str = "linear",
    ) -> Scene:
        if not animations:
            raise ValueError("play requires at least one animation")
        for animation in animations:
            if not isinstance(animation, Transform):
                raise TypeError("unsupported animation; expected Transform")
            self._schedule_transform(
                animation,
                duration=duration,
                start_time=start_time,
                easing=easing,
            )
        return self

    def _schedule_transform(
        self,
        animation: Transform,
        *,
        duration: float,
        start_time: float,
        easing: str,
    ) -> None:
        obj = animation.source
        target = animation.target
        if not isinstance(obj, Object) or obj._owner is not self._owner:
            raise ValueError("transformed object must belong to this Scene")
        if not isinstance(target, VectorPath):
            raise TypeError("Transform target must currently be a VectorPath")
        geometry = self._objects[obj.id]["geometry"]
        source = geometry.get("vector_path")
        if source is None:
            raise ValueError("the current Transform renderer supports vector paths only")
        if "morph_target" in source:
            raise ValueError("a path can currently have one geometric Transform target")
        source["morph_target"] = target.to_ir()
        self._add_scalar_track(
            obj,
            "morph",
            0.0,
            1.0,
            start_time,
            duration,
            easing,
            animation.key,
        )

    def animate_position(
''',
    1,
)
reveal_start = text.index("    def animate_reveal(\n")
morph_start = text.index("    def animate_morph(\n", reveal_start)
object_start = text.index("    def _add_object(\n", morph_start)
reveal_method = '''    def animate_reveal(
        self,
        obj: Object,
        from_: float = 0.0,
        to: float = 1.0,
        *,
        duration: float,
        start_time: float = 0.0,
        easing: str = "linear",
        key: str | None = None,
    ) -> Scene:
        self._add_scalar_track(
            obj,
            "reveal",
            _unit_interval("from", from_),
            _unit_interval("to", to),
            start_time,
            duration,
            easing,
            key,
        )
        return self

'''
morph_method = '''    def animate_morph(
        self,
        obj: Object,
        target: VectorPath,
        *,
        duration: float,
        start_time: float = 0.0,
        easing: str = "linear",
        key: str | None = None,
    ) -> Scene:
        return self.play(
            Transform(obj, target, key=key),
            duration=duration,
            start_time=start_time,
            easing=easing,
        )

'''
text = text[:reveal_start] + reveal_method + morph_method + text[object_start:]
p.write_text(text)

# Python tests: primary Transform/play surface and independent channels.
replace(
    "web/python/test_noon.py",
    "from noon import Color, PatchBatch, Scene, VectorPath",
    "from noon import Color, PatchBatch, Scene, Transform, VectorPath",
)
replace(
    "web/python/test_noon.py",
    '''        scene.animate_morph(
            path,
            target,
            duration=3.0,
            start_time=0.5,
            easing="ease_in_out_cubic",
            key="morph.shape",
        )
''',
    '''        scene.play(
            Transform(path, target, key="morph.shape"),
            duration=3.0,
            start_time=0.5,
            easing="ease_in_out_cubic",
        )
''',
)
replace(
    "web/python/test_noon.py",
    '''        self.assertEqual(track["property"], "reveal")
        self.assertEqual(track["values"]["scalar"], {"from": 0.0, "to": 1.0})

        with self.assertRaises(ValueError):
            scene.animate_reveal(path, duration=1.0)''',
    '''        self.assertEqual(track["property"], "morph")
        self.assertEqual(track["values"]["scalar"], {"from": 0.0, "to": 1.0})

        scene.animate_reveal(path, duration=1.0, key="morph.reveal")
        self.assertEqual(scene.to_document()["tracks"][1]["property"], "reveal")''',
)
replace(
    "web/python/test_noon.py",
    "    def test_scene_rejects_foreign_objects_and_invalid_timing(self) -> None:",
    '''    def test_play_supports_multiple_transform_animations(self) -> None:
        scene = Scene()
        first = scene.path(
            VectorPath().move_to((-1.0, 0.0)).line_to((1.0, 0.0)),
            fill=None,
            stroke=Color(1.0, 1.0, 1.0),
            key="first",
        )
        second = scene.path(
            VectorPath().move_to((-1.0, -1.0)).line_to((1.0, 1.0)),
            fill=None,
            stroke=Color(1.0, 1.0, 1.0),
            key="second",
        )
        scene.play(
            Transform(first, VectorPath().move_to((0.0, -1.0)).line_to((0.0, 1.0))),
            Transform(second, VectorPath().move_to((-1.0, 1.0)).line_to((1.0, -1.0))),
            duration=2.0,
        )
        self.assertEqual(
            [track["property"] for track in scene.to_document()["tracks"]],
            ["morph", "morph"],
        )

    def test_scene_rejects_foreign_objects_and_invalid_timing(self) -> None:''',
)

# Demo and UI: make Transform/play the visible primary API.
replace(
    "web/python/demo_scene.py",
    "from noon import Color, Scene, VectorPath",
    "from noon import Color, Scene, Transform, VectorPath",
)
replace(
    "web/python/demo_scene.py",
    'scene.animate_morph(curve, morph_target, key="curve.morph", **timing)',
    'scene.play(Transform(curve, morph_target, key="curve.transform"), **timing)',
)
replace(
    "web/index.html",
    '<div class="api-row"><code>scene.animate_*(...)</code><span>compiled tracks</span></div>',
    '<div class="api-row"><code>scene.play(Transform(...))</code><span>compiled transform</span></div>\n            <div class="api-row"><code>scene.animate_*(...)</code><span>low-level tracks</span></div>',
)

# Documentation: record the new semantic boundary.
p = Path("docs/path-morphing.md")
text = p.read_text()
old = 'The runtime reuses the existing normalized path scalar channel: ordinary paths interpret it as reveal, while paths with a semantic `morph_target` interpret it as morph progress. This keeps frame state compact and means morph playback changes only the path instance record; geometry is not retessellated or re-uploaded each frame. The Python API exposes this as `scene.animate_morph(path, target, ...)`.\n\nCurrent intentional boundary: morph rendering is stroke-only. Fill triangulation during topology-changing interpolation is deferred until a stable fill strategy is selected. A path cannot currently animate reveal and morph simultaneously because those operations share the normalized path channel.\n'
new = 'Morph progress is now a first-class `Property::Morph`, independent from `Property::Reveal`. `FrameState` carries both normalized values and the path instance uploads them together, so reveal and morph can be composed on the same path without retessellation. The primary Python authoring surface is Manim-like: `scene.play(Transform(path, target), duration=...)`; `scene.animate_morph(...)` remains a compatibility helper that lowers to the same morph track.\n\nCurrent intentional boundary: the first `Transform` renderer supports `VectorPath` targets and stroke geometry. Fill triangulation and generic target mobjects/style interpolation are separate follow-up capabilities.\n'
if old not in text:
    raise SystemExit("path morphing runtime paragraph missing")
text = text.replace(old, new, 1)
old = 'Next, add fill morphing and decide whether reveal+morph composition warrants separate scalar channels. The stroke morph path is already fixed-topology and GPU-interpolated, so normal playback performs no path planning, tessellation, or geometry upload per frame.'
new = 'Next, broaden `Transform` from vector-path geometry into a generic compiled transform that can interpolate target transform/style and later detached target mobjects, then add fill morphing. The stroke morph path is already fixed-topology and GPU-interpolated, so normal playback performs no path planning, tessellation, or geometry upload per frame.'
if old not in text:
    raise SystemExit("path morphing next-step paragraph missing")
p.write_text(text.replace(old, new, 1))
