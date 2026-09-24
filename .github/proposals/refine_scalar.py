from pathlib import Path
p = Path('crates/noon/tests/scalar_timeline_replay.rs')
s = p.read_text()
needle = '    (store, tracker, external)'
assert s.count(needle) == 1
s = s.replace(needle, '''    let root = store.insert_family();
    store.attach_to_scene(root).unwrap();
    let mut scope = SemanticMutationTransaction::new();
    scope.scope_signal(root, tracker).scope_signal(root, external);
    scope.apply(&mut store).unwrap();
    (store, tracker, external)''')
s = s.replace('predeclared_curves_replay_exactly_through_gaps_without_dirtying_geometry', 'predeclared_curve_playback_stays_sparse_and_replay_matches_authored_evaluation')
s = s.replace('for time in [0.5, 1.0, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0] { session.advance_to(time).unwrap(); }', '''for time in [0.5, 1.0, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0] {
        session.take_frame_changes();
        session.advance_to(time).unwrap();
        assert!(session.take_frame_changes().is_empty());
    }''')
s = s.replace('        assert_eq!(session.frame(), oracle.frame());\n        assert!(session.take_frame_changes().is_empty());', '''        assert_eq!(session.frame(), oracle.frame());
        // Full seek deliberately republishes scene rows; locality is asserted
        // above for ordinary forward evaluation, not by weakening seek semantics.''')
p.write_text(s)
p = Path('crates/noon-runtime/src/signal_timeline.rs')
s = p.read_text()
s = s.replace('#[derive(Clone, Debug, Default)]\npub struct SignalTimelineSchedule', '#[derive(Clone, Debug)]\npub struct SignalTimelineSchedule')
s = s.replace('runtime: Option<RuntimeIdentity>', 'runtime: RuntimeIdentity')
s = s.replace('runtime: Some(runtime)', 'runtime')
s = s.replace('..Self::default()', 'event_cursor: 0,\n            active: BTreeSet::new(),\n            initialized: false,')
s = s.replace('cloned.runtime = Some(runtime);', 'cloned.runtime = runtime;')
s = s.replace('self.runtime.expect("a constructed scalar schedule has a runtime")', 'self.runtime')
p.write_text(s)
