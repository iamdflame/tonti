#!/usr/bin/env python3
"""Collects each recorded take's markers (video/record/*.mjs writes <take>.json beside the .mp4) into
video/data/takes.json, which the Remotion scenes cut from. Run after recording."""
import json
from pathlib import Path

TAKES = Path('/media/dflame/UNIQ/arbit/video/takes')
out = {}
for j in sorted(TAKES.glob('*.json')):
    d = json.loads(j.read_text())
    out[d['name']] = {'frames': d['frames'], 'seconds': d['seconds'], 'marks': {m['label']: m['frame'] for m in d.get('marks', [])}}
dest = Path(__file__).with_name('takes.json')
dest.write_text(json.dumps(out, indent=1) + '\n')
print(json.dumps({k: (v['frames'], v['marks']) for k, v in out.items()}, indent=0))
