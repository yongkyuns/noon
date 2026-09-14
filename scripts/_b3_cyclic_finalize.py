from pathlib import Path


def read(path):
    return Path(path).read_text()


def write(path, text):
    Path(path).write_text(text)


def replace_once(path, old, new):
    text = read(path)
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{path}: expected one anchor, found {count}: {old[:120]!r}")
    write(path, text.replace(old, new, 1))


# --- semantic/live request carries ordered real members, never a transport-only family ---
path = "crates/noon/src/execution_session.rs"
replace_once(
    path,
    "    CyclicReplace {\n        family: SemanticNodeId,\n        options: AnimationOptions,\n    },",
    "    CyclicReplace {\n        members: Vec<SemanticNodeId>,\n        options: AnimationOptions,\n    },",
)
old = '''            SemanticCompositionRequest::CyclicReplace { family, options } => {
                let members = store
                    .semantic_family_members_checked(*family)
                    .map_err(|error| {
                        ExecutionSessionAnimationError::InvalidComposition(error.to_string())
                    })?;
                if members.len() < 2
                    || members.iter().any(|member| {
                        !self.reachability.is_object_reachable(*member)
                            || store.node(*member).is_none_or(|node| {
                                !matches!(node.kind(), noon_core::SemanticNodeKind::AuthoringObject)
                            })
                    })
                {
                    return Err(ExecutionSessionAnimationError::InvalidComposition(
                        "CyclicReplace requires at least two scene-bound flat object members"
                            .into(),
                    ));
                }
                let path_arc = options.path_arc.unwrap_or(std::f64::consts::FRAC_PI_2);
                let mut child_options = AnimationOptions::new()
                    .rate_func(RateFunction::Linear)
                    .path_arc(path_arc);
                child_options.run_time = None;
                let children = members
                    .iter()
                    .enumerate()
                    .map(|(index, source)| SemanticCompositionRequest::TransformTo {
                        source: *source,
                        target_state: members[(index + 1) % members.len()],
                        interpolation: noon_core::SemanticTransformInterpolation::CenterTranslation,
                        complete_priority: false,
                        options: child_options,
                    })
                    .collect();
                let mut composition_options = *options;
                composition_options.path_arc = None;
                let expanded = SemanticCompositionRequest::Composition {
                    kind: SemanticAnimationCompositionKind::Parallel,
                    children,
                    options: composition_options,
                };
                self.stage_composition_request(
                    store,
                    root,
                    &expanded,
                    declaration,
                    admitted,
                    removals,
                )
            }
'''
new = '''            SemanticCompositionRequest::CyclicReplace { members, options } => {
                let mut seen = HashSet::new();
                if members.len() < 2
                    || members.iter().any(|member| {
                        !seen.insert(*member)
                            || !self.reachability.is_object_reachable(*member)
                            || store.node(*member).is_none_or(|node| {
                                !matches!(node.kind(), noon_core::SemanticNodeKind::AuthoringObject)
                            })
                            || store.semantic_object_state_checked(*member).is_err()
                    })
                {
                    return Err(ExecutionSessionAnimationError::InvalidComposition(
                        "CyclicReplace requires at least two distinct scene-bound flat object members"
                            .into(),
                    ));
                }
                let path_arc = options.path_arc.unwrap_or(std::f64::consts::FRAC_PI_2);
                let mut child_options = AnimationOptions::new()
                    .rate_func(RateFunction::Linear)
                    .path_arc(path_arc);
                child_options.run_time = None;
                let children = members
                    .iter()
                    .enumerate()
                    .map(|(index, source)| SemanticCompositionRequest::TransformTo {
                        source: *source,
                        target_state: members[(index + 1) % members.len()],
                        interpolation: noon_core::SemanticTransformInterpolation::CenterTranslation,
                        complete_priority: false,
                        options: child_options,
                    })
                    .collect();
                let mut composition_options = *options;
                composition_options.path_arc = None;
                let expanded = SemanticCompositionRequest::Composition {
                    kind: SemanticAnimationCompositionKind::Parallel,
                    children,
                    options: composition_options,
                };
                self.stage_composition_request(
                    store,
                    root,
                    &expanded,
                    declaration,
                    admitted,
                    removals,
                )
            }
'''
replace_once(path, old, new)

