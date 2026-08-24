from __future__ import annotations

import sys
from pathlib import Path

from noon import PatchBatch, Scene

WEB_ROOT = Path(__file__).parents[1]

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
    ("Morph stress · 1,000", "python/examples/morph_stress_test.py", {"object_count": 1000}),
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


def emit_scene_documents(output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    for index, (name, relative_path, context) in enumerate(PLAYGROUND_SCENE_EXAMPLES):
        scene = run_scene_example(relative_path, context)
        output_path = output_dir / f"scene-{index:02d}.json"
        output_path.write_text(scene.to_json(), encoding="utf-8")
        print(f"{name}\t{output_path}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: playground_examples.py OUTPUT_DIR")
    emit_scene_documents(Path(sys.argv[1]).resolve())
