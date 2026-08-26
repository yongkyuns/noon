from pathlib import Path
import re


def edit(path: str, transform):
    p = Path(path)
    text = p.read_text()
    updated = transform(text)
    if updated == text:
        raise SystemExit(f"no change applied to {path}")
    p.write_text(updated)


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


def sub_once(text: str, pattern: str, repl: str, label: str, flags=0) -> str:
    updated, count = re.subn(pattern, repl, text, count=1, flags=flags)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return updated


# 1) Core semantic patch: geometry replacement is an object-local property mutation.
def patch_core(text: str) -> str:
    text = replace_once(
        text,
        "    validate_track_definition, ObjectDefinition, ObjectId, SceneDefinition, Style, TimelineError,\n",
        "    validate_track_definition, GeometryRef, ObjectDefinition, ObjectId, SceneDefinition, Style,\n    TimelineError,\n",
        "core import GeometryRef",
    )
    text = replace_once(
        text,
        "    RemoveObject(ObjectId),\n    SetTransform {\n",
        "    RemoveObject(ObjectId),\n    SetGeometry {\n        object: ObjectId,\n        geometry: GeometryRef,\n    },\n    SetTransform {\n",
        "core SetGeometry variant",
    )
    text = replace_once(
        text,
        "            Self::SetTransform { .. } | Self::SetStyle { .. } => MutationImpact::Property,\n",
        "            Self::SetGeometry { .. } | Self::SetTransform { .. } | Self::SetStyle { .. } => {\n                MutationImpact::Property\n            }\n",
        "core impact",
    )
    text = replace_once(
        text,
        "            ScenePatch::RemoveObject(id) => self.remove_object(id),\n            ScenePatch::SetTransform { object, transform } => {\n",
        "            ScenePatch::RemoveObject(id) => self.remove_object(id),\n            ScenePatch::SetGeometry { object, geometry } => {\n                self.object_mut(object)\n                    .ok_or(PatchError::UnknownObject(object))?\n                    .geometry = geometry;\n                Ok(())\n            }\n            ScenePatch::SetTransform { object, transform } => {\n",
        "core apply SetGeometry",
    )
    text = replace_once(
        text,
        "                    ScenePatch::SetTransform { object, .. }\n                    | ScenePatch::SetStyle { object, .. } => *object,\n",
        "                    ScenePatch::SetGeometry { object, .. }\n                    | ScenePatch::SetTransform { object, .. }\n                    | ScenePatch::SetStyle { object, .. } => *object,\n",
        "core property transaction preflight",
    )
    marker = "        assert_eq!(\n            ScenePatch::RemoveTrack(TrackId::new(2)).impact(),\n            MutationImpact::Timeline\n        );\n"
    text = replace_once(
        text,
        marker,
        "        assert_eq!(\n            ScenePatch::SetGeometry {\n                object,\n                geometry: GeometryRef::line(Vec2::ZERO, Vec2::ONE),\n            }\n            .impact(),\n            MutationImpact::Property\n        );\n" + marker,
        "core impact test",
    )
    return text

edit("crates/noon-core/src/patch.rs", patch_core)


# 2) Dense compiler patching keeps the stable slot and replaces only its geometry.
def patch_compile(text: str) -> str:
    text = replace_once(
        text,
        "                ScenePatch::SetTransform { object, .. } | ScenePatch::SetStyle { object, .. } => {\n                    if !object_indices.contains_key(object) {\n",
        "                ScenePatch::SetGeometry { object, .. }\n                | ScenePatch::SetTransform { object, .. }\n                | ScenePatch::SetStyle { object, .. } => {\n                    if !object_indices.contains_key(object) {\n",
        "compile preflight geometry",
    )
    text = replace_once(
        text,
        "            ScenePatch::SetTransform { object, transform } => {\n",
        "            ScenePatch::SetGeometry { object, geometry } => {\n                let index = self\n                    .object_index(*object)\n                    .ok_or(CompilePatchError::UnknownObject(*object))?;\n                self.objects[index as usize].geometry = geometry.clone();\n            }\n            ScenePatch::SetTransform { object, transform } => {\n",
        "compile apply geometry",
    )
    return text

edit("crates/noon-compile/src/lib.rs", patch_compile)


