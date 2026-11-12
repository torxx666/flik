"""Reuse the homepage header and footer for generated content pages."""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]

def shared_shell(document):
    home = (ROOT / 'dist/index.html').read_text()
    header = re.search(r'<div class="release-strip">.*?</header>', home, re.S).group()
    footer = re.search(r'<footer class="footer wrap">.*?</footer>', home, re.S).group()
    def links(fragment):
        def absolute(match):
            attr, value = match.groups()
            if not value.startswith(('/', 'https:', 'http:')):
                value = '/' + value
            return attr + '="' + value + '"'
        return re.sub(r'(href|src)="([^"]*)"', absolute, fragment)
    head = document.split('<head>', 1)[1].split('</head>', 1)[0]
    head = re.sub(r'<link rel="stylesheet"[^>]*>', '', head)
    head += '<link rel="stylesheet" href="/style.css"><link rel="stylesheet" href="/assets/articles.css">'
    content = re.search(r'<main id="main">(.*?)</main>', document, re.S).group(1)
    return '<!doctype html><html lang="en"><head>' + head + '</head><body><a class="skip" href="#main">Skip to content</a>' + links(header) + '<main id="main" class="article-page wrap">' + content + '</main>' + links(footer) + '</body></html>'
