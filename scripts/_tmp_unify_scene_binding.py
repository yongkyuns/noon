from __future__ import annotations

import re
from pathlib import Path


def load(path: str) -> str:
    return Path(path).read_text()


def save(path: str, text: str) -> None:
    Path(path).write_text(text)


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


def replace_block(text: str, start: str, end: str, new: str, label: str) -> str:
    pattern = rf"(?ms)^{re.escape(start)}.*?(?=^{re.escape(end)})"
    updated, count = re.subn(pattern, new, text, count=1)
    if count != 1:
        raise RuntimeError(f"{label}: expected one block, found {count}")
    return updated


# Low-level scene: one allocator for every content type, while the legacy geometry
# projection remains a filtered serialization until direct SceneSpec production lands.
path = "web/python/_noon_ir.py"
text = load(path)
text = replace_once(
    text,
    '''        self._objects: list[dict[str, Any]] = []\n        self._tracks: list[dict[str, Any]] = []\n        self._object_keys: dict[int, str] = {}\n        self._track_keys: dict[int, str] = {}\n''',
    '''        self._objects: list[dict[str, Any]] = []\n        self._tracks: list[dict[str, Any]] = []\n        self._object_keys: dict[int, str] = {}\n        self._object_positions: dict[int, int] = {}\n        self._track_keys: dict[int, str] = {}\n        self._next_object_id = 0\n        self._next_painter_order = 0\n''',
    "scene allocator fields",
)
text = replace_once(
    text,
    '''    def add(self, mobject: Mobject, *, key: str | None = None) -> Object:\n        if not isinstance(mobject, Mobject):\n            raise TypeError("add expects a detached Mobject")\n        return self._append_snapshot(mobject.to_ir(), key)\n''',
    '''    def _allocate_object(self, key: str | None = None) -> tuple[Object, int]:\n        """Allocate one scene-global object identity and painter slot.\n\n        Content backends may store their payloads in different authoring projections,\n        but identity/order are scene concerns and therefore come from this one allocator.\n        """\n        object_id = self._next_object_id\n        painter_order = self._next_painter_order\n        authoring_key = _authoring_key("key", key, f"@object:{object_id}")\n        if authoring_key in self._object_keys.values():\n            raise ValueError(f"duplicate object key: {authoring_key}")\n        self._object_keys[object_id] = authoring_key\n        self._next_object_id += 1\n        self._next_painter_order += 1\n        return Object(object_id, self._owner), painter_order\n\n    def add(self, mobject: Mobject, *, key: str | None = None) -> Object:\n        if not isinstance(mobject, Mobject):\n            raise TypeError("add expects a detached Mobject")\n        return self._append_snapshot(mobject.to_ir(), key)\n''',
    "scene allocator method",
)
text = replace_block(
    text,
    "    def _authoring_checkpoint(self) -> tuple[Any, ...]:",
    "    def _schedule_transform(",
    '''    def _authoring_checkpoint(self) -> tuple[Any, ...]:\n        return (\n            len(self._objects),\n            len(self._tracks),\n            self._next_object_id,\n            self._next_painter_order,\n            dict(self._object_keys),\n            dict(self._object_positions),\n            dict(self._track_keys),\n            dict(self._scheduled_transform_targets),\n            dict(self._scheduled_transform_ends),\n            dict(self._scheduled_fade_ends),\n        )\n\n    def _restore_authoring_checkpoint(self, checkpoint: tuple[Any, ...]) -> None:\n        (\n            object_count,\n            track_count,\n            next_object_id,\n            next_painter_order,\n            object_keys,\n            object_positions,\n            track_keys,\n            scheduled_transform_targets,\n            scheduled_transform_ends,\n            scheduled_fade_ends,\n        ) = checkpoint\n        del self._objects[object_count:]\n        del self._tracks[track_count:]\n        self._next_object_id = next_object_id\n        self._next_painter_order = next_painter_order\n        self._object_keys = object_keys\n        self._object_positions = object_positions\n        self._track_keys = track_keys\n        self._scheduled_transform_targets = scheduled_transform_targets\n        self._scheduled_transform_ends = scheduled_transform_ends\n        self._scheduled_fade_ends = scheduled_fade_ends\n\n''',
    "scene checkpoint",
)
text = replace_block(
    text,
    "    def _snapshot_for_object(self, obj: Object) -> dict[str, Any]:",
    "    def _latest_track_at(",
    '''    def _snapshot_for_object(self, obj: Object) -> dict[str, Any]:\n        if not isinstance(obj, Object) or obj._owner is not self._owner:\n            raise ValueError("object must belong to this Scene")\n        position = self._object_positions.get(obj.id)\n        if position is None:\n            raise ValueError(f"object {obj.id} is not geometry-backed")\n        stored = self._objects[position]\n        return {\n            "geometry": copy.deepcopy(stored["geometry"]),\n            "transform": copy.deepcopy(stored["transform"]),\n            "style": copy.deepcopy(stored["style"]),\n        }\n\n''',
    "geometry object lookup",
)
text = replace_block(
    text,
    "    def _append_snapshot(",
    "    def _add_object(",
    '''    def _append_snapshot(\n        self, snapshot: dict[str, Any], key: str | None\n    ) -> Object:\n        obj, _ = self._allocate_object(key)\n        stored = copy.deepcopy(snapshot)\n        stored["id"] = obj.id\n        self._object_positions[obj.id] = len(self._objects)\n        self._objects.append(stored)\n        return obj\n\n''',
    "geometry projection allocation",
)
save(path, text)