# 3) Runtime value patch updates rendered geometry in place and invalidates just that object.
def patch_runtime(text: str) -> str:
    text = replace_once(
        text,
        "            patch,\n            ScenePatch::SetTransform { .. } | ScenePatch::SetStyle { .. }\n",
        "            patch,\n            ScenePatch::SetGeometry { .. }\n                | ScenePatch::SetTransform { .. }\n                | ScenePatch::SetStyle { .. }\n",
        "runtime value routing",
    )
    text = replace_once(
        text,
        "        let object = match patch {\n            ScenePatch::SetTransform { object, .. } | ScenePatch::SetStyle { object, .. } => {\n                *object\n            }\n            _ => unreachable!(\"value patch helper only accepts transform or style patches\"),\n        };\n",
        "        let object = match patch {\n            ScenePatch::SetGeometry { object, .. }\n            | ScenePatch::SetTransform { object, .. }\n            | ScenePatch::SetStyle { object, .. } => *object,\n            _ => unreachable!(\"value patch helper only accepts object-local property patches\"),\n        };\n",
        "runtime value object",
    )
    text = replace_once(
        text,
        "        match patch {\n            ScenePatch::SetTransform { transform, .. } => {\n",
        "        match patch {\n            ScenePatch::SetGeometry { geometry, .. } => {\n                self.frame.objects[index].geometry = geometry.clone();\n                // Host callbacks run after ordinary timeline/reactive evaluation for the frame.\n                // Clearing a transient render override makes the callback geometry authoritative\n                // for this phase without rebuilding unrelated runtime slots.\n                self.frame.render_geometries[index] = None;\n            }\n            ScenePatch::SetTransform { transform, .. } => {\n",
        "runtime apply geometry",
    )
    text = text.replace(
        "            _ => unreachable!(\"value patch helper only accepts transform or style patches\"),\n",
        "            _ => unreachable!(\"value patch helper only accepts object-local property patches\"),\n",
        1,
    )
    return text

edit("crates/noon-runtime/src/lib.rs", patch_runtime)


# 4) TimedSceneInstance can wrap a legacy instance and exposes its shared inner runtime
#    for the host callback layer without duplicating signal-timeline evaluation.
def patch_signal_timeline(text: str) -> str:
    insertion = """    pub fn from_scene_instance(inner: SceneInstance) -> Self {\n        Self {\n            inner,\n            timeline: SignalTimelineDefinition::new(),\n            groups: Vec::new(),\n        }\n    }\n\n    pub fn scene(&self) -> &SceneInstance {\n        &self.inner\n    }\n\n    pub fn scene_mut(&mut self) -> &mut SceneInstance {\n        &mut self.inner\n    }\n\n"""
    text = replace_once(
        text,
        "impl TimedSceneInstance {\n    pub fn from_timed(scene: &TimedSemanticScene) -> Result<Self, TimedSceneRuntimeError> {\n",
        "impl TimedSceneInstance {\n" + insertion + "    pub fn from_timed(scene: &TimedSemanticScene) -> Result<Self, TimedSceneRuntimeError> {\n",
        "timed wrapper access",
    )
    return text

edit("crates/noon-runtime/src/reactive/signal_timeline.rs", patch_signal_timeline)


