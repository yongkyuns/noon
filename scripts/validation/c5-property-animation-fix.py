"""Staging only: pinned isolation fixes plus the two-target retirement regression."""
from pathlib import Path
import subprocess

prior = subprocess.check_output([
    'git', 'show',
    '5605d0c4cf9cdc7ef4bfb7da419aaf9b340564f0:scripts/validation/c5-property-animation-fix.py',
])
assert subprocess.check_output(['git', 'hash-object', '--stdin'], input=prior).decode().strip() == '9b240ee1d3c544e7bdd4bcc85abdb87a7fa83136'
exec(compile(prior, 'pinned-c5-property-animation-isolation-fix.py', 'exec'))

# A retargeted track changes both its previous row and its destination row.
p = 'crates/noon-runtime/src/property_animation.rs'
replace(p, '''        let Some(index) = object.and_then(|id| self.frame_index_for_object(id)) else { return; };
        let keys: Vec<_>''', '''        if let ExecutionPatch::ReplaceTrack(track) = patch {
            if let Some(previous) = self.compiled.track_object(track.id) {
                if Some(previous) != object {
                    self.retire_property_animations_for_object(previous);
                }
            }
        }
        if let Some(object) = object {
            self.retire_property_animations_for_object(object);
        }
    }

    fn retire_property_animations_for_object(&mut self, object: ObjectId) {
        let Some(index) = self.frame_index_for_object(object) else { return; };
        let keys: Vec<_>''')
p = Path('crates/noon-runtime/src/property_animation/tests.rs')
p.write_text(p.read_text() + '''
#[test]
fn track_retarget_retires_both_affected_objects_but_not_an_unrelated_effect() {
    let mut movement = moving(1, Property::Position,
        TrackValues::Vec2 { from: Vec2::ZERO, to: Vec2::new(8.0, 0.0) });
    let mut runtime = instance(3, &[movement.clone()]);
    let one = runtime.start_restoring_property_animation(&[scale(1)]).unwrap();
    let two = runtime.start_restoring_property_animation(&[scale(2)]).unwrap();
    let three = runtime.start_restoring_property_animation(&[scale(3)]).unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    movement.object = ObjectId::new(2);
    runtime.apply_execution_patch(&ExecutionPatch::ReplaceTrack(movement)).unwrap();
    assert!(runtime.property_animation_elapsed(one).is_none());
    assert!(runtime.property_animation_elapsed(two).is_none());
    assert_eq!(runtime.property_animation_elapsed(three), Some(0.5));
    assert_eq!(runtime.frame().objects[0].transform.scale, Vec2::ONE);
    assert_eq!(runtime.frame().objects[1].transform.scale, Vec2::ONE);
    assert_eq!(runtime.frame().objects[2].transform.scale, Vec2::new(1.2, 1.2));
}
''')
print('Retargeting retires effects on both affected targets without a global cancellation')
