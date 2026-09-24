from pathlib import Path

p = Path('crates/noon/src/execution_session.rs')
s = p.read_text()


def replace(old, new, count=1):
    global s
    assert s.count(old) == count, (old, s.count(old))
    s = s.replace(old, new)


replace('''        if !self.callback_schedule.is_empty() {
            return Err(ExecutionSessionAnimationError::CreateTarget {
                target: error_target,
                error: ExecutionSessionCreateError::RequiredCallbacksUnsupported,
            });
        }
''', '')
replace('''        if !self.callback_schedule.is_empty() {
            return Err(ExecutionSessionAnimationError::FadeTarget {
                target,
                error: ExecutionSessionFadeError::RequiredCallbacksUnsupported,
            });
        }''', '''        if self.lifecycle_target_has_pending_updaters(store, target) {
            return Err(ExecutionSessionAnimationError::FadeTarget {
                target,
                error: ExecutionSessionFadeError::RequiredCallbacksUnsupported,
            });
        }''', 2)
needle = '''    fn require_create_target(
        &self,
        store: &SemanticStore,
        target: SemanticNodeId,
    ) -> Result<(), ExecutionSessionAnimationError> {
'''
replace(needle, needle + '''        if self.lifecycle_target_has_pending_updaters(store, target) {
            return Err(ExecutionSessionAnimationError::CreateTarget {
                target,
                error: ExecutionSessionCreateError::RequiredCallbacksUnsupported,
            });
        }
''')
start = s.index('    fn require_family_fade_target(')
end = s.index('    fn require_present_draw_border_target(', start)
family = s[start:end]
needle = '        for leaf in &leaves {\n'
assert family.count(needle) == 1
family = family.replace(needle, needle + '''            if self.lifecycle_target_has_pending_updaters(store, *leaf) {
                return Err(ExecutionSessionAnimationError::FadeTarget {
                    target: *leaf,
                    error: ExecutionSessionFadeError::RequiredCallbacksUnsupported,
                });
            }
''')
s = s[:start] + family + s[end:]
helper = '''    /// Lifecycle admission is local to its target, not the scene's callback history.
    /// Ancestor updaters can own the same family presentation. Until target-updater
    /// suspension is supported, reject their nonempty current/future intervals too.
    /// Closed history is inert, and unrelated callback targets remain admissible.
    fn lifecycle_target_has_pending_updaters(
        &self,
        store: &SemanticStore,
        target: SemanticNodeId,
    ) -> bool {
        let time = self.frame().time;
        let mut pending = vec![target];
        let mut visited = HashSet::new();
        while let Some(target) = pending.pop() {
            if !visited.insert(target) {
                continue;
            }
            let Some(node) = store.node(target) else {
                continue;
            };
            if node.host_updaters().iter().any(|registration| {
                registration.inactive_from().is_none_or(|end| {
                    end > time.max(registration.active_from())
                })
            }) {
                return true;
            }
            pending.extend_from_slice(node.parents());
        }
        false
    }

'''
replace('    fn require_create_target(\n', helper + '    fn require_create_target(\n')
replace('Create does not yet support required host callbacks', 'Create target has active or future host updaters; updater suspension is not supported')
replace('single-leaf fade does not yet support required host callbacks', 'lifecycle target has active or future host updaters; updater suspension is not supported')
p.write_text(s)

p = Path('crates/noon/tests/callback_lifecycle_admission.rs')
s = p.read_text()
s = s.replace('for nested in [false, true] {', 'for route in 0..3 {\n        let nested = route == 2;')
old = '''        let segment = scene
            .live(&mut execution)
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap_or_else(|error| {
                panic!("{lifecycle:?}, nested={nested}: unrelated updater blocked admission: {error}")
            });'''
assert s.count(old) == 1
s = s.replace(old, '''        let result = if route == 0 {
            let mut live = scene.live(&mut execution);
            match lifecycle {
                Lifecycle::FadeIn | Lifecycle::FadeOut => live.declare_and_activate_fade_with_endpoint(
                    &target,
                    if lifecycle.removes() { SemanticFadeDirection::Out } else { SemanticFadeDirection::In },
                    FadeEndpoint::default(),
                    options(),
                ),
                Lifecycle::Create => live.declare_and_activate_create(&target, options()),
                Lifecycle::Uncreate => live.declare_and_activate_uncreate(&target, options()),
            }
        } else {
            scene.live(&mut execution).declare_and_activate_composition(&request, AnimationOptions::new())
        };
        let segment = result.unwrap_or_else(|error| {
            panic!("{lifecycle:?}, route={route}: unrelated updater blocked admission: {error}")
        });''')
