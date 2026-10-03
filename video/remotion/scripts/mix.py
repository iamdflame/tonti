#!/usr/bin/env python3
"""The audio mix, done for you: put the ElevenLabs files in /media/dflame/UNIQ/arbit/video/vo/ and run

  python3 scripts/mix.py [picture.mp4]          (default: out/picture-lock.mp4)

Expected files (any of .wav .mp3 .m4a .flac):
  VO_S01 … VO_S10   the narration, one line per scene; each starts at its cue in video/docs/cues.json
  music             the score, from 0:00; ducked under the voice (sidechain), up where nobody speaks
  SFX_<m>m<ss.s>s   optional effects, placed at their time in the name, e.g. SFX_0m27.3s.wav

It warns when a line runs past its scene. Output: out/tonti-demo-final.mp4 (picture copied, AAC 320k
48 kHz, -14 LUFS integrated, -1 dBTP: YouTube's target) and the stems mix as out/tonti-demo-mix.wav.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
CUES = json.loads((HERE.parent.parent / 'docs' / 'cues.json').read_text())
import os
BASE = Path('/media/dflame/UNIQ/arbit/video')
VO = Path(os.environ.get('VO_DIR', BASE / 'vo'))
OUT = Path(os.environ.get('OUT_DIR', BASE / 'out'))
EXT = ('.wav', '.mp3', '.m4a', '.flac', '.aac', '.ogg')


def find(stem):
    for e in EXT:
        p = VO / f'{stem}{e}'
        if p.exists():
            return p
    return None


def duration(p):
    return float(subprocess.run(['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', str(p)], capture_output=True, text=True).stdout)


def main():
    picture = Path(sys.argv[1]) if len(sys.argv) > 1 else OUT / 'picture-lock.mp4'
    total = CUES['seconds']
    inputs, voices, notes = [], [], []
    for s in CUES['scenes']:
        f = find(f"VO_{s['id']}")
        if not f:
            notes.append(f"{s['id']}: no voice file")
            continue
        d = duration(f)
        if d > s['fits']:
            notes.append(f"{s['id']}: {d:.1f} s runs {d - s['fits']:.1f} s past its scene ({s['fits']:.1f} s)")
        voices.append((len(inputs), s['voIn']))
        inputs.append(f)
    music = find('music')
    sfx = []
    for p in sorted(VO.glob('SFX_*')):
        m = re.match(r'SFX_(\d+)m([\d.]+)s', p.stem)
        if m and p.suffix in EXT:
            sfx.append((len(inputs) + len(sfx) + (1 if music else 0), int(m[1]) * 60 + float(m[2]), p))
    if not voices:
        sys.exit('no VO_Sxx files in ' + str(VO))
    args = ['ffmpeg', '-y', '-loglevel', 'error']
    for p in inputs:
        args += ['-i', str(p)]
    if music:
        args += ['-i', str(music)]
    for _, _, p in sfx:
        args += ['-i', str(p)]
    fl = []
    # the voice bus: every line at its cue, mono-safe, gently compressed
    for k, (i, t) in enumerate(voices):
        fl.append(f'[{i}:a]aresample=48000,aformat=channel_layouts=stereo,adelay={int(t * 1000)}:all=1[v{k}]')
    fl.append(''.join(f'[v{k}]' for k in range(len(voices))) + f'amix=inputs={len(voices)}:normalize=0,apad=whole_dur={total + 1},acompressor=threshold=-20dB:ratio=3:attack=5:release=120[voice]')
    mixins = ['[voice]']
    if music:
        mi = len(inputs)
        fl.append('[voice]asplit=2[vk][vm]')
        mixins = ['[vm]']
        # the score sits ~-20 dB under the voice and rises where nobody speaks; fades out with the picture
        fl.append(f'[{mi}:a]aresample=48000,aformat=channel_layouts=stereo,volume=-9dB,afade=t=out:st={total - 3}:d=3,atrim=0:{total + 1}[mu]')
        fl.append('[mu][vk]sidechaincompress=threshold=0.03:ratio=8:attack=20:release=450:makeup=1[duck]')
        mixins.append('[duck]')
    for k, (i, t, _) in enumerate(sfx):
        fl.append(f'[{i}:a]aresample=48000,aformat=channel_layouts=stereo,volume=-6dB,adelay={int(t * 1000)}:all=1[s{k}]')
        mixins.append(f'[s{k}]')
    fl.append(''.join(mixins) + f'amix=inputs={len(mixins)}:normalize=0,atrim=0:{total},loudnorm=I=-14:TP=-1:LRA=11[mix]')
    wav = OUT / 'tonti-demo-mix.wav'
    subprocess.run(args + ['-filter_complex', ';'.join(fl), '-map', '[mix]', '-ar', '48000', str(wav)], check=True)
    final = OUT / 'tonti-demo-final.mp4'
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-i', str(picture), '-i', str(wav), '-map', '0:v', '-map', '1:a', '-c:v', 'copy',
                    '-c:a', 'aac', '-b:a', '320k', '-ar', '48000', '-shortest', '-movflags', '+faststart', str(final)], check=True)
    meas = subprocess.run(['ffmpeg', '-hide_banner', '-i', str(final), '-af', 'ebur128=peak=true', '-f', 'null', '-'], capture_output=True, text=True).stderr
    summary = meas[meas.rfind('Summary:'):].split('\n')
    print('\n'.join(n for n in notes) or 'every line fits its scene')
    print(final)
    print(' '.join(x.strip() for x in summary if re.search(r'I:|Peak:', x)))


if __name__ == '__main__':
    main()
