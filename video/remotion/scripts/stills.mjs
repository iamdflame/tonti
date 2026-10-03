// Renders chosen frames of the master as PNG stills, bundling once: the review loop for the cut.
//   node scripts/stills.mjs <outDir> <frame> [frame...]     (frames: numbers, or S04+120 = scene start + 120)
import { bundle } from '@remotion/bundler';
import { renderStill, selectComposition } from '@remotion/renderer';
import { mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';

const [out, ...specs] = process.argv.slice(2);
mkdirSync(out, { recursive: true });
const serveUrl = await bundle({ entryPoint: resolve('src/index.ts'), publicDir: resolve('public') });
const composition = await selectComposition({ serveUrl, id: 'Tonti', inputProps: { captions: false }, chromiumOptions: { gl: 'swangle' } });
const starts = JSON.parse(process.env.STARTS ?? '{}');
for (const spec of specs) {
  const [id, plus] = spec.includes('+') ? spec.split('+') : [null, spec];
  const frame = (id ? starts[id] : 0) + Number(plus);
  const t0 = Date.now();
  await renderStill({ composition, serveUrl, frame, output: join(out, `${spec.replace('+', '_')}.png`), chromiumOptions: { gl: 'swangle' }, inputProps: { captions: false } });
  console.log(spec, frame, `${((Date.now() - t0) / 1000).toFixed(1)}s`);
}
