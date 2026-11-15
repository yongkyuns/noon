"""Static documentation presentation; source extraction and runtime stay elsewhere."""
from html import escape as E, unescape
import re
from itertools import groupby
from pygments import highlight
from pygments.lexers import get_lexer_by_name
from pygments.formatters import HtmlFormatter
from pygments.util import ClassNotFound


def code_block(source, language='text'):
    language = language if language in ('python','rust','bash','sh','shell','text','json','toml','javascript','js') else 'text'
    try:
        lexer = get_lexer_by_name(language, stripnl=False, ensurenl=False)
        painted = highlight(source, lexer, HtmlFormatter(nowrap=True))
        if not source.endswith('\n'):
            painted = painted.removesuffix('\n')
        # Never let presentation alter copyable code, including the last newline.
        raw = unescape(re.sub(r'<[^>]+>', '', painted))
        if raw != source:
            painted = E(source)
    except ClassNotFound:
        painted = E(source)
    label = {'sh':'Shell','bash':'Shell','python':'Python','rust':'Rust','text':'Source'}.get(language,language)
    return f'''<div class="code-block"><div class="code-toolbar"><span>{E(label)}</span><button type="button" class="copy-code" data-copy-code hidden aria-label="Copy {E(label)} code">Copy</button><span data-copy-status class="sr-only" role="status"></span></div><pre tabindex="0"><code class="language-{E(language)}">{painted}</code></pre></div>\n'''


PATHS = {
    'start': [
        ('getting-started.html','Start here','Your first animation','Run a scene in the browser, change a few lines, and see what happens.','No installation required'),
        ('python.html','Python','Continue with Python','Learn objects, grouping, and authored time. Run the same lesson natively.','Follow the walkthrough'),
        ('rust.html','Rust','Write a native scene','Author with ordinary Rust functions, typed handles, and synchronous control flow.','Follow the walkthrough'),
        ('examples.html','Explore','Learn from live examples','Open the source behind each scene and experiment without leaving the page.','Explore the examples'),
    ],
    'reference': [
        ('api.html','Look it up','API reference','Find Python classes and signatures or open generated Rust documentation.','Browse the reference'),
        ('architecture.html','Go deeper','Inside the engine','Understand scene ownership, execution, rendering, and the platform hosts.','Read the architecture'),
    ],
}


def learning_cards(kind):
    if kind not in PATHS:
        raise ValueError(f'Unknown learning path group: {kind}')
    return '<div class="learning-paths">' + ''.join(
        f'<a class="learning-card{" featured-path" if kind == "start" and i == 0 else ""}" href="{E(url)}"><span class="card-label">{E(label)}</span><strong>{E(title)}</strong><span class="card-desc">{E(description)}</span><span class="card-next">{E(next_step)} <span aria-hidden="true">→</span></span></a>'
        for i, (url,label,title,description,next_step) in enumerate(PATHS[kind])) + '</div>'


def render_shell(builder, key, title, content, headings, source):
    current = 'api' if key.startswith('python-') else key
    page = next((n for n in builder.navigation if n['slug'] == current), {})
    group = page.get('group', 'Reference' if current == 'api' else 'Documentation')
    nav = []
    for label, items in groupby(builder.navigation, key=lambda n:n['group']):
        nav.append(f'<p class="nav-group">{E(label)}</p>')
        for item in items:
            active = ' aria-current="page"' if item['slug'] == current else ''
            nav.append(f'<a href="{E(item["slug"])}.html"{active}>{E(item["title"])}</a>')
    toc = ''.join(f'<a href="#{E(identifier)}" class="toc-level-{level}">{E(label)}</a>'
                  for level,identifier,label in headings if 1 < level <= 3)
    # Keep source heading IDs and content unchanged. Only group the first title
    # and introductory paragraph into a readable page header.
    match = re.match(r'\s*(<h1\b.*?</h1>)(\s*<p>.*?</p>)?', content, re.S)
    if match:
        intro = (match[2] or '').replace('<p>', '<p class="lead">', 1)
        heading = f'<header class="page-heading"><p class="page-kicker">{E(group)}</p>{match[1]}{intro}</header>'
        content = heading + content[match.end():]
    keys = [n['slug'] for n in builder.navigation]
    adjacent = ''
    if key in keys:
        i = keys.index(key)
        adjacent = '<nav class="reading-order" aria-label="Reading order">' + ''.join(
            f'<a rel="{"prev" if j < i else "next"}" href="{E(builder.navigation[j]["slug"])}.html"><span>{label}</span>{E(builder.navigation[j]["title"])}</a>'
            for j,label in [(i-1,'← Previous'),(i+1,'Next →')] if 0 <= j < len(keys)) + '</nav>'
    help_link = '<a href="troubleshooting.html">Get help</a>' if 'troubleshooting' in keys else ''
    source_url = builder.source_link(source) if source else f'https://github.com/yongkyuns/noon/tree/{builder.revision}'
    description = page.get('description') or f'{title} — Noon documentation.'
    inpage = f'<details class="contents-mobile"><summary>On this page</summary><nav aria-label="On this page">{toc}</nav></details>' if toc else ''
    rail = f'<aside class="page-contents"><details open><summary>On this page</summary><nav aria-label="On this page">{toc}</nav></details></aside>' if toc else '<aside></aside>'
    return f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="color-scheme" content="light dark"><meta name="description" content="{E(description)}">
<title>{E(title)} — Noon</title><script src="../theme-init.js"></script><link rel="stylesheet" href="../docs.css"><link rel="stylesheet" href="../live.css">
<script type="module" src="../docs.js"></script></head><body>
<a class="skip-link" href="#main">Skip to content</a>
<header class="site-header"><div class="header-inner"><a class="wordmark" href="../../index.html" aria-label="Noon home">noon<span>.</span></a><span class="site-label">Documentation</span>
<nav class="primary-nav" aria-label="Main navigation"><a href="index.html" aria-current="page">Docs</a><a href="examples.html">Examples</a><a href="api.html">API</a><a href="../../playground.html?catalog=showcase">Playground</a></nav>
<div class="header-tools"><a class="search-link" href="search.html" data-search-shortcut>Search <kbd aria-hidden="true">⌘ K</kbd></a><label class="theme-label"><span class="sr-only">Theme</span><select data-theme-choice aria-label="Color theme"><option value="system">System</option><option value="light">Light</option><option value="dark">Dark</option></select></label></div></div></header>
<div class="docs-shell"><aside class="docs-sidebar"><details class="docs-navigation"><summary>Browse documentation</summary>
<form action="search.html" role="search"><label for="docs-search">Search topics and APIs</label><div class="search-box"><input id="docs-search" name="q" type="search" placeholder="Scene, export, text…"><button>Search</button></div></form>
<nav aria-label="Documentation">{''.join(nav)}</nav><p class="sidebar-note"><span class="experimental">Experimental</span><br>APIs and host coverage are evolving.</p></details></aside>
<main id="main" class="docs-content"><nav class="breadcrumb" aria-label="Breadcrumb"><a href="../../index.html">Noon</a><span aria-hidden="true">/</span><a href="index.html">Docs</a><span aria-hidden="true">/</span><span aria-current="page">{E(title)}</span></nav>
{inpage}{content}{adjacent}<footer class="source-note"><a href="{E(source_url)}">View page source ↗</a>{help_link}<a href="#main">Back to top ↑</a><span class="revision">Source <code>{builder.revision[:12]}</code></span></footer></main>{rail}</div></body></html>'''
