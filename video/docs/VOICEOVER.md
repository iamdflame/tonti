# Tonti demo video: voiceover, music and sound

Everything here is generated from the cut itself (`video/remotion/src/timeline.ts`), so the timecodes match `picture-lock.mp4` frame for frame. The video is **3:09.4** long: 1920×1080, 30 fps.

## 1. The voice (ElevenLabs)

### Use **Darian – Warm Grounded Storyteller** (voice ID `gOupLcAkjEnguROwi4oS`)

ElevenLabs' own description: "Warm, grounded male baritone. Natural and confident for storytelling and brand reads."

**Why Darian.** I measured the official previews of ElevenLabs' new permanent voices:

| Voice (ID) | Pitch | Pitch range | Brightness | What it suits |
|---|---|---|---|---|
| **Darian** (`gOupLcAkjEnguROwi4oS`) | **105 Hz**, the deepest steady voice | 8.6 semitones: steady | 1,528 Hz: warm | a warm, trustworthy story about mothers and money |
| Wyatt – Seasoned Mentor (`FrS6cKLB1wg4WYgPa9GW`) | 114 Hz | 7.7: the most measured | 1,796 Hz: brighter | backup: older-sounding, "ideal for long-form documentaries" |
| Florence – Atmospheric Storyteller (`22N9cF8z0o7y23njdyaY`) | 157 Hz (female) | 11.9 | 1,604 Hz: warm | if you want a woman's voice for a film about mothers |
| Eldrin, Sawyer | 110–111 Hz | 16–19: theatrical | darkest | too dramatic: trailer and noir reads |
| Caleb, Talia, Elara | 125–176 Hz | | brighter or quicker | crisp product demos, not this story |

- **Permanent.** Darian is one of the new voices ElevenLabs made to replace its old defaults (it replaces "Roger"). The old defaults stop working on 31 December 2026, and accounts made after March 2026 don't have them at all; the new ones can be used for good. Avoid the old voices (Roger, George, Brian, Adam…) and random library voices, which their owners can remove.
- **Normal price.** It costs credits ×1.
- **Made for v4.** It is a Professional Voice Clone, and ElevenLabs says those are "fully supported in Eleven v4".

**How to add it:** ElevenLabs → *Voices* → *Explore* (Voice Library) → search **Darian**, and pick the one called "Darian - Warm Grounded Storyteller" → *Add to my voices*. Then in Text to Speech choose Darian and the model **Eleven v4**.

**Settings.** Eleven v4 has **only two**: Stability and Similarity. It has no Speed, Style or Speaker-boost sliders and no SSML, so pace and pauses come from the text itself: the ellipses (…), full stops and audio tags are already in the lines below.

| Setting | Value | Why |
|---|---|---|
| Model | **Eleven v4** | ElevenLabs' most expressive model; follows tags like `[warmly]` |
| Stability | **0.50** | Enough range for `[warmly]` and `[confident]` to land, and steady enough that ten separate lines sound like one narrator. If lines differ too much in tone, raise it to 0.60; if they sound flat, lower it to 0.40 |
| Similarity | **0.75** | ElevenLabs' recommended setting: clearly Darian, still natural |
| Output | **WAV** (or MP3 192 kbps) | WAV keeps quality through your edit |

**How to generate:**
1. Generate **one scene's line at a time**: paste the *ElevenLabs text* exactly, tags and pauses included.
2. Make 2–3 takes per line and keep the best. Generations vary slightly even with the same settings, which is how you get a better read.
3. Name them `VO_S01.wav` … `VO_S10.wav`.
4. Each take must be **no longer than its "fits in" time**. v4 has no speed slider, so if one runs long, generate it again (takes vary), or send me the files and I'll retime the picture to the voice (section 5).

**Pronunciation.** The lines are already spelled for speech. If a word still comes out wrong, replace it with the IPA form, written exactly like this, slashes included:

