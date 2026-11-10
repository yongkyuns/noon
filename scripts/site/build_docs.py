#!/usr/bin/env python3
"""Build the documentation site from canonical sources and exported APIs.

No network access or runtime imports. Missing sources, stale Rustdoc and broken
local links fail the build. Authoring/runtime/renderer code is not duplicated.
"""
from __future__ import annotations

import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
from html.parser import HTMLParser
from urllib.parse import quote, unquote, urlsplit

import mistune
from python_reference import PythonReference

SCRIPT = Path(__file__).resolve().parent
REPOSITORY = 'https://github.com/yongkyuns/noon'
E = html.escape


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def plain(text):
    return html.unescape(re.sub(r'<[^>]*>', '', text)).strip()


def slug(text):
    return re.sub(r'[^\w\- ]', '', plain(text).lower()).replace(' ', '-') or 'section'


def relative(target, current):
    return Path(os.path.relpath(target, current.parent)).as_posix()


class SiteMarkdown(mistune.HTMLRenderer):
    def __init__(self, builder, source: Path, destination: Path):
        super().__init__(escape=True)
        self.builder, self.source, self.destination = builder, source, destination
        self.headings, self.ids = [], set()

    def heading(self, text, level, **attrs):
        identifier, index = slug(text), 0
        base = identifier
        while identifier in self.ids:
            index += 1
            identifier = f'{base}-{index}'
        self.ids.add(identifier)
        self.headings.append((level, identifier, plain(text)))
        return f'<h{level} id="{E(identifier)}">{text}<a class="heading-anchor" href="#{E(identifier)}" aria-label="Link to {E(plain(text))}">#</a></h{level}>\n'

    def local_url(self, url, image=False):
        split = urlsplit(html.unescape(url))
        # Canonical repository links stay in-site when that document is published.
        prefix = REPOSITORY + '/blob/'
        if url.startswith(prefix):
            rest = split.path.split('/blob/', 1)[1].split('/', 1)
            if len(rest) == 2 and rest[1] in self.builder.routes:
                path = self.builder.root / rest[1]
            else:
                return url.replace('/blob/master/', f'/blob/{self.builder.revision}/')
        elif split.scheme or split.netloc or not split.path:
            if split.scheme and split.scheme.lower() not in ('http', 'https', 'mailto'):
                raise ValueError(f'Unsupported link scheme in {self.source}: {url}')
            return url.replace(REPOSITORY + '/tree/master/', REPOSITORY + f'/tree/{self.builder.revision}/')
        elif split.path.endswith('.html') and self.source.parent == self.builder.root / 'docs/site':
            # The authored guides use explicit generated routes. Canonical source
            # documents instead resolve HTML relative to their repository path.
            return url
        else:
            path = (self.source.parent / unquote(split.path)).resolve()
        try:
            repo_path = path.relative_to(self.builder.root).as_posix()
        except ValueError as exc:
            raise ValueError(f'Link escapes source root: {self.source}: {url}') from exc
        if not path.exists():
            raise FileNotFoundError(f'{self.source}: missing link target {url}')
        suffix = (f'?{split.query}' if split.query else '') + (f'#{split.fragment}' if split.fragment else '')
        if repo_path.startswith('web/') and path.suffix.lower() == '.html':
            target = self.builder.site / repo_path.removeprefix('web/')
            if repo_path == 'web/index.html':
                target = self.builder.site / 'playground.html'
            return relative(target, self.destination) + suffix
        if repo_path in self.builder.routes:
            target = self.builder.out / self.builder.routes[repo_path]
            return relative(target, self.destination) + suffix
        if image or path.suffix.lower() in ('.svg', '.png', '.jpg', '.jpeg', '.webp', '.gif', '.d2'):
            target = self.builder.out / 'assets' / repo_path
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, target)
            self.builder.inputs[repo_path] = sha(path)
            return relative(target, self.destination)
        # Keep code/edit links pinned to the same source revision.
        kind = 'tree' if path.is_dir() else 'blob'
        return f'{REPOSITORY}/{kind}/{self.builder.revision}/{quote(repo_path)}' + (f'#{split.fragment}' if split.fragment else '')

    def link(self, text, url, title=None):
        return super().link(text, self.local_url(url), title)

    def image(self, text, url, title=None):
        return super().image(text, self.local_url(url, image=True), title)

    def block_code(self, code, info=None):
        language = (info or '').split()[0] if info else ''
        # Highlighting is optional; escaped source is the authority.
        return f'<pre tabindex="0"><code class="language-{E(language)}">{E(code)}</code></pre>\n'


