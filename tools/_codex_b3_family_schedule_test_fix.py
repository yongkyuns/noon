from pathlib import Path

path = Path("tools/_codex_b3_family_schedule_patch.py")
text = path.read_text()
replacements = [
    ("transaction.prepare(&store).unwrap()", "transaction.prepare(&mut store).unwrap()"),
    (".insert_semantic_wait_animation(AnimationOptions::new().run_time(1.0))", ".insert_semantic_wait_animation(1.0)"),
    (""".insert_semantic_animation_composition(
            noon_core::SemanticAnimationCompositionKind::Sequence,
            vec![transform, wait],
            AnimationOptions::new().rate_func(RateFunction::Linear),
        )""", """.insert_semantic_sequence_animation(
            &[transform, wait],
            AnimationOptions::new().rate_func(RateFunction::Linear),
        )"""),
]
for old, new in replacements:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"expected one test-harness match, found {count}: {old!r}")
    text = text.replace(old, new, 1)
path.write_text(text)