| Word | Say it | If the voice gets it wrong, write |
|---|---|---|
| Tonti | TON-tee | `/ˈtɒnti/` |
| Habambuhay (Filipino: "for life") | ha-bam-BOO-hai | `/habamˈbuhaj/` |
| CPF | C-P-F (already spelled out) | |
| USDG | U-S-D-G (already spelled out) | |
| S&P 500 | "S and P five hundred" (already spelled out) | |
| Vercel | VER-sell | `/vɚˈsɛl/` |

## 2. The script, scene by scene

"VO in" is where the line starts on the timeline: 0.4 s after the scene's first frame. "Fits in" is the time before the next scene.

### S01 · Singapore runs on them · VO in **0:00.4** · fits in **12.6 s** · 26 words (≈10 s)

*Picture:* Marina Bay at night from the SkyPark, then an everyday HDB estate in Tampines (Wikimedia Commons, CC BY 3.0). Kinetic type: "1,635,700 foreign workers keep Singapore running" (Ministry of Manpower, Dec 2025), "None of them can save into its pension, CPF."

**ElevenLabs text** (paste this):

> [calm] Singapore runs on one point six million foreign workers. Not one of them can save into the city’s pension... C-P-F. Every month, they send money home.

Plain script, for reading or subtitles: Singapore runs on one point six million foreign workers. Not one of them can save into the city’s pension, CPF. Every month, they send money home.

### S02 · Parents with no pension · VO in **0:13.4** · fits in **11.6 s** · 26 words (≈10 s)

*Picture:* Motion design: money leaves Singapore every month as lights along arcs to twelve home countries; on "the money stops" the flows stop and the homes go dark. Type: "Only 24% of older people in South Asia receive a pension" (ILO).

**ElevenLabs text** (paste this):

> Back home, their parents have no pension either. Only one in four older people in South Asia receives one. [softly] When the work stops... the money stops.

Plain script, for reading or subtitles: Back home, their parents have no pension either. Only one in four older people in South Asia receives one. When the work stops, the money stops.

### S03 · Tonti · VO in **0:25.4** · fits in **7.6 s** · 15 words (≈6 s)

*Picture:* The site’s dusk sky over the bay; the sun settles; the lamp in the wordmark lights. "Income for life, on Robinhood Chain."

**ElevenLabs text** (paste this):

> [warmly] Tonti. A pension pool that pays an income for life... running entirely on Robinhood Chain.

Plain script, for reading or subtitles: Tonti: a pension pool that pays an income for life, running entirely on Robinhood Chain.

### S04 · One question, answered by the chain · VO in **0:33.4** · fits in **25.2 s** · 58 words (≈23 s)

*Picture:* One continuous take of the live site: the question, the form (Mother, Philippines, born 1966, income from 65, $5,000 now, $50 a month), Ask, the answer from the chain in 1.2 s, the sun settling, US$67.67 a month "for life", the chart where saved-alone runs out at 81.

**ElevenLabs text** (paste this):

> Ask what any worker would ask: how much would my mother get, every month, for the rest of her life? A contract on the chain simulates five hundred and twelve futures... and answers in about a second. [warmly] Sixty-seven dollars a month. For life. Saved alone, it would run out at eighty-one... with even odds she is still alive.

Plain script, for reading or subtitles: Ask what any worker would ask: how much would my mother get, every month, for the rest of her life? A contract on the chain simulates five hundred and twelve futures and answers in about a second. Sixty-seven dollars a month, for life. Saved alone, it would run out at eighty-one, with even odds she is still alive.

### S05 · Run it yourself · VO in **0:59.0** · fits in **9.8 s** · 20 words (≈8 s)

*Picture:* "Run it yourself": the exact call, re-run from the browser on a public node. "Same answer, from a node we don’t run." Its gas, under a third of one call’s limit.

**ElevenLabs text** (paste this):

> Nothing is precomputed. Here is the exact call. Re-run it on a public node... and you get the same answer.

Plain script, for reading or subtitles: Nothing is precomputed. Here is the exact call. Re-run it on a public node, and you get the same answer.

### S06 · How it works · VO in **1:09.2** · fits in **27.6 s** · 61 words (≈24 s)

