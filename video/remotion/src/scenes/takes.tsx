import { AbsoluteFill, Sequence, interpolate, useCurrentFrame } from 'remotion';
import { Sky } from '../components/Sky';
import { Footage, Grain, Hand, Label, Phone, Receipt, Screen, Wordmark, prog } from '../components/kit';
import { C, mono, sans } from '../theme';
import facts from '../../../data/facts.json';
import takes from '../../../data/takes.json';
import { S04_LEN, S05_LEN, S07_FROM, S07_LEN, S10_A, S10_Q, fil, lights, quote } from '../timeline';

const clamp = { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' } as const;
type Take = { frames: number; seconds: number; marks: Record<string, number> };
const T = takes as unknown as Record<string, Take>;
const member = T['S8-member'];
const checkin = T['S8-checkin'];
const pool = T['S8-pool'];


/** S04: one question, answered by the chain. One continuous take of the live site. */
export function S04() {
  const f = useCurrentFrame();
  const m = quote.marks;
  const keys = [
    { at: 0, scale: 1.0, x: 0.5, y: 0.5 },
    { at: m.question + 10, scale: 1.04, x: 0.48, y: 0.5 },
    { at: m.form, scale: 1.55, x: 0.37, y: 0.4 },
    { at: m.ask - 6, scale: 1.55, x: 0.37, y: 0.46 },
    { at: m.answer + 6, scale: 1.3, x: 0.55, y: 0.36 },
    { at: m.answer + 40, scale: 1.65, x: 0.67, y: 0.3 },
    { at: m.settled, scale: 1.65, x: 0.67, y: 0.32 },
    { at: m.chart - 30, scale: 1.4, x: 0.66, y: 0.48 },
  ];
  const waited = ((m.answer - m.ask) / 30).toFixed(1);
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <Screen src="takes/S4-quote.mp4" keys={keys} shadow={false} />
      <AbsoluteFill style={{ padding: 56, justifyContent: 'flex-end' }}>
        <Sequence from={m.question + 6} durationInFrames={m.ask - m.question - 6} layout="none">
          <Label>One continuous take of the live site, on Robinhood Chain mainnet</Label>
        </Sequence>
        <Sequence from={m.answer + 8} durationInFrames={S04_LEN - m.answer - 8} layout="none">
          <Label>Answered by the Actuary contract in {waited} s, tap to number · 512 simulated lives in one eth_call</Label>
        </Sequence>
      </AbsoluteFill>
      <Grain opacity={0.035} />
    </AbsoluteFill>
  );
}

/** S05: run it yourself. The rest of the same take. */
export function S05() {
  const m = quote.marks;
  const o = S04_LEN;
  const keys = [
    { at: 0, scale: 1.4, x: 0.66, y: 0.48 },
    { at: m.call - o - 10, scale: 1.55, x: 0.67, y: 0.55 },
    { at: m.same - o, scale: 1.7, x: 0.67, y: 0.62 },
    { at: S05_LEN, scale: 1.75, x: 0.67, y: 0.62 },
  ];
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <Screen src="takes/S4-quote.mp4" from={o} keys={keys} shadow={false} />
      <AbsoluteFill style={{ padding: 56, justifyContent: 'flex-end' }}>
        <Sequence from={m.same - o + 4} layout="none">
          <Label>Re-run in the browser on a public Robinhood Chain node: the same answer</Label>
        </Sequence>
      </AbsoluteFill>
      <Grain opacity={0.035} />
    </AbsoluteFill>
  );
}


/** S07: we replayed history. The site's own replay, scrolled as a visitor does. */
export function S07() {
  const m = lights.marks;
  const o = S07_FROM;
  const keys = [
    // a 1080p take: the camera stays near 1:1 so the lights stay sharp
    { at: 0, scale: 1.06, x: 0.42, y: 0.4 },
    { at: m['1965'] - o - 20, scale: 1.22, x: 0.43, y: 0.52 },
    { at: m['1977'] - o, scale: 1.3, x: 0.43, y: 0.52 },
    { at: m['1999'] - o, scale: 1.3, x: 0.43, y: 0.52 },
    { at: S07_LEN, scale: 1.24, x: 0.43, y: 0.52 },
  ];
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <Screen src="takes/S7-lights.mp4" from={o} keys={keys} shadow={false} />
      <AbsoluteFill style={{ padding: 56, justifyContent: 'flex-end' }}>
        <Sequence from={m['1965'] - o} layout="none">
          <Label>The site’s replay: UN mortality for the lives, real S&P 500 and T-bill returns since 1965</Label>
        </Sequence>
      </AbsoluteFill>
      <Grain opacity={0.035} />
    </AbsoluteFill>
  );
}