# Public Mobject: object/content type owns materialization details, Scene owns identity,
# lifecycle orchestration and ordering.
path = "web/python/noon.py"
text = load(path)
text = replace_once(
    text,
    '''    def _bind(self, scene: Scene, obj: _ir.Object) -> None:\n        if self._scene is not None and self._scene is not scene:\n            raise ValueError("Mobject already belongs to another Scene")\n        self._scene = scene\n        self._object = obj\n\n    def _current_raw(self) -> _ir.Mobject:\n''',
    '''    def _bind(self, scene: Scene, obj: _ir.Object) -> None:\n        if self._scene is not None and self._scene is not scene:\n            raise ValueError("Mobject already belongs to another Scene")\n        self._scene = scene\n        self._object = obj\n\n    def _bind_to_scene(self, scene: Scene, *, key: str | None = None) -> _ir.Object:\n        obj = _ir.Scene.add(scene, self._current_raw(), key=key)\n        self._bind(scene, obj)\n        return obj\n\n    def _scene_lifecycle_state(\n        self, scene: Scene, time: float\n    ) -> tuple[bool, bool, bool]:\n        if self._scene is not scene or self._object is None:\n            raise ValueError("Mobject must belong to this Scene")\n        tracks = scene._presence_tracks(self._object)\n        has_future = bool(tracks and tracks[-1]["timing"]["start_time"] > time)\n        return bool(tracks), scene._presence_at(self._object, time), has_future\n\n    def _record_scene_presence(\n        self,\n        scene: Scene,\n        from_: bool,\n        to: bool,\n        time: float,\n        *,\n        key: str | None = None,\n    ) -> None:\n        if self._scene is not scene or self._object is None:\n            raise ValueError("Mobject must belong to this Scene")\n        scene._add_presence_track(self._object, from_, to, time, key=key)\n\n    def _is_present_in_scene(self, scene: Scene, time: float) -> bool:\n        if self._scene is not scene or self._object is None:\n            return False\n        return self._scene_lifecycle_state(scene, time)[1]\n\n    def _current_raw(self) -> _ir.Mobject:\n''',
    "mobject scene protocol",
)
text = replace_once(
    text,
    '''        stored = self._objects[obj.id]\n        stored["geometry"] = copy.deepcopy(raw.geometry)\n''',
    '''        position = self._object_positions.get(obj.id)\n        if position is None:\n            raise ValueError(f"object {obj.id} is not geometry-backed")\n        stored = self._objects[position]\n        stored["geometry"] = copy.deepcopy(raw.geometry)\n''',
    "static geometry lookup",
)
text = replace_once(
    text,
    '''            raw_object = super().add(mobject._current_raw(), key=key if index == 0 else None)\n            mobject._bind(self, raw_object)\n''',
    '''            mobject._bind_to_scene(self, key=key if index == 0 else None)\n''',
    "public scene bind",
)
save(path, text)


