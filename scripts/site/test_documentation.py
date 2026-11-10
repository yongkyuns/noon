import ast
import json
from pathlib import Path
import tempfile
import unittest

from build_docs import Builder, Links, SiteMarkdown, build_embed, slug
from python_reference import PythonReference, ReferenceError

REVISION = 'a' * 40


class SourceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.python = self.root / 'web/python'
        self.python.mkdir(parents=True)

    def tearDown(self): self.temp.cleanup()

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def facade(self):
        self.write('web/python/noon.py', '''
from _external_package import external
_PUBLIC_EXPORTS = {"Shape": "_shape"}
__all__ = ["Scene", "external", *_PUBLIC_EXPORTS]
class Scene:
    """Shared scene facade."""
    def play(self, animation, /, *more, run_time: float = 1.0, **options) -> None:
        """Play animations."""
        pass
''')
        self.write('web/python/_shape.py', '''
class Shape:
    def __init__(self, radius: float = 0.6, *, label: str = "<unsafe>"):
        pass
''')

    def test_explicit_exports_and_unresolved_are_reported(self):
        self.facade()
        result = PythonReference(self.python).build()
        self.assertEqual((result['export_count'], result['resolved_count']), (3, 2))
        self.assertEqual(result['unresolved'][0]['name'], 'external')
        self.assertEqual(len(result['source_sha256']), 2)

    def test_signature_preserves_parameter_kinds_defaults_and_return(self):
        self.facade()
        scene = next(n for n in PythonReference(self.python).build()['entries'] if n['name'] == 'Scene')
        play = scene['members'][0]
        self.assertEqual(play['signature'], 'Scene.play(self, animation, /, *more, run_time: float=1.0, **options) -> None')
        self.assertEqual([p['kind'] for p in play['parameters']], ['positional-only', 'positional-only', '*args', 'keyword-only', '**kwargs'])
        self.assertEqual(play['parameters'][3]['default'], '1.0')

    def test_no_module_execution(self):
        self.facade()
        p = self.python / 'noon.py'
        p.write_text(p.read_text() + '\nraise RuntimeError("must not execute")\n')
        self.assertEqual(PythonReference(self.python).build()['resolved_count'], 2)

    def test_local_alias_and_cycle_are_safe(self):
        self.write('web/python/noon.py', '__all__=["Alias", "cycle"]\nfrom _shape import Shape as Alias\ncycle=cycle\n')
        self.write('web/python/_shape.py', 'class Shape:\n    def move(self, distance=1): pass\n')
        result = PythonReference(self.python).build()
        alias = next(n for n in result['entries'] if n['name'] == 'Alias')
        self.assertEqual(alias['members'][0]['name'], 'Alias.move')
        self.assertEqual(alias['module'], '_shape')

    def test_dynamic_export_syntax_fails_loudly(self):
        self.write('web/python/noon.py', '__all__ = compute_public_names()')
        with self.assertRaises(ReferenceError): PythonReference(self.python).build()

    def builder(self):
        self.facade()
        nav = [{'slug':'index','title':'Docs','group':'Start','source':'docs/site/index.md'},
               {'slug':'api','title':'API','group':'Reference','source':'docs/site/api.md'},
               {'slug':'examples','title':'Examples','group':'Explore','source':'docs/site/examples.md'},
               {'slug':'architecture','title':'Architecture','group':'Internals','source':'docs/architecture.md'}]
        self.write('scripts/site/navigation.json', json.dumps(nav))
        self.write('web/site/examples.json', '[{"id":"showcase-first-scene"}]')
        self.write('web/python/examples/noon_showcase_manifest.json', json.dumps({'entries':[
            {'id':'showcase-first-scene','title':'First scene','path':'python/examples/first.py','thumbnail':'thumbnails/first.png'}]}))
        self.write('docs/site/index.md', '# Docs\n\n## Start\n\n[Architecture](../../docs/architecture.md#ownership)\n\n<!-- noon:demo showcase-first-scene -->\n')
        self.write('docs/site/api.md', '# API\n\n<!-- noon:python-index -->\n')
        self.write('docs/site/examples.md', '# Examples\n\nFixture examples.\n')
        self.write('docs/architecture.md', '# Architecture\n\n## Ownership\n\nTest fixture, not project documentation.\n\n![Diagram](diagrams/overview.svg)\n')
        self.write('docs/diagrams/overview.svg', '<svg xmlns="http://www.w3.org/2000/svg"></svg>')
        self.write('web/index.html', '<!doctype html><html lang="en"><head></head><body><script type="module" src="./python-editor-bootstrap.js"></script><script type="module" src="./main.js"></script></body></html>')
        self.site = self.root / '_site'
        self.site.mkdir()
        for name in ('index.html','playground.html','site/theme-init.js','site/docs.css','site/docs.js','python/examples/first.py','thumbnails/first.png'):
            p = self.site / name; p.parent.mkdir(parents=True, exist_ok=True); p.write_text('fixture')
        rust = self.root / 'rustdoc'
        for name in ('noon','noon_native','noon_export'):
            self.write(f'rustdoc/{name}/index.html','<html><body>TEST RUSTDOC FIXTURE</body></html>')
        self.write('rustdoc/noon-site-source.json',json.dumps({'revision':REVISION,'packages':['noon','noon-native','noon-export']}))
        return Builder(self.root, self.site, REVISION), rust

    def test_full_fixture_build_checks_sources_links_and_coverage(self):
        builder, rust = self.builder()
        builder.build(rust)
        result = builder.out
        self.assertTrue((result/'python-Scene.html').is_file())
        self.assertIn('aria-current="page"',(result/'index.html').read_text())
        self.assertIn('architecture.html#ownership',(result/'index.html').read_text())
        self.assertIn('assets/docs/diagrams/overview.svg',(result/'architecture.html').read_text())
        self.assertIn('Noon documentation',(result/'rust/noon/index.html').read_text())
        self.assertNotIn('<iframe',(result/'index.html').read_text())
        self.assertIn('data-frame-src',(result/'index.html').read_text())
        self.assertIn('&lt;unsafe&gt;',(result/'python-Shape.html').read_text())
        provenance = json.loads((result/'build-provenance.json').read_text())
        self.assertEqual(provenance['source_sha256']['docs/architecture.md'], __import__('hashlib').sha256((self.root/'docs/architecture.md').read_bytes()).hexdigest())

    def test_missing_fragment_blocks_publication(self):
        builder, rust = self.builder()
        p = self.root/'docs/site/index.md';p.write_text(p.read_text().replace('#ownership','#not-real'))
        with self.assertRaisesRegex(ValueError, 'Broken fragment'): builder.build(rust)

    def test_stale_rustdoc_blocks_publication(self):
        builder, rust = self.builder()
        stamp = rust/'noon-site-source.json'
        stamp.write_text(stamp.read_text().replace(REVISION,'b'*40))
        with self.assertRaisesRegex(ValueError, 'Rustdoc must'): builder.build(rust)

    def test_canonical_missing_source_blocks_publication(self):
        builder, rust = self.builder()
        (self.root/'docs/architecture.md').unlink()
        with self.assertRaises(FileNotFoundError): builder.build(rust)

    def test_unselected_example_rejected(self):
        builder, _ = self.builder()
        with self.assertRaisesRegex(ValueError, 'Unselected'): builder.demo('showcase-triangle-light')

    def test_heading_duplicates_and_html_escaping(self):
        builder, _ = self.builder()
        content, headings = builder.markdown('# Test\n\n## Same\n\n## Same\n\n<script>alert(1)</script>\n', self.root/'docs/site/index.md', builder.out/'index.html')
        self.assertEqual([h[1] for h in headings],['test','same','same-1'])
        self.assertNotIn('<script>',content)
        self.assertIn('&lt;script&gt;',content)

    def test_multipart_canonical_document_keeps_links_and_source(self):
        builder, rust = self.builder()
        source = self.root/'docs/architecture.md'
        source.write_text(source.read_text() + '\n# Roadmap\n\n## Ownership\n\n### Next\n\n###### Detail\n\n# Appendix\n\n## Notes\n')
        before = source.read_bytes()
        index = self.root/'docs/site/index.md'
        index.write_text(index.read_text() + '\n[Roadmap](../architecture.md#roadmap) and [later ownership](../architecture.md#ownership-1).\n')
        builder.build(rust)
        markup = (builder.out/'architecture.html').read_text()
        document = Links(); document.feed(markup)
        self.assertEqual(document.titles, 1)
        self.assertIn('<h2 id="roadmap">', markup)
        self.assertIn('<h3 id="ownership-1">', markup)
        self.assertIn('<h4 id="next">', markup)
        self.assertIn('<h6 id="detail">', markup)
        self.assertIn('<h2 id="appendix">', markup)
        self.assertIn('<h3 id="notes">', markup)
        self.assertIn('href="#roadmap" class="toc-level-2"', markup)
        self.assertIn('href="#ownership-1" class="toc-level-3"', markup)
        self.assertEqual(source.read_bytes(), before)
        records = json.loads((builder.out/'search-index.json').read_text())
        self.assertTrue(any(row['url'] == 'architecture.html#roadmap' for row in records))
        self.assertTrue(any(row['url'] == 'architecture.html#ownership-1' for row in records))

    def test_missing_title_blocks_generated_page(self):
        builder, rust = self.builder()
        self.write('docs/site/examples.md', '## Not a page title\n\nExample.\n')
        with self.assertRaisesRegex(ValueError, 'one page title in examples.html; found 0'):
            builder.build(rust)

    def test_duplicate_title_blocks_generated_page(self):
        builder, rust = self.builder()
        builder.build(rust)
        page = builder.out/'python-Scene.html'
        page.write_text(page.read_text().replace('</main>', '<h1>Accidental second title</h1></main>'))
        with self.assertRaisesRegex(ValueError, 'one page title in python-Scene.html; found 2'):
            builder.check_links()

    def test_escape_and_javascript_urls_rejected(self):
        builder, _ = self.builder()
        renderer = SiteMarkdown(builder,self.root/'docs/architecture.md',builder.out/'architecture.html')
        with self.assertRaises(ValueError): renderer.local_url('../../outside.png', image=True)
        with self.assertRaises(ValueError): renderer.local_url('javascript:alert(1)')

    def test_canonical_html_links_keep_project_prefix_query_and_fragment(self):
        builder, _ = self.builder()
        self.write('web/background-audio.html', '<html></html>')
        renderer = SiteMarkdown(builder, self.root/'web/README.md', builder.out/'web-runtime.html')
        self.assertEqual(renderer.local_url('background-audio.html?mode=demo#player'), '../../background-audio.html?mode=demo#player')
        self.assertEqual(renderer.local_url('index.html?example=first#editor'), '../../playground.html?example=first#editor')

    def test_missing_canonical_html_is_not_mistaken_for_a_generated_route(self):
        builder, _ = self.builder()
        renderer = SiteMarkdown(builder, self.root/'web/README.md', builder.out/'web-runtime.html')
        with self.assertRaises(FileNotFoundError): renderer.local_url('missing.html')

    def test_embed_reuses_main_and_removes_optional_editor_bootstrap(self):
        builder, _ = self.builder()
        before = (self.root/'web/index.html').read_bytes()
        build_embed(self.root, self.site)
        embed = (self.site/'embed.html').read_text()
        self.assertNotIn('python-editor-bootstrap.js',embed)
        self.assertIn('src="./main.js"',embed)
        self.assertIn('src="./site/embed-host.js"',embed)
        self.assertEqual(before,(self.root/'web/index.html').read_bytes())


if __name__ == '__main__': unittest.main()