# 5) Host callback runtime always uses TimedSceneInstance. Plain scenes are wrapped with
#    an empty signal timeline; timed scenes keep native signal evaluation alive.
def patch_host_callbacks(text: str) -> str:
    text = replace_once(
        text,
        "    HostCallbackId, HostCallbackRegistry, MutationImpact, MutationTransaction, ObjectId,\n    ScenePatch, Style, Transform2D,\n",
        "    HostCallbackId, HostCallbackRegistry, MutationImpact, MutationTransaction, ObjectId,\n    ReactiveValue, ScenePatch, SignalId, Style, Transform2D,\n",
        "host imports core",
    )
    text = replace_once(
        text,
        "use crate::{EvaluationError, FrameState, SceneInstance};\n",
        "use crate::{\n    FrameState, SceneInstance, TimedSceneInstance, TimedSceneRuntimeError,\n};\n",
        "host imports runtime",
    )
    text = replace_once(
        text,
        "pub struct HostDrivenScene {\n    scene: SceneInstance,\n",
        "pub struct HostDrivenScene {\n    scene: TimedSceneInstance,\n",
        "host timed field",
    )
    old_new = """    pub fn new(\n        scene: SceneInstance,\n        registry: &HostCallbackRegistry,\n    ) -> Result<Self, HostCallbackAttachError> {\n        let dense_index_by_object = scene\n"""
    new_new = """    pub fn new(\n        scene: SceneInstance,\n        registry: &HostCallbackRegistry,\n    ) -> Result<Self, HostCallbackAttachError> {\n        Self::from_timed(TimedSceneInstance::from_scene_instance(scene), registry)\n    }\n\n    pub fn from_timed(\n        scene: TimedSceneInstance,\n        registry: &HostCallbackRegistry,\n    ) -> Result<Self, HostCallbackAttachError> {\n        let dense_index_by_object = scene\n"""
    text = replace_once(text, old_new, new_new, "host constructor")
    text = replace_once(
        text,
        "    pub fn scene(&self) -> &SceneInstance {\n        &self.scene\n    }\n\n    pub fn scene_mut(&mut self) -> &mut SceneInstance {\n        &mut self.scene\n    }\n",
        "    pub fn scene(&self) -> &SceneInstance {\n        self.scene.scene()\n    }\n\n    pub fn scene_mut(&mut self) -> &mut SceneInstance {\n        self.scene.scene_mut()\n    }\n\n    pub fn reactive_value(&self, signal: SignalId) -> Option<&ReactiveValue> {\n        self.scene.reactive_value(signal)\n    }\n",
        "host scene accessors",
    )
    text = replace_once(
        text,
        "    pub fn evaluate(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {\n        self.scene.evaluate(time)\n    }\n\n    pub fn seek(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {\n        self.scene.seek(time)\n    }\n\n    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {\n        self.scene.advance_to(time)\n    }\n",
        "    pub fn evaluate(&mut self, time: f64) -> Result<&FrameState, TimedSceneRuntimeError> {\n        self.scene.evaluate(time)\n    }\n\n    pub fn seek(&mut self, time: f64) -> Result<&FrameState, TimedSceneRuntimeError> {\n        self.scene.seek(time)\n    }\n\n    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, TimedSceneRuntimeError> {\n        self.scene.advance_to(time)\n    }\n",
        "host timed evaluation",
    )
    text = replace_once(
        text,
        "                    ScenePatch::SetTransform { object, .. }\n                    | ScenePatch::SetStyle { object, .. } => *object,\n",
        "                    ScenePatch::SetGeometry { object, .. }\n                    | ScenePatch::SetTransform { object, .. }\n                    | ScenePatch::SetStyle { object, .. } => *object,\n",
        "host property preflight",
    )
    text = replace_once(
        text,
        "                if !self.scene.contains_object(object) {\n",
        "                if !self.scene.scene().contains_object(object) {\n",
        "host contains object",
    )
    text = replace_once(
        text,
        "                self.scene\n                    .apply_patch(patch)\n",
        "                self.scene\n                    .scene_mut()\n                    .apply_patch(patch)\n",
        "host property apply",
    )
    text = replace_once(
        text,
        "        if self.scene.reactive.is_some() {\n",
        "        if self.scene.scene().reactive.is_some() {\n",
        "host reactive staging guard",
    )
    text = replace_once(
        text,
        "        let mut staged = self.scene.clone();\n        for patch in transaction.mutations() {\n            staged.apply_patch(patch)?;\n        }\n        self.scene = staged;\n",
        "        let mut staged = self.scene.scene().clone();\n        for patch in transaction.mutations() {\n            staged.apply_patch(patch)?;\n        }\n        self.scene = TimedSceneInstance::from_scene_instance(staged);\n",
        "host staged plain runtime",
    )
    # Test: a signal track remains live inside the host-driven scene.
    test_anchor = """    #[test]\n    fn reactive_runtime_allows_property_callback_commits_but_rejects_structural_work() {\n"""
    test = """    #[test]\n    fn timed_host_scene_exposes_evaluated_signal_values() {\n        let mut semantic = SemanticScene::new();\n        let object = semantic.add(GeometryRef::circle(0.5));\n        let tracker = semantic.add_input(0.0_f32);\n        semantic.bind(tracker, object, noon_core::Property::Rotation);\n        let mut timeline = noon_core::SignalTimelineDefinition::new();\n        timeline\n            .add_scalar_track(\n                semantic.reactive(),\n                tracker,\n                0.0,\n                4.0,\n                noon_core::TrackTiming::new(0.0, 2.0, noon_core::RateFunction::Linear),\n            )\n            .unwrap();\n        let timed = noon_core::TimedSemanticScene::from_parts(semantic, timeline).unwrap();\n        let instance = TimedSceneInstance::from_timed(&timed).unwrap();\n        let mut registry = HostCallbackRegistry::new();\n        registry.register([object]);\n        let mut driven = HostDrivenScene::from_timed(instance, &registry).unwrap();\n\n        driven.advance_to(1.0).unwrap();\n        assert_eq!(\n            driven.reactive_value(tracker),\n            Some(&ReactiveValue::Scalar(2.0))\n        );\n        assert_eq!(driven.callback_frame().objects[0].transform.rotation, 2.0);\n    }\n\n"""
    text = replace_once(text, test_anchor, test + test_anchor, "host timed test")
    return text

