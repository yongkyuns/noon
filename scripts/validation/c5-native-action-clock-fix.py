"""Qualification-only source correction, excluded from the clean product tree."""
from pathlib import Path
p = Path('crates/noon-native/src/property_animation/tests.rs')
s = p.read_text()
assert s.count('baseline: noon_runtime::FrameState') == 1
p.write_text(s.replace('baseline: noon_runtime::FrameState', 'baseline: noon::integration::FrameState'))
p = Path('crates/noon/src/live_session/pointer_actions.rs')
s = p.read_text()
old = 'self.session.advance_property_animations_by(effect_delta)\n                .map_err(LiveSessionError::from)'
new = 'self.session.advance_property_animations_by(effect_delta)\n                .map(|_| ())\n                .map_err(LiveSessionError::from)'
assert s.count(old) == 1
p.write_text(s.replace(old, new))