*Picture:* Explainer. Thirteen home countries joined to Singapore; "1,846 cohorts, priced from UN tables, sealed on-chain". The money in three sleeves: S&P 500, T-bills, USDG. Lights: when one goes out, its share brightens the rest.

**ElevenLabs text** (paste this):

> Members are pooled by country, sex and birth year: eighteen hundred and forty-six cohorts, priced from United Nations life tables, and sealed on-chain. Savings are invested in the S and P five hundred, Treasury bills, and U-S-D-G. When a member dies, the savings they put at risk are shared among those still alive. [warmly] That is what makes the income last a lifetime.

Plain script, for reading or subtitles: Members are pooled by country, sex and birth year: eighteen hundred and forty-six cohorts, priced from United Nations life tables and sealed on-chain. Savings are invested in the S and P 500, Treasury bills and U-S-D-G. When a member dies, the savings they put at risk are shared among those still alive. That is what makes the income last a lifetime.

### S07 · We replayed history · VO in **1:37.2** · fits in **20.6 s** · 45 words (≈18 s)

*Picture:* The site’s replay: 2,000 Filipino women retire in 1965 with real market history since. The lamp drawn alone goes out in 1977 with half of them alive; the pool’s lights stay on to 1999.

**ElevenLabs text** (paste this):

> We replayed history: two thousand Filipino women retiring in nineteen sixty-five, through the real markets that followed. Drawn alone, their savings ran out in nineteen seventy-seven... with half of them still alive. [warmly] The pool paid every one of them, for as long as they lived.

Plain script, for reading or subtitles: We replayed history: two thousand Filipino women retiring in nineteen sixty-five, through the real markets that followed. Drawn alone, their savings ran out in nineteen seventy-seven, with half of them still alive. The pool paid every one of them, for as long as they lived.

### S08 · It’s live · VO in **1:58.2** · fits in **32.6 s** · 67 words (≈27 s)

*Picture:* Member #0 on an iPhone: joining, a Face ID check-in verified on-chain, "Worth US$24.97". On-chain receipts with real blocks and hashes. The Live pool: one member, one settlement, $14.86 S&P 500, $7.08 T-bills, $3.04 USDG.

**ElevenLabs text** (paste this):

> [confident] And it is live, on mainnet. Our first member joined from an iPhone. Proof of life is a passkey: one Face ID tap every three months, verified by the chain itself. Twenty-five U-S-D-G went in, and the first monthly settlement invested it: about fifteen dollars in the S and P five hundred, seven in Treasury bills, three in cash. Every number here is read straight from the chain.

Plain script, for reading or subtitles: And it is live on mainnet. Our first member joined from an iPhone. Proof of life is a passkey: one Face ID tap every three months, verified by the chain itself. Twenty-five U-S-D-G went in, and the first monthly settlement invested it: about fifteen dollars in the S and P 500, seven in Treasury bills, three in cash. Every number here is read straight from the chain.

### S09 · Nobody can take the pot · VO in **2:31.2** · fits in **21.6 s** · 51 words (≈20 s)

*Picture:* Checklist over the night sky, each line with its contract: tables sealed (Actuary), 48-hour timelock, a check-in answers a false death report and any death can be undone for five years (LifeRegistry), 59 planted bugs all caught, four independent reviews.

**ElevenLabs text** (paste this):

> Nobody can take the pot. Not even us. The tables are sealed. Every rule change waits forty-eight hours, in public. A false death report is answered by one check-in... and any death can be undone for five years. We planted fifty-nine bugs in our own contracts. [confident] The tests caught every one.

Plain script, for reading or subtitles: Nobody can take the pot, not even us. The tables are sealed. Every rule change waits forty-eight hours in public. A false death report is answered by one check-in, and any death can be undone for five years. We planted fifty-nine bugs in our own contracts; the tests caught every one.

### S10 · Habambuhay · VO in **2:53.2** · fits in **16.2 s** · 20 words (≈8 s)

