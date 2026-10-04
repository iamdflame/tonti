import { AbsoluteFill } from 'remotion';
import { Sky } from './components/Sky';
import { Receipt, Wordmark } from './components/kit';
import { C, mono, sans } from './theme';
import facts from '../../data/facts.json';

// Images for the HackQuest submission, in the film's own look: the logo, the architecture, the proof.

/** The logo: the wordmark, its dot the lamp, on the dusk over the bay. Square. */
export function Logo() {
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <AbsoluteFill style={{ background: `radial-gradient(80% 45% at 50% 78%, rgba(242,120,92,0.55), transparent 70%), linear-gradient(180deg, ${C.night} 0%, ${C.bay} 55%, ${C.bay2} 76%, #2a2f4a 80%, ${C.night} 80.4%, ${C.night} 100%)` }} />
      <AbsoluteFill style={{ alignItems: 'center', justifyContent: 'center', paddingBottom: 90 }}>
        <Wordmark size={300} />
      </AbsoluteFill>
    </AbsoluteFill>
  );
}

/** The mark alone, for small sizes: the lamp sun on the horizon. */
export function LogoMark() {
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <Sky sun={0.2} sunX={0.5} width={1024} height={1024} />
      <AbsoluteFill style={{ alignItems: 'center', justifyContent: 'center' }}>
        <div style={{ width: 360, height: 360, borderRadius: 999, background: C.lamp, boxShadow: '0 0 160px 50px rgba(255,178,63,0.45)', transform: 'translateY(40px)' }} />
      </AbsoluteFill>
      <div style={{ position: 'absolute', left: 0, right: 0, top: 818, height: 206, background: C.night }} />
      <div style={{ position: 'absolute', left: 150, right: 150, top: 806, height: 24, borderRadius: 99, background: C.coral }} />
    </AbsoluteFill>
  );
}

const box = (accent = false): React.CSSProperties => ({
  position: 'absolute', padding: '16px 20px', borderRadius: 18, background: accent ? 'rgba(28,49,87,0.92)' : 'rgba(13,26,48,0.9)',
  border: `1.5px solid ${accent ? 'rgba(255,178,63,0.55)' : 'rgba(255,255,255,0.14)'}`, boxShadow: '0 20px 50px -20px rgba(0,0,0,0.7)', color: '#fff', fontFamily: sans,
});
const Title = ({ name, tag }: { name: string; tag: string }) => (
  <div style={{ display: 'flex', alignItems: 'baseline', gap: 10, marginBottom: 6 }}>
    <span style={{ fontSize: 26, fontWeight: 800 }}>{name}</span>
    <span style={{ fontFamily: mono, fontSize: 15, color: C.lampSoft }}>{tag}</span>
  </div>
);
const Line = ({ children }: { children: React.ReactNode }) => <div style={{ fontSize: 18, lineHeight: 1.4, color: 'rgba(255,255,255,0.84)' }}>{children}</div>;

