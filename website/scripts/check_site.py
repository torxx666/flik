#!/usr/bin/env python3
"""Validate the static deployment and its privacy-sensitive content without network."""
from html.parser import HTMLParser
from pathlib import Path
import json, re, subprocess
root = Path(__file__).resolve().parents[1] / 'dist'
class Page(HTMLParser):
    def __init__(self): super().__init__(); self.links = []; self.ids = set(); self.copy_targets = []
    def handle_starttag(self, tag, attrs):
        data = dict(attrs)
        if tag == 'html': assert data.get('lang') == 'en', 'Page must be English before JavaScript runs'
        if tag == 'track': assert data.get('srclang') == 'en', 'Captions must be English'
        assert data.get('id') != 'language', 'English-only site must not offer a French toggle'
        if 'data-copy' in data: self.copy_targets.append(data['data-copy'])
        if 'id' in data:
            assert data['id'] not in self.ids, f'Duplicate id: {data["id"]}'
            self.ids.add(data['id'])
        for attr in ('src', 'href', 'poster'):
            if data.get(attr): self.links.append(data[attr])
page = Page(); page.feed((root / 'index.html').read_text())
for required in ('hero-video', 'hero-playback', 'hero-fullscreen'):
    assert required in page.ids, f'Missing walkthrough control: {required}'
for section in ('formats', 'workflow', 'demos', 'examples', 'shortcuts', 'install'):
    assert section in page.ids, f'Missing product section: {section}'
for target in page.copy_targets:
    assert target in page.ids, f'Missing copy-command target: {target}'
assert len(page.copy_targets) >= 8, 'Workflow examples need usable copy commands'
for link in page.links:
    if link.startswith('#'):
        assert link == '#' or link[1:] in page.ids, f'Missing anchor: {link}'
    elif not re.match(r'https?://', link):
        assert (root / link).is_file(), f'Missing asset: {link}'
for demo in ('kubernetes-json', 'smart-find', 'apache-levels', 'hourly-frequency'):
    for suffix in ('.mp4', '.png', '.vtt', '.en.vtt'):
        assert (root / 'assets' / 'demos' / (demo + suffix)).is_file()
for style in root.rglob('*.css'):
    for link in re.findall(r'url\([\'"]?([^\)\'"]+)', style.read_text()):
        assert (style.parent / link).is_file(), f'Missing CSS asset: {link}'
for path in root.rglob('*'):
    assert not path.is_symlink(), f'Unexpected symlink: {path}'
    if path.is_file() and path.suffix in ('.html', '.js', '.json', '.css', '.vtt', '.log', '.svg'):
        text = path.read_text()
        for pattern in (r'/Users/[^\s]+', r'/home/[^\s]+', r'(?i)(?:ghp_|github_pat_)[a-z0-9_]+'):
            assert not re.search(pattern, text), f'Private data pattern in {path}'
for path in [root / 'index.html', root / 'app.js', *(root / 'assets' / 'demos').glob('*.vtt')]:
    text = path.read_text().lower()
    for phrase in ('données fictives', 'déclic', 'français', 'retrouver le contexte', 'des statistiques', 'appuyer sur', 'les erreurs'):
        assert phrase not in text, f'Non-English public copy in {path}: {phrase}'
config = json.loads((root / 'site-config.json').read_text())
assert config['repository'] is None or re.fullmatch(r'[-\w.]+/[-\w.]+', config['repository'])
subprocess.run(['node', '--check', str(root / 'app.js')], check=True)
print('OK: static assets, internal links, captions, JS syntax and private-path/token checks, English-only content')
