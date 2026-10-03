import { AbsoluteFill, Img, staticFile } from 'remotion';
import { Sky } from './components/Sky';
import { Screen, Wordmark } from './components/kit';
import { C, mono, sans } from './theme';
import takes from '../../data/takes.json';

// YouTube thumbnails, 1280×720: few words, very large, high contrast, in the film's own look.

/** A: the answer itself. The number the chain gave, "for life" in the site's handwriting. */
export function ThumbA() {
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <Sky sun={0.34} sunX={0.84} width={1280} height={720} />
      <AbsoluteFill style={{ background: 'linear-gradient(90deg, rgba(8,14,28,0.78) 0%, rgba(8,14,28,0.45) 55%, transparent 80%)' }} />
      <div style={{ position: 'absolute', left: 64, top: 44 }}><Wordmark size={70} /></div>
      <div style={{ position: 'absolute', left: 60, top: 168 }}>
        <div style={{ fontFamily: sans, fontSize: 50, fontWeight: 700, color: 'rgba(255,255,255,0.92)', letterSpacing: '-0.01em' }}>Her income from the chain:</div>
        <div style={{ fontFamily: mono, fontSize: 196, fontWeight: 700, color: C.lamp, lineHeight: 1, marginTop: 10, letterSpacing: '-0.03em', textShadow: '0 0 60px rgba(255,178,63,0.45)' }}>$67.67</div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 22, marginTop: 6 }}>
          <span style={{ fontFamily: sans, fontSize: 58, fontWeight: 800, color: '#fff' }}>a month,</span>
          <Img src={staticFile('hand/for-life-static.svg')} style={{ height: 120, transform: 'translateY(14px)' }} />
        </div>
      </div>
      <div style={{ position: 'absolute', left: 64, bottom: 44, display: 'inline-flex', alignItems: 'center', gap: 12, padding: '10px 20px', borderRadius: 999, background: 'rgba(13,26,48,0.8)', border: '1px solid rgba(255,255,255,0.14)', fontFamily: sans, fontSize: 28, fontWeight: 600, color: '#fff' }}>
        <span style={{ width: 12, height: 12, borderRadius: 99, background: C.lamp, boxShadow: '0 0 12px 3px rgba(255,178,63,0.6)' }} />
        Live on Robinhood Chain
      </div>
    </AbsoluteFill>
  );
}

const quote = (takes as unknown as Record<string, { marks: Record<string, number> }>)['S4-quote'];

/** B: the live site's answer, as recorded, with the promise beside it. */
export function ThumbB() {
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <Sky sun={0.3} sunX={0.12} width={1280} height={720} />
      <AbsoluteFill style={{ background: 'linear-gradient(270deg, rgba(8,14,28,0.2) 0%, rgba(8,14,28,0.75) 60%)' }} />
      {/* the recording is 16:9: a 16:9 window, so nothing is cropped by the box itself */}
      <div style={{ position: 'absolute', right: 36, top: 150, width: 640, height: 360, borderRadius: 22, overflow: 'hidden', transform: 'rotate(2deg)', boxShadow: '0 40px 90px -20px rgba(0,0,0,0.75)', border: '2px solid rgba(255,255,255,0.12)' }}>
        <Screen src="takes/S4-quote.mp4" from={quote.marks.settled} keys={[{ at: 0, scale: 2.3, x: 0.668, y: 0.27 }]} shadow={false} />
      </div>
      <div style={{ position: 'absolute', left: 60, top: 40 }}><Wordmark size={64} /></div>
      <div style={{ position: 'absolute', left: 56, top: 170, width: 560, fontFamily: sans, fontWeight: 800, color: '#fff', fontSize: 92, lineHeight: 0.98, letterSpacing: '-0.025em' }}>
        Income <span style={{ color: C.lamp }}>for life</span>, priced on-chain.
      </div>
      <div style={{ position: 'absolute', left: 60, bottom: 52, fontFamily: sans, fontSize: 30, fontWeight: 600, color: C.mist }}>Robinhood Chain · Arbitrum Stylus</div>
    </AbsoluteFill>
  );
}