path = "crates/noon/src/live_session.rs"
replace_once(
    path,
    "    CyclicReplace {\n        family: &'a MobjectFamily,\n        options: AnimationOptions,\n    },",
    "    CyclicReplace {\n        members: Vec<&'a Mobject>,\n        options: AnimationOptions,\n    },",
)
replace_once(
    path,
    '''            AnimationCompositionRequest::CyclicReplace { family, options } => {
                self.require_family(family)?;
                Request::CyclicReplace {
                    family: family.node_id(),
                    options: *options,
                }
            }
''',
    '''            AnimationCompositionRequest::CyclicReplace { members, options } => {
                let members = members
                    .iter()
                    .map(|member| {
                        self.require_mobject(member)?;
                        Ok(member.node_id())
                    })
                    .collect::<Result<Vec<_>, LiveSessionError>>()?;
                Request::CyclicReplace {
                    members,
                    options: *options,
                }
            }
''',
)

# --- clean diagnostic for a peer that is not in the execution domain ---
path = "crates/noon-compile/src/semantic_lowering/animation_payload/prepared_composition.rs"
replace_once(
    path,
    "    MissingEffectiveProperties {\n        animation: SemanticTransactionNodeRef,\n        target: SemanticTransactionNodeRef,\n        execution_object_id: ObjectId,\n    },",
    "    MissingEffectiveProperties {\n        animation: SemanticTransactionNodeRef,\n        target: SemanticTransactionNodeRef,\n        execution_object_id: ObjectId,\n    },\n    MissingCenterTranslationPeer {\n        animation: SemanticTransactionNodeRef,\n        target_state: SemanticTransactionNodeRef,\n    },",
)
replace_once(
    path,
    '''                    let peer = target_state
                        .existing()
                        .and_then(|node| index.execution_object_id(node))
                        .ok_or(
                            PreparedSemanticAnimationLoweringError::MissingEffectiveProperties {
                                animation: leaf.animation,
                                target: target_state,
                                execution_object_id: ObjectId::new(u64::MAX),
                            },
                        )?;
''',
    '''                    let peer = target_state
                        .existing()
                        .and_then(|node| index.execution_object_id(node))
                        .ok_or(
                            PreparedSemanticAnimationLoweringError::MissingCenterTranslationPeer {
                                animation: leaf.animation,
                                target_state,
                            },
                        )?;
''',
)