// S08's phone: Dave's own iPhone recording of a real check-in when it exists (iphone/checkin.mp4),
// otherwise the check-in take. Set by Master from a prop.
type Clip = { src: string; from: number; len: number };
const cut = (src: string, m: Record<string, number>, a: string, b: string | number, pad = 0): Clip => {
  const from = m[a] - pad;
  const to = typeof b === 'number' ? from + b : m[b];
  return { src, from, len: to - from };
};

/** S08: it's live. Member #0 on a phone, receipts from the chain, the Live pool. */
export function S08({ iphone }: { iphone?: { src: string; from: number; len: number } }) {
  const f = useCurrentFrame();
  const mm = member.marks;
  const joinClips: Clip[] = [
    cut('takes/S8-member.mp4', mm, 'plan', 75, 0),
    cut('takes/S8-member.mp4', mm, 'key', 70, 2),
    cut('takes/S8-member.mp4', mm, 'confirm', 75, -40),
    cut('takes/S8-member.mp4', mm, 'welcome', 100, -4),
  ];
  const check: Clip = iphone ?? cut('takes/S8-checkin.mp4', checkin.marks, 'page', 200, 10);
  const account: Clip = { src: 'takes/S8-member.mp4', from: member.frames - 110, len: 110 };
  const seq: Clip[] = [...joinClips, check, account];
  const starts = seq.reduce<number[]>((a, c, i) => [...a, i === 0 ? 0 : a[i - 1] + seq[i - 1].len], []);
  const phoneEnd = starts.at(-1)! + account.len;
  const tx = facts.txs;
  const ci = facts.checkins[0];
  const poolFrom = phoneEnd - 120;
  const phoneOut = interpolate(f, [poolFrom, poolFrom + 24], [0, 1], clamp);
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <AbsoluteFill style={{ opacity: 0.55 }}><Sky sun={0.24} sunX={0.12} /></AbsoluteFill>
      <AbsoluteFill style={{ background: 'linear-gradient(90deg, rgba(13,26,48,0.25), rgba(13,26,48,0.85) 60%)' }} />
      {/* the phone, left */}
      <div style={{ position: 'absolute', left: 190, top: 70, transform: `translateX(${-phoneOut * 120}px)`, opacity: 1 - phoneOut }}>
        <Phone height={900}>
          {seq.map((c, i) => (
            // each clip runs 8 frames under the next one's fade-in: a crossfade, never a dip to black
            <Sequence key={i} from={starts[i]} durationInFrames={c.len + (i < seq.length - 1 ? 8 : 0)} layout="none">
              <ClipFade>
                <Screen src={c.src} from={c.from} keys={[{ at: 0, scale: 1, x: 0.5, y: 0.5 }]} shadow={false} />
              </ClipFade>
            </Sequence>
          ))}
        </Phone>
      </div>
      {/* receipts, right */}
      <div style={{ position: 'absolute', left: 840, top: 110, display: 'grid', gap: 22, opacity: 1 - phoneOut }}>
        <Receipt at={20} title="Joined" detail={`Member #0 · ${facts.member0.country === 'GHA' ? 'Ghana' : facts.member0.country}, born ${facts.member0.born}, income from 65`} block={tx.join.block} hash={tx.join.hash} gas={tx.join.gas} />
        <Receipt at={starts[4]} title="Face ID check-in, verified on-chain" detail="A passkey signature, checked by the chain’s P-256 precompile" block={ci.block} hash={ci.hash} gas={ci.gas} />
        <Receipt at={starts[4] + 70} title="Identity attested" detail="Birth year, sex and country, matched to the cohort key" block={tx.identity.block} hash={tx.identity.hash} gas={tx.identity.gas} />
        <Receipt at={starts[5]} title="Paid in 25 USDG" block={tx.deposit.block} hash={tx.deposit.hash} gas={tx.deposit.gas} />
      </div>
      <div style={{ position: 'absolute', left: 190, bottom: 40, opacity: 1 - phoneOut }}>
        <Label at={10}>{iphone ? 'Member #0’s own iPhone, recorded by them' : 'Member #0’s real join, replayed through the live site from its mainnet transaction'}</Label>
      </div>
      {/* the Live pool, then the settlement */}
      <Sequence from={poolFrom}>
        <PoolPart />
      </Sequence>
      <Grain opacity={0.04} />
    </AbsoluteFill>
  );
}