edit("crates/noon-runtime/src/reactive/host_callbacks.rs", patch_host_callbacks)


# 6) Browser HostScenePlayer decodes the same timed semantic document as ReactiveScenePlayer
#    and returns current native signal values with the coherent callback phase.
def patch_host_player(text: str) -> str:
    text = replace_once(
        text,
        "use noon_compile::{CompileError, CompiledScene};\n",
        "",
        "host player remove legacy compile import",
    )
    text = replace_once(
        text,
        "    HostCallbackId, HostCallbackRegistry, HostCallbackRegistryError, HostCallbackSlot,\n    MutationTransaction, ObjectId,\n",
        "    HostCallbackId, HostCallbackRegistry, HostCallbackRegistryError, HostCallbackSlot,\n    MutationTransaction, ObjectId, SignalId,\n",
        "host player SignalId",
    )
    text = replace_once(
        text,
        "use noon_ir::{decode_patch_batch, decode_scene, IrError};\n",
        "use noon_ir::{\n    decode_patch_batch, decode_timed_semantic_scene, IrError, TimedSemanticIrError,\n};\n",
        "host player timed decoder",
    )
    text = replace_once(
        text,
        "    EvaluationError, HostCallbackAttachError, HostCommitError, HostDrivenScene, SceneInstance,\n",
        "    HostCallbackAttachError, HostCommitError, HostDrivenScene, TimedSceneInstance,\n    TimedSceneRuntimeError,\n",
        "host player timed runtime import",
    )
    text = replace_once(
        text,
        "    Compile(CompileError),\n",
        "    TimedIr(TimedSemanticIrError),\n",
        "host player error timed ir",
    )
    text = replace_once(
        text,
        "    Evaluation(EvaluationError),\n",
        "    Runtime(TimedSceneRuntimeError),\n",
        "host player runtime error",
    )
    text = replace_once(
        text,
        "            Self::Compile(error) => error.fmt(formatter),\n",
        "            Self::TimedIr(error) => error.fmt(formatter),\n",
        "host player display timed ir",
    )
    text = replace_once(
        text,
        "            Self::Evaluation(error) => error.fmt(formatter),\n",
        "            Self::Runtime(error) => error.fmt(formatter),\n",
        "host player display runtime",
    )
    text = sub_once(
        text,
        r"impl From<CompileError> for HostPlayerError \{.*?\n\}\n\n",
        "impl From<TimedSemanticIrError> for HostPlayerError {\n    fn from(value: TimedSemanticIrError) -> Self {\n        Self::TimedIr(value)\n    }\n}\n\n",
        "host player From timed ir",
        flags=re.S,
    )
    text = sub_once(
        text,
        r"impl From<EvaluationError> for HostPlayerError \{.*?\n\}\n\n",
        "impl From<TimedSceneRuntimeError> for HostPlayerError {\n    fn from(value: TimedSceneRuntimeError) -> Self {\n        Self::Runtime(value)\n    }\n}\n\n",
        "host player From runtime",
        flags=re.S,
    )
    text = replace_once(
        text,
        "pub struct HostScenePlayer {\n    driven: HostDrivenScene,\n    next_sequence: u64,\n",
        "pub struct HostScenePlayer {\n    driven: HostDrivenScene,\n    signal_ids: Vec<SignalId>,\n    next_sequence: u64,\n",
        "host player signal ids field",
    )
    text = replace_once(
        text,
        "        let definition = decode_scene(scene_json)?;\n        let compiled = CompiledScene::compile(&definition)?;\n        let registry = decode_callback_registry(callback_slots_json)?;\n        let driven = HostDrivenScene::new(SceneInstance::new(compiled), &registry)?;\n        Ok(Self {\n            driven,\n            next_sequence: 0,\n        })\n",
        "        let scene = decode_timed_semantic_scene(scene_json)?;\n        let signal_ids = scene\n            .semantic()\n            .reactive()\n            .signals()\n            .iter()\n            .map(|signal| signal.id)\n            .collect();\n        let registry = decode_callback_registry(callback_slots_json)?;\n        let instance = TimedSceneInstance::from_timed(&scene)?;\n        let driven = HostDrivenScene::from_timed(instance, &registry)?;\n        Ok(Self {\n            driven,\n            signal_ids,\n            next_sequence: 0,\n        })\n",
        "host player constructor timed",
    )
    text = replace_once(
        text,
        "        let invocations = frame\n",
        "        let signals = self\n            .signal_ids\n            .iter()\n            .filter_map(|signal| {\n                self.driven.reactive_value(*signal).map(|value| {\n                    json!({\n                        \"signal\": signal.get(),\n                        \"value\": value,\n                    })\n                })\n            })\n            .collect::<Vec<_>>();\n        let invocations = frame\n",
        "host player serialize signals",
    )
    text = replace_once(
        text,
        "            \"objects\": objects,\n            \"invocations\": invocations,\n",
        "            \"objects\": objects,\n            \"signals\": signals,\n            \"invocations\": invocations,\n",
        "host player frame signals field",
    )
    # Add a timed tracker callback-frame test.
    test_anchor = """    #[test]\n    fn callback_patch_sequence_is_checked() {\n"""
    test = """    #[test]\n    fn callback_frame_reports_timeline_evaluated_signal_values() {\n        let mut semantic = noon_core::SemanticScene::new();\n        let object = semantic.add(GeometryRef::circle(0.5));\n        let tracker = semantic.add_input(0.0_f32);\n        let mut timeline = noon_core::SignalTimelineDefinition::new();\n        timeline\n            .add_scalar_track(\n                semantic.reactive(),\n                tracker,\n                0.0,\n                10.0,\n                noon_core::TrackTiming::new(0.0, 2.0, noon_core::RateFunction::Linear),\n            )\n            .unwrap();\n        let scene = noon_core::TimedSemanticScene::from_parts(semantic, timeline).unwrap();\n        let scene_json = noon_ir::encode_timed_semantic_scene(&scene).unwrap();\n        let slots = format!(r#\"[{{\\\"id\\\":0,\\\"objects\\\":[{}]}}]\"#, object.get());\n        let mut player = HostScenePlayer::from_json(&scene_json, &slots).unwrap();\n        player.advance_to(0.5).unwrap();\n        let frame: Value = serde_json::from_str(&player.callback_frame_json().unwrap()).unwrap();\n        assert_eq!(frame[\"signals\"][0][\"signal\"], tracker.get());\n        assert_eq!(frame[\"signals\"][0][\"value\"][\"scalar\"], 2.5);\n    }\n\n"""
    text = replace_once(text, test_anchor, test + test_anchor, "host player timed test")
    return text