# --- browser composition owns an ordered vector of existing member handles ---
path = "crates/noon-web/src/canonical_authoring_scene.rs"
replace_once(
    path,
    '''    FamilyTransformTo {
        source: noon::MobjectFamily,
        target_state: noon::MobjectFamily,
        options: noon_core::AnimationOptions,
    },
''',
    '''    FamilyTransformTo {
        source: noon::MobjectFamily,
        target_state: noon::MobjectFamily,
        options: noon_core::AnimationOptions,
    },
    CyclicReplace {
        members: Vec<noon::Mobject>,
        options: noon_core::AnimationOptions,
    },
''',
)
replace_once(
    path,
    '''                        noon_core::SemanticTransformInterpolation::PointCorrespondence => {
                            noon::TransformToRequest::point_correspondence(source, target, *options)
                        }
''',
    '''                        noon_core::SemanticTransformInterpolation::PointCorrespondence => {
                            noon::TransformToRequest::point_correspondence(source, target, *options)
                        }
                        noon_core::SemanticTransformInterpolation::CenterTranslation => {
                            unreachable!("CenterTranslation is emitted only by CyclicReplace")
                        }
''',
)
replace_once(
    path,
    '''                OrdinaryCompositionChild::FamilyTransformTo {
                    source,
                    target_state,
                    options,
                } => noon::AnimationCompositionRequest::FamilyTransformTo {
                    source,
                    target_state,
                    options: *options,
                },
''',
    '''                OrdinaryCompositionChild::FamilyTransformTo {
                    source,
                    target_state,
                    options,
                } => noon::AnimationCompositionRequest::FamilyTransformTo {
                    source,
                    target_state,
                    options: *options,
                },
                OrdinaryCompositionChild::CyclicReplace { members, options } => {
                    noon::AnimationCompositionRequest::CyclicReplace {
                        members: members.iter().collect(),
                        options: *options,
                    }
                }
''',
)
replace_once(
    path,
    '''                OrdinaryCompositionChild::FamilyTransformTo { .. }
                | OrdinaryCompositionChild::Indicate { .. }
''',
    '''                OrdinaryCompositionChild::FamilyTransformTo { .. }
                | OrdinaryCompositionChild::CyclicReplace { .. }
                | OrdinaryCompositionChild::Indicate { .. }
''',
)
# Validate direct member identity and Transform-specific options before any publication.
anchor = '''                OrdinaryCompositionChild::FamilyTransformTo {
                    source,
                    target_state,
                    options,
                } => {
'''
insert = '''                OrdinaryCompositionChild::CyclicReplace { members, options } => {
                    if members.len() < 2 {
                        return Err("CyclicReplace requires at least two direct members".into());
                    }
                    let mut seen = BTreeSet::new();
                    for member in members {
                        if !std::rc::Rc::ptr_eq(
                            self.scene.integration_store(),
                            member.integration_store(),
                        ) {
                            return Err("CyclicReplace member belongs to another authoring store".into());
                        }
                        member.validate().map_err(|error| error.to_string())?;
                        if !self.identities.contains_key(&member.node_id()) {
                            return Err("CyclicReplace members must already be bound to this Scene".into());
                        }
                        if !seen.insert(member.node_id()) {
                            return Err("CyclicReplace members must be distinct".into());
                        }
                    }
                    let resolved = noon_core::resolve_transform_animation_options(
                        noon_core::AnimationDefaults::MANIM,
                        *options,
                        noon_core::AnimationOptions::new(),
                    )
                    .map_err(|error| error.to_string())?;
                    if resolved.lag_ratio != 0.0
                        || resolved.reverse_rate_function
                        || resolved.remover
                        || resolved.introducer
                    {
                        return Err("CyclicReplace does not support lag, lifecycle, or reverse options".into());
                    }
                    continue;
                }
'''
text = read(path)
if text.count(anchor) != 1:
    raise RuntimeError(f"{path}: family transform validation anchor mismatch")
text = text.replace(anchor, insert + anchor, 1)
write(path, text)
replace_once(
    path,
    '''                    noon_core::resolve_animation_options(
                        noon_core::AnimationDefaults::MANIM,
                        *options,
                        noon_core::AnimationOptions::new(),
                    )
                    .map_err(|error| error.to_string())?;
                    continue;
                }
                OrdinaryCompositionChild::FamilyIndicate {
''',
    '''                    noon_core::resolve_transform_animation_options(
                        noon_core::AnimationDefaults::MANIM,
                        *options,
                        noon_core::AnimationOptions::new(),
                    )
                    .map_err(|error| error.to_string())?;
                    continue;
                }
                OrdinaryCompositionChild::FamilyIndicate {
''',
)
# WASM builder calls: create one cyclic child then append its remaining members.
method_anchor = '''        #[wasm_bindgen(js_name = appendIndicateMobject)]
'''
methods = '''        #[wasm_bindgen(js_name = appendCyclicReplace)]
        pub fn append_cyclic_replace(
            &mut self,
            first: &crate::WasmAuthoringMobjectHandle,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
            path_arc: f64,
        ) -> Result<(), JsValue> {
            let options = Self::optional_options(child_run_time, rate_function)?.path_arc(path_arc);
            self.children.push(OrdinaryCompositionChild::CyclicReplace {
                members: vec![first.semantic_mobject().clone()],
                options,
            });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendCyclicReplaceMember)]
        pub fn append_cyclic_replace_member(
            &mut self,
            member: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let Some(OrdinaryCompositionChild::CyclicReplace { members, .. }) = self.children.last_mut() else {
                return Err(js_error("CyclicReplace member must follow appendCyclicReplace"));
            };
            if !std::rc::Rc::ptr_eq(
                members[0].integration_store(),
                member.semantic_mobject().integration_store(),
            ) {
                return Err(js_error("CyclicReplace member belongs to another authoring store"));
            }
            members.push(member.semantic_mobject().clone());
            Ok(())
        }

'''
text = read(path)
if text.count(method_anchor) != 1:
    raise RuntimeError(f"{path}: append indication method anchor mismatch")
