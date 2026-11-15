from html import unescape
from pathlib import Path
from types import SimpleNamespace
import re
import unittest
from presentation import code_block, render_shell, learning_cards

class PresentationTests(unittest.TestCase):
    def builder(self):
        return SimpleNamespace(revision='a'*40,navigation=[
            {'slug':'index','title':'Introduction','group':'Start here'},
            {'slug':'getting-started','title':'Your first animation','group':'Start here'},
            {'slug':'api','title':'API reference','group':'Reference'}],
            source_link=lambda path:'https://example.test/'+path)

    def test_code_highlighting_preserves_exact_source_and_escapes_html(self):
        for language,source in [('python','from noon import *\nprint("<script>x</script>")\n'),('rust','fn main() { let x = 3; }'),('sh','echo "a & b"\n'),('text','a\t b\n\n')]:
            with self.subTest(language=language):
                markup=code_block(source,language)
                code=re.search(r'<code[^>]*>(.*?)</code>',markup,re.S)[1]
                self.assertEqual(unescape(re.sub('<[^>]+>','',code)),source)
                self.assertNotIn('<script>',markup)
                self.assertIn('data-copy-code hidden',markup)
                self.assertIn('role="status"',markup)
        self.assertIn('class="kn"',code_block('from noon import *\n','python'))
        self.assertIn('class="k"',code_block('fn main() {}','rust'))

    def test_unknown_language_is_escaped_not_evaluated(self):
        value='<img src=x onerror=evil()>'
        self.assertNotIn('<img',code_block(value,'anything'))

    def test_shell_preserves_anchor_content_and_single_title(self):
        body='<h1 id="title">My title</h1>\n<p>Original introduction.</p>\n<h2 id="next">Next</h2>\n<p>Original body.</p>'
        markup=render_shell(self.builder(),'getting-started','My title',body,[(1,'title','My title'),(2,'next','Next')],'guide.md')
        self.assertEqual(len(re.findall('<h1\\b',markup)),1)
        self.assertIn('id="title"',markup)
        self.assertIn('id="next"',markup)
        self.assertIn('Original body.',markup)
        self.assertIn('class="lead">Original introduction.',markup)
        self.assertIn('class="page-contents"',markup)
        self.assertIn('class="contents-mobile"',markup)
        self.assertIn('href="getting-started.html" aria-current="page"',markup)
        self.assertIn('rel="prev"',markup)
        self.assertIn('rel="next"',markup)

    def test_all_navigation_exists_without_javascript(self):
        markup=render_shell(self.builder(),'index','Introduction','<h1>Introduction</h1>',[],None)
        for item in self.builder().navigation:self.assertIn(item['slug']+'.html',markup)
        self.assertIn('<details class="docs-navigation">',markup)
        self.assertIn('action="search.html"',markup)
        self.assertIn('aria-label="Color theme"',markup)

    def test_python_symbol_keeps_reference_section_selected(self):
        markup=render_shell(self.builder(),'python-Scene','Scene','<h1>Scene</h1>',[],None)
        self.assertIn('href="api.html" aria-current="page"',markup)

    def test_learning_cards_use_internal_routes_and_distinct_paths(self):
        markup=learning_cards('start')
        for target in ['getting-started.html','python.html','rust.html','examples.html']:self.assertIn('href="'+target+'"',markup)
        self.assertNotIn('<script',markup)
        with self.assertRaises(ValueError):learning_cards('untrusted')

if __name__ == '__main__':unittest.main()