*Picture:* The same question in Filipino, answered with "habambuhay" written by hand. The sun sets over the sea. End card: the wordmark, tonti-life.vercel.app, Robinhood Chain · Arbitrum Stylus · USDG.

**ElevenLabs text** (paste this):

> In English... and in Filipino. [warmly] Habambuhay. For life. Ask it about your own mother, at tonti-life dot vercel dot app.

Plain script, for reading or subtitles: In English, and in Filipino. Habambuhay: for life. Ask it about your own mother, at tonti-life dot vercel dot app.

## 3. Music (Eleven Music)

One instrumental track of **3:09.9**. In Eleven Music, paste the prompt below. If your plan lets you lay out sections, use the section plan; otherwise paste the whole prompt.

**Prompt:**

> Instrumental cinematic underscore for a heartfelt fintech product film about lifelong income for families. Warm, human, hopeful, never corporate. Felt piano, soft string pads, a gentle plucked arpeggio, light modern percussion that arrives late. 72 BPM. No vocals, no vocal chops, no big drops. It must sit under a narrator: keep the mid-range clear and the arrangement sparse, with dynamics carried by texture rather than volume. Starts intimate and a little melancholic, lifts into quiet confidence, and resolves warmly at sunset, ending on a sustained piano chord that rings out. Length 3:09.9.

**Section plan** (times match the picture):

| From | To | Scene | Music |
|---|---|---|---|
| 0:00 | 0:25.0 | Singapore; money home | Solo felt piano, sparse, warm, slightly melancholic; room ambience; no drums |
| 0:25.0 | 0:33.0 | Title: the lamp lights | String pad swells in; a single celesta or bell note on the lamp at 0:27.3 |
| 0:33.0 | 1:08.8 | The live quote, run it yourself | A gentle plucked arpeggio begins, curious and hopeful; very light |
| 1:08.8 | 1:36.8 | How it works | Soft kick and shaker join; cello line; steady forward motion |
| 1:36.8 | 1:57.8 | History replay | Pull back: drums out, piano and strings, reflective; a dip at 1:46.2 when the lone lamp goes out |
| 1:57.8 | 2:30.8 | It's live: the first member | Fullest point: percussion back, warm and confident, uplifting but restrained |
| 2:30.8 | 2:52.8 | Nobody can take the pot | Steady, resolute, lower register, sustained strings |
| 2:52.8 | 3:09.4 | Habambuhay; end card | The opening piano motif returns over strings; final swell at 3:04.4; ring out to silence |

**Mix:** music at about −18 to −22 dB under the narration. Let it come up to about −10 dB where nobody speaks: the end card from 3:04.4, and the short gaps between lines.

## 4. Sound effects (ElevenLabs Text to Sound)

Each effect is placed on a moment that exists in the picture. Keep them quiet, about −20 to −26 dB: the voice leads, the sound only confirms what the eye sees. Generate each prompt once and reuse the ones that repeat (the ticks, the chime, the pencil).

