// S4 + S5: the gasp, then the proof. The live landing page: a question asked at a person's pace, the
// chain's answer, the chart, then "Run it yourself" re-asks a public node. Nothing is prepared: the
// answer comes from the Actuary contract when Ask is pressed, and every wait is its true length.
//   node video/record/take-quote.mjs [name] [dpr]
import { SITE, browser, context, hands, open, session } from './lib.mjs';

const name = process.argv[2] ?? 'S4-quote';
const dpr = Number(process.argv[3] ?? 1.5);
const b = await browser();
const ctx = await context(b, { dpr });
const page = await ctx.newPage();
const rpcMs = [];
page.on('requestfinished', (r) => { if (r.url().includes('rpc.mainnet') && r.method() === 'POST') rpcMs.push(Math.round(r.timing().responseEnd)); });
await open(page, `${SITE}/en`);
await page.evaluate(() => window.scrollTo(0, 0));
const { vt, finish } = await session(page, name);
const h = hands(page, vt);
const form = page.locator('#ask form');

// The question, as the page asks it.
await vt.wait(1800);
vt.mark('question');
await h.scrollTo(form, { offset: 96, ms: 1700 });
await vt.wait(300);
await h.start(1180, 760);
await vt.wait(400);
vt.mark('form');
await h.tap(page.locator('#ask label', { hasText: /^Mother$/ }).first(), { ms: 850 });
await vt.wait(250);
await h.moveTo(page.locator('#country'), 650);
await page.selectOption('#country', 'PHL');
await vt.wait(600);
// Income from 65 instead of 62: three taps on +.
const plus = page.getByRole('button', { name: /Income starts at age \+1/ });
await h.moveTo(plus, 700);
for (let k = 0; k < 3; k++) {
  await h.tap(plus, { ms: 1, hold: 70 });
  await vt.wait(260);
}
await h.moveTo(page.locator('#lump'), 600);
await h.type(page.locator('#lump'), '5000');
await vt.wait(300);
await h.moveTo(page.locator('#monthly'), 550);
await h.type(page.locator('#monthly'), '50');
await vt.wait(500);
const ask = page.locator('#ask form button[type=submit]');
await h.moveTo(ask, 750);
await vt.wait(250);
vt.mark('ask');
await h.tap(ask, { ms: 1 });
const amount = page.locator('#ask [aria-live=polite] p.font-mono').first();
const wait = await vt.real(() => amount.waitFor({ timeout: 30_000 }));
vt.mark('answer');
await h.hide(200);
await vt.wait(4800); // the one orchestrated moment: the sun settles, the number, "for life", the two lamps
vt.mark('settled');
await h.scrollTo(page.locator('#ask [aria-live=polite] svg').first(), { offset: 140, ms: 1900 });
await vt.wait(3800);
vt.mark('chart');

// S5: the exact call, re-run from this browser on a node we don't run.
const run = page.locator('#ask summary', { hasText: 'Run it yourself' });
await h.scrollTo(run, { offset: 300, ms: 1500 });
await h.start(1500, 820);
await h.tap(run, { ms: 800 });
await vt.wait(1600);
vt.mark('call');
const rerun = page.getByRole('button', { name: 'Re-run it from this browser' });
await h.scrollTo(rerun, { offset: 520, ms: 1200 });
await h.tap(rerun, { ms: 700 });
const same = page.getByText('Same answer, from a node we don’t run.');
const again = await vt.real(() => same.waitFor({ timeout: 30_000 }));
vt.mark('same');
await h.hide(200);
await vt.wait(3200);
const stats = await finish();
const shown = (await amount.textContent()).trim();
const computed = (await page.getByText(/Computed by the Actuary contract/).textContent()).trim();
console.log(JSON.stringify({ ...stats, rpcMs, chainSeconds: wait.seconds, rerunSeconds: again.seconds, shown, computed }));
await b.close();