class Links(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ids, self.links = set(), []
    def handle_starttag(self, tag, pairs):
        attrs = dict(pairs)
        if 'id' in attrs:
            if attrs['id'] in self.ids:
                raise ValueError(f'Duplicate HTML id: {attrs["id"]}')
            self.ids.add(attrs['id'])
        for attribute in ('href', 'src', 'poster'):
            if attrs.get(attribute): self.links.append((tag, attribute, attrs[attribute]))


def build_embed(root: Path, site: Path):
    """Reuse the original shell, main.js and its teardown; no editor bootstrap."""
    markup = (root / 'web/index.html').read_text(encoding='utf-8')
    bootstrap = '<script type="module" src="./python-editor-bootstrap.js"></script>'
    main = '<script type="module" src="./main.js"></script>'
    if (markup.count(bootstrap) != 1 or markup.count(main) != 1 or
            markup.count('</head>') != 1 or markup.count('<html lang="en">') != 1):
        raise ValueError('Unexpected playground shell: re-review embed generation')
    markup = markup.replace(bootstrap, '')
    markup = markup.replace('<html lang="en">', '<html lang="en" data-noon-embed="true">', 1)
    markup = markup.replace('</head>', '<script src="./site/theme-init.js"></script><link rel="stylesheet" href="./site/embed.css"></head>', 1)
    # A classic script runs after the DOM is parsed, before the module initializes.
    markup = markup.replace(main, '<script src="./site/embed-host.js"></script>\n' + main +
        '<script type="module">import {installTheme} from "./site/theme.js"; installTheme();</script>', 1)
    (site / 'embed.html').write_text(markup, encoding='utf-8')


class Builder:
    def __init__(self, root: Path, site: Path, revision: str):
        self.root, self.site = root.resolve(), site.resolve()
        self.out = self.site / 'site/docs'
        self.revision = revision
        if not re.fullmatch('[a-f0-9]{40}', revision):
            raise ValueError('An exact Git source revision is required')
        self.navigation = json.loads((self.root / 'scripts/site/navigation.json').read_text())
        self.routes = {item['source']: item['slug'] + '.html' for item in self.navigation}
        self.inputs, self.search, self.generated, self.heading_map = {}, [], [], {}
        manifest = json.loads((self.root / 'web/python/examples/noon_showcase_manifest.json').read_text())
        self.examples = {item['id']: item for item in manifest['entries']}
        self.selected = {item['id'] for item in json.loads((self.root / 'web/site/examples.json').read_text())}
        self.api = PythonReference(self.root / 'web/python').build()
        self.inputs.update(self.api['source_sha256'])

    def source_link(self, path, line=None):
        return f'{REPOSITORY}/blob/{self.revision}/{quote(path)}' + (f'#L{line}' if line else '')

    def markdown(self, text, source, destination):
        renderer = SiteMarkdown(self, source, destination)
        parser = mistune.create_markdown(renderer=renderer, plugins=['table', 'strikethrough', 'url'])
        content = parser(text)
        return content, renderer.headings

    def include(self, path):
        source = (self.root / path).resolve()
        source.relative_to(self.root)
        if source.suffix not in ('.py', '.rs'):
            raise ValueError('Example includes are restricted to Python/Rust source')
        self.inputs[path] = sha(source)
        language = 'python' if source.suffix == '.py' else 'rust'
        text = source.read_text(encoding='utf-8')
        fence = '`' * max(3, 1 + max((len(m[0]) for m in re.finditer(r'`+', text)), default=0))
        return f'\n{fence}{language}\n{text.rstrip()}\n{fence}\n'

    def demo(self, identifier):
        if identifier not in self.selected or identifier == 'showcase-triangle-light':
            raise ValueError(f'Unselected documentation demo: {identifier}')
        entry = self.examples[identifier]
        full = f'../../playground.html?catalog=showcase&amp;example={identifier}'
        frame = f'../../embed.html?catalog=showcase&amp;example={identifier}'
        image = '../../' + entry['thumbnail']
        return f'''<figure class="demo-card" data-demo="{identifier}" data-frame-src="{frame}">
<img class="demo-poster" src="{E(image)}" alt="Captured frame: {E(entry['title'])}" loading="lazy" width="960" height="540">
<div class="demo-actions"><button type="button" data-demo-open>Open interactive preview</button>
<button type="button" data-demo-close hidden>Close preview</button><a href="{full}">Open full editor</a>
<a href="../../{E(entry['path'])}">Python source</a></div>
<div data-demo-frame></div><figcaption data-demo-status role="status">Captured poster. No runtime loaded.</figcaption>
<noscript><p>JavaScript is disabled. Open the full editor to run this example.</p></noscript></figure>'''

    def api_index(self):
        rows = []
        for item in self.api['entries']:
            name = item['name']
            rows.append(f'<tr><td><a href="python-{quote(name)}.html"><code>{E(name)}</code></a></td><td>{item["kind"]}</td><td><code>{E(item["module"])}</code></td></tr>')
        unresolved = ''.join(f'<li><code>{E(n["name"])}</code>: {E(n["reason"])}</li>' for n in self.api['unresolved'])
        return f'''<p>{self.api['resolved_count']} of {self.api['export_count']} explicit exports resolved to local source. <a href="python-coverage.json">Machine-readable coverage</a>.</p>
<table><thead><tr><th>Export</th><th>Kind</th><th>Source module</th></tr></thead><tbody>{''.join(rows)}</tbody></table>
<details><summary>External or dynamic exports ({len(self.api['unresolved'])})</summary><ul>{unresolved}</ul></details>'''

    def document(self, entry):
        source = self.root / entry['source']
        self.inputs[entry['source']] = sha(source)
        text = source.read_text(encoding='utf-8')
        text = re.sub(r'<!-- noon:source ([\w/.-]+) -->', lambda m: self.include(m[1]), text)
        # Markers are replaced after Markdown parsing; raw source HTML stays escaped.
        demos = re.findall(r'<!-- noon:demo ([\w-]+) -->', text)
        for identifier in demos:
            text = text.replace(f'<!-- noon:demo {identifier} -->', f'NOONDEMO{identifier}END')
        text = text.replace('<!-- noon:python-index -->', 'NOONPYTHONINDEXEND')
        destination = self.out / (entry['slug'] + '.html')
        content, headings = self.markdown(text, source, destination)
        for identifier in demos:
            content = content.replace(f'<p>NOONDEMO{identifier}END</p>', self.demo(identifier))
        content = content.replace('<p>NOONPYTHONINDEXEND</p>', self.api_index())
        self.write_page(entry['slug'], entry['title'], content, headings, entry['source'])

    def shell(self, key, title, content, headings, source):
        current = 'api' if key.startswith('python-') else key
        nav, group = [], None
        for item in self.navigation:
            if item['group'] != group:
                group = item['group']
                nav.append(f'<p class="nav-group">{E(group)}</p>')
            active = ' aria-current="page"' if item['slug'] == current else ''
            nav.append(f'<a href="{item["slug"]}.html"{active}>{E(item["title"])}</a>')
        toc = ''.join(f'<a href="#{E(identifier)}" class="toc-level-{level}">{E(label)}</a>'
                      for level, identifier, label in headings if 1 < level <= 3)
        previous_next = ''
        keys = [n['slug'] for n in self.navigation]
        if key in keys:
            i = keys.index(key)
            adjacent = [(i-1, 'Previous'), (i+1, 'Next')]
            previous_next = '<nav class="reading-order" aria-label="Reading order">' + ''.join(
                f'<a rel="{"prev" if index < i else "next"}" href="{self.navigation[index]["slug"]}.html"><span>{label}</span>{E(self.navigation[index]["title"])}</a>'
                for index, label in adjacent if 0 <= index < len(keys)) + '</nav>'
        source_url = self.source_link(source) if source else f'{REPOSITORY}/tree/{self.revision}'
        return f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="color-scheme" content="light dark"><meta name="description" content="{E(title)} — Noon documentation for source revision {self.revision[:7]}.">
<title>{E(title)} — Noon</title><script src="../theme-init.js"></script><link rel="stylesheet" href="../docs.css">
<script type="module" src="../docs.js"></script></head><body>
<a class="skip-link" href="#main">Skip to content</a>
<header class="site-header"><a class="wordmark" href="../../index.html" aria-label="Noon home">noon<span>.</span></a>
<nav aria-label="Main navigation"><a href="index.html">Docs</a><a href="examples.html">Examples</a><a href="api.html">API</a><a href="architecture.html">Architecture</a><a href="../../playground.html?catalog=showcase">Playground</a></nav>
<label class="theme-label">Theme<select data-theme-choice aria-label="Color theme"><option value="system">System</option><option value="light">Light</option><option value="dark">Dark</option></select></label></header>
<div class="docs-shell"><aside class="docs-sidebar"><details open class="docs-navigation"><summary>Documentation navigation</summary>
<form action="search.html" role="search"><label for="docs-search">Find a topic or API</label><div class="search-box"><input id="docs-search" name="q" type="search" placeholder="Search documentation"><button>Search</button></div></form>
<nav aria-label="Documentation">{''.join(nav)}</nav></details></aside>
<main id="main" class="docs-content"><nav class="breadcrumb" aria-label="Breadcrumb"><a href="../../index.html">Noon</a><span>/</span><a href="index.html">Documentation</a><span>/</span><span aria-current="page">{E(title)}</span></nav>
{'<details class="page-contents" open><summary>On this page</summary><nav aria-label="On this page">'+toc+'</nav></details>' if toc else ''}
{content}{previous_next}<footer class="source-note">Built from <code>{self.revision[:12]}</code> · <a href="{source_url}">View source</a> · <a href="#main">Back to top</a></footer></main></div></body></html>'''

    def write_page(self, key, title, content, headings, source=None):
        destination = self.out / (key + '.html')
        destination.write_text(self.shell(key, title, content, headings, source), encoding='utf-8')
        self.generated.append(destination)
        self.heading_map[destination.name] = headings
        self.search.append({'title': title, 'url': destination.name, 'text': plain(content)[:3000]})
        for level, identifier, label in headings:
            if level > 1: self.search.append({'title': f'{title} / {label}', 'url': f'{destination.name}#{identifier}', 'text': label})

    def callable_html(self, member):
        params = ''.join(f'<tr><td><code>{E(p["name"])}</code></td><td>{E(p["kind"])}</td><td><code>{E(p["annotation"] or "Not annotated")}</code></td><td><code>{E(p["default"] if p["default"] is not None else "No default")}</code></td></tr>' for p in member['parameters'])
        doc = '<pre class="docstring">' + E(member['doc']) + '</pre>' if member['doc'] else '<p>No docstring declared here. Consult the source and host reference.</p>'
        source = self.source_link(f'web/python/{member["module"]}.py', member['line'])
        decorators = '<p>Decorators: <code>' + E(', '.join(member['decorators'])) + '</code></p>' if member['decorators'] else ''
        return f'''<pre tabindex="0"><code>{E(member['signature'])}</code></pre>{decorators}
<table><thead><tr><th>Parameter</th><th>Kind</th><th>Annotation</th><th>Default</th></tr></thead><tbody>{params}</tbody></table>
<p>Return annotation: <code>{E(member['returns'] or 'Not annotated')}</code></p>{doc}<p><a href="{source}">Source declaration</a></p>'''

    def python_pages(self):
        for item in self.api['entries']:
            name, kind = item['name'], item['kind']
            content = f'<h1 id="api-symbol">{E(name)}</h1><p><code>from noon import {E(name)}</code></p><p class="callout">Source-derived {kind}. Host availability is not inferred from this listing. See <a href="api.html">reference scope</a>.</p>'
            headings = []
            if kind == 'function':
                content += self.callable_html(item)
            elif kind == 'class':
                content += f'<p>Bases: <code>{E(", ".join(item["bases"]) or "object")}</code>. Declared members follow; inherited and dynamically attached members are not expanded.</p>'
                content += '<pre class="docstring">' + E(item['doc']) + '</pre>' if item['doc'] else ''
                for index, member in enumerate(item['members']):
                    identifier = f'member-{index}'
                    headings.append((2, identifier, member['name']))
                    content += f'<h2 id="{identifier}">{E(member["name"])}</h2>' + self.callable_html(member)
            else:
                content += f'<pre tabindex="0"><code>{E(item["expression"])}</code></pre><p>This binding was not evaluated by the documentation builder.</p>'
            self.write_page('python-' + name, name, content, headings, f'web/python/{item["module"]}.py')
        (self.out / 'python-coverage.json').write_text(json.dumps(self.api, indent=2) + '\n')

    def rustdoc(self, directory):
        directory = directory.resolve()
        stamp = json.loads((directory / 'noon-site-source.json').read_text())
        if stamp.get('revision') != self.revision or stamp.get('packages') != ['noon', 'noon-native', 'noon-export']:
            raise ValueError('Rustdoc must be generated for this revision and the documented package set')
        for crate in ('noon', 'noon_native', 'noon_export'):
            if not (directory / crate / 'index.html').is_file():
                raise ValueError(f'Missing generated Rust API for {crate}')
        target = self.out / 'rust'
        shutil.copytree(directory, target, dirs_exist_ok=True)
        for page in target.rglob('*.html'):
            markup = page.read_text()
            back = relative(self.out / 'api.html', page)
            banner = f'<aside style="padding:0.6rem 1rem;font-family:system-ui"><a href="{back}">← Noon documentation</a> · source {self.revision[:12]}</aside>'
            markup, count = re.subn(r'(<body\b[^>]*>)', lambda m: m[0] + banner, markup, count=1)
            if count: page.write_text(markup)

    def search_page(self):
        # All topics are present without JS; JS progressively filters this index.
        records = list(self.search)
        rows = ''.join(f'<li data-search-item data-search-text="{E((r["title"]+" "+r["text"])[:18000])}"><a href="{E(r["url"])}">{E(r["title"])}</a></li>' for r in records)
        content = '<h1 id="search">Documentation index</h1><p>Search by topic or symbol. With JavaScript disabled, use your browser’s Find command on this complete index.</p><p data-search-status role="status"></p><ul class="topic-index">'+rows+'</ul>'
        self.write_page('search', 'Documentation index', content, [], 'scripts/site/navigation.json')
        (self.out / 'search-index.json').write_text(json.dumps(records, ensure_ascii=False) + '\n')

    def check_links(self):
        cache = {}
        def parsed(path):
            if path not in cache:
                p = Links(); p.feed(path.read_text()); cache[path] = p
            return cache[path]
        for page in self.generated:
            for tag, attr, url in parsed(page).links:
                split = urlsplit(url)
                if split.scheme or split.netloc: continue
                target = (page.parent / unquote(split.path)).resolve() if split.path else page
                if target.is_dir(): target = target / 'index.html'
                try: target.relative_to(self.site)
                except ValueError as exc: raise ValueError(f'Link escapes staged site: {page}: {url}') from exc
                if not target.is_file(): raise ValueError(f'Broken local link in {page.name}: {url}')
                if split.fragment and target.suffix == '.html' and target in self.generated:
                    if unquote(split.fragment) not in parsed(target).ids:
                        raise ValueError(f'Broken fragment in {page.name}: {url}')

    def build(self, rustdoc):
        # Validate input existence before deleting any old generated docs.
        for item in self.navigation:
            if not (self.root / item['source']).is_file(): raise FileNotFoundError(item['source'])
        if self.out.exists(): shutil.rmtree(self.out)
        self.out.mkdir(parents=True)
        build_embed(self.root, self.site)
        for item in self.navigation: self.document(item)
        self.python_pages()
        self.rustdoc(rustdoc)
        self.search_page()
        self.check_links()
        report = {'revision': self.revision, 'pages': len(self.generated), 'source_sha256': self.inputs,
                  'python': {k:self.api[k] for k in ('export_count', 'resolved_count', 'unresolved', 'scope')},
                  'rustdoc': 'noon, noon-native, noon-export; default Cargo feature profile; --no-deps',
                  'renderer': f'mistune {mistune.__version__}'}
        (self.out / 'build-provenance.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps({k:report[k] for k in ('revision', 'pages', 'renderer')}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--site', type=Path, required=True)
    parser.add_argument('--rustdoc', type=Path, required=True)
    args = parser.parse_args()
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=args.root, text=True).strip()
    if mistune.__version__ != '3.2.1': raise RuntimeError('Install scripts/site/requirements.txt for the pinned Markdown parser')
    Builder(args.root, args.site, revision).build(args.rustdoc)


if __name__ == '__main__': main()
