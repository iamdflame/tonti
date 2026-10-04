#!/usr/bin/env python3
"""Checks HACKQUEST-SUBMISSION.md before it is pasted: the form's character limits, that every
address and transaction hash in it is one of ours (deployment.json, facts.json), and that every
link answers.

  python3 video/data/check-submission.py
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
md = (ROOT / 'HACKQUEST-SUBMISSION.md').read_text()
dep = json.loads((ROOT / 'config' / 'deployment.json').read_text())
facts = json.loads((Path(__file__).with_name('facts.json')).read_text())
bad = []


def block(after):
    i = md.index(after)
    a = md.index('```', i) + 3
    return md[a:md.index('```', a)].strip()


name, intro = block('### Name'), block('### Intro')
print(f'name {len(name)}/80, intro {len(intro)}/200')
if len(name) > 80:
    bad.append('name too long')
if len(intro) > 200:
    bad.append(f'intro is {len(intro)} > 200')

rows = [r for r in md.split('## Checkpoints')[1].splitlines() if re.match(r'\| \d+ \|', r)]
for r in rows:
    c = [x.strip() for x in r.strip('|').split('|')]
    n, title, desc = c[0], c[2], c[3]
    print(f'checkpoint {n}: title {len(title)}/50, description {len(desc)}/200')
    if len(title) > 50:
        bad.append(f'checkpoint {n} title {len(title)}')
    if len(desc) > 200:
        bad.append(f'checkpoint {n} description {len(desc)}')

known = {v.lower() for v in [dep[k] for k in ('actuary', 'pool', 'treasury', 'lifeRegistry', 'attestedIdentity', 'timelock', 'usdg', 'deployer')]}
known |= {t['hash'].lower() for t in facts['txs'].values()} | {c['hash'].lower() for c in facts['checkins']}
cfg = json.loads((ROOT / 'config' / 'robinhood-mainnet.json').read_text())
known |= {v['address'].lower() for v in cfg['tokens'].values()}
for h in set(re.findall(r'0x[0-9a-fA-F]{40}(?:[0-9a-fA-F]{24})?\b', md)):
    if h.lower() not in known:
        bad.append(f'unknown address or hash {h}')
print('addresses and hashes:', len(set(re.findall(r'0x[0-9a-fA-F]{40}(?:[0-9a-fA-F]{24})?\b', md))), 'all ours' if not any('unknown' in b for b in bad) else 'SOME UNKNOWN')

for u in sorted(set(re.findall(r'https://[^\s`|)]+', md))):
    code = subprocess.run(['curl', '-s', '-o', '/dev/null', '-L', '-m', '25', '-A', 'Mozilla/5.0', '-w', '%{http_code}', u], capture_output=True, text=True).stdout
    ok = code.startswith('2') or (code == '403' and 'blockscout' in u)  # Blockscout sits behind Cloudflare
    print(f'{code} {u}')
    if not ok:
        bad.append(f'{u} -> {code}')

print('\n'.join(['PROBLEMS:'] + bad) if bad else 'all checks pass')
sys.exit(1 if bad else 0)
