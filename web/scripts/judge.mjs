// The judge's check, run against the public site (not localhost):
//   node scripts/judge.mjs [baseUrl] [quotes]
// 1. Random questions typed into the live landing. For each: the eth_call the browser sent is
//    captured, re-sent by this script to the public node, and the node's P50 must equal the amount
//    on screen; the calldata must encode the inputs asked. Success rate and p50/p95 latency.
// 2. /pool's live odds equal a direct read of the Actuary.
// 3. Every route in English and Filipino at 320, 375, 768 and 1440 px: HTTP 200 (404 page 404),
//    no page errors, no horizontal scroll. axe (WCAG 2.2 AA) on each route at 375 px.
// 4. A life key made with Chrome's virtual WebAuthn authenticator on the parent's invite page.
// Writes runs/judge.json (under the data drive's runs/ when present) and screenshots to /dev/shm/judge.
import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { chromium } from '@playwright/test';
import { decodeFunctionData, decodeFunctionResult, parseAbi } from 'viem';

const BASE = (process.argv[2] ?? 'https://tonti-life.vercel.app').replace(/\/$/, '');
const N = Number(process.argv[3] ?? 20);
const RPC = 'https://rpc.mainnet.chain.robinhood.com';
const SHOTS = '/dev/shm/judge';
const RUNS = existsSync('/media/dflame/UNIQ/arbit/runs') ? '/media/dflame/UNIQ/arbit/runs' : new URL('../../runs', import.meta.url).pathname;
mkdirSync(SHOTS, { recursive: true });

