#!/usr/bin/env python3
"""Create a curated source ZIP without Git history, hosting identity or local logs."""
from pathlib import Path
import hashlib, re, zipfile
root = Path(__file__).resolve().parents[1]
version = re.search(r'^version\s*=\s*"([^"]+)"', (root / 'Cargo.toml').read_text(), re.MULTILINE).group(1)
output = root / 'artifacts' / f'flik-source-v{version}.zip'
output.parent.mkdir(exist_ok=True)
entries = ['.gitignore', '.github', 'Cargo.toml', 'Cargo.lock', 'LICENSE', 'README.md',
           'Dockerfile', 'docker-compose.yml', 'flik.sh', 'generate_live_logs.py',
           'src', 'tests', 'examples', 'scripts', 'docs', 'sample_logs', 'website']
excluded = {'.git', '.openai', '.DS_Store', '.sites-runtime', '.sites-checkout',
            'node_modules', '__pycache__', 'target'}
paths = []
for entry in entries:
    path = root / entry
    assert path.exists(), f'Missing public source entry: {entry}'
    paths.extend([path] if path.is_file() else path.rglob('*'))
with zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
    for path in sorted(set(paths)):
        relative = path.relative_to(root)
        if any(part in excluded or part.startswith('.sites-checkout-') for part in relative.parts): continue
        if path.is_dir(): continue
        assert not path.is_symlink(), f'Refusing a symlink: {relative}'
        data = path.read_bytes()
        if path.suffix in {'.rs', '.py', '.sh', '.md', '.yml', '.toml', '.json', '.log', '.html', '.css', '.js', '.txt'}:
            text = data.decode('utf-8')
            # These existing demo domains are placeholders, never links to a third-party site.
            text = text.replace('example.test', 'example.test')
            assert not re.search(r'(?i)(?:ghp_|github_pat_)[a-z0-9_]{20,}', text), f'Credential pattern: {relative}'
            assert not re.search(r'/Users/[A-Za-z0-9_.-]+/', text), f'Personal path: {relative}'
            data = text.encode()
        info = zipfile.ZipInfo(f'flik-{version}/{relative.as_posix()}')
        info.external_attr = (0o100755 if path.stat().st_mode & 0o111 else 0o100644) << 16
        info.compress_type = zipfile.ZIP_DEFLATED
        archive.writestr(info, data)
digest = hashlib.sha256(output.read_bytes()).hexdigest()
output.with_suffix('.zip.sha256').write_text(f'{digest}  {output.name}\n')
print(f'{output.relative_to(root)} — curated source ZIP with SHA-256')
