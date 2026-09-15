from pathlib import Path

def edit(name, before, after):
    p=Path(name);s=p.read_text()
    assert s.count(before)==1,(name,s.count(before),before[:80])
    p.write_text(s.replace(before,after))

edit('crates/noon/src/example_scenes.rs', 'pub mod specialized_geometry;',
     'pub mod specialized_geometry;\npub mod svg_morph;')
edit('scripts/playground-tiger-morph-smoke.mjs', "if (time < 0.5) return 'tiger';",
     "if (time <= 0.5) return 'tiger';")
edit('scripts/playground-tiger-morph-smoke.mjs',
     '    if (Number.isFinite(time) && time >= 0 && time < 0.3) armed = true;',
     '''    // The static opening wait need not present a new frame at time zero.
    // Arm on the new scene's identity/count, not an unobservable time window.
    // The previous Indicate scene has one object; this pinned tiger has 138.
    if (state.report?.metrics?.objectCount === 138 && Number.isFinite(time)) armed = true;''')
edit('crates/noon-web/src/retained_resource_transport/morph_tests.rs',
     '''            -1.0,
            0,
            0,''',
     '''            // A changing-winding fill now retains the immutable world
            // endpoints too. Its preparation hint is not a promise of a fixed
            // GPU fan: renderer-local sampling handles the singular interior.
            -1.0,
            1,
            1,''')
edit('crates/noon-web/src/retained_resource_transport/morph_tests.rs',
     'fn compiled_table_keeps_stable_local_pairs_but_excludes_dynamic_screen_space_fallback()',
     'fn compiled_table_retains_fixed_world_pairs_including_sampled_fills()')
print('Registered native/shared example and corrected the test observation contract')
