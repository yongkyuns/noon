from pathlib import Path


def replace(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    if old not in text:
        raise SystemExit(f"missing patch anchor in {path}: {old[:120]!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


# Shared host callback slots own activation timing. `active_after` is exclusive and
# `active_through` is inclusive so an updater removed at the end of a Manim wait
# receives that wait's final dt, while an updater added at the same authored time
# starts on the following frame.
replace(
    "crates/noon-core/src/host_callbacks.rs",
    '''#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCallbackSlot {
    pub id: HostCallbackId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectId>,
}
''',
    '''#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostCallbackSlot {
    pub id: HostCallbackId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectId>,
    /// Callback is inactive at this exact time and becomes active immediately after it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_after: Option<f64>,
    /// Callback remains active through this exact time, then becomes inactive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_through: Option<f64>,
}

impl HostCallbackSlot {
    pub fn is_active_at(&self, time: f64) -> bool {
        time.is_finite()
            && self.active_after.is_none_or(|start| time > start)
            && self.active_through.is_none_or(|end| time <= end)
    }

    fn validate_schedule(&self) -> Result<(), HostCallbackRegistryError> {
        if self
            .active_after
            .is_some_and(|value| !value.is_finite() || value < 0.0)
            || self
                .active_through
                .is_some_and(|value| !value.is_finite() || value < 0.0)
            || matches!((self.active_after, self.active_through), (Some(start), Some(end)) if end < start)
        {
            return Err(HostCallbackRegistryError::InvalidActivationWindow(self.id));
        }
        Ok(())
    }
}
''',
)
replace(
    "crates/noon-core/src/host_callbacks.rs",
    "#[derive(Clone, Debug, Default, PartialEq, Eq)]\npub struct HostCallbackRegistry {",
    "#[derive(Clone, Debug, Default, PartialEq)]\npub struct HostCallbackRegistry {",
)
replace(
    "crates/noon-core/src/host_callbacks.rs",
    '''        for slot in &slots {
            if !ids.insert(slot.id) {
                return Err(HostCallbackRegistryError::DuplicateCallback(slot.id));
            }
''',
    '''        for slot in &slots {
            slot.validate_schedule()?;
            if !ids.insert(slot.id) {
                return Err(HostCallbackRegistryError::DuplicateCallback(slot.id));
            }
''',
)
replace(
    "crates/noon-core/src/host_callbacks.rs",
    '''        self.slots.push(HostCallbackSlot {
            id,
            objects: unique,
        });
''',
    '''        self.slots.push(HostCallbackSlot {
            id,
            objects: unique,
            active_after: None,
            active_through: None,
        });
''',
)
replace(
    "crates/noon-core/src/host_callbacks.rs",
    '''pub enum HostCallbackRegistryError {
    DuplicateCallback(HostCallbackId),
    CallbackIdExhausted,
}
''',
    '''pub enum HostCallbackRegistryError {
    DuplicateCallback(HostCallbackId),
    InvalidActivationWindow(HostCallbackId),
    CallbackIdExhausted,
}
''',
)
replace(
    "crates/noon-core/src/host_callbacks.rs",
    '''            Self::CallbackIdExhausted => {
                formatter.write_str("Noon host callback ID space exhausted")
            }
''',
    '''            Self::InvalidActivationWindow(id) => write!(
                formatter,
                "host callback {} has an invalid activation window",
                id.get()
            ),
            Self::CallbackIdExhausted => {
                formatter.write_str("Noon host callback ID space exhausted")
            }
''',
)
replace(
    "crates/noon-core/src/host_callbacks.rs",
    '''        let slot = HostCallbackSlot {
            id: HostCallbackId::new(3),
            objects: vec![ObjectId::new(1)],
        };
''',
    '''        let slot = HostCallbackSlot {
            id: HostCallbackId::new(3),
            objects: vec![ObjectId::new(1)],
            active_after: None,
            active_through: None,
        };
''',
)
replace(
    "crates/noon-core/src/host_callbacks.rs",
    '''    #[test]
    fn transported_slots_reject_duplicate_callback_ids() {
''',
    '''    #[test]
    fn scheduled_slot_uses_exclusive_start_and_inclusive_end() {
        let slot = HostCallbackSlot {
            id: HostCallbackId::new(2),
            objects: vec![],
            active_after: Some(1.0),
            active_through: Some(2.0),
        };
        HostCallbackRegistry::from_slots(vec![slot.clone()]).unwrap();
        assert!(!slot.is_active_at(1.0));
        assert!(slot.is_active_at(1.0 + f64::EPSILON));
        assert!(slot.is_active_at(2.0));
        assert!(!slot.is_active_at(2.0 + f64::EPSILON));
    }

    #[test]
    fn transported_slots_reject_invalid_activation_windows() {
        let slot = HostCallbackSlot {
            id: HostCallbackId::new(5),
            objects: vec![],
            active_after: Some(3.0),
            active_through: Some(2.0),
        };
        assert_eq!(
            HostCallbackRegistry::from_slots(vec![slot]),
            Err(HostCallbackRegistryError::InvalidActivationWindow(
                HostCallbackId::new(5)
            ))
        );
    }

    #[test]
    fn transported_slots_reject_duplicate_callback_ids() {
''',
)

# Runtime filters invocation slots by the shared activation schedule while keeping the
# coherent object snapshot table stable across callback phases.
replace(
    "crates/noon-runtime/src/reactive/host_callbacks.rs",
    "    invocations: Vec<HostCallbackInvocation>,\n",
    "    scheduled_invocations: Vec<(HostCallbackInvocation, Option<f64>, Option<f64>)>,\n",
)
replace(
    "crates/noon-runtime/src/reactive/host_callbacks.rs",
    "        let mut invocations = Vec::with_capacity(registry.slots().len());\n",
    "        let mut scheduled_invocations = Vec::with_capacity(registry.slots().len());\n",
)
replace(
    "crates/noon-runtime/src/reactive/host_callbacks.rs",
    '''            invocations.push(HostCallbackInvocation {
                callback: slot.id,
                object_indices,
            });
''',
    '''            scheduled_invocations.push((
                HostCallbackInvocation {
                    callback: slot.id,
                    object_indices,
                },
                slot.active_after,
                slot.active_through,
            ));
''',
)
replace(
    "crates/noon-runtime/src/reactive/host_callbacks.rs",
    '''            watched_dense_indices,
            invocations,
            last_callback_time,
''',
    '''            watched_dense_indices,
            scheduled_invocations,
            last_callback_time,
''',
)
replace(
    "crates/noon-runtime/src/reactive/host_callbacks.rs",
    '''        HostCallbackFrame {
            time: frame.time,
            delta_time,
            objects,
            invocations: self.invocations.clone(),
        }
''',
    '''        let invocations = self
            .scheduled_invocations
            .iter()
            .filter(|(_, active_after, active_through)| {
                active_after.is_none_or(|start| frame.time > start)
                    && active_through.is_none_or(|end| frame.time <= end)
            })
            .map(|(invocation, _, _)| invocation.clone())
            .collect();
        HostCallbackFrame {
            time: frame.time,
            delta_time,
            objects,
            invocations,
        }
''',
)
# Add a runtime schedule regression without disturbing existing always-on tests.
replace(
    "crates/noon-runtime/src/reactive/host_callbacks.rs",
    '''    #[test]
    fn callback_delta_time_tracks_the_runtime_playhead_coherently() {
''',
    '''    #[test]
    fn callback_invocations_follow_activation_windows() {
        let (scene, objects) = plain_scene(1);
        let registry = HostCallbackRegistry::from_slots(vec![
            noon_core::HostCallbackSlot {
                id: HostCallbackId::new(0),
                objects: vec![objects[0]],
                active_after: Some(0.0),
                active_through: Some(1.0),
            },
            noon_core::HostCallbackSlot {
                id: HostCallbackId::new(1),
                objects: vec![objects[0]],
                active_after: Some(1.0),
                active_through: Some(2.0),
            },
        ])
        .unwrap();
        let mut driven = HostDrivenScene::new(scene, &registry).unwrap();

        assert!(driven.callback_frame().invocations.is_empty());
        driven.advance_to(0.5).unwrap();
        assert_eq!(driven.callback_frame().invocations[0].callback, HostCallbackId::new(0));
        driven.advance_to(1.0).unwrap();
        assert_eq!(driven.callback_frame().invocations[0].callback, HostCallbackId::new(0));
        driven.advance_to(1.5).unwrap();
        assert_eq!(driven.callback_frame().invocations[0].callback, HostCallbackId::new(1));
        driven.advance_to(2.0).unwrap();
        assert_eq!(driven.callback_frame().invocations[0].callback, HostCallbackId::new(1));
        driven.advance_to(2.1).unwrap();
        assert!(driven.callback_frame().invocations.is_empty());
    }

    #[test]
    fn callback_delta_time_tracks_the_runtime_playhead_coherently() {
''',
)

# Browser HostScenePlayer decodes the schedule fields into the shared registry.
replace(
    "crates/noon-web/src/host_player.rs",
    '''        decoded.push(HostCallbackSlot {
            id: HostCallbackId::new(id),
            objects,
        });
''',
    '''        let active_after = optional_callback_time(record, "active_after")?;
        let active_through = optional_callback_time(record, "active_through")?;
        decoded.push(HostCallbackSlot {
            id: HostCallbackId::new(id),
            objects,
            active_after,
            active_through,
        });
''',
)
replace(
    "crates/noon-web/src/host_player.rs",
    '''    Ok(HostCallbackRegistry::from_slots(decoded)?)
}

#[cfg(target_arch = "wasm32")]
''',
    '''    Ok(HostCallbackRegistry::from_slots(decoded)?)
}

fn optional_callback_time(
    record: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<f64>, HostPlayerError> {
    let Some(value) = record.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    value.as_f64().map(Some).ok_or_else(|| {
        HostPlayerError::CallbackJson(format!("callback slot {field} must be a number or null"))
    })
}

#[cfg(target_arch = "wasm32")]
''',
)
replace(
    "crates/noon-web/src/host_player.rs",
    '''    #[test]
    fn callback_frame_reports_timeline_evaluated_signal_values() {
''',
    '''    #[test]
    fn callback_frame_filters_scheduled_slots() {
        let mut definition = SceneDefinition::new();
        let object = definition.add(GeometryRef::circle(1.0));
        let scene_json = encode_scene(&definition).unwrap();
        let slots = format!(
            r#"[{{\"id\":0,\"objects\":[{}],\"active_after\":0.0,\"active_through\":1.0}},{{\"id\":1,\"objects\":[{}],\"active_after\":1.0,\"active_through\":2.0}}]"#,
            object.get(), object.get()
        );
        let mut player = HostScenePlayer::from_json(&scene_json, &slots).unwrap();
        player.advance_to(0.5).unwrap();
        let frame: Value = serde_json::from_str(&player.callback_frame_json().unwrap()).unwrap();
        assert_eq!(frame["invocations"][0]["callback"], 0);
        player.advance_to(1.5).unwrap();
        let frame: Value = serde_json::from_str(&player.callback_frame_json().unwrap()).unwrap();
        assert_eq!(frame["invocations"][0]["callback"], 1);
    }

    #[test]
    fn callback_frame_reports_timeline_evaluated_signal_values() {
''',
)

# Worker execution uses the exact same exclusive-start/inclusive-end contract.
replace(
    "web/execution-engine-worker.js",
    '''  for (const slot of hostCallbacks.slots) {
    const indices = [];
''',
    '''  for (const slot of hostCallbacks.slots) {
    if (!callbackSlotActiveAt(slot, time)) {
      continue;
    }
    const indices = [];
''',
)
replace(
    "web/execution-engine-worker.js",
    '''function validateCallbackConfig(callbacks) {
''',
    '''function callbackSlotActiveAt(slot, time) {
  if (slot.active_after !== undefined && slot.active_after !== null && !(time > slot.active_after)) {
    return false;
  }
  if (slot.active_through !== undefined && slot.active_through !== null && time > slot.active_through) {
    return false;
  }
  return true;
}

function validateCallbackConfig(callbacks) {
''',
)
replace(
    "web/execution-engine-worker.js",
    '''    for (const object of slot.objects) {
      if (!Number.isSafeInteger(object) || object < 0) {
        throw new Error("host callback slot contains an invalid object ID");
      }
    }
''',
    '''    for (const object of slot.objects) {
      if (!Number.isSafeInteger(object) || object < 0) {
        throw new Error("host callback slot contains an invalid object ID");
      }
    }
    for (const field of ["active_after", "active_through"]) {
      const value = slot[field];
      if (value !== undefined && value !== null && (!Number.isFinite(value) || value < 0)) {
        throw new Error(`host callback slot contains invalid ${field}`);
      }
    }
    if (
      slot.active_after !== undefined && slot.active_after !== null &&
      slot.active_through !== undefined && slot.active_through !== null &&
      slot.active_through < slot.active_after
    ) {
      throw new Error("host callback slot has an invalid activation window");
    }
''',
)

# Python records updater add/remove history during authoring. Runtime slot IDs select
# historical callable registrations; Python no longer infers activation from final state.
replace(
    "web/python/_manim_updaters.py",
    '''def _updaters(mobject: _base.Mobject) -> list[Callable[..., Any]]:
    value = getattr(mobject, "_noon_updaters", None)
    if value is None:
        value = []
        setattr(mobject, "_noon_updaters", value)
    return value


def add_updater(
''',
    '''@dataclass(slots=True)
class _UpdaterRegistration:
    mobject: _base.Mobject
    callback: Callable[..., Any]
    active_after: float | None
    active_through: float | None = None


def _updaters(mobject: _base.Mobject) -> list[Callable[..., Any]]:
    value = getattr(mobject, "_noon_updaters", None)
    if value is None:
        value = []
        setattr(mobject, "_noon_updaters", value)
    return value


def _registrations(mobject: _base.Mobject) -> list[_UpdaterRegistration]:
    value = getattr(mobject, "_noon_updater_registrations", None)
    if value is None:
        value = []
        setattr(mobject, "_noon_updater_registrations", value)
    return value


def _scene_time(mobject: _base.Mobject) -> float | None:
    scene = getattr(mobject, "_scene", None)
    if scene is None:
        return None
    return float(scene.time)


def add_updater(
''',
)
replace(
    "web/python/_manim_updaters.py",
    '''    callbacks = _updaters(self)
    if index is None:
        callbacks.append(update_function)
    else:
        if isinstance(index, bool) or not isinstance(index, int):
            raise TypeError("updater index must be an integer")
        callbacks.insert(index, update_function)
    _track(self)
''',
    '''    callbacks = _updaters(self)
    registrations = _registrations(self)
    registration = _UpdaterRegistration(
        mobject=self,
        callback=update_function,
        active_after=_scene_time(self),
    )
    if index is None:
        callbacks.append(update_function)
        registrations.append(registration)
    else:
        if isinstance(index, bool) or not isinstance(index, int):
            raise TypeError("updater index must be an integer")
        callbacks.insert(index, update_function)
        registrations.insert(index, registration)
    _track(self)
''',
)
replace(
    "web/python/_manim_updaters.py",
    '''    callbacks = _updaters(self)
    for index, callback in enumerate(callbacks):
        if callback is update_function:
            del callbacks[index]
            break
    return self
''',
    '''    callbacks = _updaters(self)
    registrations = _registrations(self)
    for index, callback in enumerate(callbacks):
        if callback is update_function:
            del callbacks[index]
            registration = registrations.pop(index)
            registration.active_through = _scene_time(self)
            break
    return self
''',
)
replace(
    "web/python/_manim_updaters.py",
    '''    del recursive
    _updaters(self).clear()
    return self
''',
    '''    del recursive
    end_time = _scene_time(self)
    for registration in _registrations(self):
        registration.active_through = end_time
    _updaters(self).clear()
    _registrations(self).clear()
    return self
''',
)
replace(
    "web/python/_manim_updaters.py",
    '''@dataclass(slots=True)
class _UpdaterSession:
    scene: _base.Scene
    mobjects: list[_base.Mobject]
''',
    '''@dataclass(slots=True)
class _UpdaterSession:
    scene: _base.Scene
    registrations: dict[int, _UpdaterRegistration]
''',
)
replace(
    "web/python/_manim_updaters.py",
    '''    mobjects = [
        mobject
        for mobject in _TRACKED_MOBJECTS
        if mobject._scene is scene
        and mobject._object is not None
        and bool(_updaters(mobject))
    ]
    mobjects.sort(key=lambda value: value.id)
    if not mobjects:
        return None

    session_id = _NEXT_SESSION_ID
    _NEXT_SESSION_ID += 1
    _SESSIONS[session_id] = _UpdaterSession(scene=scene, mobjects=mobjects)

    # Arbitrary Python closures may read any bound mobject. Observe the complete
    # semantic object table once per callback phase so all such reads are coherent
    # and local inside Pyodide. The Python callback context materializes the
    # corresponding Mobject snapshots lazily, so cost scales with the closure's
    # touched set rather than eagerly deep-copying every scene object.
    object_ids = [int(obj["id"]) for obj in scene._objects]
    return {
        "session_id": session_id,
        "slots": [{"id": 0, "objects": object_ids}],
    }
''',
    '''    history: list[_UpdaterRegistration] = []
    for mobject in _TRACKED_MOBJECTS:
        if mobject._scene is not scene or mobject._object is None:
            continue
        history.extend(_registrations(mobject))
    if not history:
        return None

    # Detached mobjects commonly receive updaters before Scene.add at authored time
    # zero. Resolve that pending start once the object is known to belong to this
    # scene; removals recorded after binding retain their exact scene-time endpoint.
    for registration in history:
        if registration.active_after is None:
            registration.active_after = 0.0

    session_id = _NEXT_SESSION_ID
    _NEXT_SESSION_ID += 1
    registrations = {slot_id: registration for slot_id, registration in enumerate(history)}
    _SESSIONS[session_id] = _UpdaterSession(scene=scene, registrations=registrations)

    # Arbitrary Python closures may read any bound mobject. Every scheduled slot
    # observes the same complete semantic table; the Rust runtime deduplicates that
    # table once per phase and owns which callback slots are active at the frame time.
    object_ids = [int(obj["id"]) for obj in scene._objects]
    slots = []
    for slot_id, registration in registrations.items():
        slot = {
            "id": slot_id,
            "objects": object_ids,
            "active_after": registration.active_after,
        }
        if registration.active_through is not None:
            slot["active_through"] = registration.active_through
        slots.append(slot)
    return {
        "session_id": session_id,
        "slots": slots,
    }
''',
)
replace(
    "web/python/_manim_updaters.py",
    '''    invocations = frame.get("invocations", [])
    if len(invocations) != 1 or int(invocations[0]["callback"]) != 0:
        raise RuntimeError("updater session received an unexpected callback invocation set")

    context = _CallbackContext(session.scene, frame)
''',
    '''    invocations = frame.get("invocations", [])
    context = _CallbackContext(session.scene, frame)
''',
)
replace(
    "web/python/_manim_updaters.py",
    '''    try:
        for mobject in session.mobjects:
            for callback in list(_updaters(mobject)):
                _invoke(callback, mobject, context.delta_time)
''',
    '''    try:
        for invocation in invocations:
            slot_id = int(invocation["callback"])
            try:
                registration = session.registrations[slot_id]
            except KeyError as error:
                raise RuntimeError(
                    f"updater session received unknown callback slot {slot_id}"
                ) from error
            _invoke(registration.callback, registration.mobject, context.delta_time)
''',
)

# Focused CPython lifecycle regression. It stubs the browser handle factory just enough
# to author the literal add/remove/wait sequence and inspect the scheduled slots.
Path("web/python/test_manim_updater_lifecycle.py").write_text(
    '''import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimUpdaterLifecycleTests(unittest.TestCase):
    def test_add_remove_history_becomes_runtime_activation_windows(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = str(python_dir) if not existing else os.pathsep.join((str(python_dir), existing))
        source = textwrap.dedent(
            """
            import json
            import math
            import sys
            import types

            fake_js = types.ModuleType("js")
            fake_js.noonResolveUniformCompositionSchedule = lambda *args: None
            fake_js.noonResolveAnimationOptions = lambda *args: None
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b  # noqa: F401
            import _manim_updaters as updaters
            updaters.install()

            from noon import LEFT, ORIGIN, Line, Scene

            scene = Scene()
            moving = Line(ORIGIN, LEFT)

            def forward(mobject, dt):
                mobject.rotate_about_origin(dt)

            def backward(mobject, dt):
                mobject.rotate_about_origin(-dt)

            moving.add_updater(forward)
            scene.add(moving)
            scene.wait(2)
            moving.remove_updater(forward)
            moving.add_updater(backward)
            scene.wait(2)
            moving.remove_updater(backward)
            scene.wait(0.5)

            assert not moving.has_updaters()
            config = updaters.register_scene(scene)
            assert config is not None
            assert len(config["slots"]) == 2, config
            first, second = config["slots"]
            assert first["active_after"] == 0.0
            assert first["active_through"] == 2.0
            assert second["active_after"] == 2.0
            assert second["active_through"] == 4.0

            session = config["session_id"]
            object_id = moving.id
            base = scene._objects[object_id]
            def frame(time, dt, callback):
                return {
                    "time": time,
                    "delta_time": dt,
                    "objects": [{
                        "object": object_id,
                        "transform": base["transform"],
                        "style": base["style"],
                        "presence": True,
                        "appearance": 1.0,
                        "reveal": 1.0,
                        "morph": 1.0,
                    }],
                    "signals": [],
                    "invocations": [{"callback": callback, "object_indices": [0]}],
                }

            forward_batch = json.loads(updaters.run_callback_phase(session, frame(1.0, 0.25, 0), 0))
            forward_rotation = forward_batch["patches"][0]["set_transform"]["transform"]["rotation"]
            assert abs(forward_rotation - 0.25) < 1e-6, forward_rotation

            backward_batch = json.loads(updaters.run_callback_phase(session, frame(3.0, 0.25, 1), 1))
            backward_rotation = backward_batch["patches"][0]["set_transform"]["transform"]["rotation"]
            assert abs(backward_rotation + 0.25) < 1e-6, backward_rotation
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            completed.returncode,
            0,
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
''',
    encoding="utf-8",
)