function PoolPart() {
  const f = useCurrentFrame();
  const m = pool.marks;
  const p = prog(f, 0, 26);
  const tx = facts.txs;
  const keys = [
    { at: 0, scale: 1.2, x: 0.5, y: 0.45 },
    { at: m.money - m.top + 20, scale: 1.32, x: 0.48, y: 0.45 },
    { at: 480, scale: 1.32, x: 0.48, y: 0.45 },
  ];
  return (
    <AbsoluteFill style={{ opacity: p }}>
      <div style={{ position: 'absolute', left: 120, top: 90, width: 1180, height: 664, transform: `scale(${0.96 + 0.04 * p})`, transformOrigin: 'left top' }}>
        <Screen src="takes/S8-pool.mp4" from={m.top} keys={keys} radius={24} />
      </div>
      <div style={{ position: 'absolute', left: 1340, top: 150, display: 'grid', gap: 22 }}>
        <Receipt at={30} title="First monthly settlement" detail={`$${facts.holdings.sp500.usd.toFixed(2)} S&P 500 · $${facts.holdings.tbills.usd.toFixed(2)} T-bills · $${facts.holdings.cash.usd.toFixed(2)} USDG`} block={tx.settle.block} hash={tx.settle.hash} gas={tx.settle.gas} style={{ width: 500 }} />
        <Receipt at={70} title="Invested on mainnet" detail="SPY and SGOV through Uniswap v4, USDG in Morpho" block={tx.rebalance.block} hash={tx.rebalance.hash} gas={tx.rebalance.gas} style={{ width: 500 }} />
      </div>
    </AbsoluteFill>
  );
}

function ClipFade({ children }: { children: React.ReactNode }) {
  const f = useCurrentFrame();
  const o = interpolate(f, [0, 8], [0, 1], clamp);
  return <div style={{ position: 'absolute', inset: 0, opacity: o }}>{children}</div>;
}


/** S10: habambuhay. The question in Filipino, the answer written by hand, a family at dusk, the end card. */
export function S10() {
  const f = useCurrentFrame();
  const m = fil.marks;
  const sunsetFrom = S10_Q + S10_A;
  const endFrom = sunsetFrom + 75;
  const end = prog(f, endFrom, 26);
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <Sequence durationInFrames={S10_Q}>
        <Screen src="takes/S10-fil.mp4" keys={[{ at: 0, scale: 1.06, x: 0.42, y: 0.4 }, { at: S10_Q, scale: 1.12, x: 0.4, y: 0.38 }]} shadow={false} />
      </Sequence>
      <Sequence from={S10_Q} durationInFrames={S10_A}>
        <Screen src="takes/S10-fil.mp4" from={m.answer - 8} keys={[{ at: 0, scale: 1.25, x: 0.6, y: 0.36 }, { at: S10_A, scale: 1.4, x: 0.65, y: 0.3 }]} shadow={false} />
      </Sequence>
      <Sequence from={sunsetFrom} durationInFrames={110}>
        <AbsoluteFill style={{ opacity: interpolate(f - sunsetFrom, [0, 14], [0, 1], clamp) }}>
          <Footage src="stock/mixkit-4119-30.mp4" from={30} push={0.06} grade={0.18} />
        </AbsoluteFill>
      </Sequence>
      <Sequence from={endFrom}>
        <AbsoluteFill style={{ opacity: end }}>
          <Sky sun={interpolate(f, [endFrom, endFrom + 140], [0.26, 0.12], clamp)} sunX={0.8} />
          <AbsoluteFill style={{ alignItems: 'center', justifyContent: 'center', flexDirection: 'column', gap: 26, paddingBottom: 110 }}>
            <Wordmark size={190} ignite={prog(f, endFrom + 10, 30)} />
            <div style={{ display: 'flex', alignItems: 'center', gap: 22 }}>
              <span style={{ fontFamily: sans, fontSize: 48, fontWeight: 500, color: 'rgba(255,255,255,0.92)' }}>Income for life.</span>
              <Hand which="habambuhay" progress={prog(f, endFrom + 30, 40)} height={84} />
            </div>
            <div style={{ marginTop: 30, fontFamily: mono, fontSize: 40, color: C.lamp, opacity: prog(f, endFrom + 50, 20) }}>tonti-life.vercel.app</div>
            <div style={{ fontFamily: sans, fontSize: 26, color: C.mist, opacity: prog(f, endFrom + 64, 20) }}>Robinhood Chain · Arbitrum Stylus · USDG · English and Filipino</div>
          </AbsoluteFill>
        </AbsoluteFill>
      </Sequence>
      <Grain opacity={0.04} />
    </AbsoluteFill>
  );
}
