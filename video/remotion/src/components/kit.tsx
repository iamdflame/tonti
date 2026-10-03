import type { CSSProperties, ReactNode } from 'react';
import { AbsoluteFill, Easing, Img, OffthreadVideo, interpolate, random, staticFile, useCurrentFrame, useVideoConfig } from 'remotion';
import { C, mono, out, sans } from '../theme';

const clamp = { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;

/** 0→1 over [start, start+dur] frames with the house ease-out. */
export const prog = (f: number, start: number, dur: number, easing = out) => interpolate(f, [start, start + dur], [0, 1], { ...clamp, easing });

/** Film grain is added in the final encode (scripts/render.mjs, ffmpeg's noise filter): in software
 * rendering every full-screen layer costs about half a second a frame. Kept so scenes can mark where
 * grain belongs. */
export function Grain(_: { opacity?: number }) {
  return null;
}

export function Vignette({ strength = 0.55 }: { strength?: number }) {
  return <AbsoluteFill style={{ pointerEvents: 'none', background: `radial-gradient(120% 90% at 50% 45%, transparent 55%, rgba(5,10,22,${strength}) 100%)` }} />;
}

/** Stock footage with a slow push and the dusk grade, so it sits in the same world as the UI. The
 * grade, the vignette and an optional left shade (for type) are one layer: each extra full-screen
 * layer costs about half a second a frame in software rendering. */
export function Footage({ src, from = 0, push = 0.08, drift = [0, 0], grade = 0.32, shade = 0, style }: { src: string; from?: number; push?: number; drift?: [number, number]; grade?: number; shade?: number; style?: CSSProperties }) {
  const f = useCurrentFrame();
  const { durationInFrames } = useVideoConfig();
  const t = f / Math.max(1, durationInFrames);
  const s = 1.04 + push * t;
  const layers = [
    `radial-gradient(120% 90% at 50% 45%, transparent 55%, rgba(5,10,22,0.55) 100%)`,
    ...(shade ? [`linear-gradient(90deg, rgba(8,14,28,${shade}) 0%, rgba(8,14,28,${shade * 0.48}) 38%, transparent 62%)`] : []),
    `linear-gradient(180deg, rgba(13,26,48,${grade * 1.25}) 0%, rgba(19,35,63,${grade * 0.75}) 55%, rgba(242,120,92,${grade * 0.16}) 100%)`,
  ];
  return (
    <AbsoluteFill style={{ overflow: 'hidden', backgroundColor: C.night, ...style }}>
      <AbsoluteFill style={{ transform: `scale(${s}) translate(${drift[0] * t}px, ${drift[1] * t}px)` }}>
        <OffthreadVideo src={staticFile(src)} startFrom={from} muted style={{ width: '100%', height: '100%', objectFit: 'cover' }} />
      </AbsoluteFill>
      <AbsoluteFill style={{ background: layers.join(', ') }} />
    </AbsoluteFill>
  );
}

type Key = { at: number; scale: number; x: number; y: number };
/** A screen recording (device pixels) under a virtual camera: keyframes of scale and the point the
 * camera looks at (x, y in 0..1 of the recording), eased between. */
export function Screen({ src, from = 0, keys, radius = 0, shadow = true, style, playbackRate = 1 }: { src: string; from?: number; keys: Key[]; radius?: number; shadow?: boolean; style?: CSSProperties; playbackRate?: number }) {
  const f = useCurrentFrame();
  const at = keys.map((k) => k.at);
  const pick = (k: keyof Key) => (keys.length === 1 ? (keys[0][k] as number) : interpolate(f, at, keys.map((x) => x[k] as number), { ...clamp, easing: Easing.bezier(0.45, 0, 0.2, 1) }));
  const scale = pick('scale');
  const x = pick('x');
  const y = pick('y');
  return (
    <AbsoluteFill style={{ overflow: 'hidden', borderRadius: radius, boxShadow: shadow ? '0 40px 120px -30px rgba(0,0,0,0.6)' : undefined, ...style }}>
      <AbsoluteFill style={{ transformOrigin: '0 0', transform: `translate(${-(x * scale - 0.5) * 100}%, ${-(y * scale - 0.5) * 100}%) scale(${scale})` }}>
        <OffthreadVideo src={staticFile(src)} startFrom={from} playbackRate={playbackRate} muted style={{ width: '100%', height: '100%', objectFit: 'cover' }} />
      </AbsoluteFill>
    </AbsoluteFill>
  );
}

/** An iPhone drawn in CSS (no Apple artwork): rounded titanium edge, black bezel, the island. */
export function Phone({ children, height = 900, style }: { children: ReactNode; height?: number; style?: CSSProperties }) {
  const w = height * (430 / 932);
  const r = height * 0.075;
  return (
    <div style={{ position: 'relative', width: w + 24, height: height + 24, borderRadius: r + 12, background: 'linear-gradient(145deg, #3a4150, #1b1f27 40%, #2c323d)', padding: 12, boxShadow: '0 50px 120px -30px rgba(0,0,0,0.75), inset 0 0 0 1.5px rgba(255,255,255,0.12)', ...style }}>
      <div style={{ position: 'relative', width: w, height, borderRadius: r, overflow: 'hidden', background: '#000', boxShadow: '0 0 0 6px #050608' }}>
        {children}
        <div style={{ position: 'absolute', top: height * 0.014, left: '50%', width: w * 0.3, height: height * 0.037, transform: 'translateX(-50%)', borderRadius: 999, background: '#000' }} />
      </div>
    </div>
  );
}

/** Lines that rise out of a mask, one after another. */
export function Kinetic({ lines, start = 0, stagger = 7, size = 84, weight = 800, color = '#fff', lineHeight = 1.06, style, align = 'left' }: { lines: ReactNode[]; start?: number; stagger?: number; size?: number; weight?: number; color?: string; lineHeight?: number; style?: CSSProperties; align?: 'left' | 'center' }) {
  const f = useCurrentFrame();
  return (
    <div style={{ fontFamily: sans, fontSize: size, fontWeight: weight, color, lineHeight, letterSpacing: '-0.02em', textAlign: align, ...style }}>
      {lines.map((l, i) => {
        const p = prog(f, start + i * stagger, 22);
        return (
          <div key={i} style={{ overflow: 'hidden', paddingBottom: '0.08em' }}>
            <div style={{ transform: `translateY(${(1 - p) * 105}%)`, opacity: interpolate(p, [0, 0.4, 1], [0, 1, 1]) }}>{l}</div>
          </div>
        );
      })}
    </div>
  );
}

/** A small label with the lamp dot: sources, "replay" notes, on-chain facts. */
export function Label({ children, at = 0, style, dark = true }: { children: ReactNode; at?: number; style?: CSSProperties; dark?: boolean }) {
  const f = useCurrentFrame();
  const p = prog(f, at, 18);
  return (
    <div style={{ display: 'inline-flex', alignItems: 'center', gap: 12, padding: '10px 18px', borderRadius: 999, background: dark ? 'rgba(13,26,48,0.72)' : 'rgba(255,255,255,0.9)', color: dark ? C.mist : C.ink2, fontFamily: sans, fontSize: 24, fontWeight: 500, boxShadow: '0 10px 30px -10px rgba(0,0,0,0.4)', border: `1px solid ${dark ? 'rgba(255,255,255,0.1)' : 'rgba(20,33,61,0.1)'}`, opacity: p, transform: `translateY(${(1 - p) * 14}px)`, ...style }}>
      <span style={{ width: 10, height: 10, borderRadius: 99, background: C.lamp, boxShadow: '0 0 12px 2px rgba(255,178,63,0.6)' }} />
      {children}
    </div>
  );
}

/** An on-chain receipt: what happened, the block, the transaction, the gas. Every value from facts. */
export function Receipt({ title, detail, block, hash, gas, at = 0, style }: { title: string; detail?: string; block: number; hash: string; gas?: number; at?: number; style?: CSSProperties }) {
  const f = useCurrentFrame();
  const p = prog(f, at, 20);
  const typed = Math.floor(interpolate(f, [at + 8, at + 30], [0, hash.length], clamp));
  return (
    <div style={{ width: 640, padding: '22px 26px', borderRadius: 22, background: 'rgba(13,26,48,0.86)', border: '1px solid rgba(255,255,255,0.12)', boxShadow: '0 30px 80px -30px rgba(0,0,0,0.7)', color: '#fff', fontFamily: sans, opacity: p, transform: `translateY(${(1 - p) * 24}px)`, ...style }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <span style={{ width: 12, height: 12, borderRadius: 99, background: C.lamp, boxShadow: '0 0 14px 3px rgba(255,178,63,0.55)' }} />
        <span style={{ fontSize: 30, fontWeight: 700 }}>{title}</span>
      </div>
      {detail && <div style={{ marginTop: 8, fontSize: 24, color: C.mist }}>{detail}</div>}
      <div style={{ marginTop: 14, display: 'flex', gap: 22, fontFamily: mono, fontSize: 21, color: 'rgba(255,255,255,0.82)' }}>
        <span>block {block.toLocaleString('en-US')}</span>
        {gas !== undefined && <span>{gas.toLocaleString('en-US')} gas</span>}
      </div>
      <div style={{ marginTop: 6, fontFamily: mono, fontSize: 19, color: C.mist, whiteSpace: 'nowrap', overflow: 'hidden' }}>{hash.slice(0, typed)}<span style={{ opacity: typed < hash.length ? 1 : 0 }}>▍</span></div>
    </div>
  );
}

/** The wordmark: "tont" + a dotless ı whose dot is the lamp. `ignite` 0..1 lights it. */
export function Wordmark({ size = 180, ignite = 1, color = '#fff' }: { size?: number; ignite?: number; color?: string }) {
  return (
    <div style={{ position: 'relative', display: 'inline-flex', alignItems: 'baseline', fontFamily: sans, fontWeight: 800, fontSize: size, letterSpacing: '-0.03em', color, lineHeight: 1 }}>
      tont
      <span style={{ position: 'relative' }}>
        ı
        <span style={{ position: 'absolute', top: '-0.13em', left: '50%', width: '0.3em', height: '0.3em', transform: `translateX(-50%) scale(${0.6 + 0.4 * ignite})`, borderRadius: 999, background: C.lamp, opacity: 0.25 + 0.75 * ignite, boxShadow: `0 0 ${size * 0.25 * ignite}px ${size * 0.06 * ignite}px rgba(255,178,63,${0.7 * ignite})` }} />
      </span>
    </div>
  );
}

export function Hand({ which, progress, height = 90 }: { which: 'for-life' | 'habambuhay'; progress: number; height?: number }) {
  // The site's pre-rendered handwriting, revealed left to right like a pen.
  return (
    <div style={{ height, clipPath: `inset(-20% ${(1 - progress) * 100}% -20% 0)` }}>
      <Img src={staticFile(`hand/${which}-static.svg`)} style={{ height: '100%' }} />
    </div>
  );
}

export const noise = (seed: string, f: number, amp: number) => (random(`${seed}-${Math.floor(f / 3)}`) - 0.5) * amp;