# Phase-B binding is now content-polymorphic.
path = "web/python/_manim_phase_b.py"
text = load(path)
text = replace_block(
    text,
    "def _bind_raw(",
    "def _schedule_raw_transform(",
    '''def _bind_raw(scene: _compat.Scene, member: _base.Mobject, *, key: str | None = None) -> None:\n    member._bind_to_scene(scene, key=key)\n    _persist_semantic_handle_at_binding(member)\n\n\n''',
    "phase-b polymorphic bind",
)
save(path, text)


# Compatibility Scene also delegates binding/presence to the object protocol rather
# than assuming every leaf has geometry state.
path = "web/python/_manim_compat.py"
text = load(path)
text = replace_once(
    text,
    '''        return any(\n            member._scene is self\n            and member._object is not None\n            and self._presence_at(member._object, self._cursor)\n            for member in leaves\n        )\n''',
    '''        return any(member._is_present_in_scene(self, self._cursor) for member in leaves)\n''',
    "compat presence dispatch",
)
text = replace_once(
    text,
    '''                raw_object = _ir.Scene.add(\n                    self, member._current_raw(), key=key if index == 0 else None\n                )\n                member._bind(self, raw_object)\n''',
    '''                member._bind_to_scene(self, key=key if index == 0 else None)\n''',
    "compat scene bind",
)
text = replace_once(
    text,
    '''                    raw_object = super().add(member._current_raw())\n                    member._bind(self, raw_object)\n''',
    '''                    member._bind_to_scene(self)\n''',
    "compat group introducer bind",
)
text = replace_once(
    text,
    '''                raw_object = super().add(target._current_raw())\n                target._bind(self, raw_object)\n''',
    '''                target._bind_to_scene(self)\n''',
    "compat introducer bind",
)
save(path, text)


# Lifecycle remains the single Scene.add/remove owner. Content objects supply only
# binding and presence-state realization below this boundary.
path = "web/python/_manim_lifecycle.py"
text = load(path)
text = replace_once(
    text,
    '''    assert member._object is not None\n    has_tracks, present, has_future = _presence_state(scene, member._object, time)\n    return _resolve(\n''',
    '''    has_tracks, present, has_future = member._scene_lifecycle_state(scene, time)\n    return _resolve(\n''',
    "lifecycle state dispatch",
)
text = replace_once(
    text,
    '''        assert member._object is not None\n        if plan.show_now:\n            self._add_presence_track(\n                member._object,\n                False,\n                True,\n                self._cursor,\n                key=f"@scene-add:{member._object.id}:{self._cursor:g}",\n            )\n''',
    '''        assert member._object is not None\n        if plan.show_now:\n            member._record_scene_presence(\n                self,\n                False,\n                True,\n                self._cursor,\n                key=f"@scene-add:{member._object.id}:{self._cursor:g}",\n            )\n''',
    "lifecycle add presence dispatch",
)
text = replace_once(
    text,
    '''        if plan.hide_now:\n            assert member._object is not None\n            self._add_presence_track(\n                member._object,\n                True,\n                False,\n                self._cursor,\n                key=f"@scene-remove:{member._object.id}:{self._cursor:g}",\n            )\n''',
    '''        if plan.hide_now:\n            assert member._object is not None\n            member._record_scene_presence(\n                self,\n                True,\n                False,\n                self._cursor,\n                key=f"@scene-remove:{member._object.id}:{self._cursor:g}",\n            )\n''',
    "lifecycle remove presence dispatch",
)
text = replace_once(
    text,
    '''        assert member._object is not None\n        if plan.show_now:\n            scene._add_presence_track(\n                member._object,\n                False,\n                True,\n                start_time,\n                key=f"@scene-play-add:{member._object.id}:{start_time:g}",\n            )\n''',
    '''        assert member._object is not None\n        if plan.show_now:\n            member._record_scene_presence(\n                scene,\n                False,\n                True,\n                start_time,\n                key=f"@scene-play-add:{member._object.id}:{start_time:g}",\n            )\n''',
    "lifecycle animation presence dispatch",
)
save(path, text)


