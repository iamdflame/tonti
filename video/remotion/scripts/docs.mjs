// Writes the edit pack from the timeline the video renders from, so the script, the timecodes and
// the captions always match the picture: video/docs/VOICEOVER.md, TIMELINE.md, captions.srt.
import { build } from 'esbuild';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';

const r = await build({ entryPoints: ['src/docs-entry.ts'], bundle: true, write: false, format: 'esm', platform: 'node', loader: { '.json': 'json' } });
const tmp = `${process.env.TMPDIR ?? '/tmp'}/tonti-docs-${process.pid}.mjs`;
writeFileSync(tmp, r.outputFiles[0].text);
const m = await import(tmp);
const { SCENES, TOTAL, startOf, cues, quote, lights, fil, S04_LEN, S07_FROM, S10_Q, S10_A } = m;
const facts = JSON.parse(readFileSync('../data/facts.json', 'utf8'));
const FPS = 30;
const tc = (f) => { const s = f / FPS; const mm = Math.floor(s / 60); return `${mm}:${(s - mm * 60).toFixed(1).padStart(4, '0')}`; };
const srt = (f) => { const ms = Math.round((f / FPS) * 1000); const h = Math.floor(ms / 3600000), mi = Math.floor(ms / 60000) % 60, se = Math.floor(ms / 1000) % 60; return `${String(h).padStart(2, '0')}:${String(mi).padStart(2, '0')}:${String(se).padStart(2, '0')},${String(ms % 1000).padStart(3, '0')}`; };
const words = (t) => t.split(/\s+/).length;
const at = (id, local) => startOf(id) + local;
mkdirSync('../docs', { recursive: true });

// ---------------------------------------------------------------- sound effects, from the picture
const s8 = { joined: 20, faceid: 320, identity: 390, paid: 520, settle: 540, invested: 580 };
const end10 = S10_Q + S10_A + 75;
const SFX = [
  [at('S01', 0), 'Opening breath', 'Low, airy cinematic swell rising from silence, soft and warm, no impact, 3 seconds'],
  [at('S02', 30), 'Money leaving home (loop under the arcs)', 'Very soft sparkling chime particles drifting, delicate, like tiny lights, quiet, 6 seconds'],
  [at('S02', 262), 'The money stops', 'Single soft low piano-like tone that fades, gentle, melancholic, 3 seconds'],
  [at('S03', 70), 'The lamp lights (wordmark)', 'Warm soft glowing chime, a lantern being lit, gentle shimmer, 1.5 seconds'],
  [at('S04', quote.marks.ask), 'Tap: Ask Robinhood Chain', 'Soft clean UI button tap, subtle, 0.3 seconds'],
  [at('S04', quote.marks.answer), 'The answer arrives', 'Soft magical confirmation tone with a faint warm shimmer, 1 second'],
  [at('S04', quote.marks.answer + 12), '"for life" written by hand', 'Short pencil writing on paper, light, 1 second'],
  [at('S05', quote.marks.same - S04_LEN), 'Same answer, from a node we don’t run', 'Gentle positive tick, clean and minimal, 0.5 seconds'],
  [at('S06', 0), 'Arcs draw across the map', 'Soft airy whooshes, light and quick, 3 seconds'],
  [at('S06', 290), 'To the sleeves', 'Short soft whoosh transition, 0.8 seconds'],
  [at('S06', 510 + 30), 'A light goes out', 'Single soft low tone, a small light switching off, 1 second'],
  [at('S06', 510 + 66), 'Its share flows to the others', 'Delicate rising shimmer, warm, 2 seconds'],
  [at('S07', lights.marks['1977'] - S07_FROM), '1977: the lamp drawn alone goes out', 'A single lamp switching off, soft click and fade, 1 second'],
  [at('S08', s8.joined), 'Receipt: Joined', 'Soft quick data tick, like a receipt printing, subtle, 0.6 seconds'],
  [at('S08', s8.faceid), 'Receipt: Face ID check-in', 'Soft quick data tick, subtle, 0.6 seconds'],
  [at('S08', s8.identity), 'Receipt: Identity attested', 'Soft quick data tick, subtle, 0.6 seconds'],
  [at('S08', s8.paid), 'Receipt: Paid in', 'Soft quick data tick, subtle, 0.6 seconds'],
  [at('S08', s8.settle), 'Receipt: first settlement', 'Soft quick data tick, subtle, 0.6 seconds'],
  [at('S08', s8.invested), 'Receipt: invested on mainnet', 'Soft quick data tick, subtle, 0.6 seconds'],
  ...[100, 145, 240, 345, 440, 540].map((f, i) => [at('S09', f + 8), `Checklist tick ${i + 1}`, 'Soft, low, satisfying check mark tick, 0.4 seconds']),
  [at('S10', S10_Q), '"habambuhay" written on the site', 'Short pencil writing on paper, light, 1 second'],
  [at('S10', end10 + 10), 'The lamp lights again (end card)', 'Warm soft glowing chime, a lantern being lit, gentle shimmer, 1.5 seconds'],
];

