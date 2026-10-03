// S7: "Lights that stay on". The site's own replay (2,000 Filipino women retiring in 1965, real
// market history, UN mortality), run by scrolling, as a visitor does. Held on 1977, the year the
// same money drawn alone ran out.
//   node video/record/take-lights.mjs [name] [dpr]
import { SITE, browser, context, open, session } from './lib.mjs';

const name = process.argv[2] ?? 'S7-lights';
const dpr = Number(process.argv[3] ?? 1.5);
const b = await browser();
const ctx = await context(b, { dpr });
const page = await ctx.newPage();
await open(page, `${SITE}/en`);
const geo = await page.evaluate(() => {
  const title = document.getElementById('lights-title');
  const wrap = title.closest('section').querySelector('div[class*="h-[260vh]"]');
  const r = wrap.getBoundingClientRect();
  return { titleTop: title.getBoundingClientRect().top + scrollY, wrapTop: r.top + scrollY, travel: r.height - innerHeight };
});
await page.evaluate((y) => window.scrollTo(0, y), geo.titleTop - 150);
await page.clock.runFor(500);
const { vt, finish } = await session(page, name);
const glide = async (to, ms) => {
  const from = await page.evaluate(() => scrollY);
  await page.evaluate(([dy, ms]) => window.__rec.scroll(dy, ms), [to - from, ms]);
  await vt.wait(ms + 80);
};
const atYear = (y) => geo.wrapTop + ((y - 1965) / 34) * geo.travel;

await vt.wait(2200); // the idea, in the section's own words
vt.mark('title');
await glide(geo.wrapTop, 1800); // into the replay: 1965, 2,000 lights
await vt.wait(1600);
vt.mark('1965');
await glide(atYear(1977), 5200);
vt.mark('1977');
await vt.wait(2600); // the lamp drawn alone goes out
await glide(atYear(1999), 6500);
vt.mark('1999');
await vt.wait(2400);
const stats = await finish();
console.log(JSON.stringify(stats));
await b.close();
