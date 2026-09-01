from pathlib import Path


def replace_once(path: str, old: str, new: str, label: str) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one match, found {count}")
    file.write_text(text.replace(old, new, 1))


# Retained content is no longer an alternate Scene entry path.
replace_once(
    "web/python/_manim_typst.py",
    '''"""ManimCE retained Text/Typst wrappers over Noon's source-level authoring handles.

These wrappers are intentionally not geometry adapters. They never synthesize an
``_ir.Mobject`` and ``Scene.add`` intercepts them before legacy geometry lowering.
Only source-level text authoring state crosses the Python-worker boundary; shaping,
font bytes, glyph/vector resources, and GPU atlas state remain Rust-owned.
"""
''',
    '''"""ManimCE Text/Typst wrappers over Noon's retained source authoring handles.

Text is a normal scene object at the lifecycle/identity boundary and specializes only
its content binding and retained-resource realization. It never synthesizes fake
geometry; shaping, font bytes, glyph/vector resources, and GPU atlas state remain
Rust-owned.
"""
''',
    "retained text module docstring",
)
replace_once(
    "web/python/_manim_typst.py",
    '''# Reserve the upper half of JavaScript's exact-integer range for retained text IDs.
# Legacy geometry IDs start at zero and no practical scene can approach 2^52 objects.
_RETAINED_PROTOCOL_VERSION = 2
''',
    '''_RETAINED_PROTOCOL_VERSION = 2
''',
    "obsolete retained id namespace comment",
)
replace_once(
    "web/python/_manim_typst.py",
    '''_ORIGINAL_SCENE_ADD = _compat.Scene.add
_ORIGINAL_SCENE_IS_PRESENT = _compat.Scene._is_present
''',
    "",
    "obsolete retained scene captures",
)

# Both projections use the same conservative future-event test rather than relying on
# an incidental list-order invariant.
replace_once(
    "web/python/noon.py",
    '''        has_future = bool(tracks and tracks[-1]["timing"]["start_time"] > time)
''',
    '''        has_future = any(float(track["timing"]["start_time"]) > time for track in tracks)
''',
    "geometry lifecycle future-event check",
)
replace_once(
    "web/python/_manim_typst.py",
    '''        has_future = bool(tracks and tracks[-1]["timing"]["start_time"] > time)
''',
    '''        has_future = any(float(track["timing"]["start_time"]) > time for track in tracks)
''',
    "retained lifecycle future-event check",
)

# One retained-content presence primitive is shared by Scene lifecycle and retained
# animation scheduling. Content owns its projection; lifecycle owns policy.
typst = Path("web/python/_manim_typst.py")
text = typst.read_text()
old_method = '''    def _record_scene_presence(
        self,
        scene: _compat.Scene,
        from_: bool,
        to: bool,
        time: float,
        *,
        key: str | None = None,
    ) -> None:
        if self._scene is not scene or self._object is None:
            raise ValueError("retained text Mobject must belong to this Scene")
        _ensure_scene_state(scene)
        object_id = int(self._object.id)
        tracks = _retained_presence_tracks(scene, object_id)
        previous = tracks[-1] if tracks else None
        from _manim_lifecycle import _validate_shared_presence_transition

        result = _validate_shared_presence_transition(
            previous is not None,
            0.0 if previous is None else float(previous["timing"]["start_time"]),
            False if previous is None else bool(previous["values"]["bool"]["to"]),
            float(time),
            bool(from_),
        )
        if not bool(result.ok):
            raise ValueError(str(result.message))
        track_id = len(scene._retained_animation_tracks)
        scene._retained_animation_tracks.append(
            {
                "id": track_id,
                "object": object_id,
                "property": "presence",
                "values": {"bool": {"from": bool(from_), "to": bool(to)}},
                "timing": {
                    "start_time": float(time),
                    "duration": 0.0,
                    "easing": "linear",
                },
            }
        )
        state = scene._retained_animation_state.setdefault(
            object_id, self._initial_scene_animation_state()
        )
        state["presence"] = bool(to)
'''
new_method = '''    def _record_scene_presence(
        self,
        scene: _compat.Scene,
        from_: bool,
        to: bool,
        time: float,
        *,
        key: str | None = None,
    ) -> None:
        del key
        if self._scene is not scene or self._object is None:
            raise ValueError("retained text Mobject must belong to this Scene")
        _append_retained_presence_track(
            scene,
            object_id=int(self._object.id),
            current=from_,
            target=to,
            start_time=time,
        )
        state = scene._retained_animation_state.setdefault(
            int(self._object.id), self._initial_scene_animation_state()
        )
        state["presence"] = bool(to)
'''
if text.count(old_method) != 1:
    raise RuntimeError("retained presence method: expected one match")