const abi = parseAbi([
  'function quote(uint256 key, uint256 age, uint256 year, uint256 startAge, uint256 lumpSum, uint256 monthly, uint32 paths, bool escalating) view returns (uint256[8])',
  'function qMonth(uint256 key, uint256 age, uint256 year) view returns (uint256)',
]);
const ISO = { PHL: 608, IDN: 360, IND: 356, BGD: 50, MMR: 104, LKA: 144, NPL: 524, VNM: 704, THA: 764, MYS: 458, CHN: 156, SGP: 702, GHA: 288 };
const YEAR = new Date().getUTCFullYear();
const rnd = (lo, hi) => lo + Math.floor(Math.random() * (hi - lo + 1));
const pick = (a) => a[Math.floor(Math.random() * a.length)];
const pctl = (xs, p) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length ? s[Math.min(s.length - 1, Math.floor(p * s.length))] : null;
};
// The judge's own reads: a public node's rate limit (429) is waited out, not counted against the site.
const rpc = async (method, params) => {
  for (let tries = 0; ; tries++) {
    const r = await fetch(RPC, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) });
    if (r.status === 429 && tries < 5) {
      await new Promise((ok) => setTimeout(ok, 1000 * 2 ** tries));
      continue;
    }
    const j = await r.json();
    if (j.error) throw new Error(j.error.message);
    return j.result;
  }
};
const usd = (v) => new Intl.NumberFormat('en-SG', { style: 'currency', currency: 'USD', minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(v);

const browser = await chromium.launch();
const out = { base: BASE, at: new Date().toISOString(), quotes: [], routes: [], axe: [], passkey: null, odds: null };

// ---------------------------------------------------------------- 1. quotes on the landing
{
  const ctx = await browser.newContext({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 1 });
  for (let i = 0; i < N; i++) {
    const who = pick(['mother', 'father', 'me']);
    const male = who === 'father' || (who === 'me' && Math.random() < 0.5);
    const c = pick(Object.keys(ISO));
    const b = rnd(YEAR - 79, Math.min(YEAR - 18, 2005)); // the Actuary's fitted births end in 2005
    const age = YEAR - b;
    const a = rnd(Math.max(50, Math.ceil(age + 0.5)), 80);
    const l = pick([0, 500, 1000, 3000, 7500, 20000, 120000]);
    const m = pick([0, 25, 50, 150, 400]) || (l ? 0 : 50);
    const ask = { who, sex: male ? 'male' : 'female', country: c, born: b, startAge: a, lump: l, monthly: m };
    // Each quote is a new visitor: nothing cached from the one before.
    const visit = await browser.newContext({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 1 });
    const page = await visit.newPage();
    const calls = [];
    page.on('request', (r) => {
      if (r.method() !== 'POST' || !r.url().includes('rpc')) return;
      try {
        const body = JSON.parse(r.postData() ?? '{}');
        for (const x of Array.isArray(body) ? body : [body]) if (x.method === 'eth_call' && x.params?.[0]?.data?.startsWith('0x')) calls.push(x.params[0]);
      } catch {}
    });
    const row = { ask };
    try {
      await page.goto(`${BASE}/en?w=${who}${who === 'me' && male ? '&s=m' : ''}&c=${c}&b=${b}&a=${a}&l=${l}&m=${m}`, { waitUntil: 'networkidle' });
      const t0 = Date.now();
      await page.locator('#ask form button[type=submit]').click();
      const amount = page.locator('#ask [aria-live=polite] p.font-mono').first();
      await amount.waitFor({ timeout: 30_000 });
      row.ms = Date.now() - t0;
      row.shown = (await amount.textContent()).trim();
      const q = calls.find((x) => x.data.startsWith('0x') && (() => { try { return decodeFunctionData({ abi, data: x.data }).functionName === 'quote'; } catch { return false; } })());
      if (!q) throw new Error('no quote eth_call seen from the browser');
      const args = decodeFunctionData({ abi, data: q.data }).args;
      const key = BigInt((ISO[c] * 2 + (male ? 1 : 0)) * 10_000 + b);
      row.encodesInputs = args[0] === key && args[3] === BigInt(a) * 10n ** 18n && args[4] === BigInt(Math.round(l * 1e6)) && args[5] === BigInt(Math.round(m * 1e6));
      const raw = decodeFunctionResult({ abi, functionName: 'quote', data: await rpc('eth_call', [{ to: q.to, data: q.data }, 'latest']) });
      row.node = usd(Number(raw[1]) / 1e6);
      row.same = row.node === row.shown;
      // Opened, "Run it yourself" must stay inside a phone's width (a 600-character `cast call`
      // once widened the whole column), and the chart speaks of the person asked about.
      await page.locator('#ask summary').first().click();
      await page.waitForTimeout(250);
      row.wider = await page.evaluate(() => [...document.querySelectorAll('#ask *')].filter((e) => {
        const r = e.getBoundingClientRect();
        return r.width > 0 && r.height > 0 && r.right > window.innerWidth + 1 && getComputedStyle(e).position !== 'absolute';
      }).length);
      const footer = await page.locator('#ask').getByText(/still alive then/).first().textContent().catch(() => '');
      row.pronoun = { mother: /she is/, father: /he is/, me: /you are/ }[who].test(footer ?? '');
      row.ok = row.same && row.encodesInputs && row.wider === 0 && row.pronoun;
    } catch (e) {
      row.ok = false;
      row.error = String(e.message ?? e).slice(0, 300);
      const alert = await page.locator('#ask [role=alert]').textContent().catch(() => null);
      if (alert) row.alert = alert.trim();
    }
    out.quotes.push(row);
    console.log(`quote ${i + 1}/${N}`, row.ok ? 'ok' : 'FAIL', JSON.stringify(ask), row.shown ?? '', row.ms ?? '', row.error ?? '');
    await visit.close();
  }
  // Edge: a birth year the Actuary has no mortality for is refused before any call, with the reason.
  {
    const page = await ctx.newPage();
    await page.goto(`${BASE}/en?w=me&c=PHL&b=2007&a=60&l=1000&m=0`, { waitUntil: 'networkidle' });
    const disabled = await page.locator('#ask form button[type=submit]').isDisabled();
    const hint = (await page.locator('#born-hint').textContent().catch(() => '')).trim();
    out.edge = { born: 2007, submitDisabled: disabled, hint, ok: disabled && /2005/.test(hint) };
    console.log('edge', JSON.stringify(out.edge));
    await page.close();
  }
  await ctx.close();
}

// ---------------------------------------------------------------- 2. /pool odds vs the chain
{
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const page = await ctx.newPage();
  await page.goto(`${BASE}/en/pool`, { waitUntil: 'networkidle' });
  const rows = await page.locator('#odds-title ~ div li[aria-label]').evaluateAll((els) => els.map((e) => e.getAttribute('aria-label')));
  const how = await page.locator('#odds-title ~ p').last().textContent().catch(() => '');
  const block = BigInt((how.match(/block ([\d,]+)/)?.[1] ?? '0').replace(/,/g, ''));
  const born = YEAR - 65;
  const names = new Intl.DisplayNames(['en-SG'], { type: 'region' });
  const ISO2 = { PHL: 'PH', IDN: 'ID', IND: 'IN', BGD: 'BD', MMR: 'MM', LKA: 'LK', NPL: 'NP', VNM: 'VN', THA: 'TH', MYS: 'MY', CHN: 'CN', SGP: 'SG', GHA: 'GH' };
  const { actuary } = JSON.parse(readFileSync(new URL('../src/sdk/deployment.json', import.meta.url), 'utf8'));
  const checks = [];
  let stateAt = 'the page block';
  for (const iso of ['PHL', 'SGP', 'GHA']) {
    const want = [];
    for (const male of [0, 1]) {
      let alive = 1;
      for (let a = 65; a < 85; a++) {
        const data = (await import('viem')).encodeFunctionData({ abi, functionName: 'qMonth', args: [BigInt((ISO[iso] * 2 + male) * 10_000 + born), BigInt(a * 2 + 1) * 5n * 10n ** 17n, BigInt(born + a + 1) * 10n ** 18n] });
        // At the page's block when the node still has it; qMonth reads only mortality, which never
        // changes, so a pruned block falls back to the latest (noted in the output).
        let raw;
        try {
          raw = await rpc('eth_call', [{ to: actuary, data }, `0x${block.toString(16)}`]);
        } catch (e) {
          if (!/historical state|missing trie node|pruned/i.test(String(e.message))) throw e;
          stateAt = 'latest (the page block was pruned by the public node)';
          raw = await rpc('eth_call', [{ to: actuary, data }, 'latest']);
        }
        const q = Number(BigInt(raw)) / 1e18;
        alive *= (1 - q) ** 12;
      }
      want.push(`${Math.round(alive * 100)}%`);
    }
    const name = names.of(ISO2[iso]).replace(/ \(.*\)$/, '');
    const shown = rows.find((r) => r.startsWith(`${name}:`));
    checks.push({ iso, shown, want: `${name}: women ${want[0]}, men ${want[1]}`, same: shown === `${name}: women ${want[0]}, men ${want[1]}` });
  }
  out.odds = { block: String(block), rows: rows.length, checks, stateAt };
  console.log('pool odds', JSON.stringify(out.odds));
  await ctx.close();
}

// ---------------------------------------------------------------- 3. routes × languages × widths, axe
const invite = Buffer.from(JSON.stringify({ v: 1, c: 'PHL', s: 'f', b: 1966, a: 62, e: 0, q: 0, n: 'Maria', g: '0xf910fC2fD395128A894e9754bE56479F05b54121' })).toString('base64url');
const routes = (l) => [
  [`/${l}`, 200],
  [`/${l}/q/m-PHL-1966-62-3000-30`, 200],
  [`/${l}/join?w=mother&c=PHL&b=1966&a=62`, 200],
  [`/${l}/i#${invite}`, 200],
  [`/${l}/checkin/0`, 200],
  [`/${l}/me`, 200],
  [`/${l}/pool`, 200],
  [`/${l}/operator`, 200],
  [`/${l}/relay`, 200],
  [`/${l}/no-such-page`, 404],
];
const axeSrc = await (await fetch('https://cdn.jsdelivr.net/npm/axe-core@4.10.3/axe.min.js')).text();
for (const width of [320, 375, 768, 1440]) {
  const ctx = await browser.newContext({ viewport: { width, height: 900 }, bypassCSP: true });
  for (const l of ['en', 'fil']) {
    for (const [path, want] of routes(l)) {
      const page = await ctx.newPage();
      const errors = [];
      page.on('pageerror', (e) => errors.push(String(e.message).slice(0, 200)));
      const res = await page.goto(BASE + path, { waitUntil: 'networkidle' }).catch((e) => ({ status: () => `nav: ${e.message}` }));
      await page.waitForTimeout(400);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth).catch(() => null);
      const name = `${width}-${l}-${path.split('/').slice(2).join('_').replace(/[^a-z0-9_-]/gi, '').slice(0, 40) || 'home'}.png`;
      if (width === 375 || width === 1440) await page.screenshot({ path: `${SHOTS}/${name}`, fullPage: true }).catch(() => undefined);
      const row = { width, path: path.split('#')[0], status: res.status(), want, errors, overflow, ok: res.status() === want && !errors.length && overflow !== null && overflow <= 0 };
      if (width === 375) {
        await page.addScriptTag({ content: axeSrc });
        const v = await page.evaluate(async () => (await window.axe.run(document, { runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'] } })).violations.map((x) => ({ id: x.id, impact: x.impact, n: x.nodes.length, first: x.nodes[0]?.target?.join(' ') })));
        out.axe.push({ path: row.path, violations: v });
        if (v.length) console.log('axe', row.path, JSON.stringify(v));
      }
      out.routes.push(row);
      if (!row.ok) console.log('route FAIL', JSON.stringify(row));
      await page.close();
    }
  }
  await ctx.close();
}

// ---------------------------------------------------------------- 4. a life key via a virtual authenticator
{
  const ctx = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  const cdp = await ctx.newCDPSession(page);
  await cdp.send('WebAuthn.enable');
  const { authenticatorId } = await cdp.send('WebAuthn.addVirtualAuthenticator', { options: { protocol: 'ctap2', transport: 'internal', hasResidentKey: true, hasUserVerification: true, isUserVerified: true, automaticPresenceSimulation: true } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e.message)));
  await page.goto(`${BASE}/fil/i#${invite}`, { waitUntil: 'networkidle' });
  const before = await page.locator('input[type=radio]').first().isDisabled().catch(() => null);
  await page.getByRole('button', { name: /life key/i }).first().click();
  await page.waitForTimeout(2500);
  const after = await page.locator('input[type=radio]').first().isDisabled().catch(() => null);
  const { credentials } = await cdp.send('WebAuthn.getCredentials', { authenticatorId });
  out.passkey = { stepTwoLockedBefore: before, stepTwoLockedAfter: after, credentialsOnDevice: credentials.length, residentKey: credentials[0]?.isResidentCredential ?? null, rpId: credentials[0]?.rpId ?? null, errors };
  await page.screenshot({ path: `${SHOTS}/passkey-after.png`, fullPage: true });
  console.log('passkey', JSON.stringify(out.passkey));
  await ctx.close();
}
// ---------------------------------------------------------------- in-app browsers
// A wallet's or a social app's browser can't make a passkey on a phone: the page must say so before
// anyone taps, with the link to open elsewhere; Chrome itself must not see the notice.
{
  const UA = {
    metamask: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 MetaMaskMobile',
    messenger: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/22F76 [FBAN/MessengerForiOS;FBAV/500.0]',
  };
  const invite = Buffer.from(JSON.stringify({ v: 1, c: 'GHA', s: 'f', b: 1962, a: 65, e: 0, q: 0, n: 'Ama', g: '0x1CD2B147EfE092c3BdE0B474bCE3Bd33ae3dbB37' })).toString('base64url');
  const cases = [
    ['metamask', `/en/join?w=me&s=m&c=GHA&b=1990&a=65`, true],
    ['messenger', `/fil/i#${invite}`, true],
    [null, `/en/join?w=me&s=m&c=GHA&b=1990&a=65`, false],
  ];
  out.inApp = [];
  for (const [ua, path, want] of cases) {
    const ctx = await browser.newContext({ viewport: { width: 390, height: 844 }, ...(ua ? { userAgent: UA[ua] } : {}) });
    const page = await ctx.newPage();
    await page.goto(`${BASE}${path}`, { waitUntil: 'networkidle' });
    await page.waitForTimeout(300);
    const shown = await page.getByText(/Open this page in Safari or Chrome|Buksan ang pahinang ito sa Safari o Chrome/).first().isVisible().catch(() => false);
    const link = shown ? (await page.locator('.select-all').first().textContent().catch(() => '')) : '';
    const keepsPlan = !shown || (path.includes('#') ? link.includes('#') : /c=GHA/.test(link) && /s=m/.test(link));
    out.inApp.push({ ua: ua ?? 'chrome', path: path.split('#')[0], shown, keepsPlan, ok: shown === want && keepsPlan });
    await ctx.close();
  }
  console.log('in-app', JSON.stringify(out.inApp));
}

// ---------------------------------------------------------------- after joining
// Member #0's join, already mined, replayed through the site: a stand-in wallet answers "send" with
// its hash, so every screen after it reads the real chain. Sex must be chosen (it prices the plan
// and the ID check must match it); the welcome says what comes next and links to the account; the
// account shows status in words; a check-in can be sent from the member's own wallet.
{
  const ADDR = '0x1CD2B147EfE092c3BdE0B474bCE3Bd33ae3dbB37';
  const JOIN_TX = '0xb5ddd1965943c8dea83c8069c7853d3c8ae66dd21b520f6f84f7cb073a2e5a40';
  const ctx = await browser.newContext({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 1 });
  await ctx.addInitScript(([a, tx, url]) => {
    const rpc = async (method, params) => (await (await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) })).json()).result;
    window.ethereum = { request: async ({ method, params }) => method === 'eth_requestAccounts' || method === 'eth_accounts' ? [a] : method === 'wallet_switchEthereumChain' ? null : method === 'eth_chainId' ? '0x1237' : method === 'eth_sendTransaction' ? tx : rpc(method, params ?? []) };
  }, [ADDR, JOIN_TX, RPC]);
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e.message).slice(0, 160)));
  const cdp = await ctx.newCDPSession(page);
  await cdp.send('WebAuthn.enable');
  await cdp.send('WebAuthn.addVirtualAuthenticator', { options: { protocol: 'ctap2', transport: 'internal', hasResidentKey: true, hasUserVerification: true, isUserVerified: true } });
  const r = {};
  try {
    await page.goto(`${BASE}/en/join?w=me&c=GHA&b=1976&a=65`, { waitUntil: 'networkidle' });
    const cont = page.getByRole('button', { name: 'Continue' }).first();
    r.sexRequired = await cont.isDisabled();
    await page.locator('label', { hasText: 'A woman' }).click();
    r.unlockedBySex = !(await cont.isDisabled());
    await cont.click();
    await page.getByRole('button', { name: 'Use my own wallet' }).click();
    await page.getByText(/on Robinhood Chain\./).first().waitFor({ timeout: 20_000 });
    await cont.click();
    await page.getByRole('button', { name: 'Create my life key' }).click();
    await page.getByText(/✓/).first().waitFor({ timeout: 15_000 });
    await cont.click();
    await cont.click();
    r.confirmShowsSex = /Sex\s*A woman/.test(await page.locator('dl').first().innerText());
    await page.getByRole('button', { name: 'Join Tonti' }).click();
    await page.getByText('What happens next').waitFor({ timeout: 60_000 });
    await page.getByText(/first check-in is due by \d/).waitFor({ timeout: 20_000 });
    const welcome = await page.locator('main').innerText();
    r.welcome = /member 0/.test(welcome) && /Identity check/.test(welcome) && /Pay in/.test(welcome);
    await page.getByRole('link', { name: 'Go to your account' }).click();
    await page.waitForURL(/\/en\/me/);
    await page.getByRole('button', { name: 'Use my own wallet' }).click();
    await page.getByText('Member 0').waitFor({ timeout: 30_000 });
    await page.getByText(/Next check-in by \d/).waitFor({ timeout: 20_000 });
    const account = await page.locator('main').innerText();
    r.account = /Status\s*(Checked in|Check-in due)/.test(account) && !/\bactive\b/.test(account);
    await page.goto(`${BASE}/en/checkin/0`, { waitUntil: 'networkidle' });
    await page.getByRole('button', { name: /Check in with Face ID/ }).click();
    await page.getByRole('link', { name: 'Send it from my wallet' }).click();
    await page.getByRole('heading', { name: 'Send your check-in' }).waitFor({ timeout: 30_000 });
    r.ownWalletCheckIn = await page.getByRole('button', { name: 'Use my own wallet' }).isVisible();
  } catch (e) {
    r.error = String(e.message ?? e).slice(0, 300);
  }
  r.errors = errors;
  r.ok = !r.error && !errors.length && r.sexRequired && r.unlockedBySex && r.confirmShowsSex && r.welcome && r.account && r.ownWalletCheckIn;
  out.afterJoin = r;
  console.log('after joining', JSON.stringify(r));
  await ctx.close();
}

