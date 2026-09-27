"""Where data, reports and the engine binary live, for every script in the repo.

On the build machine: the UNIQ data drive and a RAM target dir (its internal disk is nearly full).
Anywhere else: inside the repo (data/, runs/, engine/target). Override with TONTI_DATA,
TONTI_RUNS or CARGO_TARGET_DIR.
"""
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
_UNIQ = Path('/media/dflame/UNIQ/arbit')


def _pick(env, sub):
    if os.environ.get(env):
        return Path(os.environ[env])
    if (_UNIQ / sub).exists():
        return _UNIQ / sub
    return ROOT / sub


DATA = _pick('TONTI_DATA', 'data')
RUNS = _pick('TONTI_RUNS', 'runs')
RUNS.mkdir(parents=True, exist_ok=True)


def target_dir():
    if os.environ.get('CARGO_TARGET_DIR'):
        return Path(os.environ['CARGO_TARGET_DIR'])
    ram = Path('/dev/shm/arbit-target/engine')
    return ram if ram.exists() else ROOT / 'engine' / 'target'


CLI = str(target_dir() / 'release' / 'actuary-cli')