text = text.replace(old_method, new_method, 1)
anchor = '''def _retained_presence_tracks(
    scene: _compat.Scene, object_id: int
) -> list[dict[str, Any]]:
    _ensure_scene_state(scene)
    return [
        track
        for track in scene._retained_animation_tracks
        if track["object"] == object_id and track["property"] == "presence"
    ]


'''
helper = anchor + '''def _append_retained_presence_track(
    scene: _compat.Scene,
    *,
    object_id: int,
    current: bool,
    target: bool,
    start_time: float,
) -> None:
    _ensure_scene_state(scene)
    tracks = _retained_presence_tracks(scene, object_id)
    previous = tracks[-1] if tracks else None
    from _manim_lifecycle import _validate_shared_presence_transition

    result = _validate_shared_presence_transition(
        previous is not None,
        0.0 if previous is None else float(previous["timing"]["start_time"]),
        False if previous is None else bool(previous["values"]["bool"]["to"]),
        float(start_time),
        bool(current),
    )
    if not bool(result.ok):
        raise ValueError(str(result.message))
    scene._retained_animation_tracks.append(
        {
            "object": int(object_id),
            "property": "presence",
            "values": {"bool": {"from": bool(current), "to": bool(target)}},
            "timing": {
                "start_time": float(start_time),
                "duration": 0.0,
                "easing": "linear",
            },
        }
    )


'''
if text.count(anchor) != 1:
    raise RuntimeError("retained presence helper anchor: expected one match")
typst.write_text(text.replace(anchor, helper, 1))

retained = Path("web/python/_manim_retained_animate.py")
text = retained.read_text()
old = '''def _append_presence_track(
    scene: _compat.Scene,
    *,
    object_id: int,
    current: bool,
    target: bool,
    start_time: float,
) -> None:
    existing = _retained_presence_tracks(scene, object_id)
    previous = existing[-1] if existing else None
    result = _lifecycle._validate_shared_presence_transition(
        previous is not None,
        0.0 if previous is None else float(previous["timing"]["start_time"]),
        False if previous is None else bool(previous["values"]["bool"]["to"]),
        float(start_time),
        bool(current),
    )
    if not bool(result.ok):
        raise ValueError(str(result.message))

    scene._retained_animation_tracks.append(
        {
            "object": object_id,
            "property": "presence",
            "values": {"bool": {"from": bool(current), "to": bool(target)}},
            "timing": {
                "start_time": float(start_time),
                "duration": 0.0,
                "easing": "linear",
            },
        }
    )
'''
new = '''def _append_presence_track(
    scene: _compat.Scene,
    *,
    object_id: int,
    current: bool,
    target: bool,
    start_time: float,
) -> None:
    _typst._append_retained_presence_track(
        scene,
        object_id=object_id,
        current=current,
        target=target,
        start_time=start_time,
    )
'''
if text.count(old) != 1:
    raise RuntimeError("retained animation presence helper: expected one match")
retained.write_text(text.replace(old, new, 1))

# Strengthen the ratchet with the actual public compatibility path: geometry and Text
# interleave in one scene-global identity/order domain despite separate transitional
# serialization projections.
test = Path("web/python/test_unified_scene_binding.py")
text = test.read_text()
text = text.replace(
    "import _noon_ir as _ir\n",
    "import _noon_ir as _ir\nimport _manim_compat as _compat\nimport _manim_typst as _typst\nimport noon as _base\n",
    1,
)
text += '''\n\ndef test_mixed_geometry_and_text_share_public_scene_identity_and_order():
    _compat.install()
    _typst.install()
    import _manim_lifecycle  # noqa: F401 - installs the single lifecycle owner

    scene = _compat.Scene()
    first = _base.Circle(1.0)
    text = _typst.Text("AB")
    third = _base.Circle(2.0)

    scene.add(first)
    scene.add(text)
    scene.add(third)

    assert (first.id, text.id, third.id) == (0, 1, 2)
    retained = scene.retained_document()["objects"]
    assert len(retained) == 1
    assert retained[0]["object"] == 1
    assert retained[0]["order"] == 1
    assert [obj["id"] for obj in scene.to_document()["objects"]] == [0, 2]
    assert [entry["id"] for entry in scene.identity_document()["objects"]] == [0, 1, 2]
'''
test.write_text(text)

print("unified scene binding cleanup applied")
