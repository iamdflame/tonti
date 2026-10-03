// Renders each scene to its own file (out/scenes/S01.mp4 …), bundling once and skipping scenes
// already rendered, so a crash or a reboot costs one scene, not the whole cut. Then joins them
// with the grain and a captioned copy (scripts/finish.sh).
//   node scripts/render.mjs [S04 S07 …]   (only these, even if rendered)
import { bundle } from '@remotion/bundler';
import { renderMedia, selectComposition } from '@remotion/renderer';
import { existsSync, mkdirSync, renameSync } from 'node:fs';
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
for (const id of ids) {
  const file = join(dir, `${id}.mp4`);
  if (only.length ? !only.includes(id) : existsSync(file)) continue;
  const composition = await selectComposition({ serveUrl, id, inputProps: { ...inputProps, id }, chromiumOptions });
  const t0 = Date.now();
  let last = 0;
  await renderMedia({
    composition, serveUrl, codec: 'h264', crf: 14, pixelFormat: 'yuv420p', colorSpace: 'bt709', imageFormat: 'jpeg', jpegQuality: 95,
    // The video frame cache defaults to ~1.8 GB a thread; this machine has ~2 GB free and goes down when it runs out.
    offthreadVideoCacheSizeInBytes: 300 * 1024 * 1024, offthreadVideoThreads: 1,
    concurrency: 2, chromiumOptions, inputProps: { ...inputProps, id }, outputLocation: file + '.part.mp4',
    onProgress: ({ progress }) => { if (progress - last >= 0.25) { last = progress; console.log(`${id} ${(progress * 100).toFixed(0)}%`); } },
  });
  renameSync(file + '.part.mp4', file);
  console.log(`${id} done: ${composition.durationInFrames} frames in ${((Date.now() - t0) / 1000).toFixed(0)} s`);
}
if (ids.every((id) => existsSync(join(dir, `${id}.mp4`)))) {
  execFileSync(resolve('scripts/finish.sh'), { stdio: 'inherit' });
}
