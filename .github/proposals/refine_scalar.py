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
