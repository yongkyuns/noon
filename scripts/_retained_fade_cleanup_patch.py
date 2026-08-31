from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, got {count}: {old!r}")
    target.write_text(text.replace(old, new, 1))


animate = "web/python/_manim_retained_animate.py"
replace_once(
    animate,
    '''def _assert_direct_readd_runtime_is_canonical(
    scene: _compat.Scene,
    source: _typst._RetainedTextMobject,
    plan: _lifecycle.LifecyclePlan,
) -> None:
    if not plan.show_now or source._scene is not scene or source._retained_object_id is None:
        return
    state = scene._retained_animation_state.get(int(source.id))
    if state is None:
        return
    canonical_position = state["position"]
    canonical_scale = state["scale"]
    if (
        not math.isclose(float(state["appearance"]), 1.0, abs_tol=1e-15)
        or state["runtime_position"] != canonical_position
        or state["runtime_scale"] != canonical_scale
    ):
        raise NotImplementedError(
            "retained Text Scene.add cannot yet restore a transient FadeOut endpoint; "
            "reintroduce it with FadeIn or .animate until instant retained state resets "
            "are represented by the shared core timeline"
        )


''',
    '',
)
replace_once(
    animate,
    '''    for value, plan in plans:
        _assert_direct_readd_runtime_is_canonical(self, value, plan)

    result = _ORIGINAL_SCENE_ADD(self, *mobjects, key=key)
''',
    '''    result = _ORIGINAL_SCENE_ADD(self, *mobjects, key=key)
''',
)
replace_once(
    animate,
    '''    if lifecycle.hide_at_end:
        _append_presence_track(
            scene,
            object_id=object_id,
            current=True,
            target=False,
            start_time=start_time + duration,
        )
        state["presence"] = False
    state["appearance"] = 0.0
    state["runtime_position"] = copy.deepcopy(faded_position)
    state["runtime_scale"] = copy.deepcopy(faded_scale)
''',
    '''    end_time = start_time + duration
    if lifecycle.hide_at_end:
        _append_presence_track(
            scene,
            object_id=object_id,
            current=True,
            target=False,
            start_time=end_time,
        )
        state["presence"] = False

    # Manim FadeOut cleanup removes the object, then restores interpolation alpha 0.
    # Keep that restoration in the ordinary property channels so direct seek and
    # later Scene.add observe canonical state without Python-only repair state.
    _append_scalar_track(
        scene,
        object_id=object_id,
        property_name="appearance",
        current=0.0,
        target=1.0,
        start_time=end_time,
        duration=0.0,
        easing="linear",
    )
    if faded_position != canonical_position:
        _append_vec2_track(
            scene,
            object_id=object_id,
            property_name="position",
            current=faded_position,
            target=canonical_position,
            start_time=end_time,
            duration=0.0,
            easing="linear",
        )
    if faded_scale != canonical_scale:
        _append_vec2_track(
            scene,
            object_id=object_id,
            property_name="scale",
            current=faded_scale,
            target=canonical_scale,
            start_time=end_time,
            duration=0.0,
            easing="linear",
        )
    state["appearance"] = 1.0
    state["runtime_position"] = copy.deepcopy(canonical_position)
    state["runtime_scale"] = copy.deepcopy(canonical_scale)
''',
)

smoke = "scripts/retained-text-animation-smoke.mjs"
replace_once(
    smoke,
    '''        self.wait(0.5)
        self.play(label.animate.shift(UP), run_time=1.0)
        assert label in self.mobjects
''',
    '''        self.wait(0.5)
        self.add(label)
        assert label in self.mobjects
        self.play(label.animate.shift(UP), run_time=1.0)
        assert label in self.mobjects
''',
)
replace_once(
    smoke,
    '''  assertNear(fadeAppearance[2].values.scalar.from, 1, "reintroduced appearance source");
  assertNear(fadeAppearance[2].values.scalar.to, 1, "reintroduced appearance target");
  assert.equal(fadeAppearance[2].timing.start_time, 3);
  assert.equal(fadeAppearance[2].timing.duration, 1);
  assert.equal(fadeAppearance[2].timing.easing, "linear");
''',
    '''  assertNear(fadeAppearance[2].values.scalar.from, 0, "FadeOut cleanup appearance source");
  assertNear(fadeAppearance[2].values.scalar.to, 1, "FadeOut cleanup appearance target");
  assert.equal(fadeAppearance[2].timing.start_time, 2.5);
  assert.equal(fadeAppearance[2].timing.duration, 0);
  assert.equal(fadeAppearance[2].timing.easing, "linear");
''',
)
replace_once(
    smoke,
    '''  assert.deepEqual(fadePosition[2].values.vec2, {
    from: { x: -1, y: 0 },
    to: { x: -1, y: 1 },
  });
  assert.equal(fadePosition[2].timing.start_time, 3);
  assert.equal(fadePosition[2].timing.duration, 1);
  assert.equal(fadePosition[2].timing.easing, "smooth");
''',
    '''  assert.equal(fadePosition.length, 4);
  assert.deepEqual(fadePosition[2].values.vec2, {
    from: { x: 1, y: 0 },
    to: { x: -1, y: 0 },
  });
  assert.equal(fadePosition[2].timing.start_time, 2.5);
  assert.equal(fadePosition[2].timing.duration, 0);
  assert.equal(fadePosition[2].timing.easing, "linear");

  assert.deepEqual(fadePosition[3].values.vec2, {
    from: { x: -1, y: 0 },
    to: { x: -1, y: 1 },
  });
  assert.equal(fadePosition[3].timing.start_time, 3);
  assert.equal(fadePosition[3].timing.duration, 1);
  assert.equal(fadePosition[3].timing.easing, "smooth");
''',
)
replace_once(
    smoke,
    '''  assert.deepEqual(fadeScale[2].values.vec2, {
    from: { x: 1, y: 1 },
    to: { x: 1, y: 1 },
  });
  assert.equal(fadeScale[2].timing.start_time, 3);
  assert.equal(fadeScale[2].timing.duration, 1);
  assert.equal(fadeScale[2].timing.easing, "linear");
''',
    '''  assert.deepEqual(fadeScale[2].values.vec2, {
    from: { x: 1.5, y: 1.5 },
    to: { x: 1, y: 1 },
  });
  assert.equal(fadeScale[2].timing.start_time, 2.5);
  assert.equal(fadeScale[2].timing.duration, 0);
  assert.equal(fadeScale[2].timing.easing, "linear");
''',
)
replace_once(
    smoke,
    '''    "Retained Text animation smoke passed: scale, position, rotation, opacity, and FadeIn/FadeOut lower to source-level retained tracks; lifecycle transitions use retained presence/appearance channels; fade shift/scale endpoints stay transient; reintroduction restores canonical state; and unsupported retained properties fail without legacy geometry.",
''',
    '''    "Retained Text animation smoke passed: scale, position, rotation, opacity, and FadeIn/FadeOut lower to source-level retained tracks; FadeOut cleanup restores canonical appearance/position/scale at the removal timestamp; direct Scene.add reintroduces canonical state with Presence only; and unsupported retained properties fail without legacy geometry.",
''',
)