# Retained Text becomes one implementation of the common scene-object protocol.
path = "web/python/_manim_typst.py"
text = load(path)
text = text.replace("_RETAINED_OBJECT_ID_BASE = 1 << 52\n", "")
text = replace_block(
    text,
    "    def _bind_retained(",
    "    def _retained_entry(",
    '''    def _bind_retained(\n        self, scene: _compat.Scene, obj: object, order: int\n    ) -> None:\n        if self._scene is not None and self._scene is not scene:\n            raise ValueError("retained text Mobject already belongs to another Scene")\n        self._scene = scene\n        self._object = obj\n        self._retained_object_id = int(obj.id)\n        self._retained_order = int(order)\n\n    def _bind_to_scene(\n        self, scene: _compat.Scene, *, key: str | None = None\n    ) -> object:\n        if self._scene is scene and self._object is not None:\n            return self._object\n        if self._scene is not None:\n            raise ValueError("retained text Mobject already belongs to another Scene")\n        _ensure_scene_state(scene)\n        obj, order = scene._allocate_object(key)\n        self._bind_retained(scene, obj, order)\n        scene._retained_text_objects.append(self)\n        return obj\n\n    def _initial_scene_animation_state(self) -> dict[str, Any]:\n        spec = self._spec()\n        transform = spec["transform"]\n        translation = transform["translation"]\n        scale = transform["scale"]\n        return {\n            "appearance": 1.0,\n            "opacity": float(spec["opacity"]),\n            "position": {"x": float(translation["x"]), "y": float(translation["y"])},\n            "presence": True,\n            "rotation": float(transform["rotation"]),\n            "scale": {"x": float(scale["x"]), "y": float(scale["y"])},\n            "runtime_position": {\n                "x": float(translation["x"]),\n                "y": float(translation["y"]),\n            },\n            "runtime_scale": {"x": float(scale["x"]), "y": float(scale["y"])},\n        }\n\n    def _scene_lifecycle_state(\n        self, scene: _compat.Scene, time: float\n    ) -> tuple[bool, bool, bool]:\n        if self._scene is not scene or self._object is None:\n            raise ValueError("retained text Mobject must belong to this Scene")\n        _ensure_scene_state(scene)\n        object_id = int(self._object.id)\n        tracks = _retained_presence_tracks(scene, object_id)\n        state = scene._retained_animation_state.get(object_id)\n        present = True if state is None else bool(state["presence"])\n        has_future = bool(tracks and tracks[-1]["timing"]["start_time"] > time)\n        return bool(tracks), present, has_future\n\n    def _record_scene_presence(\n        self,\n        scene: _compat.Scene,\n        from_: bool,\n        to: bool,\n        time: float,\n        *,\n        key: str | None = None,\n    ) -> None:\n        if self._scene is not scene or self._object is None:\n            raise ValueError("retained text Mobject must belong to this Scene")\n        _ensure_scene_state(scene)\n        object_id = int(self._object.id)\n        tracks = _retained_presence_tracks(scene, object_id)\n        previous = tracks[-1] if tracks else None\n        from _manim_lifecycle import _validate_shared_presence_transition\n\n        result = _validate_shared_presence_transition(\n            previous is not None,\n            0.0 if previous is None else float(previous["timing"]["start_time"]),\n            False if previous is None else bool(previous["values"]["bool"]["to"]),\n            float(time),\n            bool(from_),\n        )\n        if not bool(result.ok):\n            raise ValueError(str(result.message))\n        track_id = len(scene._retained_animation_tracks)\n        scene._retained_animation_tracks.append(\n            {\n                "id": track_id,\n                "object": object_id,\n                "property": "presence",\n                "values": {"bool": {"from": bool(from_), "to": bool(to)}},\n                "timing": {\n                    "start_time": float(time),\n                    "duration": 0.0,\n                    "easing": "linear",\n                },\n            }\n        )\n        state = scene._retained_animation_state.setdefault(\n            object_id, self._initial_scene_animation_state()\n        )\n        state["presence"] = bool(to)\n\n    def _is_present_in_scene(self, scene: _compat.Scene, time: float) -> bool:\n        if self._scene is not scene or self._object is None:\n            return False\n        return self._scene_lifecycle_state(scene, time)[1]\n\n''',
    "retained text scene protocol",
)
text = replace_block(
    text,
    "def _ensure_scene_state(",
    "def _retained_document(",
    '''def _ensure_scene_state(scene: _compat.Scene) -> None:\n    if not hasattr(scene, "_retained_text_objects"):\n        scene._retained_text_objects = []\n    if not hasattr(scene, "_retained_animation_tracks"):\n        scene._retained_animation_tracks = []\n    if not hasattr(scene, "_retained_animation_state"):\n        scene._retained_animation_state = {}\n\n\ndef _retained_presence_tracks(\n    scene: _compat.Scene, object_id: int\n) -> list[dict[str, Any]]:\n    _ensure_scene_state(scene)\n    return [\n        track\n        for track in scene._retained_animation_tracks\n        if track["object"] == object_id and track["property"] == "presence"\n    ]\n\n\n''',
    "retained scene state",
)
text = text.replace("    _compat.Scene.add = _scene_add\n", "")
text = text.replace("    _compat.Scene._is_present = _scene_is_present\n", "")
save(path, text)


