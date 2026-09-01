from __future__ import annotations

import argparse
import sys
from pathlib import Path

from noon import PatchBatch, Scene

WEB_ROOT = Path(__file__).parents[1]
MORPH_STRESS_NAME = "Morph stress · 1,000"

# Ordered from basic authoring toward specialized renderer/performance behavior.
# Each picker entry has one primary teaching purpose and one source file.
PLAYGROUND_SCENE_EXAMPLES = (
    ("Getting started", "python/demo_scene.py", {}),
    ("Analytic Transform", "python/examples/analytic_transform.py", {}),
    ("Lifecycle handoffs", "python/examples/lifecycle_handoffs.py", {}),
    ("Fade & appearance", "python/examples/fade_appearance.py", {}),
    ("Matching shapes", "python/examples/matching_shapes.py", {}),
    ("Create shapes", "python/examples/create_shapes.py", {}),
    ("Path reveal", "python/examples/path_reveal.py", {}),
    ("Filled path Transform", "python/examples/filled_path_transform.py", {}),
    ("Staggered timing", "python/examples/staggered_choreography.py", {}),
    ("Instanced field · 180", "python/examples/instanced_field.py", {}),
    (MORPH_STRESS_NAME, "python/examples/morph_stress_test.py", {"object_count": 1000}),
)

PLAYGROUND_PATCH_EXAMPLES = (
    (
        "Palette swap",
        "python/demo_patch.py",
        {
            "sequence": 0,
            "palette": {
                "circle": [1.0, 0.78, 0.22],
                "rectangle": [0.72, 0.38, 0.96],
                "line": [0.22, 0.88, 0.96],
            },
        },
    ),
    ("Transform remix", "python/examples/transform_patch.py", {"sequence": 0}),
)


def scene_examples(*, morph_stress_count: int | None = None):
    if morph_stress_count is not None and morph_stress_count < 12:
        raise ValueError("morph_stress_count must be at least 12")

    examples = []
    for name, relative_path, context in PLAYGROUND_SCENE_EXAMPLES:
        effective_context = dict(context)
        if name == MORPH_STRESS_NAME and morph_stress_count is not None:
            effective_context["object_count"] = morph_stress_count
        examples.append((name, relative_path, effective_context))
    return tuple(examples)


def _execute_source(relative_path: str, context: dict[str, object]) -> object:
    source_path = WEB_ROOT / relative_path
    namespace = {"context": dict(context)}
    source = source_path.read_text(encoding="utf-8")
    exec(compile(source, str(source_path), "exec"), namespace)
    if "result" not in namespace:
        raise RuntimeError(f"{relative_path} did not assign result")
    return namespace["result"]


def run_scene_example(relative_path: str, context: dict[str, object]) -> Scene:
    result = _execute_source(relative_path, context)
    if not isinstance(result, Scene):
        raise TypeError(f"{relative_path} returned {type(result).__name__}, expected Scene")
    result.to_document()
    return result


def run_patch_example(relative_path: str, context: dict[str, object]) -> PatchBatch:
    result = _execute_source(relative_path, context)
    if not isinstance(result, PatchBatch):
        raise TypeError(
            f"{relative_path} returned {type(result).__name__}, expected PatchBatch"
        )
    result.to_document()
    return result


def emit_scene_documents(output_dir: Path, *, morph_stress_count: int | None = None) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    for index, (name, relative_path, context) in enumerate(
        scene_examples(morph_stress_count=morph_stress_count)
    ):
        scene = run_scene_example(relative_path, context)
        output_path = output_dir / f"scene-{index:02d}.json"
        output_path.write_text(scene.to_json(), encoding="utf-8")
        print(f"{name}\t{output_path}")


def _configure_manifest_stdout() -> None:
    # CLI stdout is a machine-readable UTF-8 manifest. A redirected Windows
    # stream may otherwise inherit a legacy code page and corrupt non-ASCII names.
    reconfigure = getattr(sys.stdout, "reconfigure", None)
    if reconfigure is not None:
        reconfigure(encoding="utf-8")


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("output_dir", type=Path)
    parser.add_argument("--morph-stress-count", type=int)
    return parser.parse_args()


if __name__ == "__main__":
    args = _parse_args()
    _configure_manifest_stdout()
    emit_scene_documents(
        args.output_dir.resolve(), morph_stress_count=args.morph_stress_count
    )
