// The single source of truth for the cut: scene order, lengths, and the voiceover line each scene
// carries. VOICEOVER.md, TIMELINE.md and captions.srt are generated from this file
// (scripts/docs.ts), so the picture, the script and the captions can't drift apart.
//
// Narration: ~150 words a minute (Eleven v4, speed 1.0). Each line is written to finish about a
// second before its scene ends, leaving room to breathe.
import takes from '../../data/takes.json';

export type Scene = { id: string; title: string; frames: number; vo: string; tts: string; picture: string };

// The recorded scenes take their length from the takes themselves (video/data/takes.json).
type Take = { frames: number; seconds: number; marks: Record<string, number> };
const T = takes as unknown as Record<string, Take>;
export const quote = T['S4-quote'];
export const lights = T['S7-lights'];
export const fil = T['S10-fil'];
// S04 runs from the take's first frame to the chart; S05 continues the same take: one recording.
export const S04_LEN = quote.marks.chart + 15;
export const S05_LEN = quote.frames - S04_LEN;
export const S07_FROM = Math.max(0, lights.marks.title - 20);
export const S07_LEN = lights.frames - S07_FROM;
export const S10_Q = Math.min(80, fil.marks.question + 10);
export const S10_A = fil.frames - (fil.marks.answer - 8);
export const S10_LEN = S10_Q + S10_A + 75 + 150;

