from pathlib import Path

path = Path("crates/noon-compile/src/lib.rs")
text = path.read_text()
old = ".add_track(TrackDefinition {"
new = ".add_track(noon_core::TrackDefinition {"
if old not in text:
    raise SystemExit("filled Transform compiler-test marker missing")
text = text.replace(old, new)
path.write_text(text)
