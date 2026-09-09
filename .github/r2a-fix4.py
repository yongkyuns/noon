"""Review corrections: no new validation, only fixture API use and identity forwards."""
from pathlib import Path
import re

# These handles already returned AuthoringError in the landed #1286 foundation.
# Keeping From<AuthoringError> for AuthoringError adds no information and trips
# the repository's strict useless-conversion lint.
paths = [
    'family_authoring.rs', 'family_style.rs', 'family_copy.rs',
    'semantic_mobject.rs', 'family_arrangement.rs', 'family_layout.rs',
    'semantic_mobject/style.rs', 'semantic_mobject/manim_geometry.rs',
    'semantic_mobject/layout.rs',
]
for name in paths:
    path = Path('crates/noon/src') / name
    source = path.read_text()
    source, count = re.subn(
        r'\.validate\(\)\s*\.map_err\(AuthoringError::from\)',
        '.validate()', source,
    )
    assert count > 0, name
    path.write_text(source)

# The independent test exercises Scene::path, whose public API deliberately
# takes a style; do not broaden production constructors to fit the fixture.
path = Path('../tooling/.github/r2a-tests/object_authoring_errors.rs')
source = path.read_text()
for before, after in [
    ('scene.path(invalid.clone())', 'scene.path(invalid.clone(), noon::SemanticStyle::default())'),
    ('scene.path(path)?', 'scene.path(path, noon::SemanticStyle::default())?'),
]:
    assert source.count(before) == 1, before
    source = source.replace(before, after)
path.write_text(source)