/** The architecture: six contracts on Robinhood Chain mainnet and what flows between them. */
export function Architecture() {
  const arrow = (x1: number, y1: number, x2: number, y2: number, label?: string, lx?: number, ly?: number) => (
    <g>
      <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={C.lamp} strokeWidth={2.5} markerEnd="url(#a)" opacity={0.85} />
      {label && <text x={lx ?? (x1 + x2) / 2} y={ly ?? (y1 + y2) / 2 - 8} fill={C.lampSoft} fontFamily={sans} fontSize={15} textAnchor="middle">{label}</text>}
    </g>
  );
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <AbsoluteFill style={{ opacity: 0.4 }}><Sky sun={0.12} sunX={0.9} width={1280} height={720} /></AbsoluteFill>
      <AbsoluteFill style={{ background: 'linear-gradient(180deg, rgba(13,26,48,0.7), rgba(13,26,48,0.92))' }} />
      <div style={{ position: 'absolute', left: 48, top: 30, fontFamily: sans, color: '#fff' }}>
        <div style={{ fontSize: 34, fontWeight: 800, letterSpacing: '-0.01em' }}>How Tonti works</div>
        <div style={{ fontSize: 18, color: C.mist, marginTop: 2 }}>Six contracts, live on Robinhood Chain mainnet (chain 4663). Every one owned by a 48-hour timelock.</div>
      </div>
      <svg width={1280} height={720} style={{ position: 'absolute', inset: 0 }}>
        <defs><marker id="a" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill={C.lamp} /></marker></defs>
        {arrow(266, 322, 416, 322, 'USDG in · income out', 341, 310)}
        {arrow(416, 352, 266, 352)}
        {arrow(640, 270, 640, 246)}
        <text x={652} y={262} fill={C.lampSoft} fontFamily={sans} fontSize={15}>prices every cohort</text>
        {arrow(862, 330, 940, 330, 'invest', 901, 318)}
        {arrow(640, 452, 640, 488)}
        <text x={652} y={476} fill={C.lampSoft} fontFamily={sans} fontSize={15}>alive? identified?</text>
        {arrow(150, 410, 326, 540, 'Face ID check-in', 196, 494)}
      </svg>
      {/* member */}
      <div style={{ ...box(), left: 28, top: 272, width: 236 }}>
        <Title name="Member" tag="phone" />
        <Line>Face ID passkey</Line>
        <Line>USDG from any wallet</Line>
        <Line>English · Filipino</Line>
      </div>
      {/* actuary */}
      <div style={{ ...box(true), left: 420, top: 122, width: 440 }}>
        <Title name="Actuary" tag="Stylus · Rust" />
        <Line>1,846 cohorts from UN life tables, <b>sealed</b></Line>
        <Line>512 simulated lives in one eth_call · 9.5M gas</Line>
      </div>
      {/* pool */}
      <div style={{ ...box(true), left: 420, top: 272, width: 440 }}>
        <Title name="TontiPool" tag="Stylus · Rust" />
        <Line>members and cohorts · monthly settlement in pages</Line>
        <Line>fair mortality credits, even in small pools</Line>
        <Line>a ghost-death detector inside every settlement</Line>
      </div>
      {/* treasury */}
      <div style={{ ...box(), left: 944, top: 272, width: 308 }}>
        <Title name="Treasury" tag="Solidity" />
        <Line>S&P 500 (SPY) · T-bills (SGOV)</Line>
        <Line>via Uniswap v4, Chainlink-guarded</Line>
        <Line>USDG lent in Morpho</Line>
      </div>
      {/* registry + identity */}
      <div style={{ ...box(), left: 330, top: 492, width: 340 }}>
        <Title name="LifeRegistry" tag="Solidity" />
        <Line>passkey check-ins, P-256 on-chain</Line>
        <Line>death reports, revival for 5 years</Line>
      </div>
      <div style={{ ...box(), left: 690, top: 492, width: 340 }}>
        <Title name="AttestedIdentity" tag="EIP-712" />
        <Line>birth year, sex, country</Line>
        <Line>bound to the cohort key</Line>
      </div>
      {/* governance and keeper */}
      <div style={{ position: 'absolute', left: 28, right: 28, bottom: 18, display: 'flex', gap: 18, fontFamily: sans }}>
        <div style={{ flex: 1, padding: '12px 18px', borderRadius: 14, background: 'rgba(242,120,92,0.12)', border: '1px solid rgba(242,120,92,0.4)', color: '#fff', fontSize: 18 }}>
          <b>TimelockController</b> owns every contract: any rule change waits 48 hours in public. Mortality can never change.
        </div>
        <div style={{ flex: 1, padding: '12px 18px', borderRadius: 14, background: 'rgba(255,255,255,0.05)', border: '1px solid rgba(255,255,255,0.14)', color: '#fff', fontSize: 18 }}>
          <b>Keeper</b>: permissionless. Anyone can push settlement, pay income, settle reports.
        </div>
      </div>
    </AbsoluteFill>
  );
}

