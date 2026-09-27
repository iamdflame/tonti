// Screenshots for design review: node scripts/shot.mjs <url> <out-prefix> [--ask]
import { chromium } from '@playwright/test';
const [url, out, ...flags] = process.argv.slice(2);
const browser = await chromium.launch();
for (const [name, viewport] of [['mobile', { width: 390, height: 844 }], ['desktop', { width: 1440, height: 900 }]]) {
  const page = await browser.newPage({ viewport, deviceScaleFactor: name === 'mobile' ? 2 : 1 });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
  await page.goto(url, { waitUntil: 'networkidle', timeout: 90000 });
  if (flags.includes('--ask')) {
    await page.getByRole('button', { name: /Ask Robinhood Chain|Itanong/ }).click();
    await page.getByText(/a month|kada buwan/).first().waitFor({ timeout: 60000 });
    await page.waitForTimeout(2500);
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.mouse.move(0, 0);
    await page.waitForTimeout(300);
  }
  await page.screenshot({ path: `${out}-${name}.png`, fullPage: flags.includes('--full') });
  console.log(name, errors.length ? `errors: ${errors.slice(0, 5).join(' | ')}` : 'no errors');
  await page.close();
}
await browser.close();
