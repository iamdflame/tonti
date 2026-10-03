import { SCENES, startOf } from './timeline';

export type Cue = { from: number; to: number; text: string };

// Captions from the narration itself: each scene's line split into short phrases, spread over the
// scene in proportion to their words (≈150 words a minute). When the real voiceover exists, the
// retime step replaces these estimates with measured times.
export function cues(fps = 30): Cue[] {
  const out: Cue[] = [];
  for (const s of SCENES) {
    const phrases = s.vo.match(/[^.:;?!]+[.:;?!]?/g)!.map((p) => p.trim()).filter(Boolean).flatMap((p) => split(p));
    const words = phrases.map((p) => p.split(/\s+/).length);
    const total = words.reduce((a, b) => a + b, 0);
    const start = startOf(s.id) + Math.round(0.4 * fps);
    const span = Math.min(s.frames - Math.round(0.9 * fps), Math.round((total / 150) * 60 * fps));
    let t = start;
    phrases.forEach((p, i) => {
      const len = Math.round((words[i] / total) * span);
      out.push({ from: t, to: t + len, text: p });
      t += len;
    });
  }
  return out;
}

// No caption longer than about 9 words: break at a comma near the middle.
function split(p: string): string[] {
  const w = p.split(/\s+/);
  if (w.length <= 9) return [p];
  const mid = Math.floor(w.length / 2);
  let cut = -1;
  for (let d = 0; d < mid; d++) {
    if (w[mid + d]?.endsWith(',')) { cut = mid + d + 1; break; }
    if (w[mid - d]?.endsWith(',')) { cut = mid - d + 1; break; }
  }
  if (cut <= 1 || cut >= w.length - 1) cut = mid; // a comma at either end isn't a split
  return [...split(w.slice(0, cut).join(' ')), ...split(w.slice(cut).join(' '))];
}
