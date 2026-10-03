import { AbsoluteFill, Series, interpolate, useCurrentFrame, useVideoConfig } from 'remotion';
import { S01, S02, S03 } from './scenes/opening';
import { S06, S09 } from './scenes/explain';
import { S04, S05, S07, S08, S10 } from './scenes/takes';
import { SCENES } from './timeline';
import { cues } from './captions';
import { C, sans } from './theme';

export type MasterProps = { captions: boolean; iphone?: { src: string; from: number; len: number } };

const COMPONENTS: Record<string, (p: MasterProps) => React.ReactNode> = {
  S01: () => <S01 />, S02: () => <S02 />, S03: () => <S03 />, S04: () => <S04 />, S05: () => <S05 />,
  S06: () => <S06 />, S07: () => <S07 />, S08: (p) => <S08 iphone={p.iphone} />, S09: () => <S09 />, S10: () => <S10 />,
};

// Where a scene starts or ends on a different world (footage, UI, sky), a short dip through the
// night colour; S04 → S05 is one continuous take and gets no dip.
const DIP_IN: Record<string, number> = { S02: 8, S03: 10, S04: 10, S06: 10, S07: 10, S08: 10, S09: 10, S10: 8 };
const DIP_OUT: Record<string, number> = { S01: 8, S02: 10, S03: 10, S05: 10, S06: 10, S07: 10, S08: 10, S09: 8, S10: 20 };

function Dip({ id, len, children }: { id: string; len: number; children: React.ReactNode }) {
  const f = useCurrentFrame();
  const a = DIP_IN[id] ?? 0, b = DIP_OUT[id] ?? 0;
  const o = Math.min(a ? interpolate(f, [0, a], [0, 1], { extrapolateRight: 'clamp' }) : 1, b ? interpolate(f, [len - b, len], [1, 0], { extrapolateLeft: 'clamp' }) : 1);
  return <AbsoluteFill style={{ backgroundColor: C.night }}><AbsoluteFill style={{ opacity: o }}>{children}</AbsoluteFill></AbsoluteFill>;
}

export function Captions() {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const c = cues(fps).find((x) => f >= x.from && f < x.to);
  if (!c) return null;
  return (
    <AbsoluteFill style={{ justifyContent: 'flex-end', alignItems: 'center', paddingBottom: 64, pointerEvents: 'none' }}>
      <div style={{ maxWidth: 1400, padding: '12px 26px', borderRadius: 14, background: 'rgba(8,14,28,0.78)', color: '#fff', fontFamily: sans, fontSize: 40, fontWeight: 600, lineHeight: 1.3, textAlign: 'center' }}>{c.text}</div>
    </AbsoluteFill>
  );
}

export function Master(props: MasterProps) {
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <Series>
        {SCENES.map((s) => (
          <Series.Sequence key={s.id} durationInFrames={s.frames} name={`${s.id} ${s.title}`}>
            <Dip id={s.id} len={s.frames}>{COMPONENTS[s.id](props)}</Dip>
          </Series.Sequence>
        ))}
      </Series>
      {props.captions && <Captions />}
    </AbsoluteFill>
  );
}

export function One({ id, ...props }: MasterProps & { id: string }) {
  const s = SCENES.find((x) => x.id === id)!;
  return <Dip id={id} len={s.frames}>{COMPONENTS[id](props)}</Dip>;
}