// ---------------------------------------------------------------- VOICEOVER.md
const vo = [];
vo.push(`# Tonti demo video: voiceover, music and sound

Everything here is generated from the cut itself (\`video/remotion/src/timeline.ts\`), so the timecodes match \`picture-lock.mp4\` frame for frame. The video is **${tc(TOTAL)}** long: 1920×1080, 30 fps.

## 1. The voice (ElevenLabs)

### Use **Darian – Warm Grounded Storyteller** (voice ID \`gOupLcAkjEnguROwi4oS\`)

ElevenLabs' own description: "Warm, grounded male baritone. Natural and confident for storytelling and brand reads."

**Why Darian.** I measured the official previews of ElevenLabs' new permanent voices:

| Voice (ID) | Pitch | Pitch range | Brightness | What it suits |
|---|---|---|---|---|
| **Darian** (\`gOupLcAkjEnguROwi4oS\`) | **105 Hz**, the deepest steady voice | 8.6 semitones: steady | 1,528 Hz: warm | a warm, trustworthy story about mothers and money |
| Wyatt – Seasoned Mentor (\`FrS6cKLB1wg4WYgPa9GW\`) | 114 Hz | 7.7: the most measured | 1,796 Hz: brighter | backup: older-sounding, "ideal for long-form documentaries" |
| Florence – Atmospheric Storyteller (\`22N9cF8z0o7y23njdyaY\`) | 157 Hz (female) | 11.9 | 1,604 Hz: warm | if you want a woman's voice for a film about mothers |
| Eldrin, Sawyer | 110–111 Hz | 16–19: theatrical | darkest | too dramatic: trailer and noir reads |
| Caleb, Talia, Elara | 125–176 Hz | | brighter or quicker | crisp product demos, not this story |

- **Permanent.** Darian is one of the new voices ElevenLabs made to replace its old defaults (it replaces "Roger"). The old defaults stop working on 31 December 2026, and accounts made after March 2026 don't have them at all; the new ones can be used for good. Avoid the old voices (Roger, George, Brian, Adam…) and random library voices, which their owners can remove.
- **Normal price.** It costs credits ×1.
- **Made for v4.** It is a Professional Voice Clone, and ElevenLabs says those are "fully supported in Eleven v4".

**How to add it:** ElevenLabs → *Voices* → *Explore* (Voice Library) → search **Darian**, and pick the one called "Darian - Warm Grounded Storyteller" → *Add to my voices*. Then in Text to Speech choose Darian and the model **Eleven v4**.

**Settings.** Eleven v4 has **only two**: Stability and Similarity. It has no Speed, Style or Speaker-boost sliders and no SSML, so pace and pauses come from the text itself: the ellipses (…), full stops and audio tags are already in the lines below.

| Setting | Value | Why |
|---|---|---|
| Model | **Eleven v4** | ElevenLabs' most expressive model; follows tags like \`[warmly]\` |
| Stability | **0.50** | Enough range for \`[warmly]\` and \`[confident]\` to land, and steady enough that ten separate lines sound like one narrator. If lines differ too much in tone, raise it to 0.60; if they sound flat, lower it to 0.40 |
| Similarity | **0.75** | ElevenLabs' recommended setting: clearly Darian, still natural |
| Output | **WAV** (or MP3 192 kbps) | WAV keeps quality through your edit |

**How to generate:**
1. Generate **one scene's line at a time**: paste the *ElevenLabs text* exactly, tags and pauses included.
2. Make 2–3 takes per line and keep the best. Generations vary slightly even with the same settings, which is how you get a better read.
3. Name them \`VO_S01.wav\` … \`VO_S10.wav\`.
4. Each take must be **no longer than its "fits in" time**. v4 has no speed slider, so if one runs long, generate it again (takes vary), or send me the files and I'll retime the picture to the voice (section 5).

**Pronunciation.** The lines are already spelled for speech. If a word still comes out wrong, replace it with the IPA form, written exactly like this, slashes included:

| Word | Say it | If the voice gets it wrong, write |
|---|---|---|
| Tonti | TON-tee | \`/ˈtɒnti/\` |
| Habambuhay (Filipino: "for life") | ha-bam-BOO-hai | \`/habamˈbuhaj/\` |
| CPF | C-P-F (already spelled out) | |
| USDG | U-S-D-G (already spelled out) | |
| S&P 500 | "S and P five hundred" (already spelled out) | |
| Vercel | VER-sell | \`/vɚˈsɛl/\` |

## 2. The script, scene by scene

"VO in" is where the line starts on the timeline: 0.4 s after the scene's first frame. "Fits in" is the time before the next scene.

`);
for (const s of SCENES) {
  const start = startOf(s.id);
  const fits = (s.frames - 12) / FPS;
  vo.push(`### ${s.id} · ${s.title} · VO in **${tc(start + 12)}** · fits in **${fits.toFixed(1)} s** · ${words(s.vo)} words (≈${((words(s.vo) / 150) * 60).toFixed(0)} s)

*Picture:* ${s.picture}

**ElevenLabs text** (paste this):

> ${s.tts}

Plain script, for reading or subtitles: ${s.vo}

`);
}
const sec = (id) => startOf(id) / FPS;
vo.push(`## 3. Music (Eleven Music)

One instrumental track of **${tc(TOTAL + 15)}**. In Eleven Music, paste the prompt below. If your plan lets you lay out sections, use the section plan; otherwise paste the whole prompt.

**Prompt:**

> Instrumental cinematic underscore for a heartfelt fintech product film about lifelong income for families. Warm, human, hopeful, never corporate. Felt piano, soft string pads, a gentle plucked arpeggio, light modern percussion that arrives late. 72 BPM. No vocals, no vocal chops, no big drops. It must sit under a narrator: keep the mid-range clear and the arrangement sparse, with dynamics carried by texture rather than volume. Starts intimate and a little melancholic, lifts into quiet confidence, and resolves warmly at sunset, ending on a sustained piano chord that rings out. Length ${tc(TOTAL + 15)}.

**Section plan** (times match the picture):

| From | To | Scene | Music |
|---|---|---|---|
| 0:00 | ${tc(startOf('S03'))} | Singapore; money home | Solo felt piano, sparse, warm, slightly melancholic; room ambience; no drums |
| ${tc(startOf('S03'))} | ${tc(startOf('S04'))} | Title: the lamp lights | String pad swells in; a single celesta or bell note on the lamp at ${tc(at('S03', 70))} |
| ${tc(startOf('S04'))} | ${tc(startOf('S06'))} | The live quote, run it yourself | A gentle plucked arpeggio begins, curious and hopeful; very light |
| ${tc(startOf('S06'))} | ${tc(startOf('S07'))} | How it works | Soft kick and shaker join; cello line; steady forward motion |
| ${tc(startOf('S07'))} | ${tc(startOf('S08'))} | History replay | Pull back: drums out, piano and strings, reflective; a dip at ${tc(at('S07', lights.marks['1977'] - S07_FROM))} when the lone lamp goes out |
| ${tc(startOf('S08'))} | ${tc(startOf('S09'))} | It's live: the first member | Fullest point: percussion back, warm and confident, uplifting but restrained |
| ${tc(startOf('S09'))} | ${tc(startOf('S10'))} | Nobody can take the pot | Steady, resolute, lower register, sustained strings |
| ${tc(startOf('S10'))} | ${tc(TOTAL)} | Habambuhay; end card | The opening piano motif returns over strings; final swell at ${tc(at('S10', end10))}; ring out to silence |

**Mix:** music at about −18 to −22 dB under the narration. Let it come up to about −10 dB where nobody speaks: the end card from ${tc(at('S10', end10))}, and the short gaps between lines.

## 4. Sound effects (ElevenLabs Text to Sound)

Each effect is placed on a moment that exists in the picture. Keep them quiet, about −20 to −26 dB: the voice leads, the sound only confirms what the eye sees. Generate each prompt once and reuse the ones that repeat (the ticks, the chime, the pencil).

| At | Moment | Prompt |
|---|---|---|
${SFX.sort((a, b) => a[0] - b[0]).map(([f, what, prompt]) => `| ${tc(f)} | ${what} | ${prompt} |`).join('\n')}

## 5. Putting it together

1. **Timeline:** 1920×1080, 30 fps. Put \`picture-lock.mp4\` on V1. It has no audio.
2. **Voice:** put \`VO_S01\`…\`VO_S10\` on A1, each at its "VO in" time from section 2.
3. **Music** on A2 from 0:00, ducked under the voice (section 3).
4. **Effects** on A3 at the times in section 4.
5. **Captions:** \`captions.srt\` is timed to this script. Upload it to YouTube as subtitles, or use \`picture-lock-captions.mp4\`, which has them burned in. If your final voice timing differs, YouTube can re-time an uploaded transcript automatically.
6. **Export:** H.264 High, 1080p30, 16–20 Mbps; AAC 320 kbps 48 kHz; **−14 LUFS integrated, −1 dBTP true peak** (YouTube's target).

**Want it synced for you?** Put the ten \`VO_Sxx\` files (and the music, if you like) in \`/media/dflame/UNIQ/arbit/video/vo/\` and tell me. I'll measure each line and re-render the picture so every scene fits its voice exactly, with captions timed from the real audio. You can still re-edit afterwards.

## 6. What's real in this video

- **Recorded from the live site** at https://tonti-life.vercel.app, on Robinhood Chain mainnet, in single continuous takes: the question and its answer (1.2 s from tap to number), Run it yourself, the history replay, the join, the check-in, the account and the Live pool.
- **The member scenes are a replay** of member #0's real join (transaction \`${facts.txs.join.hash.slice(0, 10)}…\`), sent through the live site. The video labels them as a replay. If you record your own iPhone (account page, then a Face ID check-in), I'll swap it in: put the file in \`/media/dflame/UNIQ/arbit/video/iphone/\`.
- **Every number on screen** comes from \`video/data/facts.json\`, read from the chain when the video was built: blocks, transactions, gas, holdings, and the member's value.
- **Explanations** (the map, the sleeves, the lights, the checklist) are motion design, drawn from the same facts.
`);
writeFileSync('../docs/VOICEOVER.md', vo.join(''));
// The voice cues as data, for scripts/mix.sh: where each line starts, and how long it may run.
writeFileSync('../docs/cues.json', JSON.stringify({ fps: FPS, seconds: TOTAL / FPS, scenes: SCENES.map((s) => ({ id: s.id, voIn: (startOf(s.id) + 12) / FPS, fits: (s.frames - 12) / FPS })) }, null, 1) + '\n');

