from pathlib import Path

p = Path("scripts/b2-browser-local-transactions.py")
text = p.read_text()

old = '            Self::Evaluation(error) => write!(formatter, \\"{error}\\"),'
new = '            Self::Evaluation(error) => write!(formatter, \\"scene evaluation failed: {error}\\"),'
if old not in text:
    raise SystemExit("stale PlayerError anchor not found in bootstrap helper")
text = text.replace(old, new, 2)

old_generic = '''replace_once(
    "crates/noon-web/src/legacy.rs",
    "        let mut instance = SceneInstance::new(compiled);",
    "        let mut instance = SlottedSceneInstance::new(compiled);",
)
'''
new_contextual = '''replace_once(
    "crates/noon-web/src/legacy.rs",
    """    pub fn replace_scene_json(&mut self, json: &str) -> Result<&FrameState, PlayerError> {
        let definition = decode_scene(json)?;
        let compiled = CompiledScene::compile(&definition)?;
        let playhead = self.instance.frame().time;
        let mut instance = SceneInstance::new(compiled);
        instance.seek(playhead)?;
""",
    """    pub fn replace_scene_json(&mut self, json: &str) -> Result<&FrameState, PlayerError> {
        let definition = decode_scene(json)?;
        let compiled = CompiledScene::compile(&definition)?;
        let playhead = self.instance.frame().time;
        let mut instance = SlottedSceneInstance::new(compiled);
        instance.seek(playhead)?;
""",
)
'''
if old_generic not in text:
    raise SystemExit("generic SceneInstance replacement not found in bootstrap helper")
text = text.replace(old_generic, new_contextual, 1)

private_track_pattern = "UnsupportedTransformGeometry(TrackId(50))"
if text.count(private_track_pattern) != 2:
    raise SystemExit(
        f"expected two private TrackId test patterns, found {text.count(private_track_pattern)}"
    )
text = text.replace(private_track_pattern, "UnsupportedTransformGeometry(_)")

marker = "# Keep the migration document explicit about this end-to-end browser slice."
if marker not in text:
    raise SystemExit("documentation bootstrap marker not found")
text = text[: text.index(marker)] + 'print("applied browser-local transaction slice")\n'

p.write_text(text)