s += '''

#[test]
fn own_current_or_future_updater_is_rejected_before_any_admission() {
    for lifecycle in [Lifecycle::FadeIn, Lifecycle::FadeOut, Lifecycle::Create, Lifecycle::Uncreate] {
        for active_from in [0.0, 0.5, 2.0] {
            let mut scene = Scene::new();
            let target = scene.square(1.0).unwrap();
            if lifecycle.removes() {
                scene.add(&target).unwrap();
            }
            let mut tx = SemanticMutationTransaction::new();
            tx.add_updater(target.node_id(), MOVE, active_from, None);
            tx.apply(&mut scene.integration_store().borrow_mut()).unwrap();
            let mut execution = scene.execution_session().unwrap();
            let before = execution.publication_context();
            let objects = execution.frame().objects.clone();
            let error = scene.live(&mut execution)
                .declare_and_activate_composition(&lifecycle.request(&target), AnimationOptions::new())
                .unwrap_err();
            assert!(error.to_string().contains("host updaters"), "{error}");
            assert_eq!(execution.publication_context(), before);
            assert_eq!(scene.revision(), before.scene_revision());
            assert_eq!(execution.frame().objects, objects);
            assert_eq!(scene.live(&mut execution).contains(&target).unwrap(), lifecycle.removes());
        }
    }
}

#[test]
fn family_fade_coexists_with_an_unrelated_updater() {
    for direction in [SemanticFadeDirection::In, SemanticFadeDirection::Out] {
        let mut scene = Scene::new();
        let moving = scene.circle(0.25).unwrap();
        let a = scene.square(1.0).unwrap();
        let b = scene.square(0.5).unwrap();
        let nested = scene.family(&[(&b).into()]).unwrap();
        let family = scene.family(&[(&a).into(), (&nested).into()]).unwrap();
        scene.add(&moving).unwrap();
        if direction == SemanticFadeDirection::Out {
            scene.add_many(&[(&family).into()]).unwrap();
        }
        let times = Rc::new(RefCell::new(Vec::new()));
        let mut callbacks = install_motion(&mut scene, &moving, &times);
        let mut execution = scene.execution_session().unwrap();
        callbacks.advance_to(&mut execution, 0.0).unwrap();
        let identity = execution.runtime_identity();
        let segment = scene.live(&mut execution).declare_and_activate_composition(
            &Request::FamilyFade { target: &family, direction, options: options() },
            AnimationOptions::new(),
        ).unwrap();
        callbacks.advance_segment_to(&mut execution, segment, 0.5).unwrap();
        assert_eq!(execution.frame().objects.len(), 3);
        assert_eq!(execution.frame().objects[0].transform.translation.x, 0.5);
        assert_eq!(execution.frame().objects[1].appearance, 0.5);
        assert_eq!(execution.frame().objects[2].appearance, 0.5);
        callbacks.advance_segment_to(&mut execution, segment, 1.0).unwrap();
        scene.live(&mut execution).complete_segment(segment).unwrap();
        assert_eq!(execution.frame().is_present(1), direction == SemanticFadeDirection::In);
        assert_eq!(execution.frame().is_present(2), direction == SemanticFadeDirection::In);
        let hold = scene.live(&mut execution).wait_segment(0.5).unwrap();
        callbacks.advance_segment_to(&mut execution, hold, 1.5).unwrap();
        scene.live(&mut execution).complete_segment(hold).unwrap();
        assert_eq!(execution.frame().objects[0].transform.translation.x, 1.5);
        assert_eq!(execution.runtime_identity(), identity);
    }
}

#[test]
fn family_or_descendant_updaters_are_rejected_without_partial_admission() {
    for callback_target in 0..3 {
        let mut scene = Scene::new();
        let leaf = scene.square(1.0).unwrap();
        let nested = scene.family(&[(&leaf).into()]).unwrap();
        let family = scene.family(&[(&nested).into()]).unwrap();
        let target = [family.node_id(), nested.node_id(), leaf.node_id()][callback_target];
        let mut tx = SemanticMutationTransaction::new();
        tx.add_updater(target, MOVE, 0.5, None);
        tx.apply(&mut scene.integration_store().borrow_mut()).unwrap();
        let mut execution = scene.execution_session().unwrap();
        let before = execution.publication_context();
        let error = scene.live(&mut execution).declare_and_activate_composition(
            &Request::FamilyFade { target: &family, direction: SemanticFadeDirection::In, options: options() },
            AnimationOptions::new(),
        ).unwrap_err();
        assert!(error.to_string().contains("host updaters"), "{error}");
        assert_eq!(execution.publication_context(), before);
        assert_eq!(scene.revision(), before.scene_revision());
        assert!(execution.frame().objects.is_empty());
    }
}
'''
p.write_text(s)
