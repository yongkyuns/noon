from pathlib import Path
import re

p=Path('crates/noon/src/execution_session.rs')
s=p.read_text()
s,n=re.subn(r'\bprepared(\s*)\.integration_store\(\)',r'prepared\1.store()',s)
assert n == 1, n
p.write_text(s)

# Keep the source copied into old-baseline builds on the ordinary API common
# to both revisions. Shared text-contract visibility belongs in the tests.
p=Path('fixtures/provider-consumer/src/main.rs')
s=p.read_text().replace('use noon::integration::TextResource;\n','')
a=s.index('    assert!(scene\n')
b=s.index('    std::hint::black_box',a)
s=s[:a]+s[b:]
p.write_text(s)
p=Path('fixtures/provider-consumer/tests/public_facade.rs')
s=p.read_text().replace('use noon::integration::{SemanticMutationTransaction, SemanticStore};','use noon::integration::{SemanticMutationTransaction, SemanticStore, TextResource};')
s=s.replace('    let arena = Rc::new(RefCell::new(SemanticStore::new()));', '''    // Shared semantic text/resource contracts are available without providers.
    let _: Option<TextResource> = None;
    let arena = Rc::new(RefCell::new(SemanticStore::new()));''')
s=s.replace('    scene.add(&circle)?;\n    let mut session = scene.execution_session()?;\n    session.take_frame_changes();','    scene.add(&circle)?;\n    assert!(arena.borrow().text_resources().is_empty());\n    let mut session = scene.execution_session()?;\n    session.take_frame_changes();')
s=s.replace('    assert_eq!(scene.revision(), raw_revision);','    assert_eq!(circle.state()?.transform.translation.x, 9.0);\n    assert_eq!(scene.revision(), raw_revision);')
p.write_text(s)
p=Path('.github/workflows/provider-features.yml')
s=p.read_text().replace('      - name: Publish evidence\n','''      - name: Qualify the minimal public facade documentation and example
        if: matrix.config == 'minimal' && matrix.target == 'x86_64-unknown-linux-gnu'
        run: |
          cargo test -p noon --no-default-features --doc
          cargo run -p noon --no-default-features --example shared_authoring
      - name: Publish evidence
''',1)
p.write_text(s)
p=Path('fixtures/provider-consumer/README.md')
p.write_text(p.read_text()+'''\nThe geometry program copied by `--baseline` uses the ordinary geometry API common\nto both revisions. Shared text-contract visibility and arena assertions live in\n`public_facade` rather than adding a dependency on new integration accessor names\nto the historical-build workload. No historical baseline code is rewritten.\n''')