edit("crates/noon-web/src/host_player.rs", patch_host_player)


# 7) Python patch emitter can transport arbitrary supported geometry dictionaries.
def patch_python_ir(text: str) -> str:
    anchor = """    def set_transform(\n        self,\n        object_id: int,\n"""
    method = """    def set_geometry(self, object_id: int, geometry: dict[str, Any]) -> PatchBatch:\n        if not isinstance(geometry, dict) or len(geometry) != 1:\n            raise TypeError(\"geometry must be a single-variant Noon geometry dictionary\")\n        self._patches.append(\n            {\n                \"set_geometry\": {\n                    \"object\": _identifier(\"object_id\", object_id),\n                    \"geometry\": copy.deepcopy(geometry),\n                }\n            }\n        )\n        return self\n\n"""
    text = replace_once(text, anchor, method + anchor, "python PatchBatch set_geometry")
    return text

edit("web/python/_noon_ir.py", patch_python_ir)


# 8) ValueTracker reads runtime values only during an active host callback phase.
def patch_python_reactive(text: str) -> str:
    text = replace_once(
        text,
        "_ORIGINAL_SCENE_PLAY = _base.Scene.play\n",
        "_ORIGINAL_SCENE_PLAY = _base.Scene.play\n_ACTIVE_CALLBACK_SIGNAL_VALUES: dict[int, dict[str, Any]] | None = None\n",
        "reactive active signals global",
    )
    text = replace_once(
        text,
        "class _ValueAnimationBuilder:\n",
        "def _enter_callback_signal_values(frame: dict[str, Any]) -> None:\n    global _ACTIVE_CALLBACK_SIGNAL_VALUES\n    if _ACTIVE_CALLBACK_SIGNAL_VALUES is not None:\n        raise RuntimeError(\"nested Noon callback signal contexts are not supported\")\n    _ACTIVE_CALLBACK_SIGNAL_VALUES = {\n        int(item[\"signal\"]): item[\"value\"] for item in frame.get(\"signals\", [])\n    }\n\n\ndef _leave_callback_signal_values() -> None:\n    global _ACTIVE_CALLBACK_SIGNAL_VALUES\n    _ACTIVE_CALLBACK_SIGNAL_VALUES = None\n\n\nclass _ValueAnimationBuilder:\n",
        "reactive signal context helpers",
    )
    text = replace_once(
        text,
        "    def get_value(self) -> float:\n        return self._value\n",
        "    def get_value(self) -> float:\n        if self._signal_id is not None and _ACTIVE_CALLBACK_SIGNAL_VALUES is not None:\n            payload = _ACTIVE_CALLBACK_SIGNAL_VALUES.get(self._signal_id)\n            if payload is not None:\n                if \"scalar\" not in payload:\n                    raise TypeError(\"ValueTracker runtime signal is not scalar\")\n                return float(payload[\"scalar\"])\n        return self._value\n",
        "reactive runtime get_value",
    )
    return text