/** The proof: member #0's life on mainnet, transaction by transaction, and what the tests found. */
export function Proof() {
  const t = facts.txs;
  const ci = facts.checkins[0];
  const h = facts.holdings;
  const w = { width: 560, padding: '14px 20px' };
  return (
    <AbsoluteFill style={{ backgroundColor: C.night }}>
      <AbsoluteFill style={{ opacity: 0.45 }}><Sky sun={0.22} sunX={0.1} width={1280} height={720} /></AbsoluteFill>
      <AbsoluteFill style={{ background: 'linear-gradient(90deg, rgba(13,26,48,0.94), rgba(13,26,48,0.8))' }} />
      <div style={{ position: 'absolute', left: 48, top: 34, width: 560, fontFamily: sans, color: '#fff' }}>
        <div style={{ fontSize: 40, fontWeight: 800, lineHeight: 1.05, letterSpacing: '-0.015em' }}>Live on mainnet,<br /><span style={{ color: C.lamp }}>with a real member.</span></div>
        <div style={{ fontSize: 19, color: C.mist, marginTop: 14, lineHeight: 1.45 }}>Member #0 joined from an iPhone, checked in with Face ID, passed the identity check and paid in 25 USDG. The first monthly settlement invested it.</div>
        <div style={{ marginTop: 26, display: 'grid', gap: 12 }}>
          {[
            [`$${h.sp500.usd.toFixed(2)}`, 'S&P 500 (SPY)'],
            [`$${h.tbills.usd.toFixed(2)}`, 'US T-bills (SGOV)'],
            [`$${h.cash.usd.toFixed(2)}`, 'USDG lent in Morpho'],
          ].map(([v, k]) => (
            <div key={k} style={{ display: 'flex', alignItems: 'baseline', gap: 14 }}>
              <span style={{ fontFamily: mono, fontSize: 34, fontWeight: 700, color: C.lampSoft, minWidth: 150 }}>{v}</span>
              <span style={{ fontSize: 20, color: 'rgba(255,255,255,0.85)' }}>{k}</span>
            </div>
          ))}
        </div>
        <div style={{ marginTop: 26, fontSize: 18, color: C.mist, lineHeight: 1.5 }}>
          <b style={{ color: '#fff' }}>{facts.tests.plantedBugs} of {facts.tests.plantedTotal}</b> bugs planted in our own contracts caught by the tests · <b style={{ color: '#fff' }}>4</b> independent adversarial reviews
        </div>
        <div style={{ marginTop: 34, fontFamily: mono, fontSize: 19, color: C.lamp, lineHeight: 1.7 }}>tonti-life.vercel.app<br />github.com/iamdflame/tonti</div>
      </div>
      <div style={{ position: 'absolute', right: 36, top: 26, display: 'grid', gap: 10, transform: 'scale(0.84)', transformOrigin: 'top right' }}>
        <Receipt at={-40} title="Joined" detail="Ghana, born 1976, income from 65" block={t.join.block} hash={t.join.hash} gas={t.join.gas} style={w} />
        <Receipt at={-40} title="Face ID check-in, verified on-chain" block={ci.block} hash={ci.hash} gas={ci.gas} style={w} />
        <Receipt at={-40} title="Identity attested (EIP-712)" block={t.identity.block} hash={t.identity.hash} gas={t.identity.gas} style={w} />
        <Receipt at={-40} title="Paid in 25 USDG" block={t.deposit.block} hash={t.deposit.hash} gas={t.deposit.gas} style={w} />
        <Receipt at={-40} title="First settlement, invested" block={t.rebalance.block} hash={t.rebalance.hash} gas={t.rebalance.gas} style={w} />
      </div>
    </AbsoluteFill>
  );
}
