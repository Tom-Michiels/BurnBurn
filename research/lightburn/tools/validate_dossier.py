#!/usr/bin/env python3
"""Check local links, source indexes, fixture hashes and JSON syntax offline."""
import hashlib,json,re
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
errors=[]
for p in ROOT.rglob('*.json'):
    try:json.loads(p.read_text())
    except Exception as exc:errors.append(f'{p.relative_to(ROOT)}: {exc}')
for p in ROOT.rglob('*.md'):
    for dest in re.findall(r'\]\(([^)]+)\)',p.read_text()):
        if dest.startswith(('https://','http://','#','mailto:')):continue
        target=(p.parent/dest.split('#')[0]).resolve()
        if not target.exists():errors.append(f'{p.relative_to(ROOT)}: missing link {dest}')
manifest=ROOT/'evidence/fixtures-manifest.json'
if manifest.exists():
    for r in json.loads(manifest.read_text()):
        p=ROOT/r['path']
        if hashlib.sha256(p.read_bytes()).hexdigest()!=r['sha256']:errors.append(f'fixture hash mismatch: {r["path"]}')
menus=json.loads((ROOT/'reference/menu-index.json').read_text())
topics=json.loads((ROOT/'reference/feature-index.json').read_text())
if len(set(r['id'] for r in menus))!=len(menus):errors.append('duplicate menu IDs')
if len(set(r['id'] for r in topics))!=len(topics):errors.append('duplicate feature IDs')
print(f'Checked {len(list(ROOT.rglob("*.md")))} Markdown files, {len(list(ROOT.rglob("*.json")))} JSON files, {len(menus)} menu items, {len(topics)} reference topics.')
if errors:
    print('\n'.join(errors));raise SystemExit(1)
print('PASS: local links, JSON, IDs and fixture hashes.')