// ---------------------------------------------------------------- TIMELINE.md
const tl = [`# Tonti demo video: timeline

${tc(TOTAL)} · 1920×1080 · 30 fps · ${SCENES.length} scenes. Generated from \`video/remotion/src/timeline.ts\`.

| Scene | In | Out | Length | Picture |
|---|---|---|---|---|
`];
for (const s of SCENES) tl.push(`| ${s.id} ${s.title} | ${tc(startOf(s.id))} | ${tc(startOf(s.id) + s.frames)} | ${(s.frames / FPS).toFixed(1)} s | ${s.picture} |\n`);
tl.push(`
## On-chain facts shown (Robinhood Chain mainnet, chain 4663)

| What | Block | Transaction | Gas |
|---|---|---|---|
${Object.entries(facts.txs).map(([k, t]) => `| ${k} | ${t.block.toLocaleString('en-US')} | \`${t.hash}\` | ${t.gas.toLocaleString('en-US')} |`).join('\n')}
${facts.checkins.map((c, i) => `| Face ID check-in ${i + 1} | ${c.block.toLocaleString('en-US')} | \`${c.hash}\` | ${c.gas.toLocaleString('en-US')} |`).join('\n')}

Holdings after the first settlement: S&P 500 $${facts.holdings.sp500.usd.toFixed(2)}, T-bills $${facts.holdings.tbills.usd.toFixed(2)}, USDG $${facts.holdings.cash.usd.toFixed(2)}. Member #0 is valued at $${facts.member0.valueUsd.toFixed(2)}.
`);
writeFileSync('../docs/TIMELINE.md', tl.join(''));

// ---------------------------------------------------------------- captions.srt
writeFileSync('../docs/captions.srt', cues(FPS).map((c, i) => `${i + 1}\n${srt(c.from)} --> ${srt(c.to)}\n${c.text}\n`).join('\n'));
console.log('docs written:', tc(TOTAL), SCENES.length, 'scenes,', cues(FPS).length, 'captions,', SFX.length, 'effects');
