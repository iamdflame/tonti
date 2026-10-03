// Prints each scene's start frame and the total, from the timeline (bundled with esbuild).
import { build } from 'esbuild';
const r = await build({ entryPoints: ['src/timeline.ts'], bundle: true, write: false, format: 'esm', platform: 'node', loader: { '.json': 'json' } });
const m = await import('data:text/javascript;base64,' + Buffer.from(r.outputFiles[0].text).toString('base64'));
const starts = Object.fromEntries(m.SCENES.map((s) => [s.id, m.startOf(s.id)]));
console.log(JSON.stringify({ starts, total: m.TOTAL, seconds: +(m.TOTAL / 30).toFixed(1), lens: Object.fromEntries(m.SCENES.map((s) => [s.id, s.frames])) }));
