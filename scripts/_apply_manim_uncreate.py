from pathlib import Path


def patch(path: str, replacements: list[tuple[str, str]]) -> None:
    file = Path(path)
    text = file.read_text()
    for old, new in replacements:
        count = text.count(old)
        if count != 1:
            raise SystemExit(f'{path}: expected one match, got {count}: {old[:140]!r}')
        text = text.replace(old, new, 1)
    file.write_text(text)


patch('web/python/noon.py', [
    (
'''@dataclass(frozen=True, slots=True)
class Create:
    """Progressively draw a shape without changing its steady-state geometry."""

    target: Mobject | _ir.Object
    key: str | None = None


@dataclass(frozen=True, slots=True)
class FadeIn:
''',
'''@dataclass(frozen=True, slots=True)
class Create:
    """Progressively draw a shape without changing its steady-state geometry."""

    target: Mobject | _ir.Object
    key: str | None = None


@dataclass(frozen=True, slots=True)
class Uncreate(Create):
    """Manim-style Create in reverse, optionally removing the target at completion."""

    reverse_rate_function: bool = True
    remover: bool = True


@dataclass(frozen=True, slots=True)
class FadeIn:
'''),
    (
'''    def play(
        self,
        *animations: Any,
''',
'''    def _schedule_uncreate(
        self,
        animation: Uncreate,
        *,
        duration: float,
        start_time: float,
        easing: str,
    ) -> None:
        obj = self._raw_object(animation.target)
        start = float(start_time)
        run_duration = float(duration)
        if not math.isfinite(start) or start < 0.0:
            raise ValueError("start_time must be finite and non-negative")
        if not math.isfinite(run_duration) or run_duration <= 0.0:
            raise ValueError("duration must be finite and positive")
        end = start + run_duration

        snapshot = self._snapshot_for_object_at(obj, start)
        geometry = snapshot["geometry"]
        if not any(name in geometry for name in ("circle", "rectangle", "line", "vector_path")):
            raise ValueError("Uncreate supports Circle, Rectangle/Square, Line, and VectorPath")

        self._ensure_lifecycle_timeline_available(obj, start, "Uncreate target")
        if not self._presence_at(obj, start):
            raise ValueError("Uncreate target must be present at animation start")

        for track in self._tracks:
            if track["object"] != obj.id or track["property"] != "reveal":
                continue
            track_start = track["timing"]["start_time"]
            track_end = track_start + track["timing"]["duration"]
            if track_start < end and start < track_end:
                raise ValueError("Create/reveal animations for one object must not overlap")

        object_key = self._object_keys[obj.id]
        root_key = animation.key or f"@uncreate:{object_key}:{start:g}"
        reverse = bool(animation.reverse_rate_function)
        self._add_scalar_track(
            obj,
            "reveal",
            1.0 if reverse else 0.0,
            0.0 if reverse else 1.0,
            start,
            run_duration,
            easing,
            root_key,
        )
        if animation.remover:
            self._add_presence_track(
                obj,
                True,
                False,
                end,
                key=f"{root_key}.remove",
            )

    def play(
        self,
        *animations: Any,
'''),
    ('        creates: list[Create] = []\n', '        creates: list[Create] = []\n        uncreates: list[Uncreate] = []\n'),
    (
'''            elif isinstance(animation, Create):
                creates.append(animation)
''',
'''            elif isinstance(animation, Uncreate):
                uncreates.append(animation)
            elif isinstance(animation, Create):
                creates.append(animation)
'''),
    (
'''            for animation in creates:
                self._schedule_create(
                    animation,
                    duration=actual_duration,
                    start_time=actual_start,
                    easing=easing,
                )
''',
'''            for animation in creates:
                self._schedule_create(
                    animation,
                    duration=actual_duration,
                    start_time=actual_start,
                    easing=easing,
                )
            for animation in uncreates:
                self._schedule_uncreate(
                    animation,
                    duration=actual_duration,
                    start_time=actual_start,
                    easing=easing,
                )
'''),
    ('    "Create",\n', '    "Create",\n    "Uncreate",\n'),
])

