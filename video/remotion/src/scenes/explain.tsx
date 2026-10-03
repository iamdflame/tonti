import { AbsoluteFill, Sequence, interpolate, random, useCurrentFrame } from 'remotion';
import { evolvePath, getLength } from '@remotion/paths';
import { Sky } from '../components/Sky';
import { Grain, Kinetic, Label, prog } from '../components/kit';
import { C, inOut, mono, sans } from '../theme';
import facts from '../../../data/facts.json';

const clamp = { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;

// Where each priced country's people come from (a capital or main city), and Singapore, where they work.
export const PLACES: [string, string, number, number][] = [
  ['PHL', 'Philippines', 121.0, 14.6], ['IDN', 'Indonesia', 106.8, -6.2], ['IND', 'India', 77.2, 28.6], ['BGD', 'Bangladesh', 90.4, 23.8],
  ['MMR', 'Myanmar', 96.2, 16.8], ['LKA', 'Sri Lanka', 79.9, 6.9], ['NPL', 'Nepal', 85.3, 27.7], ['VNM', 'Vietnam', 105.8, 21.0],
  ['THA', 'Thailand', 100.5, 13.8], ['MYS', 'Malaysia', 101.7, 3.1], ['CHN', 'China', 116.4, 39.9], ['GHA', 'Ghana', -0.19, 5.6],
];
export const SGP: [number, number] = [103.8, 1.35];
export // Label side and nudge per place, so Southeast Asia's labels don't collide.
const LBL: Record<string, [number, 'l' | 'r']> = {
  PHL: [0, 'r'], IDN: [6, 'r'], IND: [0, 'l'], BGD: [-4, 'r'], MMR: [-6, 'l'], LKA: [0, 'l'], NPL: [-22, 'r'],
  VNM: [-10, 'r'], THA: [14, 'l'], MYS: [8, 'l'], CHN: [0, 'r'], GHA: [-36, 'r'],
};
const px = (lon: number, lat: number) => [110 + ((lon + 12) / 140) * 1700, 170 + ((46 - lat) / 58) * 760] as const;

function CohortMap() {
  const f = useCurrentFrame();
  const [sx, sy] = px(...SGP);
  const n = facts.mortality.cohorts;
  const count = Math.round(interpolate(f, [70, 150], [0, n], { ...clamp, easing: inOut }));
  return (
    <AbsoluteFill>
      <svg width={1920} height={1080} style={{ position: 'absolute', inset: 0 }}>
        {/* a faint graticule, so the dots read as a map without drawing borders */}
        {Array.from({ length: 8 }, (_, i) => <line key={`v${i}`} x1={110 + i * 243} y1={150} x2={110 + i * 243} y2={950} stroke="rgba(169,180,198,0.07)" />)}
        {Array.from({ length: 5 }, (_, i) => <line key={`h${i}`} x1={110} y1={170 + i * 190} x2={1810} y2={170 + i * 190} stroke="rgba(169,180,198,0.07)" />)}
        {PLACES.map(([iso, , lon, lat], i) => {
          const [x, y] = px(lon, lat);
          const lift = iso === 'GHA' ? 360 : 120 + Math.hypot(x - sx, y - sy) * 0.25;
          const d = `M ${sx} ${sy} Q ${(sx + x) / 2} ${Math.min(sy, y) - lift} ${x} ${y}`;
          const start = iso === 'GHA' ? 112 : 10 + i * 6;
          const p = prog(f, start, iso === 'GHA' ? 40 : 28);
          const ev = evolvePath(p, d);
          return <path key={iso} d={d} fill="none" stroke={iso === 'GHA' ? C.lamp : 'rgba(255,212,138,0.55)'} strokeWidth={iso === 'GHA' ? 3 : 2} strokeDasharray={ev.strokeDasharray} strokeDashoffset={ev.strokeDashoffset} />;
        })}
        <circle cx={sx} cy={sy} r={10} fill={C.coral} />
        <circle cx={sx} cy={sy} r={26} fill="none" stroke="rgba(242,120,92,0.5)" strokeWidth={2} opacity={0.5 + 0.5 * Math.sin(f / 9)} />
      </svg>
      {PLACES.map(([iso, name, lon, lat], i) => {
        const [x, y] = px(lon, lat);
        const start = (iso === 'GHA' ? 112 : 10 + i * 6) + (iso === 'GHA' ? 38 : 24);
        const p = prog(f, start, 16);
        const [dy, side] = LBL[iso];
        const left = side === 'l';
        return (
          <div key={iso} style={{ position: 'absolute', left: x, top: y, transform: 'translate(-50%,-50%)', opacity: p }}>
            <div style={{ width: 16, height: 16, borderRadius: 99, background: C.lamp, boxShadow: `0 0 ${18 * p}px ${5 * p}px rgba(255,178,63,0.55)` }} />
            <div style={{ position: 'absolute', top: -8 + dy, [left ? 'right' : 'left']: 24, whiteSpace: 'nowrap', fontFamily: sans, fontSize: 25, fontWeight: 600, color: iso === 'GHA' ? C.lampSoft : 'rgba(255,255,255,0.88)' }}>{name}</div>
          </div>
        );
      })}
      <div style={{ position: 'absolute', left: sx - 20, top: sy + 22, fontFamily: sans, fontSize: 25, fontWeight: 700, color: C.coral }}>Singapore</div>
      <div style={{ position: 'absolute', left: 140, top: 150 }}>
        <div style={{ fontFamily: mono, fontSize: 120, fontWeight: 700, color: '#fff', lineHeight: 1, opacity: prog(f, 66, 14) }}>{count.toLocaleString('en-US')}</div>
        <Kinetic lines={['cohorts, each priced from', 'UN life tables. Sealed on-chain.']} start={88} size={44} weight={600} color="rgba(255,255,255,0.9)" style={{ marginTop: 14 }} />
        <div style={{ marginTop: 22 }}><Label at={120}>{facts.mortality.countries} countries · both sexes · born 1935–2005 · UN World Population Prospects 2024</Label></div>
      </div>
    </AbsoluteFill>
  );
}

function Sleeves() {
  const f = useCurrentFrame();
  const h = facts.holdings;
  const rows = [
    { name: 'S&P 500', token: 'SPY', usd: h.sp500.usd },
    { name: 'US Treasury bills', token: 'SGOV', usd: h.tbills.usd },
    { name: 'Cash, lent in Morpho', token: 'USDG', usd: h.cash.usd },
  ];
  const total = rows.reduce((n, r) => n + r.usd, 0);
  return (
    <AbsoluteFill style={{ padding: '170px 160px', justifyContent: 'center' }}>
      <Kinetic lines={['Savings are invested.']} size={84} />
      <div style={{ marginTop: 56, display: 'grid', gap: 34, width: 1300 }}>
        {rows.map((r, i) => {
          const p = prog(f, 18 + i * 9, 34);
          const share = r.usd / total;
          return (
            <div key={r.token} style={{ opacity: prog(f, 14 + i * 9, 14) }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', fontFamily: sans, fontSize: 34, fontWeight: 600, color: '#fff' }}>
                <span>{r.name} <span style={{ fontFamily: mono, fontSize: 26, color: C.mist, marginLeft: 12 }}>{r.token}</span></span>
                <span style={{ fontFamily: mono, color: C.lampSoft }}>{(share * 100 * p).toFixed(1)}%</span>
              </div>
              <div style={{ marginTop: 12, height: 22, borderRadius: 99, background: 'rgba(255,255,255,0.08)' }}>
                <div style={{ width: `${share * 100 * p}%`, height: '100%', borderRadius: 99, background: `linear-gradient(90deg, ${C.lamp}, ${C.lampSoft})`, boxShadow: '0 0 24px rgba(255,178,63,0.35)' }} />
              </div>
            </div>
          );
        })}
      </div>
      <div style={{ marginTop: 44 }}><Label at={40}>Member #0’s first settlement, read from the Treasury contract on mainnet</Label></div>
    </AbsoluteFill>
  );
}

function Credits() {
  const f = useCurrentFrame();
  const cols = 12, rows = 5, gap = 64;
  const dead = 29;
  const die = prog(f, 40, 30);
  const flow = prog(f, 66, 60);
  const lift = prog(f, 100, 40);
  const ox = 960 - ((cols - 1) * gap) / 2, oy = 330;
  const at = (k: number) => [ox + (k % cols) * gap, oy + Math.floor(k / cols) * gap] as const;
  const [dx, dy] = at(dead);
  return (
    <AbsoluteFill>
      <svg width={1920} height={1080} style={{ position: 'absolute', inset: 0 }}>
        {Array.from({ length: cols * rows }, (_, k) => {
          const [x, y] = at(k);
          const isDead = k === dead;
          const glow = isDead ? 1 - die : 0.55 + 0.45 * lift;
          return (
            <g key={k}>
              <circle cx={x} cy={y} r={20} fill={isDead ? `rgba(28,49,87,${0.6 + 0.4 * die})` : C.lamp} opacity={isDead ? 1 : 0.65 + 0.35 * lift} />
              {!isDead && <circle cx={x} cy={y} r={20 + 14 * glow} fill="rgba(255,178,63,0.12)" />}
              {isDead && <circle cx={x} cy={y} r={20} fill={C.lamp} opacity={1 - die} />}
            </g>
          );
        })}
        {Array.from({ length: 22 }, (_, i) => {
          const target = Math.floor(random(`t${i}`) * cols * rows);
          if (target === dead) return null;
          const [tx, ty] = at(target);
          const local = interpolate(flow, [i / 40, i / 40 + 0.45], [0, 1], clamp);
          const x = dx + (tx - dx) * local, y = dy + (ty - dy) * local - Math.sin(Math.PI * local) * 40;
          return local > 0 && local < 1 ? <circle key={i} cx={x} cy={y} r={5} fill={C.lampSoft} opacity={0.9} /> : null;
        })}
      </svg>
      <div style={{ position: 'absolute', left: 160, top: 120, width: 1600 }}>
        <Kinetic lines={['When a member dies, the savings they put', 'at risk are shared by those still alive.']} start={6} size={58} weight={700} />
      </div>
      <div style={{ position: 'absolute', left: 160, top: 720, width: 1600 }}>
        <Kinetic lines={['That is what makes income last a lifetime.']} start={150} size={58} weight={700} color={C.lampSoft} />
        <div style={{ marginTop: 26 }}><Label at={172}>Shared fairly by age and amount, even in small pools</Label></div>
      </div>
    </AbsoluteFill>
  );
}

/** S06: how it works (explainer). */
export function S06() {
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <AbsoluteFill style={{ opacity: 0.35 }}><Sky sun={0.12} sunX={0.86} /></AbsoluteFill>
      <AbsoluteFill style={{ background: 'linear-gradient(180deg, rgba(13,26,48,0.6), rgba(13,26,48,0.88))' }} />
      <Sequence durationInFrames={300}><Fader len={300}><CohortMap /></Fader></Sequence>
      <Sequence from={290} durationInFrames={230}><Fader len={230}><Sleeves /></Fader></Sequence>
      <Sequence from={510} durationInFrames={330}><Fader len={330} outt={1}><Credits /></Fader></Sequence>
      <Grain />
    </AbsoluteFill>
  );
}

function Fader({ children, len, inn = 12, outt = 12 }: { children: React.ReactNode; len: number; inn?: number; outt?: number }) {
  const f = useCurrentFrame();
  const o = Math.min(interpolate(f, [0, inn], [0, 1], clamp), interpolate(f, [len - outt, len], [1, 0], clamp));
  return <AbsoluteFill style={{ opacity: o }}>{children}</AbsoluteFill>;
}

/** S09: nobody can take the pot. Each line names the contract that enforces it. */
export function S09() {
  const f = useCurrentFrame();
  // each line lands on its words in the narration
  const at = [100, 145, 240, 345, 440, 540];
  const lines: [string, string][] = [
    ['The mortality tables are sealed. No one can change them.', 'Actuary.seal()'],
    ['Every rule change waits 48 hours, in public.', 'TimelockController'],
    ['A false death report is answered by one check-in.', 'LifeRegistry'],
    ['Any death can be undone for five years.', 'LifeRegistry.revive()'],
    [`${facts.tests.plantedBugs} bugs planted in our own contracts. All caught.`, 'mutation tests'],
    ['Four independent adversarial reviews.', 'every finding fixed or disclosed'],
  ];
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <AbsoluteFill style={{ opacity: 0.5 }}><Sky sun={interpolate(f, [0, 660], [0.16, 0.08], clamp)} sunX={0.84} /></AbsoluteFill>
      <AbsoluteFill style={{ background: 'linear-gradient(90deg, rgba(13,26,48,0.94) 0%, rgba(13,26,48,0.75) 60%, rgba(13,26,48,0.4) 100%)' }} />
      <div style={{ position: 'absolute', left: 160, top: 120 }}>
        <Kinetic lines={['Nobody can take the pot.', <span key="n" style={{ color: C.lampSoft }}>Not even us.</span>]} size={92} stagger={10} />
      </div>
      <div style={{ position: 'absolute', left: 160, top: 420, display: 'grid', gap: 26, width: 1600 }}>
        {lines.map(([text, tag], i) => {
          const p = prog(f, at[i], 22);
          return (
            <div key={i} style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', gap: 40, opacity: p, transform: `translateX(${(1 - p) * -24}px)` }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 22, fontFamily: sans, fontSize: 40, fontWeight: 600, color: '#fff' }}>
                <svg width="34" height="34" viewBox="0 0 24 24" style={{ flexShrink: 0 }}><circle cx="12" cy="12" r="11" fill="rgba(255,178,63,0.16)" /><path d="M7 12.5l3.2 3.2L17.5 8.5" fill="none" stroke={C.lamp} strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" strokeDasharray="20" strokeDashoffset={20 * (1 - prog(f, at[i] + 8, 14))} /></svg>
                {text}
              </div>
              <div style={{ fontFamily: mono, fontSize: 24, color: C.mist, whiteSpace: 'nowrap' }}>{tag}</div>
            </div>
          );
        })}
      </div>
      <Grain />
    </AbsoluteFill>
  );
}

export { getLength };
