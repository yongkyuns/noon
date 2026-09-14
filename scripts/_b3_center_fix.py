from pathlib import Path
R=Path(__file__).resolve().parents[1]
def one(path,a,b):
 p=R/path; s=p.read_text(); n=s.count(a)
 if n!=1: raise RuntimeError(f'{path}: {n} anchors')
 p.write_text(s.replace(a,b,1))

one('crates/noon/src/execution_session.rs', '''                let target_state = if *interpolation
                    == noon_core::SemanticTransformInterpolation::CenterTranslation
                {
                    *target_state
                } else {
                    self.stage_animation_target_state(store, declaration, *target_state)?
                };''', '''                let target_state: SemanticTransactionNodeRef = if *interpolation
                    == noon_core::SemanticTransformInterpolation::CenterTranslation
                {
                    (*target_state).into()
                } else {
                    self.stage_animation_target_state(store, declaration, *target_state)?.into()
                };''')

anchor='''            AnimationCompositionRequest::Indicate {
                target,
                indication,
                options,
            } => {'''
arm='''            AnimationCompositionRequest::CyclicReplace { family, options } => {
                self.require_family(family)?;
                Request::CyclicReplace {
                    family: family.node_id(),
                    options: *options,
                }
            }
'''
one('crates/noon/src/live_session.rs', anchor, arm+anchor)
print('fixed center translation compile errors')