await browser.close();

const ok = out.quotes.filter((q) => q.ok);
const ms = out.quotes.filter((q) => q.ms).map((q) => q.ms);
out.summary = {
  quotes: `${ok.length}/${out.quotes.length} equal a direct eth_call, encode the inputs asked, fit a phone with the call open and name the right person`,
  afterJoin: out.afterJoin?.ok ? 'member #0 replayed: sex chosen and shown, next steps and account link, status in words, check-in sent from own wallet' : 'FAILED',
  inApp: `${out.inApp.filter((x) => x.ok).length}/${out.inApp.length} in-app browser cases right (MetaMask, Messenger say so and keep the plan; Chrome doesn't)`,
  edge: out.edge?.ok ? 'unfitted birth year refused with its reason' : 'FAILED',
  latencyMs: { p50: pctl(ms, 0.5), p95: pctl(ms, 0.95), max: Math.max(...ms) },
  odds: out.odds ? `${out.odds.checks.filter((c) => c.same).length}/${out.odds.checks.length} rows equal direct qMonth reads at block ${out.odds.block}` : null,
  routes: `${out.routes.filter((r) => r.ok).length}/${out.routes.length} route×width×language loads clean`,
  axe: `${out.axe.reduce((n, a) => n + a.violations.length, 0)} violation types over ${out.axe.length} pages`,
  passkey: out.passkey && out.passkey.credentialsOnDevice === 1 && out.passkey.stepTwoLockedBefore === true && out.passkey.stepTwoLockedAfter === false ? `life key created (rpId ${out.passkey.rpId}); step 2 unlocked` : 'FAILED',
};
mkdirSync(RUNS, { recursive: true });
writeFileSync(`${RUNS}/judge.json`, JSON.stringify(out, null, 1));
console.log(JSON.stringify(out.summary, null, 1));
