#!/usr/bin/env python3
"""Finds Mixkit clips under the Stock Video Free License (commercial use, no attribution) in the
given categories, reading each clip's own license label: most clips are Restricted (personal use
only) and can't be used in a product's demo.

  python3 video/data/mixkit-scan.py singapore,elderly,...   (prints free clips; writes mixkit-scan.json)
"""
import json
import re
import subprocess
import sys
import time
from pathlib import Path

UA = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0 Safari/537.36'


def get(u):
    return subprocess.run(['curl', '-s', '-m', '25', '-A', UA, u], capture_output=True, text=True).stdout


dest = Path(__file__).with_name('mixkit-scan.json')
found = json.loads(dest.read_text()) if dest.exists() else {}
for q in sys.argv[1].split(','):
    html = get(f'https://mixkit.co/free-stock-video/{q}/')
    items = []
    for slug, i in re.findall(r'href="/free-stock-video/([a-z0-9-]+)-(\d+)/"', html):
        if (slug, i) not in items:
            items.append((slug, i))
    for slug, i in items[:24]:
        if i in found:
            continue
        page = get(f'https://mixkit.co/free-stock-video/{slug}-{i}/')
        lic = re.findall(r'data-license="(video[A-Za-z]+)"', page)
        found[i] = {'slug': slug, 'q': q, 'license': lic[0] if lic else '?'}
        time.sleep(0.2)
    dest.write_text(json.dumps(found, indent=1))
    free = [f"{k}:{v['slug'][:50]}" for k, v in found.items() if v['q'] == q and v['license'] == 'videoFree']
    print(q, f'({len(free)} free)', ' ; '.join(free), flush=True)
