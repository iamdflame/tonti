// S8 (phone, fallback for Dave's own recording): member 0's check-in page. Face ID is Chrome's
// virtual authenticator here; the signed check-in is handed to "Send it from my wallet" and the
// take stops before anything is sent.
import { SITE, browser, context, hands, open, session } from './lib.mjs';

const name = process.argv[2] ?? 'S8-checkin';
const b = await browser();
const ctx = await context(b, { phone: true, dpr: Number(process.argv[3] ?? 2) });
const page = await ctx.newPage();
const cdp = await ctx.newCDPSession(page);
await cdp.send('WebAuthn.enable');
await cdp.send('WebAuthn.addVirtualAuthenticator', { options: { protocol: 'ctap2', transport: 'internal', hasResidentKey: true, hasUserVerification: true, isUserVerified: true } });
// A life key on this device first, made the way the join page makes it (on Dave's phone it exists).
await page.goto(`${SITE}/en`, { waitUntil: 'load' });
await page.evaluate(async () => {
  const cred = await navigator.credentials.create({ publicKey: {
    rp: { name: 'Tonti', id: location.hostname }, user: { id: crypto.getRandomValues(new Uint8Array(16)), name: 'Tonti life key', displayName: 'Tonti life key' },
    challenge: crypto.getRandomValues(new Uint8Array(32)), pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
    authenticatorSelection: { residentKey: 'required', userVerification: 'required' } } });
  const id = btoa(String.fromCharCode(...new Uint8Array(cred.rawId))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  localStorage.setItem('tonti:life-key', id);
});
await open(page, `${SITE}/en/checkin/0`, { settle: 5000 });
const { vt, finish } = await session(page, name);
const h = hands(page, vt, { phone: true });
await vt.wait(2600);
vt.mark('page');
await h.tap(page.getByRole('button', { name: /Check in with Face ID/ }));
await vt.real(() => page.getByText(/Your check-in is signed/).waitFor({ timeout: 30_000 }));
vt.mark('signed');
await vt.wait(2600);
await h.tap(page.getByRole('link', { name: 'Send it from my wallet' }));
await vt.real(() => page.getByRole('heading', { name: 'Send your check-in' }).waitFor({ timeout: 30_000 }));
vt.mark('relay');
await vt.wait(3000);
console.log(JSON.stringify(await finish()));
await b.close();