write(path, text.replace(method_anchor, methods + method_anchor, 1))

# --- Python request becomes inert; Rust owns activation-time centers ---
write(
    "web/python/_manim_cyclic_replace.py",
    '''"""ManimCE CyclicReplace/Swap inert requests over shared Rust animation semantics."""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _manim_compat as _compat


def _direct_members(mobjects: tuple[object, ...]) -> tuple[_base.Mobject, ...]:
    if len(mobjects) == 1 and isinstance(mobjects[0], _compat.Group):
        mobjects = tuple(mobjects[0])
    if len(mobjects) < 2:
        raise ValueError("CyclicReplace requires at least two direct Mobjects")
    members: list[_base.Mobject] = []
    for mobject in mobjects:
        if isinstance(mobject, _compat.Group):
            raise NotImplementedError(
                "CyclicReplace currently supports a flat direct-member family only"
            )
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("CyclicReplace members must be Mobjects")
        members.append(mobject)
    return tuple(members)


class CyclicReplace:
    """Inert cyclic center-translation request; activation semantics remain Rust-owned."""

    def __init__(
        self,
        *mobjects: object,
        path_arc: float = _base.PI / 2.0,
        **kwargs: Any,
    ) -> None:
        members = _direct_members(tuple(mobjects))
        arc = float(path_arc)
        if not math.isfinite(arc):
            raise ValueError("CyclicReplace path_arc must be finite")
        self.mobjects = members
        self.group = mobjects[0] if len(mobjects) == 1 and isinstance(mobjects[0], _compat.Group) else None
        self.path_arc = arc
        self.anim_args = dict(kwargs)
        self.anim_args["path_arc"] = arc


class Swap(CyclicReplace):
    """Two-object ManimCE alias with identical cyclic semantics."""

    pass
''',
)

path = "web/python/_manim_scene.py"
replace_once(path, "import _manim_composition as _composition\n", "import _manim_composition as _composition\nimport _manim_cyclic_replace as _cyclic\n")
# Family resolver is Transform-specific but retains family lag support.
helper_anchor = '''def _canonical_affine_lifecycle_animation(
'''
helper = '''def _canonical_family_transform_options(
    animation: object, kwargs: dict[str, object]
) -> object | None:
    duration = kwargs.get("duration")
    run_time = kwargs.get("run_time")
    easing = kwargs.get("easing")
    rate_func = kwargs.get("rate_func")
    lag_ratio = kwargs.get("lag_ratio")
    path_arc = kwargs.get("path_arc")
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None and rate_func is not None:
        raise ValueError("use either rate_func or the low-level easing alias, not both")
    if kwargs.keys() - {
        "duration", "run_time", "start_time", "easing", "rate_func", "lag_ratio", "path_arc"
    }:
        return None
    if kwargs.get("start_time") is not None:
        return None
    try:
        resolved = _options.resolve_transform(
            builder_args=_options.builder_args(animation),
            default_lag_ratio=0.0,
            play_run_time=(run_time if run_time is not None else duration),
            play_easing=easing,
            play_rate_func=rate_func,
            play_lag_ratio=lag_ratio,
            play_path_arc=path_arc,
        )
    except NotImplementedError:
        return None
    if resolved.reverse_rate_function:
        return None
    return resolved


'''
text = read(path)
if text.count(helper_anchor) != 1:
    raise RuntimeError(f"{path}: lifecycle helper anchor mismatch")
