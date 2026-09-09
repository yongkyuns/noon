"""Adapt the exact retained-text example calls reported by the all-feature build."""
from pathlib import Path

calls = {
    'text_family_fade.rs': [
        'left.set_translation(-2.0, 0.5)',
        'right.set_translation(1.0, 0.5)',
        'writing.set_translation(-1.0, -1.0)',
        'scene.family(&[(&left).into(), (&right).into()])',
    ],
    'text_family_reveal.rs': [
        'left.set_translation(-3.0, 0.75)',
        'right.set_translation(0.0, 0.75)',
        'scene.family(&[(&left).into(), (&right).into()])',
        'solo.set_translation(-2.0, -1.25)',
        'scene.square(0.6)',
        'moving.set_translation(1.0, -1.25)',
        'moving.target_editor()',
        'moving_target.shift(2.0, 0.0)',
        'left.target_editor()',
        'left_target.shift(0.0, 1.0)',
    ],
    'text_family_write.rs': [
        'left.set_translation(-3.0, 0.75)',
        'right.set_translation(0.0, 0.75)',
        'scene.family(&[(&left).into(), (&right).into()])',
        'scene.square(0.6)',
        'moving.set_translation(0.0, -1.25)',
        'moving.target_editor()',
        'moving_target.shift(2.0, 0.0)',
        'left.target_editor()',
        'left_target.shift(0.0, 1.0)',
    ],
    'text_write.rs': [
        'moving.set_translation(-2.0, -1.0)',
        'writing.set_translation(-1.0, 1.0)',
        'moving.target_editor()',
        'target.shift(2.0, 0.0)',
    ],
}
for filename, expressions in calls.items():
    path = Path('crates/noon/src/example_scenes') / filename
    source = path.read_text()
    for expression in expressions:
        before = expression + '?'
        assert source.count(before) == 1, (filename, before)
        source = source.replace(before, expression + '.map_err(|error| error.to_string())?')
    path.write_text(source)
