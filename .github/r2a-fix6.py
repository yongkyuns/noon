"""Forward typed object errors only at the existing canonical language sink."""
from pathlib import Path
path=Path('crates/noon-web/src/canonical_authoring_scene.rs')
s=path.read_text()
def change(a,b,n=1):
    global s
    assert s.count(a)==n,(a,s.count(a))
    s=s.replace(a,b)
change('noon::MobjectFamily::create(store, &self.family_members()?)', 'noon::MobjectFamily::create(store, &self.family_members()?).map_err(|error| error.to_string())')
for call in ['family.add_many(&members)','family.remove_many(&members)']:
    change(call+',',call+'.map_err(|error| error.to_string()),')
change('fn mobject_observation<T>(', 'fn mobject_observation<T, E: std::fmt::Display>(')
change('authored: impl FnOnce(&noon::Mobject) -> Result<T, String>,', 'authored: impl FnOnce(&noon::Mobject) -> Result<T, E>,')
change('        authored(handle)\n', '        authored(handle).map_err(|error| error.to_string())\n')
change('fn authored_mobject_layout(handle: &noon::Mobject) -> Result<(f64, f64, f64, f64), String>', 'fn authored_mobject_layout(handle: &noon::Mobject) -> Result<(f64, f64, f64, f64), noon::AuthoringError>')
for call in ['handle.fill_opacity()', 'handle.stroke_opacity()']:
    change(call+'?;',call+'.map_err(|error| error.to_string())?;')
change('.layout_bounds()?\n            .ok_or', '.layout_bounds().map_err(|error| error.to_string())?\n            .ok_or')
change('noon::ManimGeometryOptions::underline(bounds, buff)\n', 'noon::ManimGeometryOptions::underline(bounds, buff).map_err(|error| error.to_string())\n')
for call in ['value_tracker(initial)','associate_value_tracker(tracker)',
    'pointer_position_signal()', 'viewport_size_signal()', 'wheel_delta_signal()',
    'key_state_signal(code, initial)', 'control_signal(name, initial)',
    'pointer_down_events(button)', 'wheel_events()', 'control_commit_events(name)',
    'bind_native_translation(object, signal)', 'bind_rotation(object, signal)',
    'bind_opacity(object, signal)', 'bind_presence(object, signal)',
    'position_from_tracker(tracker, direction, offset)', 'bind_position(object, position)',
    'value_tracker_value(tracker)', 'set_value(tracker, value)']:
    change('self.scene.'+call, 'self.scene.'+call+'.map_err(|error| error.to_string())')
change('PlayerOwnership::Unstarted => source.target_editor(),', 'PlayerOwnership::Unstarted => source.target_editor().map_err(|error| error.to_string()),')
path.write_text(s)
