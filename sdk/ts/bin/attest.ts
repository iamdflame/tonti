#!/usr/bin/env node
/**
 * The attester's tool: after checking a member's identity document (by video call), sign the
 * identity statement `AttestedIdentity` verifies, and optionally submit it.
 *
 *   node bin/attest.ts --member 3 --country PHL --sex female --born 1966 --key-file ~/.tonti-attester.key [--submit]
 *   node bin/attest.ts ... --recover-to 0xNewWallet --qx 0x.. --qy 0x..   (a lost phone or wallet)
 *
 * It refuses to sign unless the document's birth year, sex and country give exactly the cohort key
 * the member joined with: that is the check that stops anyone claiming to be older than they are.
 * With --recover-to, the statement is bound to the new payout address and passkey, and --submit
 * sends `recover`; otherwise it is a plain identification (`strongProof`, or `revive` for a member
 * whose death was final). The key file must be mode 600 and hold one hex private key.
 */
import { readFileSync, statSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { createPublicClient, createWalletClient, defineChain, http } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import { type Iso3, type Sex, attestIdentity, cohortKey, lifeRegistryAbi, recoveryAction } from '../src/index.ts';

const { values: a } = parseArgs({
  options: {
    member: { type: 'string' },
    country: { type: 'string' },
    sex: { type: 'string' },
    born: { type: 'string' },
    'key-file': { type: 'string' },
    deployment: { type: 'string', default: new URL('../../../config/deployment.json', import.meta.url).pathname },
    rpc: { type: 'string', default: 'https://rpc.mainnet.chain.robinhood.com' },
    'valid-days': { type: 'string', default: '7' },
    submit: { type: 'boolean', default: false },
    'recover-to': { type: 'string' },
    qx: { type: 'string' },
    qy: { type: 'string' },
  },
});
const need = (k: keyof typeof a) => {
  const v = a[k];
  if (v === undefined || v === '') throw new Error(`--${k} is required`);
  return String(v);
};

const keyFile = need('key-file');
if ((statSync(keyFile).mode & 0o077) !== 0) throw new Error(`${keyFile} must be chmod 600`);
const account = privateKeyToAccount(readFileSync(keyFile, 'utf8').trim() as `0x${string}`);
const dep = JSON.parse(readFileSync(a.deployment!, 'utf8'));
const transport = http(a.rpc);
const client = createPublicClient({ transport });
const chain = defineChain({ id: await client.getChainId(), name: 'robinhood', nativeCurrency: { name: 'ETH', symbol: 'ETH', decimals: 18 }, rpcUrls: { default: { http: [a.rpc!] } } });
const wallet = createWalletClient({ chain, transport, account });

const memberId = BigInt(need('member'));
const sex = need('sex') as Sex;
if (sex !== 'female' && sex !== 'male') throw new Error('--sex must be female or male');
const key = cohortKey(need('country') as Iso3, sex, Number(need('born')));
const life = await client.readContract({ address: dep.lifeRegistry, abi: lifeRegistryAbi, functionName: 'life', args: [memberId] });
if (life.key !== key) {
  throw new Error(`member ${memberId} joined as cohort ${life.key}, but the document gives ${key}: refusing to attest`);
}
const recovery = a['recover-to'] ? { payout: a['recover-to'] as `0x${string}`, qx: need('qx') as `0x${string}`, qy: need('qy') as `0x${string}` } : null;
const action = recovery ? recoveryAction(recovery.payout, recovery.qx, recovery.qy) : undefined;
const proof = await attestIdentity(wallet, { verifier: dep.attestedIdentity, registry: dep.lifeRegistry, memberId, key, action, validForDays: Number(a['valid-days']) });
console.log(JSON.stringify({ memberId: memberId.toString(), key: key.toString(), verifier: dep.attestedIdentity, action: action ?? 'identify', proof }));
if (a.submit) {
  const status = await client.readContract({ address: dep.lifeRegistry, abi: lifeRegistryAbi, functionName: 'status', args: [memberId] });
  const dead = status === 5 || status === 6;
  const hash = recovery
    ? await wallet.writeContract({ address: dep.lifeRegistry, abi: lifeRegistryAbi, functionName: 'recover', args: [memberId, recovery.payout, recovery.qx, recovery.qy, dep.attestedIdentity, proof] })
    : await wallet.writeContract({ address: dep.lifeRegistry, abi: lifeRegistryAbi, functionName: dead ? 'revive' : 'strongProof', args: [memberId, dep.attestedIdentity, proof] });
  const r = await client.waitForTransactionReceipt({ hash });
  console.log(`submitted ${hash}: ${r.status}`);
  if (recovery) console.log('recovery requested: it applies after 14 days (finishRecovery; the keeper does it) unless the member checks in with their current passkey first');
}
