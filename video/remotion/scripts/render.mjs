// Renders each scene to its own file (out/scenes/S01.mp4 …), bundling once and skipping scenes
// already rendered, so a crash or a reboot costs one scene, not the whole cut. Then joins them
// with the grain and a captioned copy (scripts/finish.sh).
//   node scripts/render.mjs [S04 S07 …]   (only these, even if rendered)
import { bundle } from '@remotion/bundler';
import { renderMedia, selectComposition } from '@remotion/renderer';
import { existsSync, mkdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, resolve } from 'node:path';

const OUT = existsSync('/media/dflame/UNIQ/arbit/video') ? '/media/dflame/UNIQ/arbit/video/out' : resolve('../out');
const dir = join(OUT, 'scenes');
mkdirSync(dir, { recursive: true });
const only = process.argv.slice(2);
const ids = ['S01', 'S02', 'S03', 'S04', 'S05', 'S06', 'S07', 'S08', 'S09', 'S10'];
const inputProps = JSON.parse(process.env.PROPS ?? '{"captions":false}');
const serveUrl = await bundle({ entryPoint: resolve('src/index.ts'), publicDir: resolve('public') });
const chromiumOptions = { gl: 'swangle' };
// Each scene renders in chunks of CHUNK frames with a pause after each: the build machine has gone
// down three times under sustained load, and a crash now costs one chunk. Chunks join without
// re-encoding (same encoder settings, every chunk starts on a keyframe).
const CHUNK = Number(process.env.CHUNK ?? 150);
const REST = Number(process.env.REST ?? 20) * 1000;
const sleep = (ms) => new Promise((ok) => setTimeout(ok, ms));
for (const id of ids) {
  const file = join(dir, `${id}.mp4`);
  if (only.length ? !only.includes(id) : existsSync(file)) continue;
  const composition = await selectComposition({ serveUrl, id, inputProps: { ...inputProps, id }, chromiumOptions });
  const n = composition.durationInFrames;
  const parts = [];
  const t0 = Date.now();
  for (let a = 0; a < n; a += CHUNK) {
    const b = Math.min(n, a + CHUNK) - 1;
    const part = join(dir, `${id}.${String(a).padStart(5, '0')}.mp4`);
    parts.push(part);
    if (existsSync(part)) continue;
    const c0 = Date.now();
    await renderMedia({
      composition, serveUrl, codec: 'h264', crf: 14, pixelFormat: 'yuv420p', colorSpace: 'bt709', imageFormat: 'jpeg', jpegQuality: 95,
      // The video frame cache defaults to ~1.8 GB a thread; this machine has ~2 GB free.
      offthreadVideoCacheSizeInBytes: 300 * 1024 * 1024, offthreadVideoThreads: 1,
      concurrency: Number(process.env.CONCURRENCY ?? 1), chromiumOptions, inputProps: { ...inputProps, id }, frameRange: [a, b],
      outputLocation: part + '.part.mp4',
    });
    renameSync(part + '.part.mp4', part);
    console.log(`${id} frames ${a}-${b} in ${((Date.now() - c0) / 1000).toFixed(0)} s`);
    await sleep(REST);
  }
  writeFileSync(join(dir, `${id}.parts.txt`), parts.map((p) => `file '${p}'`).join('\n') + '\n');
  execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'concat', '-safe', '0', '-i', join(dir, `${id}.parts.txt`), '-c', 'copy', file + '.join.mp4']);
  renameSync(file + '.join.mp4', file);
  for (const p of parts) rmSync(p);
  rmSync(join(dir, `${id}.parts.txt`));
  console.log(`${id} done: ${n} frames in ${((Date.now() - t0) / 1000).toFixed(0)} s`);
}
if (ids.every((id) => existsSync(join(dir, `${id}.mp4`)))) {
  execFileSync(resolve('scripts/finish.sh'), { stdio: 'inherit' });
}
