// JS weight of a page: every script it loads, gzipped size as served. node scripts/weight.mjs <url>
import { chromium } from '@playwright/test';
const url = process.argv[2];
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 390, height: 844 } });
const js = new Map();
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
page.on('console', (m) => m.type() === 'error' && errors.push(m.text().slice(0, 200)));
page.on('response', async (r) => {
  if (r.request().resourceType() === 'script') {
    const h = await r.headerValue('content-length');
    try {
      const body = await r.body();
      js.set(r.url(), { transfer: Number(h ?? 0), raw: body.length });
    } catch {}
  }
});
await page.goto(url, { waitUntil: 'networkidle' });
await page.waitForTimeout(1500);
const { gzipSync } = await import('node:zlib');
let raw = 0, gz = 0;
for (const [u, v] of js) raw += v.raw;
// re-fetch to measure gzip size (the dev/prod server may not compress locally)
for (const u of js.keys()) { const b = Buffer.from(await (await fetch(u)).arrayBuffer()); gz += gzipSync(b).length; }
console.log(JSON.stringify({ scripts: js.size, rawKB: Math.round(raw / 1024), gzipKB: Math.round(gz / 1024), errors }));
await browser.close();