edit("web/python/_manim_reactive.py", patch_python_reactive)


# 9) Updater callback phase enters the signal-value context and emits geometry replacements
#    in the same atomic patch batch as transform/style edits.
def patch_python_updaters(text: str) -> str:
    text = replace_once(
        text,
        "import _noon_ir as _ir\nimport noon as _base\n",
        "import _noon_ir as _ir\nimport noon as _base\nimport _manim_reactive as _reactive\n",
        "updater reactive import",
    )
    text = replace_once(
        text,
        "            if before.geometry != after.geometry:\n                raise NotImplementedError(\n                    \"host updaters cannot mutate geometry yet; use transform/style \"\n                    \"mutations or native reactive expressions\"\n                )\n",
        "            if before.geometry != after.geometry:\n                batch.set_geometry(object_id, after.geometry)\n",
        "updater geometry patch",
    )
    old = """    _ACTIVE_CONTEXTS[scene_key] = context\n    try:\n        for mobject in session.mobjects:\n            for callback in list(_updaters(mobject)):\n                _invoke(callback, mobject, context.delta_time)\n    finally:\n        _ACTIVE_CONTEXTS.pop(scene_key, None)\n"""
    new = """    _ACTIVE_CONTEXTS[scene_key] = context\n    _reactive._enter_callback_signal_values(frame)\n    try:\n        for mobject in session.mobjects:\n            for callback in list(_updaters(mobject)):\n                _invoke(callback, mobject, context.delta_time)\n    finally:\n        _reactive._leave_callback_signal_values()\n        _ACTIVE_CONTEXTS.pop(scene_key, None)\n"""
    text = replace_once(text, old, new, "updater callback signal context")
    return text

edit("web/python/_manim_updaters.py", patch_python_updaters)