patch('web/python/_manim_animate.py', [
    ('_ORIGINAL_CREATE = _base.Create\n', '_ORIGINAL_CREATE = _base.Create\n_ORIGINAL_UNCREATE = _base.Uncreate\n'),
    (
'''class Create(_ORIGINAL_CREATE):
    def __init__(self, target: object, key: str | None = None, **kwargs: Any) -> None:
        super().__init__(target, key)
        _store_animation_args(self, kwargs)


class FadeIn(_ORIGINAL_FADE_IN):
''',
'''class Create(_ORIGINAL_CREATE):
    def __init__(self, target: object, key: str | None = None, **kwargs: Any) -> None:
        super().__init__(target, key)
        _store_animation_args(self, kwargs)


class Uncreate(_ORIGINAL_UNCREATE):
    def __init__(
        self,
        target: object,
        key: str | None = None,
        reverse_rate_function: bool = True,
        remover: bool = True,
        **kwargs: Any,
    ) -> None:
        super().__init__(target, key, bool(reverse_rate_function), bool(remover))
        _store_animation_args(self, kwargs)


class FadeIn(_ORIGINAL_FADE_IN):
'''),
    ('    "Create": Create,\n', '    "Create": Create,\n    "Uncreate": Uncreate,\n'),
    (
'''def _default_lag_ratio(animation: object) -> float:
''',
'''def _uncreate_track_settings(easing: str, reverse_rate_function: bool) -> tuple[str, bool]:
    """Represent Manim's `rate_func(1 - alpha)` using Noon's scalar track.

    Most supported rate functions can be expressed by reversing the reveal endpoints
    and using the complement-reversed easing. `there_and_back` is time-symmetric and
    therefore keeps the forward endpoints instead.
    """
    if not reverse_rate_function:
        return easing, False
    if easing in {"linear", "smooth", "ease_in_out_cubic"}:
        return easing, True
    if easing == "rush_into":
        return "rush_from", True
    if easing == "rush_from":
        return "rush_into", True
    if easing == "there_and_back":
        return easing, False
    raise NotImplementedError(f"cannot reverse unsupported rate function {easing!r}")


def _default_lag_ratio(animation: object) -> float:
'''),
    (
'''            elif isinstance(animation, (_base.Create, _base.FadeIn)):
                self._bind_introducer_target(animation.target)
            elif isinstance(animation, _base.FadeOut):
''',
'''            elif isinstance(animation, _base.Uncreate):
                _bind_for_animation(self, animation.target, start_time=base_start)
            elif isinstance(animation, (_base.Create, _base.FadeIn)):
                self._bind_introducer_target(animation.target)
            elif isinstance(animation, _base.FadeOut):
'''),
    (
'''                # `noon.Scene` has already been replaced by the compatibility class
                # during install, so use the original captured facade explicitly to
                # avoid recursively re-entering this compatibility scheduler.
                _compat._BaseScene.play(
                    self,
                    lowered,
                    run_time=child_duration,
                    start_time=child_start,
                    easing=child_easing,
                )
''',
'''                if isinstance(lowered, _base.Uncreate):
                    child_easing, track_reverse = _uncreate_track_settings(
                        child_easing, lowered.reverse_rate_function
                    )
                    lowered = type(lowered)(
                        lowered.target,
                        lowered.key,
                        reverse_rate_function=track_reverse,
                        remover=lowered.remover,
                    )

                # `noon.Scene` has already been replaced by the compatibility class
                # during install, so use the original captured facade explicitly to
                # avoid recursively re-entering this compatibility scheduler.
                _compat._BaseScene.play(
                    self,
                    lowered,
                    run_time=child_duration,
                    start_time=child_start,
                    easing=child_easing,
                )
'''),
])

patch('web/python/_manim_compat.py', [
    (
'''        if isinstance(animation, (_base.Create, _base.FadeIn, _base.FadeOut)) and isinstance(
            animation.target, Group
        ):
            leaves = _leaf_mobjects(animation.target)
            return [
                type(animation)(
                    member,
                    None if animation.key is None else f"{animation.key}.{index}",
                )
                for index, member in enumerate(leaves)
            ]
''',
'''        if isinstance(animation, _base.Uncreate) and isinstance(animation.target, Group):
            leaves = _leaf_mobjects(animation.target)
            return [
                type(animation)(
                    member,
                    None if animation.key is None else f"{animation.key}.{index}",
                    reverse_rate_function=animation.reverse_rate_function,
                    remover=animation.remover,
                )
                for index, member in enumerate(leaves)
            ]

        if isinstance(animation, (_base.Create, _base.FadeIn, _base.FadeOut)) and isinstance(
            animation.target, Group
        ):
            leaves = _leaf_mobjects(animation.target)
            return [
                type(animation)(
                    member,
                    None if animation.key is None else f"{animation.key}.{index}",
                )
                for index, member in enumerate(leaves)
            ]
'''),
])

patch('scripts/manim-compat-smoke.mjs', [
    (
'''const rateFunctionSource = `
from noon import *
''',
'''const uncreateSource = `
from noon import *

class UncreateLifecycle(Scene):
    def construct(self):
        first = Square(side_length=0.6, color=BLUE)
        self.add(first)
        self.play(Uncreate(first), run_time=2.0, rate_func=rush_into)
        assert first not in self.mobjects

        kept = Circle(radius=0.25, color=PINK)
        self.add(kept)
        self.play(Uncreate(kept, remover=False), run_time=1.0)
        assert kept in self.mobjects

        forward = Square(side_length=0.4, color=GREEN)
        self.add(forward)
        self.play(Uncreate(forward, reverse_rate_function=False), run_time=1.0)
        assert forward not in self.mobjects
`;

const rateFunctionSource = `
from noon import *
'''),
    (
'''  const phaseB = await page.evaluate(
''',
'''  const uncreate = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    uncreateSource,
  );
  assert.equal(uncreate.kind, "scene_document");
  const uncreateReveals = uncreate.document.tracks.filter((track) => track.property === "reveal");
  assert.equal(uncreateReveals.length, 3);
  assert.deepEqual(uncreateReveals[0].values.scalar, { from: 1, to: 0 });
  assert.equal(uncreateReveals[0].timing.easing, "rush_from");
  assert.deepEqual(uncreateReveals[1].values.scalar, { from: 1, to: 0 });
  assert.equal(uncreateReveals[1].timing.easing, "smooth");
  assert.deepEqual(uncreateReveals[2].values.scalar, { from: 0, to: 1 });
  const uncreateRemovals = uncreate.document.tracks.filter(
    (track) => track.property === "presence" && track.values.bool?.from === true && track.values.bool?.to === false,
  );
  assert.equal(uncreateRemovals.length, 2, "remover=False must preserve scene membership");
  assert.equal(uncreateRemovals[0].timing.start_time, 2.0);
  assert.equal(uncreateRemovals[1].timing.start_time, 4.0);

  const phaseB = await page.evaluate(
'''),
])