write(path, text.replace(helper_anchor, helper + helper_anchor, 1))
# CyclicReplace must be recognized before generic AnimationGroup and never read layout in Python.
replace_once(
    path,
    '''        if isinstance(animation, _composition.AnimationGroup):
            nested_kind = "sequence" if isinstance(animation, _composition.Succession) else "parallel"
''',
    '''        if isinstance(animation, _cyclic.CyclicReplace):
            child = _canonical_family_transform_options(animation, child_kwargs)
            if child is None or child.lag_ratio != 0.0:
                raise NotImplementedError("unsupported canonical CyclicReplace options")
            members = animation.mobjects
            if len(members) < 2 or any(
                isinstance(member, _compat.Group)
                or not isinstance(member, _base.Mobject)
                or member._scene is not self
                or getattr(member, "_semantic_handle", None) is None
                for member in members
            ):
                raise NotImplementedError(
                    "CyclicReplace requires distinct scene-bound flat typed Mobjects"
                )
            if len({id(member) for member in members}) != len(members):
                raise ValueError("CyclicReplace members must be distinct")
            builder.appendCyclicReplace(
                members[0]._semantic_handle,
                float(child.run_time),
                str(child.rate_func),
                float(child.path_arc),
            )
            for member in members[1:]:
                builder.appendCyclicReplaceMember(member._semantic_handle)
            return
        if isinstance(animation, _composition.AnimationGroup):
            nested_kind = "sequence" if isinstance(animation, _composition.Succession) else "parallel"
''',
)
replace_once(
    path,
    '''            child = _canonical_affine_options(leaf, child_kwargs, allow_family_lag=True)
            if child is None or child.path_arc != 0.0 or child.reverse_rate_function:
                raise NotImplementedError("unsupported canonical family Transform options")
''',
    '''            child = _canonical_family_transform_options(leaf, child_kwargs)
            if child is None:
                raise NotImplementedError("unsupported canonical family Transform options")
''',
)
replace_once(
    path,
    '''                str(child.rate_func),
                float(child.lag_ratio),
            )
            return
        if isinstance(animation, _composition.Add):
''',
    '''                str(child.rate_func),
                float(child.lag_ratio),
                float(child.path_arc),
            )
            return
        if isinstance(animation, _composition.Add):
''',
)