export const SCENES: Scene[] = [
  {
    id: 'S01', title: 'Singapore runs on them', frames: 390,
    picture: 'Marina Bay at night from the SkyPark, then an everyday HDB estate in Tampines (Wikimedia Commons, CC BY 3.0). Kinetic type: "1,635,700 foreign workers keep Singapore running" (Ministry of Manpower, Dec 2025), "None of them can save into its pension, CPF."',
    vo: 'Singapore runs on one point six million foreign workers. Not one of them can save into the city’s pension, CPF. Every month, they send money home.',
    tts: '[calm] Singapore runs on one point six million foreign workers. Not one of them can save into the city’s pension... C-P-F. Every month, they send money home.',
  },
  {
    id: 'S02', title: 'Parents with no pension', frames: 360,
    picture: 'Motion design: money leaves Singapore every month as lights along arcs to twelve home countries; on "the money stops" the flows stop and the homes go dark. Type: "Only 24% of older people in South Asia receive a pension" (ILO).',
    vo: 'Back home, their parents have no pension either. Only one in four older people in South Asia receives one. When the work stops, the money stops.',
    tts: 'Back home, their parents have no pension either. Only one in four older people in South Asia receives one. [softly] When the work stops... the money stops.',
  },
  {
    id: 'S03', title: 'Tonti', frames: 240,
    picture: 'The site’s dusk sky over the bay; the sun settles; the lamp in the wordmark lights. "Income for life, on Robinhood Chain."',
    vo: 'Tonti: a pension pool that pays an income for life, running entirely on Robinhood Chain.',
    tts: '[warmly] Tonti. A pension pool that pays an income for life... running entirely on Robinhood Chain.',
  },
  {
    id: 'S04', title: 'One question, answered by the chain', frames: S04_LEN,
    picture: 'One continuous take of the live site: the question, the form (Mother, Philippines, born 1966, income from 65, $5,000 now, $50 a month), Ask, the answer from the chain in 1.2 s, the sun settling, US$67.67 a month "for life", the chart where saved-alone runs out at 81.',
    vo: 'Ask what any worker would ask: how much would my mother get, every month, for the rest of her life? A contract on the chain simulates five hundred and twelve futures and answers in about a second. Sixty-seven dollars a month, for life. Saved alone, it would run out at eighty-one, with even odds she is still alive.',
    tts: 'Ask what any worker would ask: how much would my mother get, every month, for the rest of her life? A contract on the chain simulates five hundred and twelve futures... and answers in about a second. [warmly] Sixty-seven dollars a month. For life. Saved alone, it would run out at eighty-one... with even odds she is still alive.',
  },
  {
    id: 'S05', title: 'Run it yourself', frames: S05_LEN,
    picture: '"Run it yourself": the exact call, re-run from the browser on a public node. "Same answer, from a node we don’t run." Its gas, under a third of one call’s limit.',
    vo: 'Nothing is precomputed. Here is the exact call. Re-run it on a public node, and you get the same answer.',
    tts: 'Nothing is precomputed. Here is the exact call. Re-run it on a public node... and you get the same answer.',
  },
  {
    id: 'S06', title: 'How it works', frames: 840,
    picture: 'Explainer. Thirteen home countries joined to Singapore; "1,846 cohorts, priced from UN tables, sealed on-chain". The money in three sleeves: S&P 500, T-bills, USDG. Lights: when one goes out, its share brightens the rest.',
    vo: 'Members are pooled by country, sex and birth year: eighteen hundred and forty-six cohorts, priced from United Nations life tables and sealed on-chain. Savings are invested in the S and P 500, Treasury bills and U-S-D-G. When a member dies, the savings they put at risk are shared among those still alive. That is what makes the income last a lifetime.',
    tts: 'Members are pooled by country, sex and birth year: eighteen hundred and forty-six cohorts, priced from United Nations life tables, and sealed on-chain. Savings are invested in the S and P five hundred, Treasury bills, and U-S-D-G. When a member dies, the savings they put at risk are shared among those still alive. [warmly] That is what makes the income last a lifetime.',
  },
  {
    id: 'S07', title: 'We replayed history', frames: S07_LEN,
    picture: 'The site’s replay: 2,000 Filipino women retire in 1965 with real market history since. The lamp drawn alone goes out in 1977 with half of them alive; the pool’s lights stay on to 1999.',
    vo: 'We replayed history: two thousand Filipino women retiring in nineteen sixty-five, through the real markets that followed. Drawn alone, their savings ran out in nineteen seventy-seven, with half of them still alive. The pool paid every one of them, for as long as they lived.',
    tts: 'We replayed history: two thousand Filipino women retiring in nineteen sixty-five, through the real markets that followed. Drawn alone, their savings ran out in nineteen seventy-seven... with half of them still alive. [warmly] The pool paid every one of them, for as long as they lived.',
  },
  {
    id: 'S08', title: 'It’s live', frames: 990,
    picture: 'Member #0 on an iPhone: joining, a Face ID check-in verified on-chain, "Worth US$24.97". On-chain receipts with real blocks and hashes. The Live pool: one member, one settlement, $14.86 S&P 500, $7.08 T-bills, $3.04 USDG.',
    vo: 'And it is live on mainnet. Our first member joined from an iPhone. Proof of life is a passkey: one Face ID tap every three months, verified by the chain itself. Twenty-five U-S-D-G went in, and the first monthly settlement invested it: about fifteen dollars in the S and P 500, seven in Treasury bills, three in cash. Every number here is read straight from the chain.',
    tts: '[confident] And it is live, on mainnet. Our first member joined from an iPhone. Proof of life is a passkey: one Face ID tap every three months, verified by the chain itself. Twenty-five U-S-D-G went in, and the first monthly settlement invested it: about fifteen dollars in the S and P five hundred, seven in Treasury bills, three in cash. Every number here is read straight from the chain.',
  },
  {
    id: 'S09', title: 'Nobody can take the pot', frames: 660,
    picture: 'Checklist over the night sky, each line with its contract: tables sealed (Actuary), 48-hour timelock, a check-in answers a false death report and any death can be undone for five years (LifeRegistry), 59 planted bugs all caught, four independent reviews.',
    vo: 'Nobody can take the pot, not even us. The tables are sealed. Every rule change waits forty-eight hours in public. A false death report is answered by one check-in, and any death can be undone for five years. We planted fifty-nine bugs in our own contracts; the tests caught every one.',
    tts: 'Nobody can take the pot. Not even us. The tables are sealed. Every rule change waits forty-eight hours, in public. A false death report is answered by one check-in... and any death can be undone for five years. We planted fifty-nine bugs in our own contracts. [confident] The tests caught every one.',
  },
  {
    id: 'S10', title: 'Habambuhay', frames: S10_LEN,
    picture: 'The same question in Filipino, answered with "habambuhay" written by hand. The sun sets over the sea. End card: the wordmark, tonti-life.vercel.app, Robinhood Chain · Arbitrum Stylus · USDG.',
    vo: 'In English, and in Filipino. Habambuhay: for life. Ask it about your own mother, at tonti-life dot vercel dot app.',
    tts: 'In English... and in Filipino. [warmly] Habambuhay. For life. Ask it about your own mother, at tonti-life dot vercel dot app.',
  },
];

export const TOTAL = SCENES.reduce((n, s) => n + s.frames, 0);
export const startOf = (id: string) => SCENES.slice(0, SCENES.findIndex((s) => s.id === id)).reduce((n, s) => n + s.frames, 0);