# Retained animation owns retained animation behavior only; it no longer owns core
# scene lifecycle methods.
path = "web/python/_manim_retained_animate.py"
text = load(path)
text = replace_once(
    text,
    "import _manim_lifecycle as _lifecycle\nimport _manim_typst as _typst\n",
    "import _manim_lifecycle as _lifecycle\nimport _manim_phase_b as _phase_b\nimport _manim_typst as _typst\n",
    "retained phase-b import",
)
for line in (
    "_ORIGINAL_SCENE_ADD = _compat.Scene.add\n",
    "_ORIGINAL_SCENE_REMOVE = _compat.Scene.remove\n",
    "_ORIGINAL_SCENE_IS_PRESENT = _compat.Scene._is_present\n",
):
    text = text.replace(line, "")
text = replace_block(
    text,
    "def _ensure_animation_state(",
    "def _vec2(",
    '''def _ensure_animation_state(scene: _compat.Scene) -> None:\n    _typst._ensure_scene_state(scene)\n\n\n''',
    "retained animation state owner",
)
text = replace_block(
    text,
    "def _initial_animation_state(",
    "def _state_for(",
    '''def _initial_animation_state(\n    source: _typst._RetainedTextMobject,\n) -> dict[str, Any]:\n    return source._initial_scene_animation_state()\n\n\n''',
    "retained initial state delegation",
)
text = replace_block(
    text,
    "def _retained_presence_tracks(",
    "def _retained_presence_at(",
    '''def _retained_presence_tracks(\n    scene: _compat.Scene, object_id: int\n) -> list[dict[str, Any]]:\n    return _typst._retained_presence_tracks(scene, object_id)\n\n\n''',
    "retained presence track delegation",
)
text = replace_block(
    text,
    "def _resolve_retained_lifecycle(",
    "def _bind_retained(",
    '''def _resolve_retained_lifecycle(\n    scene: _compat.Scene,\n    source: _typst._RetainedTextMobject,\n    intent: str,\n    time: float,\n    label: str,\n) -> _lifecycle.LifecyclePlan:\n    return _lifecycle._resolve_wrapper(scene, source, intent, time, label)\n\n\n''',
    "retained lifecycle delegation",
)
text = replace_block(
    text,
    "def _bind_retained(",
    "def _append_presence_track(",
    '''def _bind_retained(\n    scene: _compat.Scene, source: _typst._RetainedTextMobject\n) -> None:\n    if source._scene is None:\n        _phase_b._bind_raw(scene, source)\n    elif source._scene is not scene:\n        raise ValueError("retained Text already belongs to another Scene")\n    scene._register_top_level(source)\n\n\n''',
    "retained bind delegation",
)
text = replace_block(
    text,
    "def _retained_scene_add(",
    "def _retained_scene_play(",
    "",
    "remove retained scene lifecycle interceptors",
)
text = replace_once(
    text,
    '''    retained_objects_before = list(self._retained_text_objects)\n    next_object_id_before = self._retained_next_object_id\n    next_order_before = self._retained_next_painter_order\n    tracks_before = copy.deepcopy(self._retained_animation_tracks)\n''',
    '''    authoring_checkpoint = self._authoring_checkpoint()\n    retained_objects_before = list(self._retained_text_objects)\n    tracks_before = copy.deepcopy(self._retained_animation_tracks)\n''',
    "retained rollback allocator checkpoint",
)
text = replace_once(
    text,
    '''            source._scene,\n            source._retained_object_id,\n            source._retained_order,\n''',
    '''            source._scene,\n            source._object,\n            source._retained_object_id,\n            source._retained_order,\n''',
    "retained wrapper object checkpoint",
)
text = replace_once(
    text,
    '''        self._cursor = cursor_before\n        self._compat_top_level = top_level_before\n        self._retained_text_objects = retained_objects_before\n        self._retained_next_object_id = next_object_id_before\n        self._retained_next_painter_order = next_order_before\n        self._retained_animation_tracks = tracks_before\n        self._retained_animation_state = state_before\n        for source, old_scene, old_object_id, old_order in wrapper_states.values():\n            source._scene = old_scene\n            source._retained_object_id = old_object_id\n            source._retained_order = old_order\n''',
    '''        self._restore_authoring_checkpoint(authoring_checkpoint)\n        self._cursor = cursor_before\n        self._compat_top_level = top_level_before\n        self._retained_text_objects = retained_objects_before\n        self._retained_animation_tracks = tracks_before\n        self._retained_animation_state = state_before\n        for source, old_scene, old_object, old_object_id, old_order in wrapper_states.values():\n            source._scene = old_scene\n            source._object = old_object\n            source._retained_object_id = old_object_id\n            source._retained_order = old_order\n''',
    "retained rollback restore",
)
for line in (
    "    _compat.Scene.add = _retained_scene_add\n",
    "    _compat.Scene.remove = _retained_scene_remove\n",
    "    _compat.Scene._is_present = _retained_scene_is_present\n",
):
    text = text.replace(line, "")