# --- native regressions: same-Succession activation and seek equivalence ---
path = "crates/noon/src/execution_session/family_transform_tests.rs"
text = read(path)
append = r'''

fn cyclic_sequence_session() -> (SemanticStore, ExecutionSession, ExecutionSegment, [SemanticNodeId; 3]) {
    let mut store = SemanticStore::new();
    let a = object(&mut store, -2.0);
    let b = object(&mut store, 0.0);
    let c = object(&mut store, 2.0);
    let root = family(&mut store, &[a, b, c]);
    let a_after_first = object(&mut store, 4.0);
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    let first = SemanticCompositionRequest::TransformTo {
        source: a,
        target_state: a_after_first,
        interpolation: noon_core::SemanticTransformInterpolation::Affine,
        complete_priority: false,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    };
    let cyclic = SemanticCompositionRequest::CyclicReplace {
        members: vec![a, b, c],
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear)
            .path_arc(std::f64::consts::FRAC_PI_2),
    };
    let request = SemanticCompositionRequest::Composition {
        kind: SemanticAnimationCompositionKind::Sequence,
        children: vec![first, cyclic],
        options: AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, root, &request, AnimationOptions::new())
        .unwrap();
    (store, session, segment, [a, b, c])
}

fn translation_for(session: &ExecutionSession, node: SemanticNodeId) -> Vec2 {
    let object = session.execution_index.execution_object_id(node).unwrap();
    let index = session.runtime.frame_index_for_object(object).unwrap();
    session.frame().objects[index].transform.translation
}

#[test]
fn cyclic_replace_uses_same_succession_activation_centers() {
    let (_store, mut session, segment, [a, b, c]) = cyclic_sequence_session();
    session.advance_segment_to(segment, 1.5).unwrap();
    let middle_c = translation_for(&session, c);
    assert!(middle_c.x > 2.0 && middle_c.x < 4.0);
    assert!(middle_c.y.abs() > 0.1, "CyclicReplace midpoint must use the curved path");

    session.advance_segment_to(segment, 2.0).unwrap();
    let endpoints = [
        translation_for(&session, a),
        translation_for(&session, b),
        translation_for(&session, c),
    ];
    for (actual, expected) in endpoints.into_iter().zip([
        Vec2::new(0.0, 0.0),
        Vec2::new(2.0, 0.0),
        Vec2::new(4.0, 0.0),
    ]) {
        assert!((actual.x - expected.x).abs() < 1e-5);
        assert!((actual.y - expected.y).abs() < 1e-5);
    }
}

#[test]
fn cyclic_replace_direct_seek_matches_forward_playback() {
    let (_forward_store, mut forward, forward_segment, _) = cyclic_sequence_session();
    forward.advance_segment_to(forward_segment, 1.25).unwrap();
    forward.advance_segment_to(forward_segment, 1.5).unwrap();
    let forward_frame = forward.frame().clone();

    let (_direct_store, mut direct, _direct_segment, _) = cyclic_sequence_session();
    direct.seek(1.5).unwrap();
    assert_eq!(direct.frame(), &forward_frame);
}
'''
if "fn cyclic_replace_uses_same_succession_activation_centers" in text:
    raise RuntimeError("family transform tests already contain cyclic regression")
write(path, text + append)

# --- Python request contract: inert, normalized, finite, no target construction ---
write(
    "web/python/test_manim_cyclic_replace.py",
    '''import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimCyclicReplaceTests(unittest.TestCase):
    def test_requests_are_inert_and_normalize_call_shapes(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            part for part in (str(python_dir), env.get("PYTHONPATH")) if part
        )
        source = textwrap.dedent(
            """
            import sys
            import types

            fake_js = types.ModuleType("js")
            fake_js.noonResolveAnimationOptions = object()
            fake_js.noonResolveTransformAnimationOptions = object()
            sys.modules["js"] = fake_js

            import noon
            import _manim_compat as compat
            from _manim_cyclic_replace import CyclicReplace, Swap

            a = object.__new__(noon.Mobject)
            b = object.__new__(noon.Mobject)
            c = object.__new__(noon.Mobject)

            swap = Swap(a, b)
            assert swap.mobjects == (a, b)
            assert swap.anim_args == {"path_arc": noon.PI / 2.0}

            cyclic = CyclicReplace(a, b, c, path_arc=-0.75, run_time=2.0)
            assert cyclic.mobjects == (a, b, c)
            assert cyclic.anim_args == {"run_time": 2.0, "path_arc": -0.75}

            group = object.__new__(compat.Group)
            group.submobjects = [a, b, c]
            grouped = CyclicReplace(group)
            assert grouped.mobjects == (a, b, c)
            assert grouped.group is group

            try:
                CyclicReplace(a)
            except ValueError:
                pass
            else:
                raise AssertionError("single leaf must fail")

            try:
                CyclicReplace(a, b, path_arc=float("nan"))
            except ValueError:
                pass
            else:
                raise AssertionError("non-finite arc must fail")

            nested = object.__new__(compat.Group)
            nested.submobjects = [a, b]
            try:
                CyclicReplace(a, nested)
            except NotImplementedError:
                pass
            else:
                raise AssertionError("nested groups must fail closed")
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
            f"CyclicReplace request subprocess failed:\\nstdout:\\n{completed.stdout}\\nstderr:\\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
''',
)

print("patched final CyclicReplace integration")
