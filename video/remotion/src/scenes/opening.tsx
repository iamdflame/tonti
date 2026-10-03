import { AbsoluteFill, Sequence, interpolate, useCurrentFrame } from 'remotion';
import { Sky } from '../components/Sky';
import { evolvePath } from '@remotion/paths';
import { Footage, Grain, Kinetic, Label, Wordmark, prog } from '../components/kit';
import { PLACES, SGP } from './explain';
import { C, inOut, mono, out, sans } from '../theme';

const fade = (f: number, len: number, inn = 12, outt = 12) => Math.min(interpolate(f, [0, inn], [0, 1], { extrapolateRight: 'clamp' }), interpolate(f, [len - outt, len], [1, 0], { extrapolateLeft: 'clamp' }));

/** S01: Singapore runs on them. Marina Bay at night, then an everyday HDB estate (Wikimedia Commons). */
export function S01() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <Sequence durationInFrames={150}>
        <AbsoluteFill style={{ opacity: fade(f, 150, 18, 10) }}>
          <Footage src="stock/commons-mbs-skypark.webm" push={0.1} drift={[-30, 0]} grade={0.2} />
        </AbsoluteFill>
      </Sequence>
      <Sequence from={140} durationInFrames={130}>
        <Sub len={130}><Footage src="stock/commons-tampines.webm" from={150} push={0.06} grade={0.42} /></Sub>
      </Sequence>
      <Sequence from={260} durationInFrames={130}>
        <Sub len={130} outt={0}><Footage src="stock/commons-mbs-skypark.webm" from={255} push={0.08} drift={[20, 0]} grade={0.24} /></Sub>
      </Sequence>
      <AbsoluteFill style={{ background: 'linear-gradient(90deg, rgba(8,14,28,0.62) 0%, rgba(8,14,28,0.3) 38%, transparent 62%)' }} />
      <AbsoluteFill style={{ padding: '0 140px', justifyContent: 'center' }}>
        <Sequence from={22} durationInFrames={250} layout="none">
          <Out len={250}>
            <Kinetic lines={[<span key="a" style={{ fontFamily: mono, color: C.lampSoft, fontWeight: 700 }}>1,635,700</span>, 'foreign workers keep', 'Singapore running.']} size={96} stagger={8} />
            <div style={{ marginTop: 34 }}><Label at={40}>Ministry of Manpower, December 2025</Label></div>
          </Out>
        </Sequence>
        <Sequence from={272} durationInFrames={118} layout="none">
          <Kinetic lines={['None of them can save', 'into its pension, CPF.']} size={96} stagger={8} />
        </Sequence>
      </AbsoluteFill>
      <div style={{ position: 'absolute', right: 40, bottom: 28, fontFamily: sans, fontSize: 17, color: 'rgba(255,255,255,0.55)' }}>
        Footage: LN9267, vicsonhuang · Wikimedia Commons · CC BY 3.0
      </div>
      <Grain />
    </AbsoluteFill>
  );
}

function Sub({ children, len, inn = 14, outt = 14 }: { children: React.ReactNode; len: number; inn?: number; outt?: number }) {
  const f = useCurrentFrame();
  return <AbsoluteFill style={{ opacity: fade(f, len, inn, Math.max(1, outt)) }}>{children}</AbsoluteFill>;
}

function Out({ children, len }: { children: React.ReactNode; len: number }) {
  const f = useCurrentFrame();
  const o = interpolate(f, [len - 16, len], [1, 0], { extrapolateLeft: 'clamp' });
  return <div style={{ opacity: o, transform: `translateY(${(1 - o) * -10}px)` }}>{children}</div>;
}

/** S02: money goes home every month; their parents have no pension. When the work stops, the
 * money stops. Remittances as lamp-coloured lights along the arcs from Singapore. */
// S02 shows only Asia: its own projection (equal degrees both ways), filling the right of the frame.
const pa = (lon: number, lat: number) => [860 + (lon - 66) * 16.9, 110 + (44 - lat) * 16.5] as const;
const SIDE: Record<string, [number, number, 'l' | 'r']> = {
  IND: [0, 0, 'l'], NPL: [0, -30, 'r'], BGD: [0, 0, 'r'], MMR: [0, 0, 'l'], LKA: [0, 0, 'l'], VNM: [0, 0, 'r'],
  THA: [0, 18, 'r'], MYS: [0, 0, 'l'], CHN: [0, 0, 'r'], PHL: [0, 0, 'l'], IDN: [0, 0, 'r'],
};

