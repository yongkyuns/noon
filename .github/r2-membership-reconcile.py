"""One-shot R2 reconciliation tooling; never included in the production PR."""
from pathlib import Path
import re
import subprocess

BASE = '7e3ee4feab9aa281efd7f256e292cffcc18265fa'
TYPED = '6e97a36ac3834f1f0aac280e788b18055ec87d67'
def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()
assert git('rev-parse', 'HEAD') == BASE
assert not git('status', '--porcelain')
common = git('merge-base', BASE, TYPED)
assert common == '1641398818b6817678ae178faf2887e4ac1e730f'
patch = subprocess.check_output(['git', 'diff', '--binary', '--full-index', common, TYPED], text=True)
patch = patch.replace('crates/noon-core/src/reactive/semantic_scene_operations.rs',
                      'crates/noon-core/src/semantic_store/semantic_scene_operations.rs')
result = subprocess.run(['git', 'apply', '--3way'], input=patch, text=True)
assert result.returncode == 1
expected = {'.github/ci/README.md', 'fixtures/provider-consumer/README.md',
            'crates/noon/src/family_copy.rs', 'crates/noon/src/lib.rs',
            'crates/noon/src/scene_membership.rs'}
assert set(git('diff', '--name-only', '--diff-filter=U').splitlines()) == expected
for rel in sorted(expected):
    p = Path(rel)
    def resolve(match):
        ours, theirs = match.groups()
        if rel.endswith('README.md'):
            return ours + '\n' + theirs
        if rel.endswith('/lib.rs'):
            return ours.replace('AnimationOptions, Color', 'AnimationOptions, AuthoringError, Color')
        return theirs.replace('.store()', '.integration_store()')
    text, count = re.subn(r'<<<<<<< ours\n(.*?)=======\n(.*?)>>>>>>> theirs\n',
                          resolve, p.read_text(), flags=re.S)
    assert count
    p.write_text(text)

p = Path('crates/noon/src/lib.rs')
s = p.read_text().replace('mod authoring_error;\npub use authoring_error::AuthoringError;', 'mod authoring_error;')
s = s.replace('pub use animation_authoring::DeclaredAnimation;',
              'pub use animation_authoring::DeclaredAnimation;\npub use authoring_error::AuthoringError;')
p.write_text(s)
p = Path('crates/noon/src/scene_membership.rs')
p.write_text(p.read_text().replace('.store()', '.integration_store()'))
p = Path('fixtures/provider-consumer/README.md')
p.write_text(p.read_text().replace(
    'strings, and public export/raw-store narrowing remains under #958. Ordinary',
    'strings. Public export/raw-store narrowing is retained from #1290. Ordinary'))

p = Path('fixtures/provider-consumer/tests/authoring_errors.rs')
s = p.read_text().replace('.store()', '.integration_store()').replace('Scene::with_store(', 'Scene::with_integration_store(')
a = s.index('use noon::{')
b = s.index('\n};', a) + 3
s = s[:a] + '''use noon::integration::{
    CallbackAdvance, FrameState, HostCallbackId, SemanticMutationTransaction,
    SemanticSceneOperationError, SemanticStoreError,
};
use noon::{
    AnimationOptions, AuthoringError, ExecutionSession, ExecutionSessionPublicationError,
    LiveSessionError, MobjectFamilyMember, RateFunction, Scene, SceneRevision, SemanticNodeId, Vec2,
};''' + s[b:]
s = s.replace('frame: noon::FrameState,', 'frame: FrameState,')
s = s.replace('    use noon::{SemanticMutationTransactionError, SemanticObjectProperty, SemanticVec3};',
              '    use noon::integration::SemanticMutationTransactionError;\n    use noon::{SemanticObjectProperty, SemanticVec3};')
s = s.replace('    let error = scene\n        .add_many(&[',
              '    let error: noon::prelude::AuthoringError = scene\n        .add_many(&[', 1)
p.write_text(s)

# Adapt only newly added callers whose surrounding APIs still return String.
for relative, old in [('family_arrangement.rs', 'family.validate()?;'),
                      ('family_style.rs', 'self.validate()?;')]:
    p = Path('crates/noon/src') / relative
    s = p.read_text()
    assert old in s
    p.write_text(s.replace(old, old[:-2] + '.map_err(|error| error.to_string())?;'))
for relative in ['family_affine.rs', 'renderer_recovery.rs', 'renderer_fixtures.rs']:
    p = Path('crates/noon/src/example_scenes') / relative
    s, count = re.subn(r'(scene\s*\.add(?:_many)?\([\s\S]*?\))\?;',
                       r'\1.map_err(|error| error.to_string())?;', p.read_text())
    assert count
    p.write_text(s)
subprocess.run(['git', 'add', '.'], check=True)
subprocess.run(['git', 'diff', '--cached', '--check'], check=True)
assert not Path('crates/noon-core/src/reactive/semantic_scene_operations.rs').exists()
assert 'pub use noon_core::*' not in Path('crates/noon/src/lib.rs').read_text()
assert 'pub fn store(' not in Path('crates/noon/src/scene.rs').read_text()
# Preserve existing provider/public-facade qualification exactly.
for relative in ['fixtures/provider-consumer/tests/public_facade.rs',
                 '.github/workflows/provider-features.yml',
                 'crates/noon/src/integration.rs', 'docs/architecture.md']:
    assert not git('diff', '--cached', '--', relative), relative
print('Reconciled existing typed-membership delta; no extra authority or legacy API restored.')
