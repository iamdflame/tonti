// S8 (phone): member #0's join, replayed through the live site. A stand-in wallet answers "send"
// with the hash of the join that is already on mainnet (0xb5dd…2a40), so the welcome and the
// account screens read the real chain. The life key comes from Chrome's virtual authenticator.
// The video labels this as a replay; Dave's own iPhone recording carries the real Face ID moment.
//   node video/record/take-member.mjs [name] [dpr]
import { SITE, browser, context, hands, open, session } from './lib.mjs';

const name = process.argv[2] ?? 'S8-member';
const dpr = Number(process.argv[3] ?? 2);
const ADDR = '0x1CD2B147EfE092c3BdE0B474bCE3Bd33ae3dbB37';
const JOIN_TX = '0xb5ddd1965943c8dea83c8069c7853d3c8ae66dd21b520f6f84f7cb073a2e5a40';
const RPC = 'https://rpc.mainnet.chain.robinhood.com';

const b = await browser();
const ctx = await context(b, { phone: true, dpr });
await ctx.addInitScript(([a, tx, url]) => {
  const rpc = async (method, params) => (await (await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) })).json()).result;
  window.ethereum = { request: async ({ method, params }) => method === 'eth_requestAccounts' || method === 'eth_accounts' ? [a] : method === 'wallet_switchEthereumChain' ? null : method === 'eth_chainId' ? '0x1237' : method === 'eth_sendTransaction' ? tx : rpc(method, params ?? []) };
}, [ADDR, JOIN_TX, RPC]);
const page = await ctx.newPage();
const cdp = await ctx.newCDPSession(page);
await cdp.send('WebAuthn.enable');
await cdp.send('WebAuthn.addVirtualAuthenticator', { options: { protocol: 'ctap2', transport: 'internal', hasResidentKey: true, hasUserVerification: true, isUserVerified: true } });
await open(page, `${SITE}/en/join?w=me&c=GHA&b=1976&a=65`);
const { vt, finish } = await session(page, name);
const h = hands(page, vt, { phone: true });
const cont = () => page.getByRole('button', { name: 'Continue' }).first();

await vt.wait(1200);
vt.mark('plan');
await h.tap(page.locator('label', { hasText: 'A woman' }));
await vt.wait(900);
await h.scrollTo(cont(), { offset: 560, ms: 1300 });
await vt.wait(500);
await h.tap(cont());
await vt.wait(700);
vt.mark('wallet');
await h.tap(page.getByRole('button', { name: 'Use my own wallet' }));
await vt.real(() => page.getByText(/on Robinhood Chain\./).first().waitFor({ timeout: 30_000 }));
await vt.wait(1600);
await h.tap(cont());
await vt.wait(800);
vt.mark('key');
await h.tap(page.getByRole('button', { name: 'Create my life key' }));
await vt.real(() => page.getByText(/✓/).first().waitFor({ timeout: 20_000 }));
await vt.wait(1400);
await h.tap(cont());
await vt.wait(1000);
await h.tap(cont());
await vt.wait(800);
vt.mark('confirm');
await vt.real(() => page.getByText(/on Robinhood Chain\./).first().waitFor({ timeout: 30_000 }));
await vt.wait(2400);
await h.scrollTo(page.getByRole('button', { name: 'Join Tonti' }), { offset: 640, ms: 1100 });
await vt.wait(400);
vt.mark('join');
await h.tap(page.getByRole('button', { name: 'Join Tonti' }));
await vt.real(() => page.getByText(/first check-in is due by \d/).waitFor({ timeout: 60_000 }));
await page.evaluate(() => window.scrollTo(0, 0));
vt.mark('welcome');
await vt.wait(3600);
await h.scrollTo(page.getByRole('link', { name: 'Go to your account' }), { offset: 520, ms: 1300 });
await vt.wait(700);
await h.tap(page.getByRole('link', { name: 'Go to your account' }));
await vt.real(() => page.getByRole('button', { name: 'Use my own wallet' }).waitFor({ timeout: 30_000 }));
await vt.wait(500);
vt.mark('account');
await h.tap(page.getByRole('button', { name: 'Use my own wallet' }));
await vt.real(() => page.getByText(/Worth US\$/).waitFor({ timeout: 40_000 }));
await vt.wait(1500);
await h.scrollTo(page.getByText('Member 0'), { offset: 110, ms: 1500 });
await vt.wait(3800);
const stats = await finish();
console.log(JSON.stringify({ ...stats, worth: (await page.getByText(/Worth US\$/).textContent()).trim() }));
await b.close();