text = text.replace(
    '    """Install retained animation scheduling after retained Text Scene hooks."""\n',
    '    """Install retained animation scheduling without taking over Scene lifecycle."""\n',
)
save(path, text)


# Worker boot order comment now documents the ownership rule rather than the old hook.
path = "web/python-worker.source.js"
text = load(path)
text = replace_once(
    text,
    '''# Retained text must wrap the final lifecycle-aware Scene.add implementation so\n# it is intercepted before any legacy geometry binding occurs.\nimport _manim_typst\n''',
    '''# Lifecycle owns Scene.add/remove for every content type. Retained text contributes\n# only its object-binding/resource realization below that shared scene boundary.\nimport _manim_typst\n''',
    "worker scene ownership comment",
)
save(path, text)


# Topology + allocator ratchet: future content modules must not reclaim Scene lifecycle.
path = "web/python/test_unified_scene_binding.py"
save(
    path,
    '''from pathlib import Path\n\nimport _noon_ir as _ir\n\n\ndef test_scene_allocator_is_shared_across_content_projections():\n    scene = _ir.Scene()\n    first = scene.add(_ir.Circle(1.0))\n    retained, retained_order = scene._allocate_object()\n    third = scene.add(_ir.Circle(2.0))\n\n    assert (first.id, retained.id, third.id) == (0, 1, 2)\n    assert retained_order == 1\n    assert [obj["id"] for obj in scene.to_document()["objects"]] == [0, 2]\n    assert [entry["id"] for entry in scene.identity_document()["objects"]] == [0, 1, 2]\n\n\ndef test_retained_content_modules_do_not_own_scene_lifecycle():\n    root = Path(__file__).parent\n    typst = (root / "_manim_typst.py").read_text()\n    retained_animation = (root / "_manim_retained_animate.py").read_text()\n\n    forbidden = (\n        "_compat.Scene.add =",\n        "_compat.Scene.remove =",\n        "_compat.Scene._is_present =",\n    )\n    for source in (typst, retained_animation):\n        for assignment in forbidden:\n            assert assignment not in source\n\n\ndef test_phase_b_binding_is_content_polymorphic():\n    source = (Path(__file__).parent / "_manim_phase_b.py").read_text()\n    assert "member._bind_to_scene(scene, key=key)" in source\n    assert "_base.Scene.add(scene, raw" not in source\n''',
)

print("unified scene binding rewrite applied")
