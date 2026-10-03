// S8 (desktop): the Live pool page, read from the chain as it loads: one member, one settlement,
// the money by sleeve, the odds table priced by the Actuary, the contracts.
import { SITE, browser, context, hands, open, session } from './lib.mjs';

const name = process.argv[2] ?? 'S8-pool';
const b = await browser();
const ctx = await context(b, { dpr: Number(process.argv[3] ?? 1.5) });
const page = await ctx.newPage();
await open(page, `${SITE}/en/pool`, { settle: 9000 });
const { vt, finish } = await session(page, name);
const h = hands(page, vt);
await vt.wait(3200);
vt.mark('top');
await h.scrollTo(page.getByText('Money in the pool').first(), { offset: 140, ms: 2000 });
await vt.wait(3000);
vt.mark('money');
await h.scrollTo(page.getByText('Priced on the chain, country by country').first(), { offset: 110, ms: 2200 });
await vt.wait(3500);
vt.mark('odds');
await h.scroll(700, 2600);
await vt.wait(2500);
vt.mark('contracts');
console.log(JSON.stringify(await finish()));
await b.close();
