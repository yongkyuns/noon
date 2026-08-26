from pathlib import Path

path = Path("web/python/_manim_updaters.py")
text = path.read_text()

old = '''def _registrations(mobject: _base.Mobject) -> list[_UpdaterRegistration]:
    value = getattr(mobject, "_noon_updater_registrations", None)
    if value is None:
        value = []
        setattr(mobject, "_noon_updater_registrations", value)
    return value


def _scene_time'''
new = '''def _registrations(mobject: _base.Mobject) -> list[_UpdaterRegistration]:
    """Active updater occurrences, kept index-aligned with ``_updaters``."""
    value = getattr(mobject, "_noon_updater_registrations", None)
    if value is None:
        value = []
        setattr(mobject, "_noon_updater_registrations", value)
    return value


def _registration_history(mobject: _base.Mobject) -> list[_UpdaterRegistration]:
    """All authored updater intervals, including registrations later removed."""
    value = getattr(mobject, "_noon_updater_registration_history", None)
    if value is None:
        value = []
        setattr(mobject, "_noon_updater_registration_history", value)
    return value


def _scene_time'''
if old not in text:
    raise SystemExit("missing registration helper anchor")
text = text.replace(old, new, 1)

old = '''    if index is None:
        callbacks.append(update_function)
        registrations.append(registration)
    else:
        if isinstance(index, bool) or not isinstance(index, int):
            raise TypeError("updater index must be an integer")
        callbacks.insert(index, update_function)
        registrations.insert(index, registration)
    _track(self)
'''
new = '''    if index is None:
        callbacks.append(update_function)
        registrations.append(registration)
    else:
        if isinstance(index, bool) or not isinstance(index, int):
            raise TypeError("updater index must be an integer")
        callbacks.insert(index, update_function)
        registrations.insert(index, registration)
    _registration_history(self).append(registration)
    _track(self)
'''
if old not in text:
    raise SystemExit("missing add history anchor")
text = text.replace(old, new, 1)

old = '''    for registration in _registrations(self):
        registration.active_through = end_time
    _updaters(self).clear()
    _registrations(self).clear()
'''
new = '''    for registration in _registrations(self):
        registration.active_through = end_time
    _updaters(self).clear()
    _registrations(self).clear()
'''
# Deliberately unchanged structurally: active occurrences clear, history remains.
if old not in text:
    raise SystemExit("missing clear updater anchor")

old = '''        history.extend(_registrations(mobject))
'''
new = '''        history.extend(_registration_history(mobject))
'''
if old not in text:
    raise SystemExit("missing register history anchor")
text = text.replace(old, new, 1)

path.write_text(text)
