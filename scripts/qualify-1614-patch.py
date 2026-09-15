# Temporary exact-source patch runner for the isolated #1614 qualification branch.
from pathlib import Path
import subprocess

root = Path('.')
BASE = 'c32666fbb7e39d10fae2e067f3ad397abe870b7f'
for path in ['crates/noon-web/src/canonical_authoring_scene.rs', 'crates/noon-web/src/semantic_execution_player.rs', 'web/python/_manim_scene.py', 'scripts/manim-compat-smoke.mjs']:
    subprocess.run(['git', 'checkout', BASE, '--', path], check=True)

def replace_once(text, old, new):
    assert text.count(old) == 1, (old[:100], text.count(old))
    return text.replace(old, new, 1)

p = root / 'crates/noon-web/src/canonical_authoring_scene.rs'
s = p.read_text()
s = replace_once(s, 'use std::collections::{BTreeMap, BTreeSet};\n', 'use std::collections::{BTreeMap, BTreeSet};\n\nmod membership_bindings;\n')
start = s.index('        let mut new_bindings = Vec::new();', s.index('    fn edit_membership(&mut self, batch: SceneMembershipBatch)'))
end = s.index('        let mut borrowed = Vec::with_capacity(batch.members.len());', start)
old = s[start:end]
helper = '''use super::*;

impl CanonicalAuthoringScene {
    /// Validate derived wrapper identities without changing semantic membership.
    pub(super) fn prepare_membership_bindings(
        &self,
        batch: &SceneMembershipBatch,
    ) -> Result<Vec<(ObjectId, noon_core::SemanticNodeId)>, AuthoringFailure> {
''' + old + '''        Ok(new_bindings)
    }

    /// Associate handles already published by an engine-owned lifecycle operation.
    ///
    /// This boundary updates only the language wrapper's derived identity maps.
    /// It neither admits scene members nor publishes an execution transaction.
    /// Validate every reservation against the completed coherent runtime before
    /// committing any mapping; work is limited to the affected handles.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(super) fn associate_published_bindings(
        &mut self,
        batch: SceneMembershipBatch,
    ) -> Result<(), AuthoringFailure> {
        if batch.kind != SceneMembershipBatchKind::Add || !batch.members.is_empty() {
            return Err(AuthoringFailure::new(
                "invalid_input",
                "boundary.association_members",
                "published association accepts binding reservations only",
            ));
        }
        let new_bindings = self.prepare_membership_bindings(&batch)?;
        self.active_live_player()?.require_completed_live_segment()?;
        for (_, target) in &batch.bindings {
            if !self.contains_mobject(target)? {
                return Err(AuthoringFailure::new(
                    "invalid_input",
                    "boundary.association_absent",
                    "associated mobject is not in the published Scene",
                ));
            }
            // The shared effective query also rejects stale publications. No
            // handle-only authored fallback or execution rebuild is permitted.
            self.active_live_player()?.live_effective(target)?;
        }
        for (id, node) in new_bindings {
            self.bindings.insert(id, node);
            self.identities.insert(node, id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
'''
s = s[:start] + '        let new_bindings = self.prepare_membership_bindings(&batch)?;\n        let mut seen_nodes = BTreeSet::new();\n        seen_nodes.extend(batch.bindings.iter().map(|(_, handle)| handle.node_id()));\n' + s[end:]
needle = '        /// Return the authoritative direct-root semantic identities in painter order.\n'
s = replace_once(s, needle, '''        /// Associate wrapper identities after an engine-owned membership change.
        #[wasm_bindgen(js_name = associatePublishedBindings)]
        pub fn associate_published_bindings(
            &mut self,
            batch: WasmSceneMembershipBatch,
        ) -> Result<(), JsValue> {
            self.inner
                .associate_published_bindings(batch.inner)
                .map_err(typed_js_error)
        }

''' + needle)
p.write_text(s)
(root / 'crates/noon-web/src/canonical_authoring_scene/membership_bindings.rs').write_text(helper)
p = root / 'crates/noon-web/src/semantic_execution_player.rs'
s = replace_once(p.read_text(), '    fn require_completed_live_segment(&self)', '    pub(crate) fn require_completed_live_segment(&self)')
p.write_text(s)
p = root / 'web/python/_manim_scene.py'
s = p.read_text()
needle = 'def _canonical_scene_mobjects(scene: _base.Scene) -> list[object]:\n'
s = replace_once(s, needle, '''def _associate_published_membership(
    scene: _base.Scene, replacements: list[tuple[object, object]]
) -> None:
    """Refresh affected wrapper identities after Rust completed the replacement.

    The binding batch only associates existing published handles. It must never
    replay Scene.add/replace or decide membership from Python's animation intent.
    """
    context = _context(scene)
    batch = engine_call(context.beginMembershipBatch, "add", operation="Scene.membership")
    next_object_id = scene._next_object_id
    reservations = []
    binding_keys = set()
    for _, target in replacements:
        next_object_id, appended = _membership_leaf_bindings(
            scene, batch, target, next_object_id=next_object_id,
            key=None, binding_keys=binding_keys,
        )
        reservations.extend(appended)
    engine_call(context.associatePublishedBindings, batch, operation="Scene.membership")
    for member, reservation, handle in reservations:
        _commit_typed_binding(member, scene, reservation, handle)
    for _, target in replacements:
        _register_membership_wrappers(scene, target)
    _sync_membership_wrapper_attachments(
        scene, "remove", tuple(source for source, _ in replacements)
    )


''' + needle)
s = replace_once(s, '    tracker_associations: list[_reactive.ValueTracker] = []\n    next_object_id', '    tracker_associations: list[_reactive.ValueTracker] = []\n    completed_memberships: list[tuple[object, object]] = []\n    next_object_id')
s = replace_once(s, '''                float(child.path_arc),
            )
            return
        if isinstance(animation, _composition.Add):''', '''                float(child.path_arc),
            )
            if type(leaf) is _animate.TransformMatchingShapes:
                completed_memberships.append((source, target))
            return
        if isinstance(animation, _composition.Add):''')