export function S02() {
  const f = useCurrentFrame();
  const [sx, sy] = pa(...SGP);
  const STOP = 262; // "when the work stops... the money stops"
  const dim = prog(f, STOP, 40);
  const homes = PLACES.filter(([iso]) => iso !== 'GHA'); // the South and Southeast Asian homes
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <AbsoluteFill style={{ opacity: 0.32 }}><Sky sun={0.1} sunX={0.86} /></AbsoluteFill>
      <AbsoluteFill style={{ background: 'linear-gradient(180deg, rgba(13,26,48,0.55), rgba(13,26,48,0.9))' }} />
      <svg width={1920} height={1080} style={{ position: 'absolute', inset: 0 }}>
        {Array.from({ length: 8 }, (_, i) => <line key={`v${i}`} x1={110 + i * 243} y1={150} x2={110 + i * 243} y2={950} stroke="rgba(169,180,198,0.07)" />)}
        {Array.from({ length: 5 }, (_, i) => <line key={`h${i}`} x1={110} y1={170 + i * 190} x2={1810} y2={170 + i * 190} stroke="rgba(169,180,198,0.07)" />)}
        {homes.map(([iso, , lon, lat], i) => {
          const [x, y] = pa(lon, lat);
          const lift = 60 + Math.hypot(x - sx, y - sy) * 0.3;
          const cx = (sx + x) / 2, cy = Math.min(sy, y) - lift;
          const d = `M ${sx} ${sy} Q ${cx} ${cy} ${x} ${y}`;
          const draw = evolvePath(prog(f, 6 + i * 4, 30), d);
          // a remittance leaves Singapore every ~40 frames on each arc, until the work stops
          const pulses = Array.from({ length: 6 }, (_, k) => {
            const born = 30 + i * 5 + k * 40;
            if (born > STOP) return null;
            const t = (f - born) / 55;
            if (t < 0 || t > 1) return null;
            const e = 1 - Math.pow(1 - t, 2);
            const bx = (1 - e) * (1 - e) * sx + 2 * (1 - e) * e * cx + e * e * x;
            const by = (1 - e) * (1 - e) * sy + 2 * (1 - e) * e * cy + e * e * y;
            return <circle key={k} cx={bx} cy={by} r={5} fill={C.lampSoft} opacity={Math.sin(Math.PI * t)} />;
          });
          return (
            <g key={iso}>
              <path d={d} fill="none" stroke={`rgba(255,212,138,${0.4 * (1 - dim) + 0.08})`} strokeWidth={2} strokeDasharray={draw.strokeDasharray} strokeDashoffset={draw.strokeDashoffset} />
              {pulses}
            </g>
          );
        })}
        <circle cx={sx} cy={sy} r={10} fill={C.coral} />
      </svg>
      {homes.map(([iso, name, lon, lat], i) => {
        const [x, y] = pa(lon, lat);
        const p = prog(f, 28 + i * 4, 16);
        const [, dy, side] = SIDE[iso];
        const left = side === 'l';
        const lit = 1 - dim;
        return (
          <div key={iso} style={{ position: 'absolute', left: x, top: y, transform: 'translate(-50%,-50%)', opacity: p }}>
            <div style={{ width: 16, height: 16, borderRadius: 99, background: lit > 0.5 ? C.lamp : C.bay2, opacity: 0.35 + 0.65 * lit, boxShadow: `0 0 ${18 * lit}px ${5 * lit}px rgba(255,178,63,0.5)` }} />
            <div style={{ position: 'absolute', top: -8 + dy, [left ? 'right' : 'left']: 26, whiteSpace: 'nowrap', fontFamily: sans, fontSize: 26, fontWeight: 600, color: `rgba(255,255,255,${0.5 + 0.38 * lit})` }}>{name}</div>
          </div>
        );
      })}
      <div style={{ position: 'absolute', left: sx + 22, top: sy + 4, fontFamily: sans, fontSize: 26, fontWeight: 700, color: C.coral }}>Singapore</div>
      <div style={{ position: 'absolute', left: 130, top: 170, width: 760 }}>
        <Sequence from={20} durationInFrames={190} layout="none">
          <Out len={190}>
            <Kinetic lines={['Every month,', 'they send money home.', <span key="p" style={{ color: C.mist }}>Their parents have</span>, <span key="q" style={{ color: C.mist }}>no pension either.</span>]} size={66} weight={700} stagger={9} />
          </Out>
        </Sequence>
        <Sequence from={212} layout="none">
          <Kinetic lines={[<span key="s">Only <span style={{ fontFamily: mono, color: C.lampSoft }}>24%</span> of older</span>, 'people in South Asia', 'receive a pension.']} size={66} weight={700} stagger={8} />
          <div style={{ marginTop: 26 }}><Label at={22}>International Labour Organization, World Social Protection Report 2024–26</Label></div>
        </Sequence>
      </div>
      <Grain />
    </AbsoluteFill>
  );
}

/** S03: the title. The site's own dusk over the bay; the sun settles and the lamp lights. */
export function S03() {
  const f = useCurrentFrame();
  const sun = interpolate(f, [0, 200], [0.78, 0.3], { extrapolateRight: 'clamp', easing: inOut });
  const ignite = prog(f, 70, 40);
  const t = prog(f, 40, 30);
  const sub = prog(f, 105, 26);
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <Sky sun={sun} sunX={0.8} />
      <AbsoluteFill style={{ alignItems: 'center', justifyContent: 'center', flexDirection: 'column', gap: 34, paddingBottom: 90 }}>
        <div style={{ opacity: t, transform: `translateY(${(1 - t) * 26}px) scale(${0.97 + 0.03 * t})` }}>
          <Wordmark size={230} ignite={ignite} />
        </div>
        <div style={{ fontFamily: sans, fontSize: 52, fontWeight: 500, color: 'rgba(255,255,255,0.92)', letterSpacing: '-0.01em', opacity: sub, transform: `translateY(${(1 - sub) * 16}px)` }}>
          Income for life, on Robinhood Chain.
        </div>
      </AbsoluteFill>
      <Grain opacity={0.05} />
    </AbsoluteFill>
  );
}

export { out };