| At | Moment | Prompt |
|---|---|---|
| 0:00.0 | Opening breath | Low, airy cinematic swell rising from silence, soft and warm, no impact, 3 seconds |
| 0:14.0 | Money leaving home (loop under the arcs) | Very soft sparkling chime particles drifting, delicate, like tiny lights, quiet, 6 seconds |
| 0:21.7 | The money stops | Single soft low piano-like tone that fades, gentle, melancholic, 3 seconds |
| 0:27.3 | The lamp lights (wordmark) | Warm soft glowing chime, a lantern being lit, gentle shimmer, 1.5 seconds |
| 0:46.1 | Tap: Ask Robinhood Chain | Soft clean UI button tap, subtle, 0.3 seconds |
| 0:47.3 | The answer arrives | Soft magical confirmation tone with a faint warm shimmer, 1 second |
| 0:47.7 | "for life" written by hand | Short pencil writing on paper, light, 1 second |
| 1:05.4 | Same answer, from a node we don’t run | Gentle positive tick, clean and minimal, 0.5 seconds |
| 1:08.8 | Arcs draw across the map | Soft airy whooshes, light and quick, 3 seconds |
| 1:18.5 | To the sleeves | Short soft whoosh transition, 0.8 seconds |
| 1:26.8 | A light goes out | Single soft low tone, a small light switching off, 1 second |
| 1:28.0 | Its share flows to the others | Delicate rising shimmer, warm, 2 seconds |
| 1:46.2 | 1977: the lamp drawn alone goes out | A single lamp switching off, soft click and fade, 1 second |
| 1:58.4 | Receipt: Joined | Soft quick data tick, like a receipt printing, subtle, 0.6 seconds |
| 2:08.4 | Receipt: Face ID check-in | Soft quick data tick, subtle, 0.6 seconds |
| 2:10.8 | Receipt: Identity attested | Soft quick data tick, subtle, 0.6 seconds |
| 2:15.1 | Receipt: Paid in | Soft quick data tick, subtle, 0.6 seconds |
| 2:15.8 | Receipt: first settlement | Soft quick data tick, subtle, 0.6 seconds |
| 2:17.1 | Receipt: invested on mainnet | Soft quick data tick, subtle, 0.6 seconds |
| 2:34.4 | Checklist tick 1 | Soft, low, satisfying check mark tick, 0.4 seconds |
| 2:35.9 | Checklist tick 2 | Soft, low, satisfying check mark tick, 0.4 seconds |
| 2:39.0 | Checklist tick 3 | Soft, low, satisfying check mark tick, 0.4 seconds |
| 2:42.5 | Checklist tick 4 | Soft, low, satisfying check mark tick, 0.4 seconds |
| 2:45.7 | Checklist tick 5 | Soft, low, satisfying check mark tick, 0.4 seconds |
| 2:49.0 | Checklist tick 6 | Soft, low, satisfying check mark tick, 0.4 seconds |
| 2:55.4 | "habambuhay" written on the site | Short pencil writing on paper, light, 1 second |
| 3:04.7 | The lamp lights again (end card) | Warm soft glowing chime, a lantern being lit, gentle shimmer, 1.5 seconds |

## 5. Putting it together

1. **Timeline:** 1920×1080, 30 fps. Put `picture-lock.mp4` on V1. It has no audio.
2. **Voice:** put `VO_S01`…`VO_S10` on A1, each at its "VO in" time from section 2.
3. **Music** on A2 from 0:00, ducked under the voice (section 3).
4. **Effects** on A3 at the times in section 4.
5. **Captions:** `captions.srt` is timed to this script. Upload it to YouTube as subtitles, or use `picture-lock-captions.mp4`, which has them burned in. If your final voice timing differs, YouTube can re-time an uploaded transcript automatically.
6. **Export:** H.264 High, 1080p30, 16–20 Mbps; AAC 320 kbps 48 kHz; **−14 LUFS integrated, −1 dBTP true peak** (YouTube's target).

**Want it synced for you?** Put the ten `VO_Sxx` files (and the music, if you like) in `/media/dflame/UNIQ/arbit/video/vo/` and tell me. I'll measure each line and re-render the picture so every scene fits its voice exactly, with captions timed from the real audio. You can still re-edit afterwards.

## 6. What's real in this video

- **Recorded from the live site** at https://tonti-life.vercel.app, on Robinhood Chain mainnet, in single continuous takes: the question and its answer (1.2 s from tap to number), Run it yourself, the history replay, the join, the check-in, the account and the Live pool.
- **The member scenes are a replay** of member #0's real join (transaction `0xb5ddd196…`), sent through the live site. The video labels them as a replay. If you record your own iPhone (account page, then a Face ID check-in), I'll swap it in: put the file in `/media/dflame/UNIQ/arbit/video/iphone/`.
- **Every number on screen** comes from `video/data/facts.json`, read from the chain when the video was built: blocks, transactions, gas, holdings, and the member's value.
- **Explanations** (the map, the sleeves, the lights, the checklist) are motion design, drawn from the same facts.