s = replace_once(s, '''        tracker_associations,
    ) if supported else False''', '''        tracker_associations,
        completed_memberships,
    ) if supported else False''')
s = replace_once(s, '''    tracker_associations: list[_reactive.ValueTracker],
) -> _base.Scene''', '''    tracker_associations: list[_reactive.ValueTracker],
    completed_memberships: list[tuple[object, object]],
) -> _base.Scene''')
s = replace_once(s, '''    def completed() -> None:
        for target in removals:''', '''    def completed() -> None:
        if completed_memberships:
            _associate_published_membership(self, completed_memberships)
        for target in removals:''')
p.write_text(s)
p = root / 'scripts/manim-compat-smoke.mjs'
s = p.read_text()
source = '''const matchingCompletionSource = `
from noon import *

class MatchingCompletion(Scene):
    def construct(self):
        leaf = VMobject().set_points_as_corners(
            [(-1, -1, 0), (1, -0.5, 0), (-0.25, 1, 0), (-1, -1, 0)]
        ).set_fill(BLUE, opacity=0.9).set_stroke(opacity=0)
        source = VGroup(leaf)
        self.add(source)
        target_leaf = leaf.copy().shift(RIGHT * 4)
        target = VGroup(target_leaf)
        original_source_id = leaf.id
        original_target_handle = target_leaf._semantic_handle

        self.play(TransformMatchingShapes(source, target, run_time=0.2, rate_func=linear))
        assert self.mobjects == [target]
        assert target[0] is target_leaf
        assert target_leaf._semantic_handle is original_target_handle
        assert target_leaf._scene is self
        assert leaf._scene is None
        assert abs(target_leaf.get_center().x - 4) < 1e-6
        self.play(Indicate(target, run_time=0.2))
        self.play(target_leaf.animate.shift(UP), run_time=0.2, rate_func=linear)
        assert abs(target_leaf.get_center().x - 4) < 1e-6
        assert abs(target_leaf.get_center().y - 1) < 1e-6

        # Re-add the removed source only AFTER proving the replacement target's
        # independent lifecycle. This is not a target-association workaround.
        self.add(source)
        assert self.mobjects == [target, source]
        assert source[0] is leaf and leaf.id == original_source_id
        self.play(Indicate(source, run_time=0.2))
        self.remove(source)
        self.wait(0.05)
        assert self.mobjects == [target]
`;

'''
s = replace_once(s, 'const foundationSource = `\n', source + 'const foundationSource = `\n')
assertions = '''  const matchingCompletion = await page.evaluate(
    pythonSource => window.noonManimCompat.runLive(pythonSource), matchingCompletionSource,
  );
  assert.ok(Math.abs(matchingCompletion.duration - 0.85) < 1e-9);
  assert.equal(matchingCompletion.metrics.objectCount, 1, "matching retains the original target only");
  assert.ok(matchingCompletion.metrics.presentedFrames > 0, "Python matching lifecycle must present");
  assert.ok(Math.abs(matchingCompletion.frame.objects[0].center[0] - 4) < 1e-6);
  assert.ok(Math.abs(matchingCompletion.frame.objects[0].center[1] - 1) < 1e-6);

'''
s = replace_once(s, '  const groupFades = await page.evaluate(\n', assertions + '  const groupFades = await page.evaluate(\n')
p.write_text(s)
print('Applied completion-association candidate on', BASE)
