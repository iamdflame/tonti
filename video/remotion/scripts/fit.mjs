// Does each scene's narration fit its picture? Words at 150 a minute against the scene's length.
import { build } from 'esbuild';
const r = await build({ entryPoints: ['src/timeline.ts'], bundle: true, write: false, format: 'esm', platform: 'node', loader: { '.json': 'json' } });
const m = await import('data:text/javascript;base64,' + Buffer.from(r.outputFiles[0].text).toString('base64'));
let total = 0;
for (const s of m.SCENES) {
  const words = s.vo.split(/\s+/).length;
  const need = (words / 150) * 60, have = s.frames / 30;
  total += words;
  console.log(`${s.id} ${String(words).padStart(3)} words  ${need.toFixed(1).padStart(5)} s spoken / ${have.toFixed(1).padStart(5)} s scene  ${need > have - 1 ? 'OVER by ' + (need - have + 1).toFixed(1) + ' s' : 'ok, ' + (have - need).toFixed(1) + ' s spare'}`);
}
console.log('total words', total, 'video', (m.TOTAL / 30).toFixed(1), 's');
