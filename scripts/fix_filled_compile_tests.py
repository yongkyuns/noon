from pathlib import Path

path = Path("crates/noon-compile/src/lib.rs")
text = path.read_text()
old = '''scene
            .add_track(TrackDefinition {
                id: noon_core::TrackId::new(0),
                object,
                property: Property::Transform,
                values: TrackValues::Object { from, to },
                timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
            })'''
new = '''scene
            .add_track(
                object,
                Property::Transform,
                TrackValues::Object { from, to },
                TrackTiming::new(0.0, 2.0, Easing::Linear),
            )'''
count = text.count(old)
if count != 2:
    raise SystemExit(f"expected two filled Transform compiler-test tracks, found {count}")
text = text.replace(old, new)
path.write_text(text)
