"""Reconcile #1354's typed producers with the pinned, already-landed family paint.

This development-only script operates on the exact merge of 461fb174 and 406944f8.
It does not touch Python, the R3 mapper, or the separately owned R2b composition.
"""
from pathlib import Path
import re

ROOT = Path.cwd()

def replace(path: str, old: str, new: str, count: int = 1) -> None:
    target = ROOT / path
    text = target.read_text()
    assert text.count(old) == count, (path, old, text.count(old))
    target.write_text(text.replace(old, new))

style = ROOT / 'crates/noon/src/semantic_mobject/style.rs'
text = style.read_text()
conflict = re.compile(r'<<<<<<< HEAD\n(.*?)=======\n(.*?)>>>>>>> 406944f8a58218629c14ff247894b3fd2e8c471a\n', re.S)
blocks = list(conflict.finditer(text))
assert len(blocks) == 3, len(blocks)
names = ['edit_manim_opacity', 'edit_stroke_opacity', 'edit_stroke_width']
for block, name in zip(blocks, names):
    assert f'fn {name}(' in block.group(1), name
    assert f'fn {name}<S: PaintStyleEdit>' in block.group(2), name
    assert block.group(1).count('Result<(), AuthoringError>') == 1
    assert block.group(2).count('Result<(), String>') == 1
style.write_text(conflict.sub(lambda block: block.group(2).replace('Result<(), String>', 'Result<(), AuthoringError>'), text))

family = 'crates/noon/src/family_callback_paint.rs'
replace(family, 'fn apply(self, style: &mut Style) -> Result<(), String>',
        'fn apply(self, style: &mut Style) -> Result<(), AuthoringError>')
replace(family, 'InvalidPaint(String),', 'InvalidPaint(AuthoringError),')
replace(family, 'Self::InvalidPaint(e) => f.write_str(e),', 'Self::InvalidPaint(e) => e.fmt(f),')
replace(family, 'Self::Authoring(e) => Some(e),',
        'Self::Authoring(e) | Self::InvalidPaint(e) => Some(e),')

examples = 'crates/noon/src/example_scenes.rs'
for old in [
    'let nested = scene.family(&[(&circle).into(), (&anchor).into()])?;',
    'let family = scene.family(&[(&nested).into(), (&circle).into()])?;',
    'let missing = scene.circle(0.1)?;',
    'let invalid_family = scene.family(&[(&circle).into(), (&missing).into()])?;',
]:
    replace(examples, old, old[:-2] + '.map_err(|error| error.to_string())?;')

consumer = ROOT / 'fixtures/provider-consumer/tests/object_authoring_errors.rs'
test_name = 'fn callback_family_invalid_paint_retains_shared_cause_before_reads_and_recovers'
assert test_name not in consumer.read_text()
with consumer.open('a') as output:
    output.write('''

#[test]
fn callback_family_invalid_paint_retains_shared_cause_before_reads_and_recovers() -> TestResult {
    use noon::{FamilyCallbackPaintError, FamilyPaint, Style};

    let scene = Scene::new();
    let first = scene.circle(0.5)?;
    let second = scene.square(0.5)?;
    let nested = scene.family(&[(&first).into(), (&second).into()])?;
    let family = scene.family(&[(&nested).into(), (&first).into()])?;
    let before = snapshot(&scene, &[&first, &second]);
    let revision = scene.revision();
    let mut reads = Vec::new();
    let error = family.prepare_callback_paint(
        revision,
        FamilyPaint::Fill { color: None, opacity: Some(1.5) },
        |node| { reads.push(node); Ok(Style::default()) },
    ).unwrap_err();
    assert!(matches!(
        &error,
        FamilyCallbackPaintError::InvalidPaint(AuthoringError::InvalidOpacity {
            value: 1.5, ..
        })
    ));
    let cause = error.source().unwrap().downcast_ref::<AuthoringError>().unwrap();
    assert!(matches!(cause, AuthoringError::InvalidOpacity { value: 1.5, .. }));
    assert!(cause.source().is_none());
    assert!(reads.is_empty(), "invalid paint must be rejected before effective reads");
    assert_eq!(snapshot(&scene, &[&first, &second]), before);

    let changes = family.prepare_callback_paint(
        revision,
        FamilyPaint::Fill { color: None, opacity: Some(0.5) },
        |node| { reads.push(node); Ok(Style::default()) },
    )?;
    assert_eq!(reads, [first.node_id(), second.node_id()]);
    assert_eq!(changes.len(), 2);
    for (_, style) in changes {
        assert_eq!(style.fill.unwrap().alpha, 0.5);
    }
    // Preparation returns an effective write batch; even recovery is not an
    // authored edit and must not change membership, resources or scene revision.
    assert_eq!(snapshot(&scene, &[&first, &second]), before);
    Ok(())
}
''')

for path in (ROOT / 'crates').rglob('*.rs'):
    text = path.read_text()
    assert not re.search(r'^(<<<<<<<|=======|>>>>>>>)', text, re.M), path
print('Resolved three generic paint signatures; preserved typed family paint cause; added external rejection/recovery regression.')