# 10) Manim-facing match_points copies geometric point state/placement but preserves source style.
def patch_python_geometry(text: str) -> str:
    text = replace_once(
        text,
        "import math\nfrom typing import Any\n",
        "import copy\nimport math\nfrom typing import Any\n",
        "geometry copy import",
    )
    anchor = """def install() -> None:\n"""
    fn = """def match_points(self: _base.Mobject, mobject: object) -> _base.Mobject:\n    if not isinstance(mobject, _base.Mobject):\n        raise TypeError(\"match_points expects a Mobject\")\n    source = self._current_raw()\n    target = mobject._current_raw()\n    source_kind = next(iter(source.geometry), None)\n    target_kind = next(iter(target.geometry), None)\n    if source_kind != target_kind or source_kind not in {\"line\", \"vector_path\"}:\n        raise NotImplementedError(\n            \"match_points currently supports Line/VMobject-path pairs with matching geometry kinds\"\n        )\n    raw = _base._raw_mobject(source)\n    raw.geometry = copy.deepcopy(target.geometry)\n    # Manim stores transformed VMobject points directly. Noon separates affine placement,\n    # so copying the target point state also copies its affine placement while source style\n    # (notably MovingDots' red line color) remains untouched.\n    raw.transform = copy.deepcopy(target.transform)\n    return self._apply(raw)\n\n\n"""
    text = replace_once(text, anchor, fn + anchor, "geometry match_points function")
    text = replace_once(
        text,
        "    _compat._bounds_for = _bounds_for\n",
        "    _compat._bounds_for = _bounds_for\n    _base.Mobject.match_points = match_points\n",
        "geometry install match_points",
    )
    return text

edit("web/python/_manim_geometry.py", patch_python_geometry)


# Focused Python regression tests for callback tracker reads and style-preserving match_points.
Path("web/python/test_moving_dots_primitives.py").write_text('''import unittest\n\nimport _manim_compat as manim\nimport _manim_reactive as reactive\nimport _manim_updaters as updaters\n\n\nclass MovingDotsPrimitiveTests(unittest.TestCase):\n    def test_value_tracker_reads_callback_runtime_signal_value(self):\n        scene = manim.Scene()\n        tracker = reactive.value_tracker(scene, 0.0)\n        reactive._enter_callback_signal_values(\n            {\"signals\": [{\"signal\": tracker.signal_id, \"value\": {\"scalar\": 2.25}}]}\n        )\n        try:\n            self.assertEqual(tracker.get_value(), 2.25)\n        finally:\n            reactive._leave_callback_signal_values()\n        self.assertEqual(tracker.get_value(), 0.0)\n\n    def test_line_match_points_preserves_source_color(self):\n        source = manim.Line((-1.0, 0.0), (1.0, 0.0)).set_color(manim.RED)\n        target = manim.Line((2.0, 3.0), (4.0, 5.0))\n        source.match_points(target)\n        self.assertEqual(source.geometry, target.geometry)\n        self.assertEqual(source.transform, target.transform)\n        self.assertEqual(source.style[\"stroke\"], manim.RED.to_ir())\n\n    def test_updater_patch_batch_contains_geometry_replacement(self):\n        scene = manim.Scene()\n        line = manim.Line((-1.0, 0.0), (1.0, 0.0)).set_color(manim.RED)\n        scene.add(line)\n        line.add_updater(lambda mob: mob.match_points(manim.Line((0.0, 0.0), (2.0, 1.0))))\n        registration = updaters.register_scene(scene)\n        frame = {\n            \"time\": 0.5,\n            \"delta_time\": 0.5,\n            \"signals\": [],\n            \"objects\": [\n                {\n                    \"object\": line.id,\n                    \"transform\": line.transform,\n                    \"style\": line.style,\n                    \"presence\": True,\n                    \"appearance\": 1.0,\n                    \"reveal\": 1.0,\n                    \"morph\": 0.0,\n                }\n            ],\n            \"invocations\": [{\"callback\": 0, \"object_indices\": [0]}],\n        }\n        import json\n        batch = json.loads(updaters.run_callback_phase(registration[\"session_id\"], frame, 0))\n        self.assertIn(\"set_geometry\", batch[\"patches\"][0])\n\n\nif __name__ == \"__main__\":\n    unittest.main()\n''')
