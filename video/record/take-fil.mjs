// S10: the same question in Filipino, on the live site. The answer ends with "habambuhay" (for life),
// written by hand.
import { SITE, browser, context, hands, open, session } from './lib.mjs';

const name = process.argv[2] ?? 'S10-fil';
const b = await browser();
const ctx = await context(b, { dpr: Number(process.argv[3] ?? 1.5) });
const page = await ctx.newPage();
const rpcMs = [];
page.on('requestfinished', (r) => { if (r.url().includes('rpc.mainnet') && r.method() === 'POST') rpcMs.push(Math.round(r.timing().responseEnd)); });
await open(page, `${SITE}/fil?w=mother&c=PHL&b=1966&a=65&l=5000&m=50`);
await page.evaluate(() => window.scrollTo(0, 0));
const { vt, finish } = await session(page, name);
const h = hands(page, vt);
await vt.wait(2400);
vt.mark('question');
const ask = page.locator('#ask form button[type=submit]');
await h.scrollTo(page.locator('#ask form'), { offset: 96, ms: 1600 });
await h.start(1180, 700);
await h.moveTo(ask, 900);
await vt.wait(300);
vt.mark('ask');
await h.tap(ask, { ms: 1 });
const amount = page.locator('#ask [aria-live=polite] p.font-mono').first();
const w = await vt.real(() => amount.waitFor({ timeout: 30_000 }));
vt.mark('answer');
await h.hide(200);
await vt.wait(6000);
const stats = await finish();
console.log(JSON.stringify({ ...stats, rpcMs, chainSeconds: w.seconds, shown: (await amount.textContent()).trim() }));
await b.close();
